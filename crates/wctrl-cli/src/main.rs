//! Headless driver and diagnostics.
//!
//! Exists ahead of the UI so every layer can be exercised on real hardware and a
//! real DCS-BIOS stream before any of it is wrapped in Tauri.

use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use wctrl_bios::{BiosState, Listener, Write as BiosWrite};
use wctrl_config::{Catalogue, DeviceInventory, Profile};
use wctrl_engine::{Batch, Cause, Engine};
use wctrl_hid::Device;

#[derive(Parser)]
#[command(name = "wctrl", about = "WinCtrl module signal converter")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List connected WinCtrl HID interfaces.
    Devices,
    /// Broadcast a heartbeat; every sub-part answers with its own id.
    Parts {
        #[arg(long, value_parser = parse_hex16, default_value = "0xbf05")]
        pid: u16,
    },
    /// Set one LED.
    Led {
        #[arg(long, value_parser = parse_hex16, default_value = "0xbf05")]
        pid: u16,
        #[arg(long, value_parser = parse_hex32, default_value = "0xbf05")]
        part: u32,
        #[arg(long)]
        index: u8,
        #[arg(long, default_value_t = 255)]
        value: u8,
    },
    /// Blink an LED a given number of times, for identification across a room.
    Blink {
        #[arg(long, value_parser = parse_hex16, default_value = "0xbf05")]
        pid: u16,
        #[arg(long, value_parser = parse_hex32, default_value = "0xbf05")]
        part: u32,
        #[arg(long, default_value_t = 1)]
        index: u8,
        #[arg(long, default_value_t = 5)]
        count: u32,
        #[arg(long, default_value_t = 6)]
        countdown: u32,
    },
    /// Map LED indices to physical lamps by observation.
    ///
    /// `--mode all` lights every index at once to reveal how many lamps exist;
    /// `--mode each` walks them one at a time so each can be named.
    Sweep {
        #[arg(long, value_parser = parse_hex16, default_value = "0xbf05")]
        pid: u16,
        #[arg(long, value_parser = parse_hex32, default_value = "0xbf05")]
        part: u32,
        #[arg(long, default_value = "all")]
        mode: String,
        #[arg(long, default_value_t = 0)]
        from: u8,
        #[arg(long, default_value_t = 31)]
        to: u8,
        #[arg(long, default_value_t = 3)]
        hold: u64,
        #[arg(long, default_value_t = 8)]
        countdown: u32,
    },
    /// Determine whether an on/off LED takes a magnitude, or only 0/1 with its
    /// brightness governed by a master dimmer.
    ProbeBrightness {
        #[arg(long, value_parser = parse_hex16, default_value = "0xbf05")]
        pid: u16,
        #[arg(long, value_parser = parse_hex32, default_value = "0xbf05")]
        part: u32,
        /// The dimmer believed to govern the test lamp (PTO2 SL = 2).
        #[arg(long, default_value_t = 2)]
        master: u8,
        /// An on/off lamp to observe (PTO2 HOOK = 17).
        #[arg(long, default_value_t = 17)]
        lamp: u8,
        #[arg(long, default_value_t = 8)]
        countdown: u32,
        /// Seconds to hold each phase.
        #[arg(long, default_value_t = 5)]
        hold: u64,
    },
    /// Listen to the DCS-BIOS export stream and report what arrives.
    Listen {
        #[arg(long, default_value_t = 15)]
        seconds: u64,
        /// Print every write rather than a summary.
        #[arg(long)]
        verbose: bool,
        /// Watch a signal, repeatable. Either a catalogue id such as
        /// --watch FLAP_POS, or a raw address:mask:shift in hex.
        ///
        /// Several watches share one timeline, which is the only way to see how
        /// signals line up: a gauge alone cannot say which detent it was
        /// travelling towards.
        #[arg(long)]
        watch: Vec<String>,
        /// Catalogue module for resolving watch names. Detected from the stream
        /// when omitted, which costs the first few samples.
        #[arg(long)]
        module: Option<String>,
        #[arg(long, default_value = "data/catalogue")]
        catalogue: PathBuf,
    },
    /// Run the converter: watch DCS-BIOS and drive the panels.
    Run {
        #[arg(long, default_value = "data/devices.json")]
        devices: PathBuf,
        #[arg(long, default_value = "data/catalogue")]
        catalogue: PathBuf,
        #[arg(long, default_value = "data/profiles")]
        profiles: PathBuf,
        /// Print what would be written without opening any device. Lets the
        /// whole pipeline be checked against live DCS with no hardware present.
        #[arg(long)]
        dry_run: bool,
        /// Log every action on one timeline: the signals the active profile
        /// reads as they move, and every LED write by lamp name. Works with or
        /// without --dry-run, so it can be used while actually flying.
        #[arg(long)]
        verbose: bool,
        /// Stop after this many seconds. Runs until Ctrl-C when omitted.
        #[arg(long)]
        seconds: Option<u64>,
    },
    /// Catalogue summary, or one module's signals.
    Catalogue {
        #[arg(long, default_value = "data/catalogue")]
        dir: PathBuf,
        /// Runtime aircraft name, e.g. F-4E-45MC
        #[arg(long)]
        aircraft: Option<String>,
        /// Case-insensitive filter on identifier or description.
        #[arg(long)]
        find: Option<String>,
    },
}

