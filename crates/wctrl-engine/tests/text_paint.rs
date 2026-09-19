//! A text grid, end to end: the A-10C default's CDU lines onto the MCDU.
//!
//! The MCDU draws characters from a font it holds, so what the engine hands
//! over is a screen of characters and colours plus the font they need. These
//! pin what that screen says, not how it reaches the panel.

use std::path::Path;
use std::time::{Duration, Instant};

use wctrl_bios::Write as BiosWrite;
use wctrl_config::{
    text_cells, Catalogue, Colour, DeviceInventory, DisplayCatalogue, Profile, Transport,
};
use wctrl_engine::{Batch, Engine, LcdWrite};

const MCDU: &str = "MCDU_Captain";

fn r(p: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(p)
}

/// Pack a field the way DCS-BIOS does, two bytes per word, bytes as sent.
fn bytes_at(address: u16, b: &[u8]) -> Vec<BiosWrite> {
    (0..b.len().div_ceil(2))
        .map(|i| BiosWrite {
            address: address + (i as u16) * 2,
            value: u16::from(b[i * 2]) | (u16::from(*b.get(i * 2 + 1).unwrap_or(&0)) << 8),
        })
        .collect()
}

fn profile() -> Profile {
    Profile::load(&r("data/defaults/a-10c.json")).expect("A-10C default")
}

fn engine(p: Profile) -> Engine {
    let devices = DeviceInventory::load(&r("data/devices.json")).expect("devices");
    let cat = Catalogue::load_dir(&r("data/catalogue")).expect("catalogue");
    let displays = DisplayCatalogue::load_dir(&r("data/displays")).expect("displays");
    let mut e = Engine::new(devices, cat, vec![p]).with_displays(displays);
    e.set_connected(vec![MCDU.into()]);
    e
}

fn line(e: &Engine, n: usize) -> u16 {
    e.catalogue()
        .module("A-10C")
        .and_then(|m| m.signal(&format!("CDU_LINE{n}")))
        .and_then(|s| s.primary())
        .expect("CDU line in the catalogue")
        .address
}

/// Load the A-10C with `lines` on the CDU, past the settle window.
fn fly(e: &mut Engine, aircraft: &str, lines: &[(usize, &[u8])]) -> Batch {
    let mut name = aircraft.as_bytes().to_vec();
    name.resize(24, 0);
    let mut writes = bytes_at(0, &name);
    for (n, text) in lines {
        let mut padded = text.to_vec();
        padded.resize(24, b' ');
        writes.extend(bytes_at(line(e, *n), &padded));
    }
    let t0 = Instant::now();
    let batch = e.ingest(&writes, t0);
    if batch.lcd.is_empty() {
        return e.tick(t0 + Duration::from_secs(5));
    }
    batch
}

fn screen(batch: &Batch) -> &LcdWrite {
    let texts: Vec<&LcdWrite> = batch
        .lcd
        .iter()
        .filter(|w| w.transport == Transport::Text)
        .collect();
    assert_eq!(texts.len(), 1, "one whole-screen write: {:?}", batch.lcd);
    texts[0]
}

fn row(w: &LcdWrite, n: usize) -> String {
    text_cells(&w.bytes)[(n - 1) * 24..n * 24]
        .iter()
        .map(|c| c.ch)
        .collect()
}

#[test]
fn the_cdu_lines_land_on_rows_5_to_14_with_their_symbols() {
    let mut e = engine(profile());
    // Line 1 as DCS-BIOS sends an arrow each way and a box: single bytes.
    let batch = fly(
        &mut e,
        "A-10C_2",
        &[(0, b"\xabWAYPT\xbb  \xa1"), (9, b"SCRATCH")],
    );
    let w = screen(&batch);
    assert_eq!(w.device, MCDU);
    assert_eq!(w.part_id, 0xbb32);
    assert_eq!(w.display, "MCDU");
    for n in 1..=4 {
        assert_eq!(row(w, n).trim(), "", "row {n} is left empty");
    }
    assert_eq!(
        row(w, 5),
        format!("{:<24}", "\u{2190}WAYPT\u{2192}  \u{2610}")
    );
    assert_eq!(row(w, 14).trim_end(), "SCRATCH");
}

