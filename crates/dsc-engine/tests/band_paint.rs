//! Alias bands, per-band colour and `abs`, end to end onto a text grid.
//!
//! The F-16's trim indicators are the case these exist for: DCS-BIOS reports a
//! needle at 0 to 65535 and the cockpit is marked in units nose up and units
//! nose down, so what belongs on the glass is a magnitude and a direction, not
//! a negative number.
//!
//! Driven from `fixtures/f-16-trim-bands.json` rather than the shipped F-16
//! default. A shipped profile is a living document, and its author retuning a
//! band should not fail a test about the engine.

use std::path::Path;
use std::time::{Duration, Instant};

use dsc_bios::Write as BiosWrite;
use dsc_config::{
    text_cells, Catalogue, Colour, DeviceInventory, DisplayCatalogue, Profile, TextCell, Transport,
};
use dsc_engine::{Batch, Engine, LcdWrite};

const MCDU: &str = "MCDU_Captain";
const MODULE: &str = "F-16C_50";

/// What the fixture's pitch and yaw faces are marked with, and the roll face.
const PITCH: [f64; 2] = [-1.5, 1.5];
const ROLL: [f64; 2] = [-3.0, 3.0];

/// The profile as `with_pages` leaves it: every field on the MCDU marked as
/// its start page's. A text grid takes its fields only from a page, and
/// these fixtures hold the fields a page would put there.
fn resolved(p: &Profile) -> Profile {
    let mut p = p.clone();
    for r in &mut p.readouts {
        if r.display == "MCDU" {
            r.page = Some("fixture".into());
        }
    }
    p
}

fn r(p: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(p)
}

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

fn profile() -> Profile {
    Profile::load(&fixture("f-16-trim-bands.json")).expect("the trim band fixture")
}

fn engine(p: Profile) -> Engine {
    let devices = DeviceInventory::load(&r("data/devices.json")).expect("devices");
    let cat = Catalogue::load_dir(&r("data/catalogue")).expect("catalogue");
    let displays = DisplayCatalogue::load_dir(&r("data/displays")).expect("displays");
    let mut e = Engine::new(devices, cat, vec![p]).with_displays(displays);
    e.set_connected(vec![MCDU.into()]);
    e
}

/// The raw count DCS-BIOS would send for a needle sitting at `want` on a face
/// marked `reads`, which is the conversion the engine does, read backwards.
fn raw(reads: [f64; 2], want: f64) -> u16 {
    let [low, high] = reads;
    ((want - low) / (high - low) * 65535.0).round() as u16
}

fn address(e: &Engine, signal: &str) -> u16 {
    e.catalogue()
        .module(MODULE)
        .and_then(|m| m.signal(signal))
        .and_then(|s| s.primary())
        .unwrap_or_else(|| panic!("{signal} in the catalogue"))
        .address
}