fn parse_hex16(s: &str) -> Result<u16, std::num::ParseIntError> {
    let s = s.trim_start_matches("0x");
    u16::from_str_radix(s, 16)
}

fn parse_hex32(s: &str) -> Result<u32, std::num::ParseIntError> {
    let s = s.trim_start_matches("0x");
    u32::from_str_radix(s, 16)
}

fn open(pid: u16) -> Result<Device> {
    let api = hidapi::HidApi::new().context("opening HID API")?;
    Device::open(&api, pid).with_context(|| format!("opening device 0x{pid:04x}"))
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Devices => {
            let api = hidapi::HidApi::new()?;
            let found = wctrl_hid::enumerate(&api);
            if found.is_empty() {
                println!("No WinCtrl devices (vendor 0x{:04x}) found.", wctrl_hid::VENDOR_ID);
            }
            for d in found {
                println!("PID=0x{:04x}  {}", d.product_id, d.product);
                if !d.serial.is_empty() {
                    println!("    serial {}", d.serial);
                }
            }
        }

        Command::Parts { pid } => {
            let device = open(pid)?;
            let parts = device.discover_parts(Duration::from_millis(1500))?;
            if parts.is_empty() {
                println!("No replies. Is another program holding the device busy?");
            }
            for part in parts {
                println!("part 0x{part:04x}");
            }
        }

        Command::Led {
            pid,
            part,
            index,
            value,
        } => {
            let device = open(pid)?;
            // Clear anything already queued, so what we read back is a response
            // to this write rather than a report that arrived beforehand.
            device.drain_replies(Duration::from_millis(250));
            device.set_led(part, index, value)?;
            println!("part 0x{part:04x} led {index} = {value}");
            let replies = device.drain_replies(Duration::from_millis(400));
            if replies.is_empty() {
                println!("  no ack - the device did not answer this write");
            }
            for reply in replies {
                let echoed = reply.data.len() >= 3
                    && reply.data[0] == wctrl_hid::CMD_SET_LEDX
                    && reply.data[1] == index
                    && reply.data[2] == value;
                println!(
                    "  ack part 0x{:04x} {:02x?}{}",
                    reply.part_id,
                    reply.data,
                    if echoed { "  (matches our write)" } else { "" }
                );
            }
        }

        Command::Blink {
            pid,
            part,
            index,
            count,
            countdown,
        } => {
            println!("Blinking part 0x{part:04x} led {index}, {count} times.");
            for n in (1..=countdown).rev() {
                println!("  starting in {n}...");
                std::thread::sleep(Duration::from_secs(1));
            }
            let device = open(pid)?;
            for n in 1..=count {
                device.set_led(part, index, 255)?;
                std::thread::sleep(Duration::from_millis(500));
                device.set_led(part, index, 0)?;
                std::thread::sleep(Duration::from_millis(400));
                println!("  blink {n}/{count}");
            }
        }

        Command::Sweep {
            pid,
            part,
            mode,
            from,
            to,
            hold,
            countdown,
        } => sweep(pid, part, &mode, from, to, hold, countdown)?,

        Command::ProbeBrightness {
            pid,
            part,
            master,
            lamp,
            countdown,
            hold,
        } => probe_brightness(pid, part, master, lamp, countdown, hold)?,

        Command::Listen {
            seconds,
            verbose,
            watch,
            module,
            catalogue,
        } => listen(seconds, verbose, &watch, module.as_deref(), &catalogue)?,

        Command::Run {
            devices,
            catalogue,
            profiles,
            dry_run,
            verbose,
            seconds,
        } => run(&devices, &catalogue, &profiles, dry_run, verbose, seconds)?,

        Command::Catalogue {
            dir,
            aircraft,
            find,
        } => catalogue(&dir, aircraft.as_deref(), find.as_deref())?,
    }
    Ok(())
}