#[test]
fn the_screen_asks_for_the_aircrafts_font_in_green() {
    let mut e = engine(profile());
    let batch = fly(&mut e, "A-10C", &[(2, b"STEERPOINT")]);
    let w = screen(&batch);
    assert_eq!(w.font.as_deref(), Some("../mcdu/a10c-font-21x31.json"));
    let lit = text_cells(&w.bytes)
        .into_iter()
        .find(|c| c.ch == 'S')
        .expect("text drawn");
    assert_eq!(lit.fg, Colour::Green.ordinal());
    assert_eq!(lit.bg, Colour::Black.ordinal());
    assert!(!lit.small);
}

#[test]
fn the_screen_backlight_comes_up_with_the_page() {
    let mut e = engine(profile());
    let batch = fly(&mut e, "A-10C_2", &[(0, b"TEST")]);
    let lit = batch
        .writes
        .iter()
        .find(|w| w.id.device == MCDU && w.id.index == 1)
        .expect("Screen_Backlight is driven with the screen");
    assert_eq!(lit.value, 255);
}

#[test]
fn nothing_is_resent_when_nothing_moved() {
    let mut e = engine(profile());
    fly(&mut e, "A-10C_2", &[(0, b"SAME")]);
    let again = e.ingest(
        &bytes_at(line(&e, 0), b"SAME                    "),
        Instant::now(),
    );
    assert!(again.lcd.is_empty(), "{:?}", again.lcd);
}

#[test]
fn mission_end_blanks_the_screen_without_a_font() {
    let mut e = engine(profile());
    fly(&mut e, "A-10C_2", &[(0, b"GONE SOON")]);
    let end = e.mission_ended();
    let w = screen(&end);
    assert!(text_cells(&w.bytes).iter().all(|c| c.ch == ' '), "blanked");
    assert_eq!(w.font, None, "a blank screen needs no glyphs");
}

#[test]
fn the_shipped_a10c_default_passes_its_checks() {
    let e = engine(profile());
    let devices = DeviceInventory::load(&r("data/devices.json")).unwrap();
    let displays = DisplayCatalogue::load_dir(&r("data/displays")).unwrap();
    let module = e.catalogue().module("A-10C").unwrap();
    let problems = profile().problems(module, &devices, &displays);
    assert!(problems.is_empty(), "{problems:?}");
}

#[test]
fn an_aircraft_without_a_cdu_font_cannot_put_fields_on_the_mcdu() {
    // Its font is the aircraft's, not a choice, so an aircraft with no CDU
    // font has nothing to draw with until field customisation arrives.
    let mut p = profile();
    p.aircraft.push("A-10A".into());
    let e = engine(p.clone());
    let devices = DeviceInventory::load(&r("data/devices.json")).unwrap();
    let displays = DisplayCatalogue::load_dir(&r("data/displays")).unwrap();
    let module = e.catalogue().module("A-10C").unwrap();
    let problems = p.problems(module, &devices, &displays);
    assert!(
        problems.iter().any(|e| e.to_string().contains("A-10A")),
        "{problems:?}"
    );
}

#[test]
fn a_colour_on_a_segment_display_is_refused() {
    let hornet = Profile::load(&r("data/defaults/fa-18.json")).unwrap();
    let mut p = hornet.clone();
    p.readouts[0].colour = Some(Colour::Amber);
    let e = engine(p.clone());
    let devices = DeviceInventory::load(&r("data/devices.json")).unwrap();
    let displays = DisplayCatalogue::load_dir(&r("data/displays")).unwrap();
    let module = e.catalogue().module(&p.module).unwrap();
    let problems = p.problems(module, &devices, &displays);
    assert!(
        problems
            .iter()
            .any(|e| e.to_string().contains("only a text grid")),
        "{problems:?}"
    );
}