/// Load the F-16 with each needle where it is asked for, past the settle
/// window.
fn fly(e: &mut Engine, needles: &[(&str, u16)]) -> Batch {
    let mut name = MODULE.as_bytes().to_vec();
    name.resize(24, 0);
    let mut writes: Vec<BiosWrite> = (0..12)
        .map(|i| BiosWrite {
            address: i * 2,
            value: u16::from(name[i as usize * 2]) | (u16::from(name[i as usize * 2 + 1]) << 8),
        })
        .collect();
    for (signal, value) in needles {
        writes.push(BiosWrite {
            address: address(e, signal),
            value: *value,
        });
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

fn cells(w: &LcdWrite, n: usize) -> Vec<TextCell> {
    text_cells(&w.bytes)[(n - 1) * 24..n * 24].to_vec()
}

fn row(w: &LcdWrite, n: usize) -> String {
    cells(w, n).iter().map(|c| c.ch).collect()
}

/// Both needles hard over, one way and then the other.
fn trim(e: &mut Engine, pitch: f64, roll: f64, yaw: f64) -> Batch {
    fly(
        e,
        &[
            ("PITCHTRIMIND", raw(PITCH, pitch)),
            ("ROLLTRIMIND", raw(ROLL, roll)),
            ("YAW_TRIM", raw(PITCH, yaw)),
        ],
    )
}

#[test]
fn the_band_fixture_is_valid() {
    let p = profile();
    let e = engine(p.clone());
    let devices = DeviceInventory::load(&r("data/devices.json")).unwrap();
    let displays = DisplayCatalogue::load_dir(&r("data/displays")).unwrap();
    let module = e.catalogue().module(&p.module).expect("the module");
    let refusals: Vec<String> = resolved(&p)
        .problems(module, &devices, &displays, &dsc_config::PageLibrary::default())
        .iter()
        .map(|e| e.to_string())
        .collect();
    assert!(refusals.is_empty(), "{refusals:?}");
}

#[test]
fn a_band_draws_its_word_and_abs_drops_the_sign() {
    let mut e = engine(profile());

    // Nose up, right wing down, yaw hard right. The magnitude is drawn without
    // a sign and the direction is drawn as the word its band asks for.
    let batch = trim(&mut e, 1.5, 3.0, 1.5);
    let w = screen(&batch);
    assert_eq!(row(w, 2), " 1.5NU      RWD      1.5");

    // Hard the other way. The pitch magnitude is the same characters, which is
    // the whole point of `abs`: the sign is not information the word beside it
    // does not already carry. The yaw piece has no `abs` and no band down
    // here, so it draws a signed number, which is the contrast.
    let batch = trim(&mut e, -1.5, -3.0, -0.8);
    let w = screen(&batch);
    assert_eq!(row(w, 2), " 1.5ND      LWD     -0.8");
}

#[test]
fn a_band_naming_one_reading_can_draw_nothing() {
    // Dead centre is its own band, drawn blank, so the row says nothing rather
    // than saying zero three times.
    let mut e = engine(profile());
    let batch = trim(&mut e, 0.0, 0.0, 0.0);
    let w = screen(&batch);
    let drawn = row(w, 2);
    assert_eq!(&drawn[4..7], "   ", "the pitch direction is blank: {drawn:?}");
    assert_eq!(&drawn[12..16], "    ", "and so is the roll: {drawn:?}");
    // The magnitude beside it is still a number, and still unsigned.
    assert_eq!(&drawn[0..4], " 0.0", "{drawn:?}");
}

#[test]
fn a_reading_no_band_claims_draws_as_the_number() {
    // The yaw piece bands only its three lowest readings and dead centre.
    // Everything else falls through to the number, which is what lets a face
    // be part named and part read.
    let mut e = engine(profile());
    let batch = trim(&mut e, 0.0, 0.0, -1.4);
    let w = screen(&batch);
    assert_eq!(&row(w, 2)[20..24], "  L3", "a list key claims it");

    let batch = trim(&mut e, 0.0, 0.0, 0.8);
    let w = screen(&batch);
    assert_eq!(&row(w, 2)[20..24], " 0.8", "nothing claims it, so it reads");
}

#[test]
fn a_bands_colour_is_its_own_and_beats_the_pieces() {
    let mut e = engine(profile());
    let batch = trim(&mut e, 1.5, 3.0, 1.5);
    let w = screen(&batch);
    let up = cells(w, 2);

    // The magnitude takes the piece's colour, because nothing nearer has an
    // opinion about it.
    assert_eq!(up[1].fg, Colour::Amber.ordinal(), "the magnitude is amber");
    // The nose-up band asked for green, and a band is nearer the reading than
    // the piece it sits in.
    assert_eq!(up[4].fg, Colour::Green.ordinal(), "NU is green");
    assert_eq!(up[5].fg, Colour::Green.ordinal());

    // Nose down has no colour of its own, so it falls back to the piece, which
    // has none either, and lands on the profile's default.
    let batch = trim(&mut e, -1.5, -3.0, -1.5);
    let w = screen(&batch);
    let down = cells(w, 2);
    assert_ne!(down[4].fg, Colour::Green.ordinal(), "ND is not the NU colour");
}