/// Discover which LED indices drive a physical lamp.
///
/// The vendor's `DeviceConfig.js` names are not trustworthy past the few we have
/// verified: index 17 acks and lights nothing. Measuring beats inheriting a
/// table, so this drives indices directly and lets the operator name them.
fn sweep(
    pid: u16,
    part: u32,
    mode: &str,
    from: u8,
    to: u8,
    hold: u64,
    countdown: u32,
) -> Result<()> {
    anyhow::ensure!(from <= to, "--from must not exceed --to");
    let all = match mode {
        "all" => true,
        "each" => false,
        other => anyhow::bail!("--mode wants 'all' or 'each', got {other:?}"),
    };

    println!("Sweeping part {part:#06x}, indices {from}..={to}, mode '{mode}'.");
    for n in (1..=countdown).rev() {
        println!("  starting in {n}...");
        std::thread::sleep(Duration::from_secs(1));
    }
    println!();

    let device = open(pid)?;
    // Always start from a known dark panel so anything lit is ours.
    for index in from..=to {
        device.set_led(part, index, 0)?;
    }
    std::thread::sleep(Duration::from_millis(400));

    if all {
        println!("  ALL indices {from}..={to} -> 255. Count what lights up.");
        for index in from..=to {
            device.set_led(part, index, 255)?;
        }
        std::thread::sleep(Duration::from_secs(hold.max(5)));
        println!("  clearing.");
        for index in from..=to {
            device.set_led(part, index, 0)?;
        }
    } else {
        for index in from..=to {
            println!("  index {index:>2}");
            device.set_led(part, index, 255)?;
            std::thread::sleep(Duration::from_secs(hold));
            device.set_led(part, index, 0)?;
            std::thread::sleep(Duration::from_millis(350));
        }
        println!("\nName the lamp that lit at each index; indices that did nothing are unused.");
    }
    Ok(())
}

/// Walk a sequence of states that separates two hypotheses:
///
///   A. the on/off lamp takes a magnitude, like the dimmers do
///   B. it takes only on/off, and `master` scales it behind the scenes
///
/// Phases 1-3 vary only the lamp's value at full master: if they look identical,
/// the lamp ignores magnitude. Phase 4 lowers the master *without rewriting the
/// lamp*: if the lamp dims anyway, the master is applied live rather than at
/// write time, which decides whether we must re-send lamps when the master moves.
fn probe_brightness(
    pid: u16,
    part: u32,
    master: u8,
    lamp: u8,
    countdown: u32,
    hold: u64,
) -> Result<()> {
    let phases: &[(&str, u8, u8, &str)] = &[
        ("1", 255, 1, "master FULL, lamp = 1"),
        ("2", 255, 128, "master FULL, lamp = 128"),
        ("3", 255, 255, "master FULL, lamp = 255"),
        ("4", 40, 255, "master LOW (40), lamp unchanged at 255"),
        ("5", 40, 1, "master LOW (40), lamp rewritten as 1"),
        ("6", 255, 0, "lamp off, master restored"),
    ];

    println!("Probing part {part:#06x}: master dimmer index {master}, test lamp index {lamp}.");
    println!("Watch the lamp. Each phase holds {hold}s and is announced before it starts.\n");
    for n in (1..=countdown).rev() {
        println!("  starting in {n}...");
        std::thread::sleep(Duration::from_secs(1));
    }
    println!();

    let device = open(pid)?;
    for (name, master_value, lamp_value, description) in phases {
        println!("  PHASE {name}: {description}");
        device.set_led(part, master, *master_value)?;
        device.set_led(part, lamp, *lamp_value)?;
        std::thread::sleep(Duration::from_secs(hold));
    }
    device.set_led(part, lamp, 0)?;

    println!(
        "\nWhat to report, per phase:\n\
         \x20 1 vs 2 vs 3 identical      -> the lamp ignores magnitude; it is on/off only\n\
         \x20 1 < 2 < 3 in brightness    -> the lamp is dimmable after all\n\
         \x20 4 dimmer than 3            -> the master scales lamps live\n\
         \x20 4 same as 3                -> the master only applies to writes made after it changes\n\
         \x20 5 same as 4                -> confirms the lamp value is a boolean"
    );
    Ok(())
}

