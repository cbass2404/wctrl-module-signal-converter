//! Headless driver and diagnostics.
//!
//! Exists ahead of the UI so every layer can be exercised on real hardware and a
//! real DCS-BIOS stream before any of it is wrapped in Tauri.

use std::collections::HashMap;
use std::net::{Ipv4Addr, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use dsc_bios::{BiosState, Listener, Write as BiosWrite};
use dsc_config::catalogue_build::{self, Freshness};
use dsc_config::nightly_only::{Change, NightlyOnly};
use dsc_config::paths::Paths;
use dsc_config::{Flag, Place, Unsound};
use dsc_config::{file_stem, profile_name_for, Catalogue, DeviceInventory, DisplayCatalogue, Profile, Profiles, Readout, Transport};
use dsc_engine::{Batch, Cause, Engine, Watcher};
use wctrl_hid::Device;

#[derive(Parser)]
#[command(name = "dcs-signal", about = "DCS Signal Converter", version = dsc_config::version())]
struct Cli {
    #[command(subcommand)]
    command: Command,
    /// DCS-BIOS's `doc/json` folder, the catalogue's source. Only needed once
    /// for an install outside Saved Games: the catalogue remembers it.
    #[arg(long, global = true)]
    bios: Option<PathBuf>,
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
    /// Put a test page on an MCDU's screen.
    ///
    /// Declares the grid, uploads a font, and paints every colour, both font
    /// sizes and the corners of the 24x14 grid. The screen keeps the page after
    /// this exits, and a power cycle clears the font.
    McduTest {
        /// CAPTAIN 0xbb36, CO-PILOT 0xbb3e, OBSERVER 0xbb3a.
        #[arg(long, value_parser = parse_hex16, default_value = "0xbb36")]
        pid: u16,
        /// Where `mcdu.json` is, which says the grid and the font upload.
        #[arg(long)]
        displays: Option<PathBuf>,
        /// Which of the display's fonts to use, by aircraft.
        #[arg(long, default_value = "A-10C_2")]
        aircraft: String,
        /// Screen brightness, 0..=255. Screen_Backlight, index 1.
        #[arg(long, default_value_t = 255)]
        brightness: u8,
        /// Skip the font upload, to see what the grid does with no font.
        #[arg(long)]
        no_font: bool,
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
        #[arg(long)]
        catalogue: Option<PathBuf>,
    },
    /// Name a signal by moving it in the cockpit.
    ///
    /// Reads every signal the loaded module publishes and reports what moved,
    /// fewest movements first, so a switch thrown once comes out above the
    /// gauges that never stop. This is learn mode without the editor window,
    /// and it is the way to find a control whose identifier nobody knows.
    Learn {
        /// Length of each watch window. Every window ends with a table and
        /// starts a fresh sheet, so several controls can be found in one run.
        #[arg(long, default_value_t = 5)]
        seconds: u64,
        /// Rows per table. A busy cockpit moves more than anyone can read.
        #[arg(long, default_value_t = 12)]
        top: usize,
        /// Catalogue module. Detected from the stream when omitted.
        #[arg(long)]
        module: Option<String>,
        #[arg(long)]
        catalogue: Option<PathBuf>,
    },
    /// Run the converter: watch DCS-BIOS and drive the panels.
    Run {
        #[arg(long)]
        devices: Option<PathBuf>,
        #[arg(long)]
        catalogue: Option<PathBuf>,
        #[arg(long)]
        profiles: Option<PathBuf>,
        /// Shipped profiles, copied into --profiles at startup for any name
        /// that is not there yet. Never overwrites one the user already has.
        #[arg(long)]
        defaults: Option<PathBuf>,
        /// Segment display maps, for panels with glass. A missing directory is
        /// not an error: most panels have none.
        #[arg(long)]
        displays: Option<PathBuf>,
        /// Signals the shipped defaults need from the DCS-BIOS nightly, so a
        /// warning can say so. Missing is fine: the warning is plainer.
        #[arg(long)]
        nightly_only: Option<PathBuf>,
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
        /// Exit once the export stream has been quiet this long, having seen it
        /// at least once. Meant for the DCS hook: DCS-BIOS re-exports several
        /// times a second while a mission is loaded, so lasting silence means
        /// the mission ended or DCS is gone, and the panels should not be left
        /// lit either way.
        #[arg(long, value_name = "SECONDS")]
        exit_when_idle: Option<u64>,
    },
    /// Catalogue summary, or one module's signals.
    Catalogue {
        #[arg(long)]
        dir: Option<PathBuf>,
        /// Runtime aircraft name, e.g. F-4E-45MC
        #[arg(long)]
        aircraft: Option<String>,
        /// Case-insensitive filter on identifier or description.
        #[arg(long)]
        find: Option<String>,
        /// Build the catalogue again even though it matches the installed
        /// DCS-BIOS. It is rebuilt on its own whenever the version changes.
        #[arg(long)]
        rebuild: bool,
    },
    /// List the signals the shipped defaults read that the latest stable
    /// DCS-BIOS lacks or reports differently. A release step: see
    /// `tools/nightly_only.py`, which fetches the stable release and runs this.
    NightlyOnly {
        /// The stable release's `doc/json` folder.
        #[arg(long)]
        stable: PathBuf,
        #[arg(long)]
        defaults: Option<PathBuf>,
        /// The nightly catalogue, built from the DCS-BIOS installed here.
        #[arg(long)]
        catalogue: Option<PathBuf>,
        #[arg(long)]
        out: Option<PathBuf>,
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

fn mcdu_test(
    pid: u16,
    displays: &std::path::Path,
    aircraft: &str,
    brightness: u8,
    no_font: bool,
) -> Result<()> {
    use dsc_config::mcdu_font::{font_upload, McduFont, PacketMap, UploadStep};
    use wctrl_hid::GridCell;

    let catalogue = DisplayCatalogue::load_dir(displays)?;
    let display = catalogue.get("MCDU").context("no MCDU display in that directory")?;
    let grid = display.text.as_ref().context("the MCDU display has no text grid")?;
    let part = display.part_id;
    let origin = (grid.origin[0], grid.origin[1]);
    let (rows, columns) = (grid.rows as u16, grid.columns as u16);
    anyhow::ensure!((rows, columns) == (14, 24), "the test page is laid out for 24x14");

    let device = open(pid)?;
    device.declare_grid(part, origin, rows, columns)?;
    device.paint_grid(&[GridCell::BLANK; 24 * 14])?;
    println!("grid declared, screen blanked");

    if no_font {
        device.set_led(part, 1, brightness)?;
    } else {
        let file = grid
            .font_for(aircraft)
            .with_context(|| format!("no native font for {aircraft}"))?;
        let map = PacketMap::load(&grid.path(&grid.upload))?;
        let font = McduFont::load(&grid.path(file))?;
        let at = (grid.font_origin[0], grid.font_origin[1]);
        let mut reports = 0;
        for step in font_upload(&map, &font, part, at, brightness)? {
            match step {
                UploadStep::Report(r) => {
                    device.send_screen_reports(std::slice::from_ref(&r))?;
                    reports += 1;
                }
                UploadStep::SetLed { index, value } => device.set_led(part, index, value)?,
            }
        }
        println!("font {:?} uploaded in {reports} reports", font.name);
        // The upload declares a grid of its own; put ours back before painting.
        device.declare_grid(part, origin, rows, columns)?;
    }

    let mut cells = [GridCell::BLANK; 24 * 14];
    let mut put = |row: usize, col: usize, text: &str, fg: u8, small: bool| {
        for (i, ch) in text.chars().enumerate() {
            if col + i < 24 {
                cells[row * 24 + col + i] = GridCell { ch, fg, bg: 0, small };
            }
        }
    };
    put(0, 0, "A", 2, false);
    put(0, 7, "MCDU TEST", 2, false);
    put(0, 23, "B", 2, false);
    let colours = ["AMBER", "WHITE", "CYAN", "GREEN", "MAGENTA", "RED", "YELLOW", "BROWN", "GREY", "KHAKI"];
    for (n, name) in colours.iter().enumerate() {
        let row = 1 + n / 2;
        let col = (n % 2) * 12;
        put(row, col, &format!("{} {name}", n + 1), (n + 1) as u8, false);
    }
    put(7, 0, "LARGE ABCDEFGHIJKLMNOPQR", 4, false);
    put(8, 0, "small abcdefghijklmnopqr", 4, true);
    put(9, 0, "0123456789 ./-+:()*#%", 3, false);
    put(10, 0, "\u{2610}\u{2190}\u{2191}\u{2192}\u{2193}\u{0394}\u{2b21}\u{00b0}", 7, false);
    put(12, 0, "INVERSE", 0, false);
    put(13, 0, "C", 2, false);
    put(13, 6, "ROW 14 OF 14", 1, false);
    put(13, 23, "D", 2, false);
    for c in &mut cells[12 * 24..12 * 24 + 7] {
        c.bg = 4;
    }
    device.paint_grid(&cells)?;
    println!("test page painted: corners A B C D, ten colours, both font sizes");
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let bios = cli.bios.as_deref();
    // Anything not given on the command line comes from where the checkout
    // or the install keeps it.
    let paths = Paths::resolve();
    match cli.command {
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

        Command::McduTest {
            pid,
            displays,
            aircraft,
            brightness,
            no_font,
        } => {
            let displays = displays.unwrap_or(paths.displays);
            mcdu_test(pid, &displays, &aircraft, brightness, no_font)?
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
        } => {
            let catalogue = catalogue.unwrap_or(paths.catalogue);
            listen(seconds, verbose, &watch, module.as_deref(), &catalogue, bios)?
        }

        Command::Learn {
            seconds,
            top,
            module,
            catalogue,
        } => {
            let catalogue = catalogue.unwrap_or(paths.catalogue);
            learn(seconds, top, module.as_deref(), &catalogue, bios)?
        }

        Command::Run {
            devices,
            catalogue,
            profiles,
            defaults,
            displays,
            nightly_only,
            dry_run,
            verbose,
            seconds,
            exit_when_idle,
        } => run(
            &devices.unwrap_or(paths.devices),
            &catalogue.unwrap_or(paths.catalogue),
            &profiles.unwrap_or(paths.profiles.active),
            &defaults.unwrap_or(paths.profiles.defaults),
            &displays.unwrap_or(paths.displays),
            &nightly_only.unwrap_or(paths.nightly_only),
            bios,
            dry_run,
            verbose,
            seconds,
            exit_when_idle,
        )?,

        Command::Catalogue {
            dir,
            aircraft,
            find,
            rebuild,
        } => {
            let dir = dir.unwrap_or(paths.catalogue);
            catalogue(&dir, aircraft.as_deref(), find.as_deref(), bios, rebuild)?
        }

        Command::NightlyOnly {
            stable,
            defaults,
            catalogue,
            out,
        } => nightly_only(
            &stable,
            &defaults.unwrap_or(paths.profiles.defaults),
            &catalogue.unwrap_or(paths.catalogue),
            &out.unwrap_or(paths.nightly_only),
            bios,
        )?,
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

/// How a watched signal is read out of the stream, and what it last read.
///
/// A string is not a wide number. DCS-BIOS packs it two bytes to a word across
/// several addresses, so the whole run has to be reassembled before it can be
/// compared with what was seen last, and it is read with `text` rather than
/// `string` so the padding survives. On a display field the padding is the
/// layout: the Hornet UFC scratchpad is right aligned in its window.
enum Reading {
    Number { mask: u16, shift: u8, last: Option<u16> },
    Text { len: u16, last: Option<String> },
}

/// One signal being followed, and the last value seen for it.
struct Watch {
    label: String,
    address: u16,
    reading: Reading,
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
    bios: Option<&Path>,
) -> Result<()> {
    // Only load the catalogue when a watch is given by name.
    let needs_names = watch.iter().any(|w| parse_watch_triple(w).is_none());
    let cat = if needs_names {
        Some(load_catalogue(catalogue_dir, bios)?)
    } else {
        None
    };

    let mut watches: Vec<Watch> = Vec::new();
    let mut unresolved: Vec<String> = Vec::new();
    for spec in watch {
        match parse_watch_triple(spec) {
            // A raw triple is always a number: naming a string signal by
            // address would also have to say how long it is.
            Some((address, mask, shift)) => watches.push(Watch {
                label: spec.clone(),
                address,
                reading: Reading::Number { mask, shift, last: None },
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
                            Some(o) if o.r#type == "string" => {
                                // max_length is what DCS-BIOS sizes a string
                                // by; without it there is no way to know where
                                // the field ends, so say so rather than guess.
                                match o.max_length {
                                    Some(len) => {
                                        println!(
                                            "  watching {id} as {len} characters at {:#06x}",
                                            o.address
                                        );
                                        watches.push(Watch {
                                            label: id.clone(),
                                            address: o.address,
                                            reading: Reading::Text { len, last: None },
                                        });
                                    }
                                    None => println!(
                                        "  {id} is a string with no max_length; cannot read it"
                                    ),
                                }
                            }
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
                                    reading: Reading::Number {
                                        mask: o.mask.unwrap_or(u16::MAX),
                                        shift: o.shift,
                                        last: None,
                                    },
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
            let label = &w.label;
            match &mut w.reading {
                Reading::Number { mask, shift, last } => {
                    let now = state.value(w.address, *mask, *shift);
                    if now != *last {
                        match now {
                            Some(v) => println!("{elapsed:>7} ms  {label:<32} = {v}"),
                            None => println!("{elapsed:>7} ms  {label:<32} = (unset)"),
                        }
                        *last = now;
                    }
                }
                Reading::Text { len, last } => {
                    let now = state.text(w.address, *len);
                    if now != *last {
                        match &now {
                            // Quoted, because on a display field the spaces are
                            // the layout. Printing it bare would make a right
                            // aligned scratchpad indistinguishable from a left
                            // aligned one.
                            Some(s) => println!("{elapsed:>7} ms  {label:<32} = {s:?}"),
                            None => println!("{elapsed:>7} ms  {label:<32} = (unset)"),
                        }
                        *last = now;
                    }
                }
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

/// Learn mode on the command line.
///
/// Windows rather than one long capture. A single table over a whole flight
/// would list every signal in the module; a table per window is one answer per
/// thing the user did, and re-arming keeps the map so the second control is
/// found as fast as the first.
fn learn(
    seconds: u64,
    top: usize,
    module: Option<&str>,
    catalogue_dir: &PathBuf,
    bios: Option<&Path>,
) -> Result<()> {
    let cat = load_catalogue(catalogue_dir, bios)?;

    let mut listener = Listener::bind(Ipv4Addr::UNSPECIFIED)
        .context("joining the DCS-BIOS multicast group on 239.255.50.10:5010")?;
    listener.set_read_timeout(Some(Duration::from_millis(250)))?;

    let running = Arc::new(AtomicBool::new(true));
    {
        let flag = Arc::clone(&running);
        ctrlc::set_handler(move || flag.store(false, Ordering::SeqCst))
            .context("installing the Ctrl-C handler")?;
    }

    // The module has to be known before a watcher can exist, and until the
    // stream names the aircraft there is nothing to look one up by. Reading the
    // name costs about a second, which is the same wait the editor has.
    let mut watcher: Option<Watcher> = None;
    let mut names = BiosState::new();
    let mut writes: Vec<BiosWrite> = Vec::new();
    let mut announced = false;
    let mut window_ends = Instant::now() + Duration::from_secs(seconds);

    match module {
        Some(key) => println!("Learning signals of {key}. Ctrl-C to stop."),
        None => println!("Waiting for DCS to say what you are flying. Ctrl-C to stop."),
    }

    while running.load(Ordering::SeqCst) {
        writes.clear();
        match listener.recv(&mut writes) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
            Err(e) => return Err(e).context("receiving from the export stream"),
        }
        let now = Instant::now();

        let watching = match &mut watcher {
            Some(w) => w,
            None => {
                for write in &writes {
                    names.apply(*write);
                }
                let key = match module {
                    Some(key) => key.to_string(),
                    None => {
                        let Some(aircraft) = names.string(0, 24) else { continue };
                        let aircraft = aircraft.trim().to_string();
                        if aircraft.is_empty() {
                            continue;
                        }
                        match cat.for_aircraft(&aircraft) {
                            Some(m) => {
                                println!("  flying {aircraft}, which is module {}", m.module);
                                m.module.clone()
                            }
                            None => {
                                println!("  flying {aircraft}, which is not in the catalogue");
                                return Ok(());
                            }
                        }
                    }
                };
                let Some(m) = cat.module(&key) else {
                    anyhow::bail!("{key} is not in {}", catalogue_dir.display());
                };
                println!("  watching {} signals", m.signals.len());
                window_ends = now + Duration::from_secs(seconds);
                watcher.insert(Watcher::new(m, now))
            }
        };

        watching.ingest(&writes, now);
        if watching.ready() && !announced {
            println!("  ready. Flip something in the cockpit.");
            println!();
            announced = true;
        }
        if now < window_ends {
            continue;
        }
        window_ends = now + Duration::from_secs(seconds);
        if !watching.ready() {
            continue;
        }

        let changes = watching.changes();
        watching.rearm(now);
        if changes.is_empty() {
            continue;
        }

        println!("{:>5}  {:<34} {}", "moves", "signal", "value");
        for change in changes.iter().take(top) {
            // Quoted for a string and bare for a number, for the reason the
            // verbose log quotes them: on a display field the spaces are the
            // layout, and " 1" and "1 " are different readings.
            let show = |v: &str| {
                if change.text {
                    format!("{v:?}")
                } else {
                    v.to_string()
                }
            };
            let from = change.from.as_deref().map(show).unwrap_or_default();
            let description = cat
                .module(watching.module())
                .and_then(|m| m.signal(&change.id))
                .map(|s| s.description.as_str())
                .unwrap_or_default();
            println!(
                "{:>5}  {:<34} {} -> {}   {}",
                change.moves,
                change.id,
                from,
                show(&change.to),
                description
            );
        }
        if changes.len() > top {
            println!("  and {} more, which moved less recently", changes.len() - top);
        }
        println!();
    }
    Ok(())
}

/// What the DCS-BIOS running inside DCS says it is, against the catalogue.
enum Running {
    /// Its version string has not arrived yet.
    NotYet,
    Matches,
    /// It said something else. Also what a version read from the wrong
    /// address looks like, which is itself a sign the catalogue is wrong.
    Differs(String),
    /// The catalogue has nowhere to read a version from.
    Unreported,
}

fn running_version(engine: &Engine) -> Running {
    let Some((address, len)) = engine.catalogue().version_signal() else {
        return Running::Unreported;
    };
    let Some(running) = engine.state().string(address, len) else {
        return Running::NotYet;
    };
    let running = running.trim().to_string();
    if running.is_empty() {
        return Running::NotYet;
    }
    if engine.catalogue().bios_version() == Some(running.as_str()) {
        Running::Matches
    } else {
        Running::Differs(running)
    }
}

/// Write `data/nightly-only.json`: what the defaults read that stable lacks.
///
/// The stable catalogue is built into a temporary folder with the same builder
/// the apps use, so both sides are read the same way.
fn nightly_only(
    stable: &Path,
    defaults: &Path,
    catalogue_dir: &Path,
    out: &Path,
    bios: Option<&Path>,
) -> Result<()> {
    let nightly = load_catalogue(catalogue_dir, bios)?;
    if nightly.bios_version().is_some_and(|v| !v.contains("nightly")) {
        println!(
            "note: the installed DCS-BIOS is {}, not a nightly",
            nightly.bios_version().unwrap_or_default()
        );
    }
    let version = catalogue_build::installed_version(stable)
        .unwrap_or_else(|| catalogue_build::UNKNOWN_VERSION.to_string());
    let scratch = std::env::temp_dir().join(format!("dsc-stable-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    catalogue_build::build(stable, &scratch, &version)
        .with_context(|| format!("building a catalogue from {}", stable.display()))?;
    let stable_cat = Catalogue::load_dir(&scratch);
    let _ = std::fs::remove_dir_all(&scratch);
    let stable_cat = stable_cat.context("reading the stable catalogue back")?;

    let mut profiles = Vec::new();
    let mut paths: Vec<PathBuf> = std::fs::read_dir(defaults)
        .with_context(|| format!("reading {}", defaults.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect();
    paths.sort();
    for path in paths {
        profiles.push(Profile::load(&path).with_context(|| format!("reading {}", path.display()))?);
    }

    let (list, unknown) = NightlyOnly::compare(&profiles, &nightly, &stable_cat);
    for line in &unknown {
        println!("not in the nightly either, fix the default: {line}");
    }
    println!("stable {}, nightly {}", list.stable, list.nightly);
    for (module, signals) in &list.signals {
        for (id, change) in signals {
            let what = match change {
                Change::Missing => "not in stable".to_string(),
                Change::Range { stable, nightly } => format!(
                    "range {} in stable, {} in the nightly",
                    stable.map_or("none".into(), |v| v.to_string()),
                    nightly.map_or("none".into(), |v| v.to_string())
                ),
                Change::Kind { stable, nightly } => format!("{stable} in stable, {nightly} in the nightly"),
            };
            println!("  {module:<18} {id:<32} {what}");
        }
    }
    let count: usize = list.signals.values().map(|s| s.len()).sum();
    let text = serde_json::to_string_pretty(&list)?.replace('\n', "\r\n") + "\r\n";
    std::fs::write(out, text).with_context(|| format!("writing {}", out.display()))?;
    println!("{count} signal(s) written to {}", out.display());
    Ok(())
}

/// Bring the catalogue up to date with the installed DCS-BIOS, then load it.
///
/// Every command that reads the catalogue comes through here, so none of them
/// can read addresses from a DCS-BIOS release that is no longer installed.
/// Rebuilding is skipped when the versions already match, which is almost
/// always, so this costs one small file read.
fn load_catalogue(dir: &Path, bios: Option<&Path>) -> Result<Catalogue> {
    let bios_json = catalogue_build::locate_bios_json(dir, bios);
    let fresh = catalogue_build::ensure(&bios_json, dir)
        .with_context(|| format!("updating the catalogue in {}", dir.display()))?;
    match &fresh {
        Freshness::Current { .. } => {}
        Freshness::NoBios { have_catalogue: false, .. } => anyhow::bail!("{fresh}"),
        _ => println!("{fresh}"),
    }
    Catalogue::load_dir(dir).with_context(|| format!("loading the catalogue from {}", dir.display()))
}

fn catalogue(
    dir: &PathBuf,
    aircraft: Option<&str>,
    find: Option<&str>,
    bios: Option<&Path>,
    rebuild: bool,
) -> Result<()> {
    if rebuild {
        let bios_json = catalogue_build::locate_bios_json(dir, bios);
        let fresh = catalogue_build::rebuild(&bios_json, dir)
            .with_context(|| format!("rebuilding the catalogue in {}", dir.display()))?;
        println!("{fresh}");
    }
    let catalogue = load_catalogue(dir, bios)?;

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
    /// Address to the signals read from it, because several signals share one
    /// 16-bit word and a string spans several words.
    sources: HashMap<u16, Vec<Followed>>,
    /// Last value logged per signal, by name. DCS-BIOS repeats a word when
    /// anything in it moves, so without this a shared address logs its
    /// neighbours too, and a string logs once per word of it that arrives.
    last: HashMap<String, String>,
    /// Display fields that convert a position into a reading, by signal name.
    ///
    /// The raw number is what the stream carries and the converted one is what
    /// reaches the glass, and neither alone answers "is this right". A log
    /// showing 8738 cannot be checked against a cockpit gauge, and one showing
    /// only 100 cannot be checked against the stream.
    ///
    /// The readout itself is kept rather than its numbers copied out, so the
    /// line and the glass cannot drift apart: both go through
    /// `Readout::format_number`.
    converts: HashMap<String, (Readout, u16)>,
}

/// One signal the active profile reads, and how to read it.
///
/// A string is not a wide number. It is packed two bytes to a word across
/// several addresses, so it is registered under every address it occupies and
/// read back whole; read as a number it would print the two characters that
/// happen to share a word.
#[derive(PartialEq)]
enum Followed {
    Number { name: String, mask: u16, shift: u8 },
    Text { name: String, address: u16, len: u16 },
}

impl Followed {
    fn name(&self) -> &str {
        match self {
            Followed::Number { name, .. } | Followed::Text { name, .. } => name,
        }
    }
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
        self.converts.clear();
        let Some(module) = cat.module(&profile.module) else {
            return;
        };
        let sources = profile
            .bindings
            .iter()
            .flat_map(|b| b.conditions.iter().map(|c| c.source.as_str()))
            // A display field reads the stream exactly as a lamp condition
            // does. Leaving it out meant a paint line appeared with nothing
            // above it saying what had moved.
            .chain(profile.readouts.iter().map(|r| r.source.as_str()));

        for source in sources {
            let Some(o) = module.signal(source).and_then(|s| s.primary()) else {
                continue;
            };
            if o.r#type == "string" {
                // Without max_length there is no way to know where the field
                // ends. Skipping it costs a log line; guessing would print the
                // next field as part of this one.
                let Some(len) = o.max_length else { continue };
                // Registered under every word it occupies, so it is re-read
                // whichever part of it arrives.
                for word in 0..len.div_ceil(2) {
                    let entry = Followed::Text {
                        name: source.to_string(),
                        address: o.address,
                        len,
                    };
                    let slot = self.sources.entry(o.address + word * 2).or_default();
                    if !slot.contains(&entry) {
                        slot.push(entry);
                    }
                }
            } else {
                // A field reading this signal knows what the dial is marked
                // with. Held by name rather than folded into the entry, so a
                // signal that is both a lamp condition and a display field
                // still registers once and still logs once.
                if let Some(r) = profile
                    .readouts
                    .iter()
                    .find(|r| r.source == source && r.reads.is_some())
                {
                    let max = o
                        .max_value
                        .unwrap_or(u32::from(u16::MAX))
                        .min(u32::from(u16::MAX)) as u16;
                    self.converts
                        .insert(source.to_string(), (r.clone(), max));
                }
                let entry = Followed::Number {
                    name: source.to_string(),
                    mask: o.mask.unwrap_or(u16::MAX),
                    shift: o.shift,
                };
                let slot = self.sources.entry(o.address).or_default();
                if !slot.contains(&entry) {
                    slot.push(entry);
                }
            }
        }
    }

    fn lamp(&self, id: &dsc_engine::LedId) -> String {
        self.lamps
            .get(&(id.device.clone(), id.part_id, id.index))
            .cloned()
            .unwrap_or_else(|| format!("{} 0x{:04x}[{}]", id.device, id.part_id, id.index))
    }

    /// Log the bound signals that moved in this datagram.
    ///
    /// A string is read back out of `state` rather than off the datagram,
    /// because it is only whole once every word of it has been applied.
    fn signals(&mut self, writes: &[BiosWrite], state: &BiosState, elapsed: u128) {
        for w in writes {
            let Some(signals) = self.sources.get(&w.address) else {
                continue;
            };
            for followed in signals {
                let shown = match followed {
                    Followed::Number { name, mask, shift } => {
                        let raw = (w.value & mask).checked_shr(u32::from(*shift)).unwrap_or(0);
                        // Both numbers, because the raw one is checkable
                        // against the stream and the converted one against the
                        // gauge in the cockpit.
                        match self.converts.get(name) {
                            Some((r, max)) => format!("{raw} -> {}", r.format_number(raw, *max)),
                            None => raw.to_string(),
                        }
                    }
                    // Quoted, because on a display field the padding is the
                    // layout: a right aligned scratchpad would otherwise read
                    // the same as a left aligned one.
                    Followed::Text { address, len, .. } => match state.text(*address, *len) {
                        Some(text) => format!("{text:?}"),
                        None => continue,
                    },
                };
                let name = followed.name();
                if self.last.get(name) == Some(&shown) {
                    continue;
                }
                println!("{elapsed:>8} ms  signal  {name:<28} = {shown}");
                self.last.insert(name.to_string(), shown);
            }
        }
    }
}

/// Loopback address the daemon binds to prove it is the only one running.
///
/// Nothing is ever sent to it. It exists because binding is atomic and the
/// operating system releases it when the process dies, crash included, so a
/// second daemon can ask "is one already running" and get a truthful answer
/// with no stale state to clean up.
const INSTANCE_LOCK: &str = "127.0.0.1:16539";

/// Claim the right to drive the panels, or report who already has it.
///
/// Two daemons on one set of panels mostly looks fine, because both write the
/// same values from the same stream. It goes wrong at the end: one exits and
/// clears the lamps while the other is still lighting them.
///
/// This is reachable in ordinary use. If DCS crashes and is restarted inside
/// the idle window, the new DCS gets a fresh Lua state, so the hook's own
/// "already started" flag is gone and it launches a second daemon while the
/// first is still alive.
///
/// A lock file would survive a crash and then need its own liveness check,
/// which is the problem this is meant to solve rather than a solution to it.
fn take_instance_lock() -> std::io::Result<UdpSocket> {
    UdpSocket::bind(INSTANCE_LOCK)
}

/// How often to re-ask whether DCS is still there, once the stream has gone
/// quiet. Only reached while quiet, so it costs nothing during a flight.
const DCS_RECHECK: Duration = Duration::from_secs(5);

/// Whether DCS is running at all.
///
/// A quiet export stream is not the same as a dead DCS: sitting in the menu
/// between missions is silent too, and exiting there would mean the panels only
/// worked on the first mission of each session. This is the question that
/// actually decides whether to leave.
///
/// Asked through `tasklist` rather than a Windows API crate: it is reached only
/// while the stream is already quiet, so a process spawn every few seconds is
/// cheaper than a dependency. `CREATE_NO_WINDOW` keeps it from flashing a
/// console, which matters when the daemon was launched hidden by the DCS hook.
#[cfg(windows)]
fn dcs_is_running() -> bool {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    match std::process::Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq DCS.exe", "/NH"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    {
        Ok(out) => process_listed(&String::from_utf8_lossy(&out.stdout)),
        // If the question cannot be answered, assume DCS is alive. Staying up
        // wrongly costs an idle process; exiting wrongly leaves the user with
        // dark panels and no explanation.
        Err(_) => true,
    }
}

#[cfg(not(windows))]
fn dcs_is_running() -> bool {
    true
}

/// Did `tasklist` list a matching process?
///
/// Split out so it can be tested without closing DCS. A filter that matches
/// nothing does not produce empty output, it produces a sentence, and that
/// sentence must not be mistaken for a match.
fn process_listed(stdout: &str) -> bool {
    stdout.to_ascii_lowercase().contains("dcs.exe")
}

#[cfg(test)]
mod tests {
    use super::{process_listed, Followed, Trace};
    use dsc_bios::{BiosState, Write as BiosWrite};
    use dsc_config::{Catalogue, Module, Profile};

    /// A module with one lamp signal and one string signal, so a trace can be
    /// followed without the generated catalogue, which is machine-local.
    fn fixture() -> (Catalogue, Profile) {
        let module: Module = serde_json::from_str(
            r#"{
              "module": "FA-18C_hornet",
              "aircraft": ["FA-18C_hornet"],
              "signals": [
                {
                  "id": "MASTER_CAUTION_LT",
                  "control_type": "led",
                  "outputs": [{"address": 100, "mask": 4, "shift": 2, "max_value": 1, "max_length": null}]
                },
                {
                  "id": "UFC_SCRATCHPAD_NUMBER_DISPLAY",
                  "outputs": [{"address": 200, "mask": null, "max_value": null, "max_length": 8, "type": "string"}]
                }
              ]
            }"#,
        )
        .expect("the fixture module parses");

        let profile: Profile = serde_json::from_str(
            r#"{
              "name": "Hornet",
              "aircraft": ["FA-18C_hornet"],
              "module": "FA-18C_hornet",
              "bindings": [
                {
                  "device": "PTO2",
                  "led": "MASTER_CAUTION",
                  "conditions": [{"source": "MASTER_CAUTION_LT", "on_when": {"equals": 1}}]
                }
              ],
              "readouts": [
                {
                  "device": "CarrierAce_UFC",
                  "display": "UFC1",
                  "cells": "2-8",
                  "source": "UFC_SCRATCHPAD_NUMBER_DISPLAY",
                  "align": "right"
                }
              ]
            }"#,
        )
        .expect("the fixture profile parses");

        (Catalogue::from_modules(vec![module]), profile)
    }

    #[test]
    fn a_display_field_is_followed_the_same_as_a_lamp_condition() {
        let (cat, profile) = fixture();
        let mut trace = Trace::default();
        trace.follow(&profile, &cat);

        assert!(
            trace.sources.contains_key(&100),
            "the lamp condition is still followed"
        );
        // A readout was reported by --verbose as a paint line with nothing
        // above it saying what had moved, because only bindings were followed.
        assert!(
            trace.sources.contains_key(&200),
            "the display field is followed too"
        );
        // Eight characters is four words, and any of them arriving is a reason
        // to read the field again.
        for address in [200, 202, 204, 206] {
            assert!(
                trace.sources.contains_key(&address),
                "word at {address} is part of the field"
            );
        }
        assert!(!trace.sources.contains_key(&208), "and the field ends there");

        match &trace.sources[&200][0] {
            Followed::Text { name, len, .. } => {
                assert_eq!(name, "UFC_SCRATCHPAD_NUMBER_DISPLAY");
                assert_eq!(*len, 8);
            }
            Followed::Number { .. } => panic!("a string read as a number prints packed characters"),
        }
    }

    #[test]
    fn a_gauge_logs_the_position_and_what_it_converts_to() {
        let module: Module = serde_json::from_str(
            r#"{
              "module": "HIND",
              "aircraft": ["Mi-24P"],
              "signals": [
                {
                  "id": "PLT_RV5_ALT",
                  "control_type": "analog_gauge",
                  "outputs": [{"address": 300, "mask": 65535, "shift": 0, "max_value": 65535, "max_length": null}]
                }
              ]
            }"#,
        )
        .unwrap();
        let profile: Profile = serde_json::from_str(
            r#"{
              "name": "Hind", "aircraft": ["Mi-24P"], "module": "HIND",
              "readouts": [{
                "device": "CarrierAce_UFC", "display": "UFC1", "cells": "30-33",
                "source": "PLT_RV5_ALT", "reads": [0, 750]
              }]
            }"#,
        )
        .unwrap();

        let mut trace = Trace::default();
        trace.follow(&profile, &Catalogue::from_modules(vec![module]));

        // 8738 of 65535 on a face marked 0 to 750 is 100 metres. Neither number
        // alone is checkable: the position can be compared with the stream and
        // the reading with the gauge in the cockpit, and a fault shows up as
        // the two disagreeing.
        let mut state = BiosState::new();
        let writes = vec![BiosWrite { address: 300, value: 8738 }];
        for w in &writes {
            state.apply(*w);
        }
        trace.signals(&writes, &state, 0);
        assert_eq!(
            trace.last.get("PLT_RV5_ALT").map(String::as_str),
            Some("8738 -> 100")
        );
    }

    #[test]
    fn a_signal_nothing_converts_is_logged_as_it_arrives() {
        // Only a display field knows what a dial reads. A lamp condition does
        // not, so nothing is invented for it.
        let (cat, profile) = fixture();
        let mut trace = Trace::default();
        trace.follow(&profile, &cat);

        let mut state = BiosState::new();
        let writes = vec![BiosWrite { address: 100, value: 0b100 }];
        for w in &writes {
            state.apply(*w);
        }
        trace.signals(&writes, &state, 0);
        assert_eq!(trace.last.get("MASTER_CAUTION_LT").map(String::as_str), Some("1"));
    }

    #[test]
    fn a_string_is_logged_once_it_is_whole_and_not_once_per_word() {
        let (cat, profile) = fixture();
        let mut trace = Trace::default();
        trace.follow(&profile, &cat);

        // " 264.000" packed two characters to a word, low byte first.
        let mut state = BiosState::new();
        let writes: Vec<BiosWrite> = [(200u16, " 2"), (202, "64"), (204, ".0"), (206, "00")]
            .iter()
            .map(|(address, pair)| {
                let b = pair.as_bytes();
                BiosWrite {
                    address: *address,
                    value: u16::from(b[0]) | (u16::from(b[1]) << 8),
                }
            })
            .collect();
        for w in &writes {
            state.apply(*w);
        }

        // Nothing here asserts on stdout; what it proves is that the field is
        // read whole from state and deduplicated by name, so four words that
        // complete one value do not log four times.
        trace.signals(&writes, &state, 0);
        assert_eq!(
            trace.last.get("UFC_SCRATCHPAD_NUMBER_DISPLAY").map(String::as_str),
            Some("\" 264.000\""),
            "the whole field, with the padding that is its layout"
        );

        let before = trace.last.clone();
        trace.signals(&writes, &state, 1);
        assert_eq!(trace.last, before, "an unchanged field is not logged again");
    }

    #[test]
    fn tasklist_output_is_read_correctly() {
        // Both captured from a real tasklist on this machine.
        assert!(process_listed(
            "DCS.exe                      19820 Console                    1    404,240 K"
        ));
        assert!(!process_listed(
            "INFO: No tasks are running which match the specified criteria."
        ));
        // Nothing at all is not a match either, which is the case if the
        // command somehow produced no output.
        assert!(!process_listed(""));
    }

    #[test]
    fn a_profile_reading_a_missing_signal_loads_with_that_row_off() {
        // One renamed signal used to cost the whole profile. Now it costs the
        // row that reads it, and the rest of the profile runs.
        use super::load_profiles;
        use dsc_config::nightly_only::NightlyOnly;
        use dsc_config::{DeviceInventory, DisplayCatalogue};

        let (cat, _) = fixture();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let inventory = DeviceInventory::load(&root.join("data/devices.json")).unwrap();
        let displays = DisplayCatalogue::load_dir(&root.join("data/displays")).unwrap();
        let dir = std::env::temp_dir().join(format!("dsc-flagged-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("hornet.json"),
            r#"{"name": "Hornet", "aircraft": ["FA-18C_hornet"], "module": "FA-18C_hornet",
                "bindings": [
                  {"device": "TAKEOFF_PLANEL_2", "led": "Master_Caution", "off": 0,
                   "conditions": [{"source": "MASTER_CAUTION_LT", "on_when": {"equals": 1}}]},
                  {"device": "TAKEOFF_PLANEL_2", "led": "HOOK", "off": 0,
                   "conditions": [{"source": "RENAMED_LT", "on_when": {"equals": 1}}]}
                ]}"#,
        )
        .unwrap();

        let loaded = load_profiles(&dir, &cat, &inventory, &displays, &NightlyOnly::default());
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(loaded.skipped, 0, "{:?}", loaded.messages);
        assert_eq!(loaded.profiles.len(), 1);
        let p = &loaded.profiles[0];
        assert_eq!(p.bindings[0].conditions.len(), 1, "the good row runs");
        assert!(p.bindings[1].is_placeholder(), "the flagged row is off");
        let warning = loaded.messages.iter().find(|m| m.starts_with("warning")).expect("a warning");
        assert!(warning.contains("hornet.json"), "{warning}");
        assert!(
            loaded.messages.iter().any(|m| m.contains("RENAMED_LT") && m.contains("1 lamp(s) off")),
            "{:?}",
            loaded.messages
        );
    }
}

/// How often the profile directory is checked for edits.
const PROFILE_POLL: Duration = Duration::from_millis(500);

/// The result of reading a profile directory.
struct Loaded {
    profiles: Vec<Profile>,
    /// One line per profile accepted or skipped, for the caller to print. The
    /// loader is called again on every reload, and a reload should not repeat
    /// the whole startup listing unless something actually went wrong.
    messages: Vec<String>,
    skipped: usize,
}

/// Read every profile in a directory, keeping the ones that are usable.
///
/// One broken profile must not stop the others. A user with six aircraft
/// configured should lose the one they mistyped, not the whole session.
fn load_profiles(
    dir: &PathBuf,
    cat: &Catalogue,
    inventory: &DeviceInventory,
    displays: &DisplayCatalogue,
    nightly: &NightlyOnly,
) -> Loaded {
    let mut out = Loaded {
        profiles: Vec::new(),
        messages: Vec::new(),
        skipped: 0,
    };
    if !dir.is_dir() {
        return out;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect();
    paths.sort();

    for path in paths {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let p = match Profile::load(&path) {
            Ok(p) => p,
            Err(e) => {
                out.messages.push(format!("skipped  {name}: {e}"));
                out.skipped += 1;
                continue;
            }
        };
        let Some(module) = cat.module(&p.module) else {
            out.messages.push(format!(
                "skipped  {name}: module {:?} has no catalogue entry. Either DCS-BIOS does not support it, or the catalogue needs rebuilding.",
                p.module
            ));
            out.skipped += 1;
            continue;
        };
        if let Err(e) = p.validate(module, inventory, displays) {
            out.messages.push(format!("skipped  {name}: {e}"));
            out.skipped += 1;
            continue;
        }
        // What this DCS-BIOS cannot back is turned off rather than costing the
        // whole profile, and the copy that runs is the one without it.
        let flags = p.flags(module);
        if !flags.is_empty() {
            out.messages.extend(flag_lines(&name, &p.module, &flags, cat.bios_version(), nightly));
        }
        let p = p.runnable(module);

        for note in p.inert() {
            out.messages.push(format!("note     {name}: {note}"));
        }
        for caution in p.cautions(inventory) {
            out.messages.push(format!("caution  {name}: {caution}"));
        }
        let unset = p.bindings.iter().filter(|b| b.is_placeholder()).count();
        out.messages.push(format!(
            "profile  {:<22} {:>2} set, {:>2} unset  for {}",
            p.name,
            p.bindings.len() - unset,
            unset,
            p.aircraft.join(", ")
        ));
        out.profiles.push(p);
    }
    if out.skipped > 0 {
        out.messages
            .push(format!("{} profile(s) skipped; the rest still run.", out.skipped));
    }
    out
}

/// One warning for a profile with flagged conditions, grouped by reason.
///
/// Says what is off and why, and, where the shipped list knows, that the
/// DCS-BIOS nightly has it, so the user can choose between updating and doing
/// without. Grouped because one missing signal is often read in many places:
/// the F-14's CDNU lines appear on all three MCDU names, and 24 lines saying
/// the same thing bury the one fact in them.
fn flag_lines(
    name: &str,
    module: &str,
    flags: &[Flag],
    version: Option<&str>,
    nightly: &NightlyOnly,
) -> Vec<String> {
    let mut out = vec![format!(
        "warning  {name}: DCS-BIOS {} lacks what some rows read, so those rows are off. Everything else runs.",
        version.unwrap_or("here")
    )];
    // Reason, then the sources with that reason and what turning them off cost.
    let mut groups: Vec<(String, Vec<&str>, [usize; 3], Vec<&str>)> = Vec::new();
    for f in flags {
        let reason = match (&f.why, nightly.get(module, &f.source)) {
            (Unsound::Missing, Some(_)) => {
                format!("need the DCS-BIOS nightly; stable {} does not have them", nightly.stable)
            }
            (Unsound::Missing, None) => "are not in this DCS-BIOS".to_string(),
            (Unsound::AboveRange { value, max }, Some(Change::Range { nightly: Some(n), .. })) => format!(
                "tested against {value}, above the highest here, {max}; the DCS-BIOS nightly goes to {n}"
            ),
            (Unsound::AboveRange { value, max }, _) => {
                format!("tested against {value}, above the highest, {max}")
            }
        };
        let i = match groups.iter().position(|g| g.0 == reason) {
            Some(i) => i,
            None => {
                groups.push((reason, Vec::new(), [0; 3], Vec::new()));
                groups.len() - 1
            }
        };
        let group = &mut groups[i];
        if !group.1.contains(&f.source.as_str()) {
            group.1.push(&f.source);
        }
        group.2[match f.place {
            Place::Condition { .. } => 0,
            Place::Branch { .. } => 1,
            Place::Field { .. } => 2,
        }] += 1;
        if !group.3.contains(&f.device.as_str()) {
            group.3.push(&f.device);
        }
    }
    for (reason, sources, counts, devices) in groups {
        let cost: Vec<String> = [(counts[0], "lamp(s) off"), (counts[1], "alternative(s) dropped"), (counts[2], "field(s) blank")]
            .iter()
            .filter(|(n, _)| *n > 0)
            .map(|(n, what)| format!("{n} {what}"))
            .collect();
        out.push(format!(
            "           {}: {reason}. {} on {}",
            sources.join(", "),
            cost.join(", "),
            devices.join(", ")
        ));
    }
    out
}

/// A cheap summary of a profile directory: name, size and modification time.
///
/// Polled rather than watched with a filesystem notification API, because the
/// daemon already wakes every 100 ms for the socket and a directory of a dozen
/// small files costs nothing to stat. It also avoids a dependency whose
/// behaviour differs per platform, for a feature that is pure convenience.
fn profiles_fingerprint(dir: &PathBuf) -> Vec<(String, u64, u64)> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let stamp = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        out.push((
            path.file_name().unwrap_or_default().to_string_lossy().into_owned(),
            meta.len(),
            stamp,
        ));
    }
    out.sort();
    out
}

/// Run the converter.
///
/// The loop is deliberately boring: decode, feed the engine, write whatever it
/// asks for. Every policy decision - when to sweep, what to leave alone - lives
/// in `dsc-engine`, where it is tested without DCS or hardware.
fn run(
    devices_path: &PathBuf,
    catalogue_dir: &PathBuf,
    profiles_dir: &PathBuf,
    defaults_dir: &PathBuf,
    displays_dir: &PathBuf,
    nightly_path: &Path,
    bios: Option<&Path>,
    dry_run: bool,
    verbose: bool,
    seconds: Option<u64>,
    exit_when_idle: Option<u64>,
) -> Result<()> {
    println!("DCS Signal Converter {}", dsc_config::version());
    let inventory = DeviceInventory::load(devices_path)
        .with_context(|| format!("loading {}", devices_path.display()))?;
    let cat = load_catalogue(catalogue_dir, bios)?;
    let bios_json = catalogue_build::locate_bios_json(catalogue_dir, bios);
    let displays = DisplayCatalogue::load_dir(displays_dir)
        .with_context(|| format!("loading {}", displays_dir.display()))?;
    let nightly = NightlyOnly::load(nightly_path).unwrap_or_else(|e| {
        println!("could not read {}: {e}", nightly_path.display());
        NightlyOnly::default()
    });

    // Shipped profiles are copied in rather than read from a second folder, so
    // there is only ever one place profiles live and one place the user edits.
    // Seeding adds and never replaces, so this is safe on every start.
    let seeded = Profiles::new(defaults_dir, profiles_dir)
        .seed()
        .with_context(|| {
            format!(
                "seeding {} from {}",
                profiles_dir.display(),
                defaults_dir.display()
            )
        })?;
    if !seeded.is_empty() {
        println!(
            "seeded   {} profile(s) from {}",
            seeded.len(),
            defaults_dir.display()
        );
    }

    // Seeding only helps a profile the user does not have. This is the other
    // half: a profile written before we supported a panel has no rows for it,
    // and their file is the one that runs, so without this the lamps on that
    // panel could never be configured at all.
    for note in Profiles::new(defaults_dir, profiles_dir)
        .merge_new(&inventory)
        .with_context(|| format!("updating profiles in {}", profiles_dir.display()))?
    {
        println!("updated  {note}");
    }

    let loaded = load_profiles(profiles_dir, &cat, &inventory, &displays, &nightly);
    for line in &loaded.messages {
        println!("{line}");
    }
    if loaded.profiles.is_empty() {
        println!(
            "No usable profiles in {}. Every LED will be swept to zero on module load.",
            profiles_dir.display()
        );
    }
    let profiles = loaded.profiles;

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
        let glass: Vec<&str> = spec.displays().map(|(_, k)| k).collect();
        println!(
            "device   {:<22} pid 0x{:04x}{}",
            spec.display_name,
            spec.usb_pid,
            if glass.is_empty() {
                String::new()
            } else {
                // Say whether the map for that glass was actually found. A
                // display declared with no map is silent otherwise: the panel
                // simply never lights and nothing explains it.
                format!(
                    "  display {}",
                    glass
                        .iter()
                        .map(|k| if displays.get(k).is_some() {
                            (*k).to_string()
                        } else {
                            format!("{k} (NO MAP FOUND)")
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        );
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

    let mut panels = Panels { handles, displays: displays.clone(), fonts: HashMap::new() };
    let mut engine = Engine::new(inventory, cat, profiles).with_displays(displays.clone());
    engine.set_connected(connected);

    // Held for the lifetime of the run. A dry run writes nothing, so it is
    // allowed alongside a real daemon: the lock exists to stop two processes
    // driving one panel, not to stop two processes existing.
    let _lock = if dry_run {
        None
    } else {
        match take_instance_lock() {
            Ok(socket) => Some(socket),
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                println!(
                    "Another DCS Signal Converter daemon is already running and driving the panels.\n\
                     Leaving it alone. Stop it first if you meant to replace it."
                );
                return Ok(());
            }
            Err(e) => return Err(e).context("claiming the single-instance lock"),
        }
    };

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

    // Each mission, the DCS-BIOS that DCS actually loaded is asked which
    // release it is. `rebuilt` allows one catalogue rebuild per mission, so a
    // mismatch that a rebuild cannot fix ends the run instead of looping.
    let mut version_checked = false;
    let mut rebuilt = false;

    // Silence only counts once the stream has been heard at least once, so
    // starting before DCS does not exit immediately. `None` means nothing has
    // arrived yet, which is a wait rather than a death.
    let idle_limit = exit_when_idle.map(Duration::from_secs);
    let mut last_traffic: Option<Instant> = None;
    let mut next_dcs_check = Instant::now();
    let mut cleared_for_idle = false;

    // Profile hot reload. The directory is checked on a timer, and a change is
    // acted on only once it has stopped changing, so a profile caught halfway
    // through being written is not read.
    let mut fingerprint = profiles_fingerprint(profiles_dir);
    let mut settling: Option<Vec<(String, u64, u64)>> = None;
    let mut next_check = Instant::now() + PROFILE_POLL;

    while running.load(Ordering::SeqCst) {
        if let Some(limit) = seconds {
            if started.elapsed() >= Duration::from_secs(limit) {
                break;
            }
        }

        writes.clear();
        match listener.recv(&mut writes) {
            Ok(_) => {
                if !writes.is_empty() {
                    last_traffic = Some(Instant::now());
                    cleared_for_idle = false;
                }
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::TimedOut
                    || e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e).context("reading the export stream"),
        }

        let now = Instant::now();
        let elapsed = started.elapsed().as_millis();

        // A quiet stream means the cockpit is gone, which is worth clearing the
        // panels for either way: they latch, so the last frame of the last
        // mission would otherwise stay lit. Whether to *exit* is a separate
        // question, and only a dead DCS answers it yes. Sitting in the menu
        // between missions is silent too, and exiting there would leave the
        // panels working only on the first mission of each session.
        if let (Some(limit), Some(seen)) = (idle_limit, last_traffic) {
            if now.saturating_duration_since(seen) >= limit && now >= next_dcs_check {
                next_dcs_check = now + DCS_RECHECK;

                if !cleared_for_idle {
                    cleared_for_idle = true;
                    let batch = engine.mission_ended();
                    apply(&batch, &mut panels, dry_run, verbose.then_some((&trace, elapsed)))?;
                    last_aircraft = None;
                    println!("stream quiet for {}s. Panels cleared.", limit.as_secs());
                }

                if !dcs_is_running() {
                    println!("DCS is no longer running. Exiting.");
                    break;
                }
            }
        }

        if now >= next_check {
            next_check = now + PROFILE_POLL;
            let current = profiles_fingerprint(profiles_dir);
            if current != fingerprint {
                // Seen changed once; act on it when it looks the same twice
                // running. An editor saving a file and a hand edit both settle
                // within one poll, and a half-written file does not.
                if settling.as_ref() == Some(&current) {
                    fingerprint = current;
                    settling = None;

                    let reloaded =
                        load_profiles(profiles_dir, engine.catalogue(), engine.devices(), &displays, &nightly);
                    println!(
                        "reloaded {} profile(s) from {}",
                        reloaded.profiles.len(),
                        profiles_dir.display()
                    );
                    for line in &reloaded.messages {
                        if line.starts_with("skipped") || reloaded.skipped > 0 {
                            println!("{line}");
                        }
                    }
                    let batch = engine.set_profiles(reloaded.profiles);
                    apply(&batch, &mut panels, dry_run, verbose.then_some((&trace, elapsed)))?;
                    if let Some(name) = engine.aircraft() {
                        if let Some(p) = engine.active_profile() {
                            let p = p.clone();
                            trace.follow(&p, engine.catalogue());
                            println!("aircraft {name}  ->  profile {}", p.name);
                        }
                    }
                } else {
                    settling = Some(current);
                }
            } else {
                settling = None;
            }
        }

        let batch = if writes.is_empty() {
            engine.tick(now)
        } else {
            engine.ingest(&writes, now)
        };

        // After ingest, so a signal line and the lamp it moved read in the
        // order they happened.
        if verbose {
            trace.signals(&writes, engine.state(), elapsed);
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
            version_checked = false;
            rebuilt = false;
        }

        apply(
            &batch,
            &mut panels,
            dry_run,
            verbose.then_some((&trace, elapsed)),
        )?;

        if !version_checked && engine.aircraft().is_some() {
            match running_version(&engine) {
                Running::NotYet => {}
                Running::Matches => version_checked = true,
                Running::Unreported => {
                    version_checked = true;
                    println!("DCS-BIOS does not report its version, so it cannot be checked against the catalogue.");
                }
                Running::Differs(running) => {
                    let built = engine.catalogue().bios_version().unwrap_or("unknown").to_string();
                    let installed = catalogue_build::installed_version(&bios_json);
                    // The catalogue is behind the installed DCS-BIOS: it was
                    // updated between missions while this daemon ran. What DCS
                    // loaded this mission is what is installed, so rebuilding
                    // fixes it. The version is read again afterwards, through
                    // the new catalogue's address.
                    if !rebuilt && installed.as_deref() != Some(built.as_str()) {
                        rebuilt = true;
                        let cat = load_catalogue(catalogue_dir, bios)?;
                        let reloaded = load_profiles(profiles_dir, &cat, engine.devices(), &displays, &nightly);
                        for line in &reloaded.messages {
                            println!("{line}");
                        }
                        let batch = engine.set_catalogue(cat, reloaded.profiles);
                        apply(&batch, &mut panels, dry_run, verbose.then_some((&trace, elapsed)))?;
                        if let Some(p) = engine.active_profile() {
                            let p = p.clone();
                            trace.follow(&p, engine.catalogue());
                        }
                    } else {
                        // DCS runs a release that is not installed, which is an
                        // update made while the mission was loaded, or one the
                        // catalogue cannot even read the version of. No
                        // catalogue matches the code DCS is running.
                        println!(
                            "DCS is running DCS-BIOS {running:?}, but the catalogue is built from {built} and {} is installed. Signals would be read from the wrong addresses, so DCS Signal Converter is stopping. The next mission loads the installed DCS-BIOS and starts it again with a catalogue that matches it.",
                            installed.as_deref().unwrap_or("no version")
                        );
                        break;
                    }
                }
            }
        }
    }

    println!();
    let batch = engine.shutdown();
    let cleared = batch.writes.len();
    apply(
        &batch,
        &mut panels,
        dry_run,
        verbose.then(|| (&trace, started.elapsed().as_millis())),
    )?;
    println!("Stopped. Cleared {cleared} LED(s).");
    Ok(())
}

/// The open panels, and what each text grid has been sent this run.
struct Panels {
    handles: HashMap<String, Device>,
    displays: DisplayCatalogue,
    /// Per device, the font its text grid holds: absent until the grid has
    /// been declared this run, `None` once declared with no font sent. The
    /// panel keeps no font across a power cycle and we cannot ask it which it
    /// has, so a run starts by assuming nothing.
    fonts: HashMap<String, Option<String>>,
}

/// Get a text grid ready for `w`: declared, and holding the font it needs.
///
/// The font upload is the panel's whole glyph set, about 600 reports, so it
/// goes out only when the font changes, not per paint. It resets the grid to
/// the size SimAppPro uses, so the grid is declared again after it, exactly as
/// WwDevicesDotnet does.
fn prepare_text_grid(dev: &Device, w: &dsc_engine::LcdWrite, panels: &mut Panels) -> Result<()> {
    use dsc_config::mcdu_font::{font_upload, McduFont, PacketMap, UploadStep};
    let grid = panels
        .displays
        .get(&w.display)
        .and_then(|d| d.text.as_ref())
        .with_context(|| format!("display {} has no text grid", w.display))?;
    let origin = (grid.origin[0], grid.origin[1]);
    let (rows, columns) = (grid.rows as u16, grid.columns as u16);
    let have = panels.fonts.get(&w.device);
    let upload = match (&w.font, have) {
        (Some(want), Some(Some(held))) if want == held => None,
        (Some(want), _) => Some(want.clone()),
        (None, Some(_)) => return Ok(()),
        (None, None) => None,
    };
    dev.declare_grid(w.part_id, origin, rows, columns)?;
    if let Some(file) = &upload {
        dev.paint_grid(&vec![wctrl_hid::GridCell::BLANK; grid.rows * grid.columns])?;
        let map = PacketMap::load(&grid.path(&grid.upload))?;
        let font = McduFont::load(&grid.path(file))?;
        let at = (grid.font_origin[0], grid.font_origin[1]);
        for step in font_upload(&map, &font, w.part_id, at, 255)? {
            match step {
                UploadStep::Report(r) => dev.send_screen_reports(std::slice::from_ref(&r))?,
                UploadStep::SetLed { index, value } => dev.set_led(w.part_id, index, value)?,
            }
        }
        dev.declare_grid(w.part_id, origin, rows, columns)?;
    }
    panels.fonts.insert(w.device.clone(), upload.or_else(|| have.cloned().flatten()));
    Ok(())
}

/// A text grid's buffer as the lines it shows, for the trace.
fn text_rows(w: &dsc_engine::LcdWrite, displays: &DisplayCatalogue) -> Vec<String> {
    let columns = displays
        .get(&w.display)
        .and_then(|d| d.text.as_ref())
        .map_or(24, |t| t.columns);
    dsc_config::text_cells(&w.bytes)
        .chunks(columns)
        .map(|row| row.iter().map(|c| c.ch).collect())
        .collect()
}

fn apply(
    batch: &Batch,
    panels: &mut Panels,
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
                    Cause::ProfileReload => "reload",
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
        if let Some(dev) = panels.handles.get(&w.id.device) {
            dev.set_led(w.id.part_id, w.id.index, w.value)
                .with_context(|| format!("writing {} index {}", w.id.device, w.id.index))?;
        }
    }

    // Displays. A write is a piece of a device-side bitmap, segments or pixels,
    // so it is not readable as text here; the engine has already decided what
    // the glass should say and this only carries it.
    for w in &batch.lcd {
        // A text grid is readable, so it is shown as its lines, blank ones
        // left out. Everything else is a bitmap and prints as bytes.
        let hex = || {
            if w.transport == Transport::Text {
                return text_rows(w, &panels.displays)
                    .iter()
                    .enumerate()
                    .filter(|(_, row)| !row.trim().is_empty())
                    .map(|(n, row)| format!("\n      row {:>2} |{row}|", n + 1))
                    .collect::<String>();
            }
            w.bytes
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(" ")
        };
        match trace {
            Some((_, elapsed)) => println!(
                "{elapsed:>8} ms  {:<7} {:<28} = {}",
                match batch.cause {
                    Cause::Shutdown => "blank",
                    _ => "paint",
                },
                format!("{} group {}", w.device, w.group),
                hex()
            ),
            None if dry_run => println!(
                "  {:?}  {} display group {:<2} = {}",
                batch.cause,
                w.device,
                w.group,
                hex()
            ),
            None => {}
        }
        if dry_run {
            continue;
        }
        let Some(dev) = panels.handles.get(&w.device) else { continue };
        match w.transport {
            Transport::Segment => dev
                .set_lcd(w.part_id, w.group, &w.bytes)
                .with_context(|| format!("writing {} display group {}", w.device, w.group))?,
            Transport::Pixel => dev
                .write_pixels(w.part_id, w.offset, &w.bytes)
                .with_context(|| format!("writing {} screen from row {}", w.device, w.group))?,
            Transport::Text => {
                // Taken out and put back so the handle and the font record
                // can both be used; nothing else touches `handles` meanwhile.
                let dev = panels.handles.remove(&w.device).expect("present above");
                let result = prepare_text_grid(&dev, w, panels).and_then(|()| {
                    let cells: Vec<wctrl_hid::GridCell> = dsc_config::text_cells(&w.bytes)
                        .into_iter()
                        .map(|c| wctrl_hid::GridCell { ch: c.ch, fg: c.fg, bg: c.bg, small: c.small })
                        .collect();
                    dev.paint_grid(&cells)?;
                    Ok(())
                });
                panels.handles.insert(w.device.clone(), dev);
                result.with_context(|| format!("writing {} text screen", w.device))?;
                // Screens sent back to back can garble; WwDevicesDotnet
                // pauses this long after each one for the same reason.
                std::thread::sleep(Duration::from_millis(40));
            }
        }
    }

    // A pixel screen shows nothing written until it is committed, so each one
    // written to in this batch is committed once, after all its writes.
    if !dry_run {
        let mut committed: Vec<(&str, u32)> = Vec::new();
        for w in batch.lcd.iter().filter(|w| w.transport == Transport::Pixel) {
            if committed.contains(&(w.device.as_str(), w.part_id)) {
                continue;
            }
            committed.push((w.device.as_str(), w.part_id));
            if let Some(dev) = panels.handles.get(&w.device) {
                dev.commit_pixels(w.part_id)
                    .with_context(|| format!("committing {} screen", w.device))?;
            }
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

    // Named for a person, so NONE becomes "No aircraft"; the aircraft it claims
    // stays what DCS-BIOS reports, since that is what gets matched.
    let name = profile_name_for(aircraft);
    let stem = file_stem(name);
    if stem.is_empty() {
        println!("  {aircraft:?} gives no usable file name, so no starter profile was written.");
        return Ok(());
    }
    let path = profiles_dir.join(format!("{stem}.json"));
    if path.exists() {
        return Ok(());
    }

    std::fs::create_dir_all(profiles_dir)?;
    let profile = Profile::stub(name, aircraft, &module.module, devices);
    let lamps = profile.bindings.len();
    profile.save(&path)?;
    println!("  wrote {} with {lamps} unassigned lamp(s)", path.display());
    Ok(())
}
