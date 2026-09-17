//! Learn mode: naming a signal by moving it in the cockpit.
//!
//! The catalogue for one module runs to hundreds of signals and as many as
//! 1,440, and the identifiers are DCS-BIOS's rather than anything a pilot would
//! say. Searching works when you already know roughly what the thing is called.
//! When you do not, the cockpit knows: flip the switch and see what moved.
//!
//! Free of I/O for the same reason as the rest of the engine. A [`Watcher`]
//! takes decoded writes and hands back a ranked list, so the whole of it can be
//! tested against a synthetic stream with no DCS and no window.
//!
//! **What makes this work at all** is that `BiosState::apply` reports whether a
//! word actually moved. DCS-BIOS re-exports its entire map several times a
//! second, so "arrived" is worthless and "changed" is everything.
//!
//! **What makes it usable** is ranking by how often a signal moved. A cockpit
//! in flight is never still: gauges, clocks, needles and engine instruments
//! move on every cycle. A switch moves once. Sorting by the count puts the
//! thing the user just did at the top and leaves the scenery at the bottom,
//! without hiding anything, which matters because some of the scenery is
//! exactly what somebody wants to bind.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use wctrl_bios::{BiosState, Write};
use wctrl_config::Module;

use crate::{ACFT_NAME_ADDRESS, ACFT_NAME_LEN};

/// How long the stream must go without showing an address we have never seen
/// before the baseline is taken as complete.
///
/// DCS-BIOS re-exports the whole map roughly every 300 ms, so one quiet period
/// of this length means every address the module publishes has been seen once
/// and every signal now has something to be compared against.
pub const DEFAULT_BASELINE_QUIET: Duration = Duration::from_millis(500);

enum Reading {
    Number { mask: u16, shift: u8 },
    Text { len: u16 },
}

/// One signal of the module, and what it has done since the watcher was armed.
struct Field {
    id: String,
    address: u16,
    reading: Reading,
    /// The value as last read. `None` until the signal has been seen whole,
    /// which for a string means every word of the field has arrived.
    last: Option<String>,
    /// What it read immediately before the first movement after arming.
    from: Option<String>,
    moves: u32,
    /// Milliseconds after arming at which it last moved.
    at_ms: u128,
}

/// One signal that moved, as the caller should show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub id: String,
    /// What it read before the first movement. `None` only when the field was
    /// still incomplete then, which a string field can be.
    pub from: Option<String>,
    pub to: String,
    /// How many times it has moved since arming. One is a switch being thrown;
    /// a hundred is a gauge, and the number is shown so the difference is the
    /// user's to judge rather than ours to hide.
    pub moves: u32,
    pub at_ms: u128,
    /// True where the value is characters rather than a number, so the caller
    /// can quote it. On a display field the spaces are the layout.
    pub text: bool,
}

/// Watches every signal in one module and reports what moved.
pub struct Watcher {
    module: String,
    fields: Vec<Field>,
    /// Word address to the fields that read it. A word carries several masked
    /// signals and a string field spans several words, so this is many to many
    /// in both directions.
    by_address: HashMap<u16, Vec<usize>>,
    state: BiosState,
    armed: Instant,
    quiet: Duration,
    /// When an address was last seen for the very first time.
    last_new: Option<Instant>,
    settled: bool,
    datagrams: u64,
}

impl Watcher {
    /// Arm a watcher over every readable signal in `module`.
    pub fn new(module: &Module, at: Instant) -> Self {
        let mut fields = Vec::new();
        let mut by_address: HashMap<u16, Vec<usize>> = HashMap::new();

        for signal in &module.signals {
            // The first output is the canonical one everywhere else in this
            // codebase, and a signal that publishes none cannot be watched.
            let Some(out) = signal.primary() else { continue };
            let reading = if out.r#type == "string" {
                // A string with no max_length gives no way to know where the
                // field ends. Skipping it is better than reading a guessed
                // length, which would report movement belonging to whatever
                // signal sits after it.
                match out.max_length {
                    Some(len) if len > 0 => Reading::Text { len },
                    _ => continue,
                }
            } else {
                Reading::Number {
                    mask: out.mask.unwrap_or(u16::MAX),
                    shift: out.shift,
                }
            };

            let index = fields.len();
            let words: u16 = match &reading {
                Reading::Number { .. } => 1,
                Reading::Text { len } => len.div_ceil(2),
            };
            for word in 0..words {
                by_address
                    .entry(out.address.wrapping_add(word * 2))
                    .or_default()
                    .push(index);
            }
            fields.push(Field {
                id: signal.id.clone(),
                address: out.address,
                reading,
                last: None,
                from: None,
                moves: 0,
                at_ms: 0,
            });
        }