/// One signal being followed, and the last value seen for it.
struct Watch {
    label: String,
    address: u16,
    mask: u16,
    shift: u8,
    last: Option<u16>,
}

/// Parse `address:mask:shift` in hex. `None` for anything else, which is then
/// treated as a catalogue signal id.
fn parse_watch_triple(spec: &str) -> Option<(u16, u16, u8)> {
    let parts: Vec<&str> = spec.split(':').collect();
    if parts.len() != 3 {
        return None;
    }
    Some((
        u16::from_str_radix(parts[0].trim_start_matches("0x"), 16).ok()?,
        u16::from_str_radix(parts[1].trim_start_matches("0x"), 16).ok()?,
        parts[2].parse::<u8>().ok()?,
    ))
}

fn listen(
    seconds: u64,
    verbose: bool,
    watch: &[String],
    module: Option<&str>,
    catalogue_dir: &PathBuf,
) -> Result<()> {
    // Only load the catalogue when a watch is given by name.
    let needs_names = watch.iter().any(|w| parse_watch_triple(w).is_none());
    let cat = if needs_names {
        Some(Catalogue::load_dir(catalogue_dir).with_context(|| {
            format!(
                "loading {} (generated - build it with: python tools/build_catalogue.py)",
                catalogue_dir.display()
            )
        })?)
    } else {
        None
    };

    let mut watches: Vec<Watch> = Vec::new();
    let mut unresolved: Vec<String> = Vec::new();
    for spec in watch {
        match parse_watch_triple(spec) {
            Some((address, mask, shift)) => watches.push(Watch {
                label: spec.clone(),
                address,
                mask,
                shift,
                last: None,
            }),
            None => unresolved.push(spec.clone()),
        }
    }

    // Resolve names now if the module was given, otherwise once the stream says
    // what we are flying.
    let mut resolve_from = module.map(str::to_string);
    let mut resolved = false;

    let mut listener = Listener::bind(Ipv4Addr::UNSPECIFIED)
        .context("joining the DCS-BIOS multicast group on 239.255.50.10:5010")?;
    listener.set_read_timeout(Some(Duration::from_millis(500)))?;

    println!("Listening for {seconds}s on 239.255.50.10:5010. Start a mission in DCS.");
    if !unresolved.is_empty() && resolve_from.is_none() {
        println!(
            "Resolving {} watch name(s) once the aircraft is known.",
            unresolved.len()
        );
    }

    let mut state = BiosState::new();
    let mut writes: Vec<BiosWrite> = Vec::new();
    let (mut datagrams, mut total) = (0u64, 0u64);
    let started = Instant::now();
    let deadline = started + Duration::from_secs(seconds);

    // Without this, Ctrl-C kills the process outright: the summary never prints
    // and Windows reports 0xc000013a, which reads like a crash and is not one.
    let running = Arc::new(AtomicBool::new(true));
    {
        let flag = Arc::clone(&running);
        ctrlc::set_handler(move || flag.store(false, Ordering::SeqCst))
            .context("installing the Ctrl-C handler")?;
    }

    while Instant::now() < deadline && running.load(Ordering::SeqCst) {
        writes.clear();
        match listener.recv(&mut writes) {
            Ok(_) => datagrams += 1,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
            Err(e) => return Err(e).context("receiving from the export stream"),
        }
        for w in &writes {
            total += 1;
            state.apply(*w);
            if verbose {
                println!("  {:#06x} = {:#06x}", w.address, w.value);
            }
        }

        if !resolved && !unresolved.is_empty() {
            if resolve_from.is_none() {
                if let Some(raw) = state.string(0, 24) {
                    let name = raw.trim().to_string();
                    if !name.is_empty() {
                        println!("  aircraft {name}");
                        resolve_from = cat
                            .as_ref()
                            .and_then(|c| c.for_aircraft(&name))
                            .map(|m| m.module.clone());
                        if resolve_from.is_none() {
                            println!("  no catalogue entry for {name}; names cannot resolve");
                            resolved = true;
                        }
                    }
                }
            }
            if let (Some(key), Some(c)) = (resolve_from.as_deref(), cat.as_ref()) {
                if let Some(m) = c.module(key) {
                    for id in &unresolved {
                        match m.signal(id).and_then(|sig| sig.primary()) {
                            Some(o) => {
                                println!(
                                    "  watching {id} at {:#06x} & {:#06x} >> {}",
                                    o.address,
                                    o.mask.unwrap_or(u16::MAX),
                                    o.shift
                                );
                                watches.push(Watch {
                                    label: id.clone(),
                                    address: o.address,
                                    mask: o.mask.unwrap_or(u16::MAX),
                                    shift: o.shift,
                                    last: None,
                                });
                            }
                            None => println!("  {id} is not a signal in module {key}"),
                        }
                    }
                    resolved = true;
                }
            }
        }

        // Elapsed milliseconds, so a plateau reads as a gap in time rather than
        // having to be inferred from the shape of the numbers.
        let elapsed = started.elapsed().as_millis();
        for w in &mut watches {
            let now = state.value(w.address, w.mask, w.shift);
            if now != w.last {
                match now {
                    Some(v) => println!("{elapsed:>7} ms  {:<16} = {v}", w.label),
                    None => println!("{elapsed:>7} ms  {:<16} = (unset)", w.label),
                }
                w.last = now;
            }
        }
    }

    println!(
        "\n{datagrams} datagrams, {total} writes, {} distinct addresses.",
        state.len()
    );
    if state.is_empty() {
        println!(
            "Nothing received. Check DCS is running with a mission loaded, that DCS-BIOS is\n\
             installed in Saved Games/DCS/Scripts, and that no firewall rule is blocking\n\
             multicast on this interface."
        );
    }
    Ok(())
}

