//! The daemon's flight log.
//!
//! The DCS hook starts the daemon through `run-hidden.vbs`, with no console and
//! nothing redirected, so every line it prints goes somewhere nobody can read.
//! That is fine until a user reports that a panel went dark in the middle of a
//! flight, at which point there is nothing at all to look at: not the error, not
//! the profile it was running, not even whether the device was ever opened.
//!
//! So the daemon writes the same account to a file, and writes it beside
//! `dcs.log` in `Saved Games\DCS\Logs`, because that is the folder users already
//! know to zip up when something goes wrong.
//!
//! Two files are kept, this session's and the one before it: [`FILE`] and
//! [`BACKUP`]. A session that fills [`MAX_BYTES`] rolls over into the same pair,
//! so a long flight cannot fill a disk, and the fresh file opens with the
//! session header repeated so it still says what was running.
//!
//! Every record is written and flushed on the spot. The failures this exists to
//! explain are crashes and hangs, and a buffered line is precisely the one that
//! would be lost.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// This session's log.
pub const FILE: &str = "dcs-signal.log";
/// The session before this one. One is kept, and it is replaced on each start.
pub const BACKUP: &str = "dcs-signal.log.bak";

/// Roll over at ten megabytes, which is a few hours of a busy cockpit.
///
/// Rolling rather than stopping, because a log that goes quiet halfway through
/// is worse than useless: it reads exactly like the daemon having died.
pub const MAX_BYTES: u64 = 10 * 1024 * 1024;

/// How often any one named thing may appear in the log.
///
/// A gauge bound to a readout moves on every export frame, thirty times a
/// second, and logging each change would bury the one line that matters and
/// roll the file away within the hour. One line a second, carrying the latest
/// value and how many changes it stands for, is the most a person reads anyway.
pub const THROTTLE: Duration = Duration::from_secs(1);

/// How loud a record is, so a reader can skim for the bad news.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// Something failed. Always worth reading.
    Error,
    /// Something is not as it should be, but the run continues.
    Warn,
    /// What the daemon did: startup, devices, aircraft, profiles, shutdown.
    Info,
    /// Traffic: signals arriving, lamps and screens being written.
    Trace,
}

impl Level {
    /// Padded, so the text of every record starts in the same column.
    fn tag(self) -> &'static str {
        match self {
            Level::Error => "ERROR",
            Level::Warn => "WARN ",
            Level::Info => "INFO ",
            Level::Trace => "TRACE",
        }
    }
}

/// The open log, if one is open. Nothing here is fatal: a daemon that cannot
/// write its log still drives the panels, which is what the user wants from it.
struct Sink {
    /// `None` only while rolling over, when the handle must be closed before
    /// Windows will let the file be renamed.
    file: Option<File>,
    dir: PathBuf,
    bytes: u64,
    cap: u64,
    /// Repeated at the top of each rolled file, so it stands on its own.
    header: Vec<String>,
    /// The one line that changes during a session: which aircraft, which
    /// profile. Held apart from the header because it is replaced, not added to.
    context: Option<String>,
}

static SINK: OnceLock<Mutex<Option<Sink>>> = OnceLock::new();

/// The sink, locked. A poisoned lock is taken anyway: a panic elsewhere is the
/// moment the log matters most, and the panic hook itself writes through here.
fn locked() -> std::sync::MutexGuard<'static, Option<Sink>> {
    SINK.get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Begin a session's log in `dir`, rotating what was there.
///
/// Returns the file written to, so the caller can say where it is.
pub fn start(dir: &Path) -> std::io::Result<PathBuf> {
    start_capped(dir, MAX_BYTES)
}

/// [`start`] with a smaller rollover, for tests.
pub fn start_capped(dir: &Path, cap: u64) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    rotate(dir);
    let file = open_fresh(dir)?;
    *locked() = Some(Sink {
        file: Some(file),
        dir: dir.to_path_buf(),
        bytes: 0,
        cap,
        header: Vec::new(),
        context: None,
    });
    Ok(dir.join(FILE))
}

/// Stop logging and close the file. Only tests need this; a daemon logs until
/// the process ends.
pub fn stop() {
    *locked() = None;
}

/// Write one record, if a log is open.
pub fn record(level: Level, text: &str) {
    if let Some(sink) = locked().as_mut() {
        sink.line(level, text);
    }
}

/// Write a record and keep it for the top of any rolled file.
///
/// For the things that are true for the whole session: the build, where its
/// files came from, which panels were found.
pub fn header(text: &str) {
    if let Some(sink) = locked().as_mut() {
        sink.line(Level::Info, text);
        sink.header.push(text.to_string());
    }
}