/// A field whose colours arrive as a second line, and which seat it follows:
/// the CH-47F's CDU, one line of it, for each station.
fn chinook() -> Profile {
    let field = |seat: u32, pre: &str| {
        serde_json::json!({
            "device": MCDU, "display": "MCDU", "cells": "0-23",
            "source": format!("{pre}_CDU_LINE1"), "seat": seat, "colour": "white",
            "colours": {
                "source": format!("{pre}_CDU_LINE1_COLOR"),
                "codes": {"g": "green", "p": "magenta"}
            }
        })
    };
    serde_json::from_value(serde_json::json!({
        "name": "CH-47F", "aircraft": ["CH-47Fbl1"], "module": "CH-47F",
        "readouts": [field(0, "PLT"), field(1, "CPLT")]
    }))
    .expect("profile")
}

fn chinook_writes(e: &Engine, seat: u16) -> Vec<BiosWrite> {
    let module = e.catalogue().module("CH-47F").expect("CH-47F in the catalogue");
    let at = |id: &str| module.signal(id).and_then(|s| s.primary()).expect(id).clone();
    let mut name = b"CH-47Fbl1".to_vec();
    name.resize(24, 0);
    let mut w = bytes_at(0, &name);
    for (id, text) in [
        ("PLT_CDU_LINE1", b"PILOT" as &[u8]),
        ("PLT_CDU_LINE1_COLOR", b"gp  w"),
        ("CPLT_CDU_LINE1", b"COPILOT"),
        ("CPLT_CDU_LINE1_COLOR", b"ppppppp"),
    ] {
        let mut padded = text.to_vec();
        padded.resize(24, b' ');
        w.extend(bytes_at(at(id).address, &padded));
    }
    let s = at("SEAT_POSITION");
    let mask = s.mask.unwrap_or(u16::MAX);
    w.push(BiosWrite { address: s.address, value: (seat << s.shift) & mask });
    w
}

#[test]
fn each_character_takes_the_colour_its_twin_line_names() {
    let mut e = engine(chinook());
    let t0 = Instant::now();
    let _ = e.ingest(&chinook_writes(&e, 0), t0);
    let batch = e.tick(t0 + Duration::from_secs(5));
    let w = screen(&batch);
    assert_eq!(w.font.as_deref(), Some("../mcdu/ch47f-font-21x31.json"));
    assert_eq!(row(w, 1).trim_end(), "PILOT", "the pilot's CDU, from the pilot seat");
    let fg: Vec<u8> = text_cells(&w.bytes)[..5].iter().map(|c| c.fg).collect();
    assert_eq!(
        fg,
        [Colour::Green, Colour::Magenta, Colour::White, Colour::White, Colour::White]
            .map(|c| c.ordinal()),
        "g and p as coded; a space, and a letter with no code, keep the field's white"
    );
}

#[test]
fn the_screen_follows_the_seat() {
    let mut e = engine(chinook());
    let t0 = Instant::now();
    let _ = e.ingest(&chinook_writes(&e, 1), t0);
    let batch = e.tick(t0 + Duration::from_secs(5));
    let w = screen(&batch);
    assert_eq!(row(w, 1).trim_end(), "COPILOT");
    assert!(text_cells(&w.bytes)[..7]
        .iter()
        .all(|c| c.fg == Colour::Magenta.ordinal()));
}