fn catalogue(dir: &PathBuf, aircraft: Option<&str>, find: Option<&str>) -> Result<()> {
    let catalogue = Catalogue::load_dir(dir)
        .with_context(|| format!("loading catalogue from {}", dir.display()))?;

    let Some(aircraft) = aircraft else {
        let mut modules: Vec<_> = catalogue.modules().collect();
        modules.sort_by_key(|m| m.module.clone());
        for m in &modules {
            println!(
                "{:22} {:5} signals  {}",
                m.module,
                m.signals.len(),
                m.aircraft.join(", ")
            );
        }
        println!("\n{} modules.", modules.len());
        return Ok(());
    };

    let module = catalogue
        .for_aircraft(aircraft)
        .with_context(|| format!("no catalogue entry for aircraft {aircraft:?}"))?;
    let needle = find.map(str::to_lowercase);

    // Lamps first: they are the likeliest intent, though every signal is bindable.
    let mut signals: Vec<_> = module
        .signals
        .iter()
        .filter(|s| match &needle {
            None => true,
            Some(n) => {
                s.id.to_lowercase().contains(n) || s.description.to_lowercase().contains(n)
            }
        })
        .collect();
    signals.sort_by_key(|s| (!s.is_lamp(), s.category.clone(), s.id.clone()));

    for s in signals.iter().take(60) {
        let Some(out) = s.primary() else { continue };
        let range = if out.discrete {
            out.values
                .iter()
                .map(|v| format!("{}={}", v.value, v.label))
                .collect::<Vec<_>>()
                .join(" ")
        } else {
            format!("0..{}", out.max_value.unwrap_or(0))
        };
        println!(
            "{:38} {:16} {:#06x}/{:#06x}>>{:<2} {}",
            s.id,
            s.control_type,
            out.address,
            out.mask.unwrap_or(0xffff),
            out.shift,
            range
        );
    }
    println!("\n{} matching signals in {}.", signals.len(), module.module);
    Ok(())
}