/// Write a record and keep it as the current state of the session, replacing
/// whatever was kept before. For the aircraft and profile in use.
pub fn context(text: &str) {
    if let Some(sink) = locked().as_mut() {
        sink.line(Level::Info, text);
        sink.context = Some(text.to_string());
    }
}

/// Delete the old backup and make this session's log the backup.
///
/// Best effort on purpose. If the user has the log open in something that locks
/// it, a failure here must not stop the daemon: the worst case is a session
/// appended to the wrong file, which still beats no log at all.
fn rotate(dir: &Path) {
    let _ = std::fs::remove_file(dir.join(BACKUP));
    let _ = std::fs::rename(dir.join(FILE), dir.join(BACKUP));
}

fn open_fresh(dir: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(dir.join(FILE))
}

impl Sink {
    fn line(&mut self, level: Level, text: &str) {
        let Some(file) = self.file.as_mut() else { return };
        // CRLF, and embedded newlines too: a screen's contents are logged as
        // the rows a person would read, and Windows tools expect both endings.
        let text = if text.contains('\n') { text.replace('\n', "\r\n") } else { text.to_string() };
        let record = format!("{}  {}  {text}\r\n", stamp(), level.tag());
        if file.write_all(record.as_bytes()).is_err() {
            return;
        }
        let _ = file.flush();
        self.bytes += record.len() as u64;
        if self.bytes >= self.cap {
            self.roll();
        }
    }

    /// Start a new file, keeping one behind, and repeat what the session was.
    fn roll(&mut self) {
        // Closed first: Windows will not rename a file that is still open.
        self.file = None;
        rotate(&self.dir);
        match open_fresh(&self.dir) {
            Ok(file) => {
                self.file = Some(file);
                self.bytes = 0;
            }
            // Nothing more can be done, and the daemon carries on regardless.
            Err(_) => return,
        }
        self.line(
            Level::Info,
            &format!("log     rolled over at {} bytes; what came before is in {BACKUP}", self.cap),
        );
        // Taken out and put back so the header can be written through `line`,
        // which needs the whole sink.
        let header = std::mem::take(&mut self.header);
        for line in &header {
            self.line(Level::Info, line);
        }
        self.header = header;
        let context = self.context.take();
        if let Some(line) = &context {
            self.line(Level::Info, line);
        }
        self.context = context;
    }
}

/// Log panics, which are otherwise completely invisible in a hidden process.
///
/// The previous hook still runs, so a developer at a console sees what they
/// always saw.
pub fn catch_panics() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        record(Level::Error, &format!("panic   {info}"));
        previous(info);
    }));
}

/// One line a second per named thing, with the rest counted rather than written.
///
/// Keyed by whatever the caller names: a signal, a lamp, a screen. The text is
/// built by the caller and the latest one wins, so what is finally written is
/// the current value rather than the first of a burst.
pub struct Throttle {
    window: Duration,
    last: HashMap<String, Instant>,
    held: HashMap<String, Held>,
}

struct Held {
    text: String,
    /// Changes since the last record for this key, the one held included.
    changes: u32,
}

impl Default for Throttle {
    fn default() -> Self {
        Self::new(THROTTLE)
    }
}

impl Throttle {
    pub fn new(window: Duration) -> Self {
        Throttle { window, last: HashMap::new(), held: HashMap::new() }
    }

    /// Offer a change. `Some` is the line to write now; `None` means it was
    /// folded into a later one.
    pub fn offer(&mut self, key: &str, now: Instant, text: String) -> Option<String> {
        let due = self.last.get(key).map_or(true, |t| now.duration_since(*t) >= self.window);
        if due {
            self.last.insert(key.to_string(), now);
            self.held.remove(key);
            return Some(text);
        }
        let held = self.held.entry(key.to_string()).or_insert(Held { text: String::new(), changes: 0 });
        held.text = text;
        held.changes += 1;
        None
    }

    /// The held lines whose second has now passed.
    ///
    /// Called every pass of the main loop, because a value that stops changing
    /// must still be written: without this, the last move of a switch could sit
    /// in `held` forever and the log would show it in the wrong position.
    pub fn due(&mut self, now: Instant) -> Vec<String> {
        let ready: Vec<String> = self
            .held
            .keys()
            .filter(|k| self.last.get(*k).map_or(true, |t| now.duration_since(*t) >= self.window))
            .cloned()
            .collect();
        let mut out = Vec::new();
        for key in ready {
            let Some(held) = self.held.remove(&key) else { continue };
            self.last.insert(key, now);
            out.push(format!("{}  (x{})", held.text, held.changes));
        }
        out
    }

    /// Forget everything. Called when the aircraft changes, since the signals
    /// being followed change with it.
    pub fn clear(&mut self) {
        self.last.clear();
        self.held.clear();
    }
}

