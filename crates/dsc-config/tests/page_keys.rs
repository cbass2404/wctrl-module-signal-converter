//! Page keys: which of a panel's buttons swap its pages, and what each slot
//! holds once a profile runs.
//!
//! A panel's buttons are its own, listed by name in its `devices.json` entry,
//! and its page keys name some of them. The count of keys is the count of
//! slots. See docs/CONFIG.md "Swapping".

use std::path::{Path, PathBuf};

use dsc_config::{DeviceInventory, Error, Page, PageLibrary, PageSlots, Profile, Readout, Slot, SlotRun};

const CAPTAIN: &str = "MCDU_Captain";
const COPILOT: &str = "MCDU_CoPilot";

fn r(p: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(p)
}

/// An inventory of one panel, written to a file so it is read the way
/// `devices.json` is.
fn inventory(buttons: &str, page_keys: &str) -> dsc_config::Result<DeviceInventory> {
    let dir = std::env::temp_dir().join(format!("dsc-page-keys-{}-{:x}", std::process::id(), {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (buttons, page_keys).hash(&mut h);
        h.finish()
    }));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("devices.json");
    std::fs::write(
        &path,
        format!(
            r#"{{"devices": [{{"key": "P", "display_name": "P", "usb_pid": 1, "parts": [],
                "buttons": [{buttons}], "page_keys": [{page_keys}]}}]}}"#
        ),
    )
    .unwrap();
    let read = DeviceInventory::load(&path);
    let _ = std::fs::remove_dir_all(&dir);
    read
}

#[test]
fn the_mcdu_page_keys_are_the_left_line_select_keys() {
    let devices = DeviceInventory::load(&r("data/devices.json")).expect("devices");
    for key in [CAPTAIN, COPILOT, "MCDU_Observer"] {
        let d = devices.device(key).expect("an MCDU entry");
        assert_eq!(d.slot_count(), 6, "{key}");
        for n in 1..=6u16 {
            assert_eq!(d.slot_of_button(n), Some(usize::from(n) - 1), "{key} button {n}");
        }
        assert_eq!(d.button("LSK_1R").map(|b| b.number), Some(7));
        assert_eq!(d.slot_of_button(7), None, "LSK 1R is a button, not a page key");
    }
}

#[test]
fn a_device_with_no_keys_has_one_slot() {
    let inv = inventory("", "").unwrap();
    assert_eq!(inv.devices[0].slot_count(), 1);
}

#[test]
fn a_page_key_must_name_a_button_of_its_own_device() {
    let one = r#"{"number": 1, "name": "A"}"#;
    assert!(inventory(one, r#""A""#).is_ok());
    assert!(matches!(inventory(one, r#""B""#), Err(Error::UnknownPageKey(_, k)) if k == "B"));
    assert!(matches!(inventory(one, r#""A", "A""#), Err(Error::ButtonTwice(..))));
    let twice = r#"{"number": 1, "name": "A"}, {"number": 1, "name": "B"}"#;
    assert!(matches!(inventory(twice, ""), Err(Error::ButtonTwice(..))), "one number, two names");
    let same = r#"{"number": 1, "name": "A"}, {"number": 2, "name": "A"}"#;
    assert!(matches!(inventory(same, ""), Err(Error::ButtonTwice(..))), "one name, two numbers");
}

fn field(cells: &str) -> Readout {
    let mut f = Readout::reading("", "MCDU", cells.parse().unwrap(), "CHAN");
    f.device.clear();
    f
}

fn profile() -> Profile {
    let mut p: Profile =
        serde_json::from_str(r#"{"schema_version": 2, "name": "T", "aircraft": ["TEST"], "module": "TEST"}"#).unwrap();
    let mut s = PageSlots::empty(6);
    s.slots[0] = Some(Slot::new("aaaaaa"));
    s.slots[1] = Some(Slot::blank());
    s.slots[2] = Some(Slot::new("gone"));
    s.slots[3] = Some(Slot::new("bbbbbb"));
    s.start = Some(1);
    p.screens.insert(CAPTAIN.into(), s);
    p
}

fn library() -> PageLibrary {
    let page = |id: &str, name: &str, cells: &str| Page {
        id: id.into(),
        name: name.into(),
        display: "MCDU".into(),
        fields: vec![field(cells)],
    };
    PageLibrary::of("TEST", vec![page("aaaaaa", "Radios", "0-1"), page("bbbbbb", "Fuel", "24-25")])
}

fn cells(p: &Profile, device: &str) -> Vec<String> {
    p.readouts.iter().filter(|r| r.device == device).map(|r| r.cells.to_string()).collect()
}

#[test]
fn every_slot_is_resolved_when_the_profile_runs() {
    let run = profile().with_pages(&library());
    let pages = &run.page_runs[CAPTAIN];
    assert_eq!((pages.start, pages.shown), (0, 0));
    assert!(matches!(&pages.slots[0], SlotRun::Page { name, .. } if name == "Radios"));
    assert!(matches!(pages.slots[1], SlotRun::Blank));
    assert!(matches!(pages.slots[2], SlotRun::Off), "a page not in the library does nothing");
    assert!(matches!(&pages.slots[3], SlotRun::Page { name, .. } if name == "Fuel"));
    assert!(matches!(pages.slots[4], SlotRun::Off));
}

#[test]
fn showing_a_slot_swaps_only_the_page_fields_of_its_device() {
    let mut run = profile().with_pages(&library());
    assert_eq!(cells(&run, CAPTAIN), vec!["0-1"]);
    assert!(run.show_slot(CAPTAIN, 3));
    assert_eq!(cells(&run, CAPTAIN), vec!["24-25"]);
    assert!(!run.show_slot(CAPTAIN, 3), "already showing");
    assert!(!run.show_slot(CAPTAIN, 2), "its page is gone");
    assert!(!run.show_slot(CAPTAIN, 4), "disabled");
    assert_eq!(cells(&run, CAPTAIN), vec!["24-25"], "the page shown stays");
    assert!(run.show_slot(CAPTAIN, 1));
    assert!(cells(&run, CAPTAIN).is_empty(), "blank takes the screen dark");
    run.reset_pages();
    assert_eq!(cells(&run, CAPTAIN), vec!["0-1"], "back to the start page");
}

#[test]
fn a_follower_has_the_leaders_slots_and_its_own_page() {
    let mut p = profile();
    p.follows.insert(COPILOT.into(), CAPTAIN.into());
    let mut run = p.with_pages(&library()).with_followers();
    assert_eq!(cells(&run, COPILOT), vec!["0-1"], "it starts alike");
    assert!(run.show_slot(COPILOT, 3));
    assert_eq!(cells(&run, COPILOT), vec!["24-25"]);
    assert_eq!(cells(&run, CAPTAIN), vec!["0-1"], "and the leader stays where it was");
}