/// Names for the verbose log, so an action reads as `PTO2.HALF = 1` rather than
/// `part 0xbf05 index 16 = 1`.
///
/// Only the signals the active profile actually reads are followed. A cockpit
/// pushes thousands of writes a second and logging all of them would bury the
/// handful that drive a lamp.
#[derive(Default)]
struct Trace {
    lamps: HashMap<(String, u32, u8), String>,
    /// Address to the signals read from it: name, mask and shift, because
    /// several signals share one 16-bit word.
    sources: HashMap<u16, Vec<(String, u16, u8)>>,
    /// Last value logged per signal. DCS-BIOS repeats a word when anything in
    /// it moves, so without this a shared address logs its neighbours too.
    last: HashMap<(u16, u16, u8), u16>,
}

impl Trace {
    fn lamp_names(inventory: &DeviceInventory) -> HashMap<(String, u32, u8), String> {
        let mut out = HashMap::new();
        for spec in &inventory.devices {
            for (part, led) in spec.leds() {
                out.insert(
                    (spec.key.clone(), part.part_id, led.index),
                    format!("{}.{}", spec.key, led.name),
                );
            }
        }
        out
    }

    /// Index the signals the active profile reads. Called on every aircraft
    /// change, since a different profile reads different signals.
    fn follow(&mut self, profile: &Profile, cat: &Catalogue) {
        self.sources.clear();
        self.last.clear();
        let Some(module) = cat.module(&profile.module) else {
            return;
        };
        for b in &profile.bindings {
            for condition in &b.conditions {
                let Some(o) = module.signal(&condition.source).and_then(|s| s.primary()) else {
                    continue;
                };
                let entry = (
                    condition.source.clone(),
                    o.mask.unwrap_or(u16::MAX),
                    o.shift,
                );
                let slot = self.sources.entry(o.address).or_default();
                if !slot.contains(&entry) {
                    slot.push(entry);
                }
            }
        }
    }

    fn lamp(&self, id: &wctrl_engine::LedId) -> String {
        self.lamps
            .get(&(id.device.clone(), id.part_id, id.index))
            .cloned()
            .unwrap_or_else(|| format!("{} 0x{:04x}[{}]", id.device, id.part_id, id.index))
    }

    /// Log the bound signals that moved in this datagram.
    fn signals(&mut self, writes: &[BiosWrite], elapsed: u128) {
        for w in writes {
            let Some(signals) = self.sources.get(&w.address) else {
                continue;
            };
            for (name, mask, shift) in signals {
                let value = (w.value & mask) >> shift;
                if self.last.insert((w.address, *mask, *shift), value) == Some(value) {
                    continue;
                }
                println!("{elapsed:>8} ms  signal  {name:<28} = {value}");
            }
        }
    }
}