#[test]
fn a_colour_line_is_checked_like_any_other_signal() {
    let mut p = chinook();
    let colours = p.readouts[0].colours.as_mut().unwrap();
    colours.source = "PLT_CDU_LINE1_COLOUR".into();
    colours.codes.insert("gr".into(), Colour::Green);
    let e = engine(p.clone());
    let devices = DeviceInventory::load(&r("data/devices.json")).unwrap();
    let displays = DisplayCatalogue::load_dir(&r("data/displays")).unwrap();
    let module = e.catalogue().module(&p.module).unwrap();
    let problems: Vec<String> = p
        .problems(module, &devices, &displays)
        .iter()
        .map(|e| e.to_string())
        .collect();
    // A colour line this DCS-BIOS lacks is flagged, and the field left blank,
    // rather than refusing the profile; a malformed code is still a problem.
    let flags = p.flags(module);
    assert!(flags.iter().any(|f| f.source == "PLT_CDU_LINE1_COLOUR"), "{flags:?}");
    assert!(problems.iter().any(|e| e.contains("\"gr\"")), "{problems:?}");
    assert_eq!(chinook().problems(module, &devices, &displays).len(), 0);
}

/// The F-14BU default with the Captain's screen bound to the CDNU's own knob.
fn tomcat_on_its_knob() -> Profile {
    let mut p = Profile::load(&r("data/defaults/f-14bu.json")).expect("F-14BU default");
    let b = p
        .bindings
        .iter_mut()
        .find(|b| b.device == MCDU && b.led == "Screen_Backlight")
        .expect("the screen's lamp is a row");
    *b = serde_json::from_value(serde_json::json!({
        "device": MCDU, "led": "Screen_Backlight",
        "conditions": [{"source": "RIO_CDNU_BRIGHTNESS", "on_when": {"scale": [0, 65535]}}]
    }))
    .expect("binding");
    p
}

/// Fly the F-14BU with the CDNU knob at `knob` and return what the screen's
/// lamp was last set to, if it was written.
fn screen_lamp_at(e: &mut Engine, knob: u16) -> Option<u8> {
    let module = e.catalogue().module("F-14").expect("F-14 in the catalogue");
    let s = module.signal("RIO_CDNU_BRIGHTNESS").and_then(|s| s.primary()).expect("knob").clone();
    let mut name = b"F-14BU".to_vec();
    name.resize(24, 0);
    let mut w = bytes_at(0, &name);
    let mask = s.mask.unwrap_or(u16::MAX);
    w.push(BiosWrite { address: s.address, value: (knob << s.shift) & mask });
    let t0 = Instant::now();
    let first = e.ingest(&w, t0);
    let batch = if first.writes.is_empty() { e.tick(t0 + Duration::from_secs(5)) } else { first };
    batch
        .writes
        .iter()
        .filter(|w| w.id.device == MCDU && w.id.index == 1)
        .map(|w| w.value)
        .last()
}

#[test]
fn a_bound_screen_follows_its_knob_while_it_has_a_page() {
    let mut e = engine(tomcat_on_its_knob());
    let half = screen_lamp_at(&mut e, 32768).expect("the screen's lamp is set with the page");
    assert!((126..=129).contains(&half), "half the knob is half the screen: {half}");

    let module = e.catalogue().module("F-14").unwrap();
    let s = module.signal("RIO_CDNU_BRIGHTNESS").and_then(|s| s.primary()).unwrap().clone();
    let turned = e.ingest(&[BiosWrite { address: s.address, value: u16::MAX }], Instant::now());
    let full = turned
        .writes
        .iter()
        .find(|w| w.id.device == MCDU && w.id.index == 1)
        .expect("turning the knob moves the screen");
    assert_eq!(full.value, 255);
}

#[test]
fn a_bound_screen_with_nothing_on_it_stays_black() {
    let mut p = tomcat_on_its_knob();
    p.readouts.clear();
    let mut e = engine(p);
    assert_eq!(screen_lamp_at(&mut e, u16::MAX).unwrap_or(0), 0, "no page, no light");
}