        Watcher {
            module: module.module.clone(),
            fields,
            by_address,
            state: BiosState::new(),
            armed: at,
            quiet: DEFAULT_BASELINE_QUIET,
            last_new: None,
            settled: false,
            datagrams: 0,
        }
    }

    pub fn module(&self) -> &str {
        &self.module
    }

    /// Feed one datagram's worth of writes.
    pub fn ingest(&mut self, writes: &[Write], at: Instant) {
        self.datagrams += 1;

        let mut touched: Vec<usize> = Vec::new();
        for w in writes {
            if self.state.word(w.address).is_none() {
                self.last_new = Some(at);
            }
            // Only a word that actually moved is worth looking at. Everything
            // else in the stream is the periodic re-export, which is most of it.
            if !self.state.apply(*w) {
                continue;
            }
            if let Some(fields) = self.by_address.get(&w.address) {
                touched.extend(fields.iter().copied());
            }
        }

        // The baseline is complete once nothing new has appeared for a while.
        // Sticky, because an address arriving late is not a reason to tell the
        // user we are no longer ready.
        if !self.settled {
            if let Some(seen) = self.last_new {
                self.settled = at.saturating_duration_since(seen) >= self.quiet;
            }
        }

        // A string spans several words, so one datagram can touch the same
        // field more than once and it should still count as one movement.
        touched.sort_unstable();
        touched.dedup();

        let elapsed = at.saturating_duration_since(self.armed).as_millis();
        for i in touched {
            let Some(now) = self.read(i) else { continue };
            let field = &mut self.fields[i];
            match &field.last {
                // First sighting is the baseline, never a movement. Without
                // this every signal in the module would be reported the moment
                // the watcher was armed.
                None => field.last = Some(now),
                // The word moved but this signal's bits did not. A word carries
                // several signals, and this is where most of the noise goes.
                Some(prev) if *prev == now => {}
                Some(prev) => {
                    if field.moves == 0 {
                        field.from = Some(prev.clone());
                    }
                    field.moves += 1;
                    field.last = Some(now);
                    field.at_ms = elapsed;
                }
            }
        }
    }

    fn read(&self, i: usize) -> Option<String> {
        let field = &self.fields[i];
        match &field.reading {
            Reading::Number { mask, shift } => self
                .state
                .value(field.address, *mask, *shift)
                .map(|v| v.to_string()),
            Reading::Text { len } => self.state.text(field.address, *len),
        }
    }

    /// Whether every signal has a value to be compared against yet.
    ///
    /// Worth reporting, because until it is true an empty list means "still
    /// listening" and afterwards it means "nothing in the cockpit moved", and
    /// those call for opposite reactions from the user.
    pub fn ready(&self) -> bool {
        self.settled
    }

    pub fn datagrams(&self) -> u64 {
        self.datagrams
    }

    /// The aircraft DCS is flying, once the stream has said.
    ///
    /// The user can perfectly well have the editor open on one profile while
    /// sitting in another aircraft, and then nothing they flip will ever
    /// appear. Reporting it is what turns that from a mystery into a sentence.
    pub fn aircraft(&self) -> Option<String> {
        let name = self.state.string(ACFT_NAME_ADDRESS, ACFT_NAME_LEN)?;
        let name = name.trim();
        (!name.is_empty()).then(|| name.to_string())
    }

    /// What has moved since arming, most switch-like first.
    pub fn changes(&self) -> Vec<Change> {
        let mut out: Vec<Change> = self
            .fields
            .iter()
            .filter(|f| f.moves > 0)
            .map(|f| Change {
                id: f.id.clone(),
                from: f.from.clone(),
                to: f.last.clone().unwrap_or_default(),
                moves: f.moves,
                at_ms: f.at_ms,
                text: matches!(f.reading, Reading::Text { .. }),
            })
            .collect();
        // Fewest movements first, and within a count the most recent first. A
        // switch thrown once outranks a gauge that has moved ninety times, and
        // two switches thrown in turn come back in the reverse of that order,
        // which is what someone who just flipped two things is looking for.
        out.sort_by(|a, b| a.moves.cmp(&b.moves).then_with(|| b.at_ms.cmp(&a.at_ms)));
        out
    }

    /// Forget what has moved, keeping every baseline.
    ///
    /// This is the "watch again" case, and it deliberately does not rebuild the
    /// state: the second switch should be found as quickly as the first, not
    /// after another full export cycle.
    pub fn rearm(&mut self, at: Instant) {
        for field in &mut self.fields {
            field.moves = 0;
            field.from = None;
            field.at_ms = 0;
        }
        self.armed = at;
    }
}