/// Run the converter.
///
/// The loop is deliberately boring: decode, feed the engine, write whatever it
/// asks for. Every policy decision - when to sweep, what to leave alone - lives
/// in `wctrl-engine`, where it is tested without DCS or hardware.
fn run(
    devices_path: &PathBuf,
    catalogue_dir: &PathBuf,
    profiles_dir: &PathBuf,
    dry_run: bool,
    verbose: bool,
    seconds: Option<u64>,
) -> Result<()> {
    let inventory = DeviceInventory::load(devices_path)
        .with_context(|| format!("loading {}", devices_path.display()))?;
    let cat = Catalogue::load_dir(catalogue_dir).with_context(|| {
        format!(
            "loading {} (generated - build it with: python tools/build_catalogue.py)",
            catalogue_dir.display()
        )
    })?;

    let mut profiles = Vec::new();
    let mut skipped = 0usize;
    if profiles_dir.is_dir() {
        let mut paths: Vec<PathBuf> = std::fs::read_dir(profiles_dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
            .collect();
        paths.sort();

        // One broken profile must not stop the others. A user with six aircraft
        // configured should lose the one they mistyped, not the whole session.
        for path in paths {
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            let p = match Profile::load(&path) {
                Ok(p) => p,
                Err(e) => {
                    println!("skipped  {name}: {e}");
                    skipped += 1;
                    continue;
                }
            };
            let Some(module) = cat.module(&p.module) else {
                println!(
                    "skipped  {name}: module {:?} has no catalogue entry. Either DCS-BIOS does not support it, or the catalogue needs rebuilding.",
                    p.module
                );
                skipped += 1;
                continue;
            };
            if let Err(e) = p.validate(module, &inventory) {
                println!("skipped  {name}: {e}");
                skipped += 1;
                continue;
            }

            let unset = p.bindings.iter().filter(|b| b.is_placeholder()).count();
            println!(
                "profile  {:<22} {:>2} set, {:>2} unset  for {}",
                p.name,
                p.bindings.len() - unset,
                unset,
                p.aircraft.join(", ")
            );
            profiles.push(p);
        }
    }
    if skipped > 0 {
        println!("{skipped} profile(s) skipped; the rest still run.");
    }
    if profiles.is_empty() {
        println!(
            "No usable profiles in {}. Every LED will be swept to zero on module load.",
            profiles_dir.display()
        );
    }

    // Only drive hardware that is actually plugged in. A shared profile may
    // name panels this user does not own, which is not an error.
    let api = hidapi::HidApi::new().context("opening HID API")?;
    let present: Vec<u16> = wctrl_hid::enumerate(&api)
        .iter()
        .map(|d| d.product_id)
        .collect();
    let mut connected = Vec::new();
    let mut handles: HashMap<String, Device> = HashMap::new();
    for spec in &inventory.devices {
        if !present.contains(&spec.usb_pid) {
            continue;
        }
        println!("device   {:<22} pid 0x{:04x}", spec.display_name, spec.usb_pid);
        connected.push(spec.key.clone());
        if !dry_run {
            let dev = Device::open(&api, spec.usb_pid)
                .with_context(|| format!("opening {}", spec.display_name))?;
            handles.insert(spec.key.clone(), dev);
        }
    }
    if connected.is_empty() {
        println!("No known devices connected. Nothing to drive.");
        return Ok(());
    }

    let mut trace = Trace {
        lamps: Trace::lamp_names(&inventory),
        ..Trace::default()
    };

    let mut engine = Engine::new(inventory, cat, profiles);
    engine.set_connected(connected);

    let mut listener = Listener::bind(Ipv4Addr::UNSPECIFIED)
        .context("joining the DCS-BIOS multicast group on 239.255.50.10:5010")?;
    listener.set_read_timeout(Some(Duration::from_millis(100)))?;

    // The panels latch. Ctrl-C must reach the clearing code rather than killing
    // the process, or the lamps stay lit until something else writes them.
    let running = Arc::new(AtomicBool::new(true));
    {
        let flag = Arc::clone(&running);
        ctrlc::set_handler(move || flag.store(false, Ordering::SeqCst))
            .context("installing the Ctrl-C handler")?;
    }

    println!(
        "Running{}{}. Ctrl-C to stop and clear the panels.",
        if dry_run { " (dry run - no HID writes)" } else { "" },
        if verbose { " (verbose)" } else { "" }
    );

    let started = Instant::now();
    let mut writes: Vec<BiosWrite> = Vec::new();
    let mut last_aircraft: Option<String> = None;

    while running.load(Ordering::SeqCst) {
        if let Some(limit) = seconds {
            if started.elapsed() >= Duration::from_secs(limit) {
                break;
            }
        }

        writes.clear();
        match listener.recv(&mut writes) {
            Ok(_) => {}
            Err(e)
                if e.kind() == std::io::ErrorKind::TimedOut
                    || e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e).context("reading the export stream"),
        }

        let now = Instant::now();
        let elapsed = started.elapsed().as_millis();
        let batch = if writes.is_empty() {
            engine.tick(now)
        } else {
            engine.ingest(&writes, now)
        };

        // After ingest, so a signal line and the lamp it moved read in the
        // order they happened.
        if verbose {
            trace.signals(&writes, elapsed);
        }

        let current = engine.aircraft().map(str::to_string);
        if current != last_aircraft {
            if let Some(name) = &current {
                match engine.active_profile() {
                    Some(p) => {
                        println!("aircraft {name}  ->  profile {}", p.name);
                        if verbose {
                            trace.follow(p, engine.catalogue());
                            println!(
                                "  following {} signal address(es) for this profile",
                                trace.sources.len()
                            );
                        }
                    }
                    None => {
                        println!("aircraft {name}  ->  no profile; panels will clear");
                        // Write a stub so the aircraft shows up in the editor
                        // with every lamp listed and nothing assigned. Takes
                        // effect next run; the panel stays cleared this time.
                        if let Err(e) = write_stub(profiles_dir, name, engine.catalogue(), engine.devices())
                        {
                            println!("  could not write a starter profile: {e:#}");
                        }
                    }
                }
            }
            last_aircraft = current;
        }

        apply(
            &batch,
            &handles,
            dry_run,
            verbose.then_some((&trace, elapsed)),
        )?;
    }

    println!();
    let batch = engine.shutdown();
    let cleared = batch.writes.len();
    apply(
        &batch,
        &handles,
        dry_run,
        verbose.then(|| (&trace, started.elapsed().as_millis())),
    )?;
    println!("Stopped. Cleared {cleared} LED(s).");
    Ok(())
}

