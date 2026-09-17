//! The glass, end to end: profile plus stream in, display writes out.

use std::path::Path;
use std::time::{Duration, Instant};

use wctrl_bios::Write as BiosWrite;
use wctrl_config::{Catalogue, DeviceInventory, DisplayCatalogue, Profile};
use wctrl_engine::Engine;

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn r(p: &str) -> std::path::PathBuf {
    root().join("../..").join(p)
}

/// Pack an ASCII field the way DCS-BIOS does, two bytes per word.
fn text_at(address: u16, s: &str) -> Vec<BiosWrite> {
    let b = s.as_bytes();
    (0..b.len().div_ceil(2))
        .map(|i| BiosWrite {
            address: address + (i as u16) * 2,
            value: u16::from(b[i * 2]) | (u16::from(*b.get(i * 2 + 1).unwrap_or(&0)) << 8),
        })
        .collect()
}

fn engine() -> Engine {
    let devices = DeviceInventory::load(&r("data/devices.json")).expect("devices");
    let cat = Catalogue::load_dir(&r("data/catalogue")).expect("catalogue");
    let displays = DisplayCatalogue::load_dir(&r("data/displays")).expect("displays");
    let profile = Profile::load(&r("data/defaults/fa-18c-hornet.json")).expect("hornet profile");
    let mut e = Engine::new(devices, cat, vec![profile]).with_displays(displays);
    e.set_connected(vec![
        "TAKEOFF_PLANEL_2".into(),
        "Orion_Throttle_Base_II".into(),
        "CarrierAce_UFC".into(),
    ]);
    e
}

#[test]
fn loading_the_hornet_paints_the_ufc() {
    let mut e = engine();
    let mut writes = text_at(0, "FA-18C_hornet\0\0\0\0\0\0\0\0\0\0\0");
    // The COMM page, as captured from a real mission.
    writes.extend(text_at(29746, "GRCV"));
    writes.extend(text_at(29750, "SQCH"));
    writes.extend(text_at(29766, " 265.000"));
    writes.extend(text_at(29732, " 3"));

    // The settle window is wall-clock, so time is passed in rather than read,
    // and the test can jump forward instead of sleeping through it.
    let t0 = Instant::now();
    let mut batch = e.ingest(&writes, t0);
    if batch.lcd.is_empty() {
        batch = e.tick(t0 + Duration::from_secs(5));
    }

    assert!(
        !batch.lcd.is_empty(),
        "loading an aircraft whose profile has readouts must paint the glass"
    );
    for w in &batch.lcd {
        assert_eq!(w.device, "CarrierAce_UFC");
        assert_eq!(w.part_id, 0xbed0);
        assert_eq!(w.bytes.len(), 4);
    }
    assert!(
        batch.lcd.iter().any(|w| w.bytes.iter().any(|b| *b != 0)),
        "some segments must actually be lit: {:?}",
        batch.lcd
    );
}

#[test]
fn the_painted_bytes_are_the_ones_the_hardware_was_sent() {
    // Ground truth, captured from SimAppPro driving a real UFC on the Hornet's
    // COMM page. If our pipeline agrees with these, it agrees with the device.
    const CAPTURED: &[(u8, [u8; 4])] = &[
        (0, [0x00, 0xd9, 0x02, 0x40]), // scratchpad " 3" "_"
        (1, [0xb6, 0xc7, 0xd3, 0xc6]), // "265."
        (2, [0xf5, 0xf5, 0xf5, 0x00]), // "000"
        (4, [0xd0, 0x91, 0x7c, 0x51]), // "GR" and the start of the next cell
        (5, [0xd5, 0x91, 0x50, 0x08]),
        (6, [0x02, 0x00, 0x00, 0x00]), // "V"
    ];

    let mut e = engine();
    let mut writes = text_at(0, "FA-18C_hornet\0\0\0\0\0\0\0\0\0\0\0");
    writes.extend(text_at(29774, " 3")); // scratchpad string 1
    writes.extend(text_at(29776, "--")); // string 2, aliased to "_"
    writes.extend(text_at(29766, " 265.000")); // 8 characters into 7 cells
    writes.extend(text_at(29746, "GRCV"));
    writes.extend(text_at(29750, "SQCH"));
    writes.extend(text_at(29754, "CPHR"));
    writes.extend(text_at(29758, "AM  "));
    writes.extend(text_at(29762, "MENU"));
    writes.extend(text_at(29736, " ")); // cueing 1 blank
    writes.extend(text_at(29738, ":")); // cueing 2 lit
    writes.extend(text_at(29740, " "));
    writes.extend(text_at(29742, ":")); // cueing 4 lit
    writes.extend(text_at(29744, " "));
    writes.extend(text_at(29732, " 3")); // comm 1
    writes.extend(text_at(29734, " 1")); // comm 2

    let t0 = Instant::now();
    let mut batch = e.ingest(&writes, t0);
    if batch.lcd.is_empty() {
        batch = e.tick(t0 + Duration::from_secs(5));
    }

    for (group, want) in CAPTURED {
        let got = batch
            .lcd
            .iter()
            .find(|w| w.group == *group)
            .unwrap_or_else(|| panic!("group {group} was never painted"));
        assert_eq!(
            got.bytes.as_slice(),
            want.as_slice(),
            "group {group}: painted {:02x?}, hardware was sent {:02x?}",
            got.bytes,
            want
        );
    }
}

#[test]
fn a_mission_ending_blanks_the_glass() {
    // The buffer latches with no host attached, verified on hardware. Leaving a
    // stale frequency lit after a mission is exactly the failure that makes
    // this necessary.
    let mut e = engine();
    let mut writes = text_at(0, "FA-18C_hornet\0\0\0\0\0\0\0\0\0\0\0");
    writes.extend(text_at(29746, "GRCV"));
    let t0 = Instant::now();
    let mut batch = e.ingest(&writes, t0);
    if batch.lcd.is_empty() {
        batch = e.tick(t0 + Duration::from_secs(5));
    }
    assert!(batch.lcd.iter().any(|w| w.bytes.iter().any(|b| *b != 0)));

    let ended = e.mission_ended();
    assert!(!ended.lcd.is_empty(), "the glass must be blanked");
    assert!(
        ended.lcd.iter().all(|w| w.bytes.iter().all(|b| *b == 0)),
        "every group written on mission end must be zeros"
    );
}
