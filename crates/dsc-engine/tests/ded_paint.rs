//! The ICP's DED, end to end: the shipped F-16 profile plus a DCS-BIOS stream
//! in, pixel writes out.

use std::path::Path;
use std::time::{Duration, Instant};

use dsc_bios::Write as BiosWrite;
use dsc_config::{Catalogue, DeviceInventory, DisplayCatalogue, Profile, Screen, Transport};
use dsc_engine::{Batch, Engine, LcdWrite};

const ICP: u32 = 0xbf06;
const SCREEN_BYTES: usize = 1600;

fn r(p: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(p)
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

fn profile() -> Profile {
    Profile::load(&r("data/defaults/f-16.json")).expect("F-16 profile")
}

fn engine() -> (Engine, DisplayCatalogue) {
    let devices = DeviceInventory::load(&r("data/devices.json")).expect("devices");
    let cat = Catalogue::load_dir(&r("data/catalogue")).expect("catalogue");
    let displays = DisplayCatalogue::load_dir(&r("data/displays")).expect("displays");
    let mut e = Engine::new(devices, cat, vec![profile()]).with_displays(displays.clone());
    e.set_connected(vec!["ViperAce_ICP".into()]);
    (e, displays)
}

/// DCS-BIOS writes for one DED line and its format.
fn line(cat: &Catalogue, n: usize, text: &str, format: &str) -> Vec<BiosWrite> {
    let module = cat.module("F-16C_50").expect("F-16 module");
    let at = |id: &str| module.signal(id).and_then(|s| s.primary()).expect(id).address;
    let mut out = text_at(at(&format!("DED_L{n}")), text);
    out.extend(text_at(at(&format!("DED_L{n}_FORMAT")), format));
    out
}

const BLANK: &str = "                        ";

/// The UHF page: a highlighted field, on line 3.
fn uhf_page(cat: &Catalogue) -> Vec<BiosWrite> {
    let mut w = text_at(0, "F-16C_50\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0");
    w.extend(line(cat, 1, "     UHF     BOTH       ", BLANK));
    w.extend(line(cat, 2, "  305.00                ", BLANK));
    w.extend(line(cat, 3, "             *305.00*   ", "             i      i   "));
    w.extend(line(cat, 4, "  PRE   1 a      TOD    ", BLANK));
    w.extend(line(cat, 5, "     305.00       NB    ", BLANK));
    w
}

fn load(e: &mut Engine, writes: &[BiosWrite]) -> Batch {
    let t0 = Instant::now();
    let batch = e.ingest(writes, t0);
    if !batch.is_empty() {
        return batch;
    }
    e.tick(t0 + Duration::from_secs(5))
}

/// Apply writes to a framebuffer the way the device does.
fn replay(fb: &mut [u8], writes: &[LcdWrite]) {
    for w in writes {
        assert_eq!(w.transport, Transport::Pixel);
        assert_eq!((w.device.as_str(), w.part_id), ("ViperAce_ICP", ICP));
        assert_eq!(w.offset, usize::from(w.group) * 25, "a run starts on a row");
        fb[w.offset..w.offset + w.bytes.len()].copy_from_slice(&w.bytes);
    }
}

/// What our map says the page should look like, drawn directly.
fn expected(displays: &DisplayCatalogue, lines: [(&str, &str); 5]) -> Vec<u8> {
    let ded = displays.get("DED").unwrap();
    let mut screen = Screen::new(ded);
    for (n, (text, format)) in lines.iter().enumerate() {
        for (col, (c, f)) in text.chars().zip(format.chars()).enumerate() {
            screen
                .draw_styled(ded, n * 24 + col, &c.to_string(), f == 'i')
                .unwrap();
        }
    }
    screen.bytes().to_vec()
}

#[test]
fn the_shipped_profile_is_valid() {
    let (e, displays) = engine();
    let devices = DeviceInventory::load(&r("data/devices.json")).unwrap();
    let module = e.catalogue().module("F-16C_50").expect("F-16 module");
    let problems = profile().problems(module, &devices, &displays);
    assert!(problems.is_empty(), "{problems:?}");
}

#[test]
fn loading_the_f16_paints_the_whole_ded_in_one_write() {
    let (mut e, displays) = engine();
    let page = uhf_page(e.catalogue());
    let batch = load(&mut e, &page);

    // A screen we have not driven is painted whole, because whatever
    // SimAppPro left on it latched. Every row is consecutive, so it is one run.
    assert_eq!(batch.lcd.len(), 1, "{:?}", batch.lcd.iter().map(|w| w.group).collect::<Vec<_>>());
    assert_eq!(batch.lcd[0].bytes.len(), SCREEN_BYTES);

    let mut fb = vec![0u8; SCREEN_BYTES];
    replay(&mut fb, &batch.lcd);
    let want = expected(&displays, [
        ("     UHF     BOTH       ", BLANK),
        ("  305.00                ", BLANK),
        ("             *305.00*   ", "             i      i   "),
        ("  PRE   1 a      TOD    ", BLANK),
        ("     305.00       NB    ", BLANK),
    ]);
    assert_eq!(fb, want);

    // The two stars are inverse boxes: row 1 of line 3 is solid over them.
    let row = (2 * 13 + 1) * 25;
    assert_eq!((fb[row + 13], fb[row + 20]), (0xff, 0xff));
    assert_eq!(fb[row + 14], 0, "only the marked cells are inverse");

    // And the DED is lit: its backlight goes to full with the first paint.
    assert_eq!(backlight(&batch), Some(255));
}

/// What a batch wrote to the DED backlight, if anything.
fn backlight(batch: &Batch) -> Option<u8> {
    batch
        .writes
        .iter()
        .find(|w| w.id.part_id == ICP && w.id.index == 1)
        .map(|w| w.value)
}

#[test]
fn the_ded_backlight_is_a_lamp_a_profile_can_bind() {
    let devices = DeviceInventory::load(&r("data/devices.json")).unwrap();
    let icp = devices.device("ViperAce_ICP").unwrap();
    assert!(icp.led("Screen_Backlight").is_some(), "offered to profiles");
    assert!(icp.leds().any(|(_, l)| l.name == "Screen_Backlight"), "and to the editor");
    assert_eq!(icp.display_lamps().map(|(_, l)| l.index).collect::<Vec<_>>(), [1]);
}

#[test]
fn a_change_rewrites_only_the_rows_it_touched() {
    let (mut e, displays) = engine();
    let mut fb = vec![0u8; SCREEN_BYTES];
    let page = uhf_page(e.catalogue());
    replay(&mut fb, &load(&mut e, &page).lcd);

    // The frequency on line 2 changes, and nothing else.
    let tuned = line(e.catalogue(), 2, "  305.10                ", BLANK);
    let batch = e.ingest(&tuned, Instant::now());
    assert!(!batch.lcd.is_empty());
    for w in &batch.lcd {
        let first = usize::from(w.group);
        let last = first + w.bytes.len() / 25 - 1;
        assert!(
            (13..26).contains(&first) && (13..26).contains(&last),
            "rows {first}-{last} are outside line 2"
        );
    }
    replay(&mut fb, &batch.lcd);
    assert_eq!(
        fb,
        expected(&displays, [
            ("     UHF     BOTH       ", BLANK),
            ("  305.10                ", BLANK),
            ("             *305.00*   ", "             i      i   "),
            ("  PRE   1 a      TOD    ", BLANK),
            ("     305.00       NB    ", BLANK),
        ])
    );

    // The backlight was set with the first paint and is not written again.
    assert_eq!(backlight(&batch), None);

    // Nothing moved, so nothing is written.
    let quiet = e.ingest(&tuned, Instant::now());
    assert!(quiet.lcd.is_empty());
}

#[test]
fn ending_the_mission_blanks_the_ded() {
    let (mut e, _) = engine();
    let mut fb = vec![0u8; SCREEN_BYTES];
    let page = uhf_page(e.catalogue());
    replay(&mut fb, &load(&mut e, &page).lcd);
    assert!(fb.iter().any(|b| *b != 0));

    // The screen latches, so leaving it would show a dead cockpit's page. Its
    // backlight goes off with it.
    let ended = e.mission_ended();
    replay(&mut fb, &ended.lcd);
    assert!(fb.iter().all(|b| *b == 0), "the DED must be blank");
    assert_eq!(backlight(&ended), Some(0));
}