/// The F-14BU default's CDNU, from either seat: the module reports none.
#[test]
fn the_tomcat_cdnu_sits_on_the_bottom_eight_rows_one_column_in() {
    let p = Profile::load(&r("data/defaults/f-14bu.json")).expect("F-14BU default");
    let mut e = engine(p.clone());
    let module = e.catalogue().module("F-14").expect("F-14 in the catalogue");
    let devices = DeviceInventory::load(&r("data/devices.json")).unwrap();
    let displays = DisplayCatalogue::load_dir(&r("data/displays")).unwrap();
    let problems = p.problems(module, &devices, &displays);
    assert!(problems.is_empty(), "{problems:?}");

    let at = |n: usize| {
        module
            .signal(&format!("RIO_CDNU_LINE{n}"))
            .and_then(|s| s.primary())
            .expect("CDNU line in the catalogue")
            .address
    };
    let mut name = b"F-14BU".to_vec();
    name.resize(24, 0);
    let mut w = bytes_at(0, &name);
    // The module's stand-ins, as single bytes: the line-select markers, the
    // scroll arrows, the scratchpad's arrows and diamond, and its cursor.
    for (n, text) in [(1, b"\xabFlt Pln\xbb" as &[u8]), (8, b"{}\xae\xa9\x13_")] {
        let mut padded = text.to_vec();
        padded.resize(22, b' ');
        w.extend(bytes_at(at(n), &padded));
    }
    let t0 = Instant::now();
    let _ = e.ingest(&w, t0);
    let batch = e.tick(t0 + Duration::from_secs(5));
    let w = screen(&batch);
    assert_eq!(w.font.as_deref(), Some("../mcdu/f14bu-font-21x31.json"));
    for n in 1..=6 {
        assert_eq!(row(w, n).trim(), "", "row {n} is left empty");
    }
    assert_eq!(row(w, 7),format!(" {:<23}", "\u{2190}Flt Pln\u{2192}"), "lowercase kept");
    assert_eq!(
        row(w, 14),
        format!(" {:<23}", "\u{2191}\u{2193}\u{0394}\u{25c0}\u{25a1}\u{2b21}")
    );
}

/// The AH-64D default's KU scratchpad: the seated crew member's own, alone on
/// the bottom row.
fn apache_at(seat: u16) -> LcdWrite {
    let p = Profile::load(&r("data/defaults/ah-64d.json")).expect("AH-64D default");
    let mut e = engine(p.clone());
    let module = e.catalogue().module("AH-64D").expect("AH-64D in the catalogue");
    let devices = DeviceInventory::load(&r("data/devices.json")).unwrap();
    let displays = DisplayCatalogue::load_dir(&r("data/displays")).unwrap();
    let problems = p.problems(module, &devices, &displays);
    assert!(problems.is_empty(), "{problems:?}");

    let at = |id: &str| module.signal(id).and_then(|s| s.primary()).expect(id).clone();
    let mut name = b"AH-64D_BLK_II".to_vec();
    name.resize(24, 0);
    let mut w = bytes_at(0, &name);
    // The pilot's with the cursor, the CPG's with an arrow and a square.
    for (id, text) in [("PLT_KU_DISPLAY", b"PILOT~" as &[u8]), ("CPG_KU_DISPLAY", b"<CPG=>")] {
        let mut padded = text.to_vec();
        padded.resize(22, b' ');
        w.extend(bytes_at(at(id).address, &padded));
    }
    let s = at("SEAT_POSITION");
    let mask = s.mask.unwrap_or(u16::MAX);
    w.push(BiosWrite { address: s.address, value: (seat << s.shift) & mask });
    let t0 = Instant::now();
    let _ = e.ingest(&w, t0);
    let batch = e.tick(t0 + Duration::from_secs(5));
    screen(&batch).clone()
}

#[test]
fn the_apache_scratchpad_sits_on_the_bottom_row_for_the_seat() {
    let w = apache_at(0);
    assert_eq!(w.font.as_deref(), Some("../mcdu/ah64d-font-21x31.json"));
    for n in 1..=13 {
        assert_eq!(row(&w, n).trim(), "", "row {n} is left empty");
    }
    assert_eq!(row(&w, 14), format!(" {:<23}", "PILOT\u{2588}"));

    let w = apache_at(1);
    assert_eq!(row(&w, 14), format!(" {:<23}", "\u{25c0}CPG\u{25a0}\u{25b6}"));
}