/// Local wall clock to the millisecond, formatted as `dcs.log` formats it, so
/// the two can be read side by side.
#[cfg(windows)]
fn stamp() -> String {
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;

    // SAFETY: GetLocalTime fills the struct it is given and cannot fail.
    let t = unsafe {
        let mut t = std::mem::zeroed();
        GetLocalTime(&mut t);
        t
    };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
        t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond, t.wMilliseconds
    )
}

/// UTC, for builds that are not the product: only Windows has panels to drive.
#[cfg(not(windows))]
fn stamp() -> String {
    let since = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let (days, rest) = (since.as_secs() / 86_400, since.as_secs() % 86_400);
    let (y, m, d) = civil_from_days(days as i64);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}.{:03}",
        rest / 3600,
        (rest % 3600) / 60,
        rest % 60,
        since.subsec_millis()
    )
}

/// Days since 1970-01-01 to a calendar date, by Howard Hinnant's algorithm.
#[cfg(not(windows))]
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_burst_becomes_one_line_a_second() {
        let mut t = Throttle::new(Duration::from_millis(100));
        let start = Instant::now();

        // The first change of anything is written as it happens: a switch
        // thrown once must not wait a second to appear.
        assert_eq!(t.offer("sig:GEAR", start, "GEAR = 1".into()), Some("GEAR = 1".into()));
        // Everything inside the window is held, and the latest value wins.
        assert_eq!(t.offer("sig:GEAR", start + Duration::from_millis(10), "GEAR = 2".into()), None);
        assert_eq!(t.offer("sig:GEAR", start + Duration::from_millis(20), "GEAR = 3".into()), None);
        // Nothing is due until the window has passed.
        assert!(t.due(start + Duration::from_millis(50)).is_empty());
        assert_eq!(t.due(start + Duration::from_millis(120)), vec!["GEAR = 3  (x2)".to_string()]);
        // Written once, not again.
        assert!(t.due(start + Duration::from_millis(400)).is_empty());
    }

    #[test]
    fn keys_are_throttled_apart() {
        // One busy gauge must not silence a lamp that moved in the same frame.
        let mut t = Throttle::new(Duration::from_millis(100));
        let now = Instant::now();
        assert!(t.offer("sig:ALT", now, "ALT = 900".into()).is_some());
        assert!(t.offer("led:pto2.hook", now, "hook = 255".into()).is_some());
        assert!(t.offer("sig:ALT", now, "ALT = 901".into()).is_none());
        assert!(t.offer("led:pto2.hook", now, "hook = 0".into()).is_none());
        let due = t.due(now + Duration::from_millis(150));
        assert_eq!(due.len(), 2, "both keys come out: {due:?}");
    }

    /// One test for the file, because the log is process-wide and two tests
    /// writing at once would be writing to each other's.
    #[test]
    fn a_session_rotates_the_last_one_and_rolls_when_full() {
        let dir = std::env::temp_dir().join(format!("dsc-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let path = start_capped(&dir, 4096).expect("opening a log");
        header("run     first session");
        record(Level::Info, "hello");
        stop();
        assert!(std::fs::read_to_string(&path).expect("the log").contains("hello"));

        // A second session keeps the first as the backup.
        start_capped(&dir, 4096).expect("opening a log");
        header("run     second session");
        context("aircraft F-16C_50  ->  profile F-16C");
        record(Level::Error, "something went wrong");
        let backup = std::fs::read_to_string(dir.join(BACKUP)).expect("the backup");
        assert!(backup.contains("first session"), "the previous session is kept");

        // Fill it, and stop at the roll: the file shrinking back to its
        // header is what a roll looks like from outside.
        let mut rolled = false;
        let mut was = std::fs::metadata(&path).expect("the log").len();
        for n in 0..1000 {
            record(Level::Trace, &format!("{n:>8} ms  write  a lamp that keeps moving = {n}"));
            let now = std::fs::metadata(&path).expect("the log").len();
            if now < was {
                rolled = true;
                break;
            }
            was = now;
        }
        assert!(rolled, "the cap was never reached");
        stop();
        let rolled = std::fs::read_to_string(&path).expect("the rolled log");
        assert!(rolled.contains("rolled over"), "it says why it is short: {rolled}");
        assert!(rolled.contains("second session"), "the header is repeated");
        assert!(rolled.contains("profile F-16C"), "and what it was flying");
        assert!(
            std::fs::read_to_string(dir.join(BACKUP)).expect("the backup").contains("something went wrong"),
            "the roll keeps what came before as the backup"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_record_carries_a_date_and_a_time() {
        let s = stamp();
        assert_eq!(s.len(), 23, "yyyy-mm-dd hh:mm:ss.mmm, got {s:?}");
        assert!(s.starts_with("20"), "got {s:?}");
    }
}
