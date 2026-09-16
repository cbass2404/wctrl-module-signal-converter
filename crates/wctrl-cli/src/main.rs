//! Headless driver and diagnostics.
//!
//! Exists ahead of the UI so every layer can be exercised on real hardware and a
//! real DCS-BIOS stream before any of it is wrapped in Tauri.

use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use wctrl_bios::{BiosState, Listener, Write as BiosWrite};
use wctrl_config::Catalogue;
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
        /// Watch one signal by address/mask/shift, e.g. --watch 2af8:0001:0
        #[arg(long)]
        watch: Option<String>,
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
        } => listen(seconds, verbose, watch.as_deref())?,

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

fn listen(seconds: u64, verbose: bool, watch: Option<&str>) -> Result<()> {
    let watched = watch
        .map(|spec| {
            let parts: Vec<&str> = spec.split(':').collect();
            anyhow::ensure!(parts.len() == 3, "--watch wants address:mask:shift in hex");
            Ok::<_, anyhow::Error>((
                u16::from_str_radix(parts[0].trim_start_matches("0x"), 16)?,
                u16::from_str_radix(parts[1].trim_start_matches("0x"), 16)?,
                parts[2].parse::<u8>()?,
            ))
        })
        .transpose()?;

    let mut listener = Listener::bind(Ipv4Addr::UNSPECIFIED)
        .context("joining the DCS-BIOS multicast group on 239.255.50.10:5010")?;
    listener.set_read_timeout(Some(Duration::from_millis(500)))?;

    println!("Listening for {seconds}s on 239.255.50.10:5010. Start a mission in DCS.");
    let mut state = BiosState::new();
    let mut writes: Vec<BiosWrite> = Vec::new();
    let (mut datagrams, mut total) = (0u64, 0u64);
    let mut last_watched: Option<u16> = None;
    let deadline = Instant::now() + Duration::from_secs(seconds);

    while Instant::now() < deadline {
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
        if let Some((address, mask, shift)) = watched {
            let now = state.value(address, mask, shift);
            if now != last_watched {
                println!("  watched {address:#06x} & {mask:#06x} >> {shift} = {now:?}");
                last_watched = now;
            }
        }
    }

    println!("\n{datagrams} datagrams, {total} writes, {} distinct addresses.", state.len());
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