fn apply(
    batch: &Batch,
    handles: &HashMap<String, Device>,
    dry_run: bool,
    trace: Option<(&Trace, u128)>,
) -> Result<()> {
    if batch.is_empty() {
        return Ok(());
    }
    for w in &batch.writes {
        match trace {
            Some((t, elapsed)) => println!(
                "{elapsed:>8} ms  {:<7} {:<28} = {}",
                match batch.cause {
                    Cause::ModuleLoad => "sweep",
                    Cause::SignalChange => "write",
                    Cause::Shutdown => "clear",
                },
                t.lamp(&w.id),
                w.value
            ),
            // Without --verbose a dry run still says what it would have sent,
            // which is the whole point of a dry run.
            None if dry_run => println!(
                "  {:?}  {} part 0x{:04x} index {:<2} = {}",
                batch.cause, w.id.device, w.id.part_id, w.id.index, w.value
            ),
            None => {}
        }
        if dry_run {
            continue;
        }
        if let Some(dev) = handles.get(&w.id.device) {
            dev.set_led(w.id.part_id, w.id.index, w.value)
                .with_context(|| format!("writing {} index {}", w.id.device, w.id.index))?;
        }
    }
    Ok(())
}

/// Write a starter profile for an aircraft nothing is configured for.
///
/// Never overwrites: if the file exists the user has already started on it, and
/// a profile they are part-way through editing is worth more than a fresh stub.
fn write_stub(
    profiles_dir: &PathBuf,
    aircraft: &str,
    cat: &Catalogue,
    devices: &DeviceInventory,
) -> Result<()> {
    let Some(module) = cat.for_aircraft(aircraft) else {
        // No DCS-BIOS support means there is nothing to bind to, so a stub would
        // be a file that can never work. Say so instead of writing one.
        println!("  {aircraft} has no DCS-BIOS catalogue entry, so nothing can be bound to it.");
        return Ok(());
    };

    let slug: String = aircraft
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    let path = profiles_dir.join(format!("{slug}.json"));
    if path.exists() {
        return Ok(());
    }

    std::fs::create_dir_all(profiles_dir)?;
    let profile = Profile::stub(aircraft, aircraft, &module.module, devices);
    let lamps = profile.bindings.len();
    profile.save(&path)?;
    println!("  wrote {} with {lamps} unassigned lamp(s)", path.display());
    Ok(())
}
