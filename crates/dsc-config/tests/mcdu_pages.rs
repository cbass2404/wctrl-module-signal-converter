//! Pages: slots in a profile pointing into a library kept apart from it.
//!
//! What is pinned here is what a profile relies on: that version 1 is refused,
//! that every screen takes its fields only from pages, that the start page is
//! what runs, and that a page gone from the library costs a slot rather than
//! the profile. See docs/CONFIG.md "Pages".

use std::path::Path;

use dsc_config::{
    DeviceInventory, DisplayCatalogue, Error, Module, Page, PageFile, PageLibrary, PageSlots,
    Profile, Readout, Slot,
};

const CAPTAIN: &str = "MCDU_Captain";
const COPILOT: &str = "MCDU_CoPilot";
const FONT: &str = "../mcdu/f14bu-font-21x31.json";

fn r(p: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(p)
}

fn module() -> Module {
    serde_json::from_str(
        r#"{
          "module": "TEST",
          "aircraft": ["TEST"],
          "signals": [
            {
              "id": "CHAN",
              "control_type": "display",
              "outputs": [{"address": 200, "mask": null, "max_value": null, "max_length": 2, "type": "string"}]
            }
          ]
        }"#,
    )
    .expect("the fixture module parses")
}

fn devices() -> DeviceInventory {
    DeviceInventory::load(&r("data/devices.json")).expect("devices")
}

fn displays() -> DisplayCatalogue {
    DisplayCatalogue::load_dir(&r("data/displays")).expect("displays")
}

/// A profile of version 2 on TEST, with a font so its pages can be drawn.
fn profile(body: &str) -> Profile {
    let body = if body.is_empty() { String::new() } else { format!(", {body}") };
    serde_json::from_str(&format!(
        r#"{{"schema_version": 2, "name": "T", "aircraft": ["TEST"], "module": "TEST", "font": "{FONT}"{body}}}"#
    ))
    .expect("the fixture profile parses")
}

fn field(cells: &str, source: &str) -> Readout {
    let mut f = Readout::reading("", "MCDU", cells.parse().unwrap(), source);
    f.device.clear();
    f
}

fn page(id: &str, name: &str, fields: Vec<Readout>) -> Page {
    Page { id: id.into(), name: name.into(), display: "MCDU".into(), fields }
}

fn library() -> PageLibrary {
    PageLibrary::of(
        "TEST",
        vec![
            page("aaaaaa", "Radios", vec![field("0-1", "CHAN")]),
            page("bbbbbb", "Fuel", vec![field("24-25", "CHAN")]),
        ],
    )
}

fn slots(start: Option<usize>, pages: &[Option<&str>]) -> PageSlots {
    let mut s = PageSlots::empty(6);
    for (i, p) in pages.iter().enumerate() {
        s.slots[i] = p.map(Slot::new);
    }
    s.start = start;
    s
}

fn problems(p: &Profile, lib: &PageLibrary) -> Vec<Error> {
    p.problems(&module(), &devices(), &displays(), lib)
}

#[test]
fn a_version_1_profile_is_refused_with_one_reason() {
    let mut p = profile("");
    p.schema_version = 1;
    let found = problems(&p, &library());
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(matches!(found[0], Error::MadeBeforePages(1)));
    assert!(found[0].to_string().contains("before MCDU pages"), "{}", found[0]);
}

#[test]
fn a_profile_that_gives_no_version_is_from_before_pages() {
    let p: Profile =
        serde_json::from_str(r#"{"name": "T", "aircraft": ["TEST"], "module": "TEST"}"#).unwrap();
    assert!(matches!(problems(&p, &library())[..], [Error::MadeBeforePages(1)]));
}

#[test]
fn a_newer_version_is_refused_too() {
    let mut p = profile("");
    p.schema_version = 3;
    assert!(matches!(problems(&p, &library())[..], [Error::NewerSchema(3)]));
}

#[test]
fn a_field_of_the_profiles_own_on_any_screen_is_refused() {
    // The text grid, the segment display and the pixel screen alike.
    for (device, display) in [("MCDU_Captain", "MCDU"), ("CarrierAce_UFC", "UFC1"), ("ViperAce_ICP", "DED")] {
        let p = profile(&format!(
            r#""readouts": [{{"device": "{device}", "display": "{display}", "cells": "0-1", "source": "CHAN"}}]"#
        ));
        let found = problems(&p, &library());
        assert!(found.iter().any(|e| matches!(e, Error::LooseScreenField(..))), "{display}: {found:?}");
    }
}

#[test]
fn slots_pointing_at_the_library_are_a_clean_profile() {
    let mut p = profile("");
    p.screens.insert(CAPTAIN.into(), slots(Some(2), &[Some("aaaaaa"), Some("bbbbbb")]));
    assert!(problems(&p, &library()).is_empty(), "{:?}", problems(&p, &library()));
}

#[test]
fn a_slot_shows_only_pages_for_its_own_screen() {
    // A UFC page in an MCDU slot would draw UFC cells on the MCDU's glass.
    let mut ufc = page("cccccc", "UFC", Vec::new());
    ufc.display = "UFC1".into();
    let mut lib = library();
    lib.files.get_mut("TEST").unwrap().pages.push(ufc);

    let mut p = profile("");
    p.screens.insert(CAPTAIN.into(), slots(Some(1), &[Some("cccccc")]));
    let found = problems(&p, &lib);
    assert!(found.iter().any(|e| matches!(e, Error::PageOnOtherDisplay(_, 1, _, _))), "{found:?}");

    let mut p = profile("");
    p.screens.insert("CarrierAce_UFC".into(), slots(Some(1), &[Some("cccccc")]));
    assert!(problems(&p, &lib).is_empty(), "{:?}", problems(&p, &lib));
}

#[test]
fn the_start_page_is_what_runs() {
    let mut p = profile("");
    p.screens.insert(CAPTAIN.into(), slots(Some(2), &[Some("aaaaaa"), Some("bbbbbb")]));
    let run = p.with_pages(&library());
    assert!(run.screens.is_empty(), "the slots are set aside");
    assert_eq!(run.readouts.len(), 1);
    let f = &run.readouts[0];
    assert_eq!((f.device.as_str(), f.display.as_str()), (CAPTAIN, "MCDU"));
    assert_eq!(f.cells.to_string(), "24-25", "slot 2 is Fuel");
    assert_eq!(f.page.as_deref(), Some("bbbbbb"));
    // What runs is still a profile the checks accept, its fields being a page's.
    assert!(problems(&run, &library()).is_empty(), "{:?}", problems(&run, &library()));
}

#[test]
fn a_follower_shows_the_page_of_the_device_it_follows() {
    let mut p = profile("");
    p.screens.insert(CAPTAIN.into(), slots(Some(1), &[Some("aaaaaa")]));
    // Its own slots are kept and ignored, the way its rows are.
    p.screens.insert(COPILOT.into(), slots(Some(1), &[Some("bbbbbb")]));
    p.follows.insert(COPILOT.into(), CAPTAIN.into());
    let run = p.with_pages(&library()).with_followers();
    let on = |d: &str| run.readouts.iter().filter(|r| r.device == d).map(|r| r.cells.to_string()).collect::<Vec<_>>();
    assert_eq!(on(CAPTAIN), vec!["0-1"]);
    assert_eq!(on(COPILOT), vec!["0-1"]);
}

#[test]
fn a_page_gone_from_the_library_empties_its_slot_and_start_moves_on() {
    let mut p = profile("");
    p.screens.insert(CAPTAIN.into(), slots(Some(1), &[Some("gone00"), None, Some("bbbbbb")]));
    let lib = library();
    assert!(problems(&p, &lib).is_empty(), "a missing page is not a refusal: {:?}", problems(&p, &lib));
    let run = p.with_pages(&lib);
    assert_eq!(run.readouts[0].page.as_deref(), Some("bbbbbb"), "the first filled slot that loads");
    let notes: Vec<String> = p.slot_notes(&lib).into_iter().map(|n| n.text).collect();
    assert!(notes.iter().any(|n| n.contains("not in the library")), "{notes:?}");
    assert!(notes.iter().any(|n| n.contains("starts on slot 3")), "{notes:?}");
}

#[test]
fn a_page_file_that_will_not_load_takes_only_its_module_out() {
    let dir = std::env::temp_dir().join(format!("dsc-pages-broken-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("test.json"), "{ not json").unwrap();
    std::fs::write(dir.join("other-one.json"), r#"{"module": "OTHER_one", "pages": []}"#).unwrap();
    std::fs::write(dir.join("wrong.json"), r#"{"module": "NOT_WRONG", "pages": []}"#).unwrap();
    std::fs::write(dir.join("Upper.json"), r#"{"module": "Upper", "pages": []}"#).unwrap();
    let lib = PageLibrary::load_dir(&dir);
    let _ = std::fs::remove_dir_all(&dir);

    assert!(lib.broken("TEST").is_some());
    assert!(lib.broken("WRONG").is_some(), "a file named for one module holding another's is refused");
    assert!(lib.broken("Upper").is_some(), "so is one named by the module key as it is");
    assert!(lib.files.contains_key("OTHER_one"), "named as a profile would be, by the lowercase stem");

    let mut p = profile("");
    p.screens.insert(CAPTAIN.into(), slots(Some(1), &[Some("aaaaaa")]));
    assert!(problems(&p, &lib).is_empty());
    assert!(p.with_pages(&lib).readouts.is_empty(), "every slot on the module loads empty");
    let notes = p.slot_notes(&lib);
    assert!(notes.iter().any(|n| n.slot.is_none() && n.text.contains("did not load")), "{notes:?}");
}

#[test]
fn a_page_on_another_module_is_refused() {
    let mut lib = library();
    lib.files.insert(
        "OTHER".into(),
        PageFile { module: "OTHER".into(), pages: vec![page("cccccc", "Elsewhere", vec![field("0-1", "CHAN")])] },
    );
    let mut p = profile("");
    p.screens.insert(CAPTAIN.into(), slots(Some(1), &[Some("cccccc")]));
    let found = problems(&p, &lib);
    assert!(found.iter().any(|e| matches!(e, Error::PageOnOtherModule(..))), "{found:?}");
    assert!(p.with_pages(&lib).readouts.is_empty(), "and it does not run");
}

#[test]
fn the_shape_of_the_slots_is_checked() {
    let lib = library();
    let check = |s: PageSlots| {
        let mut p = profile("");
        p.screens.insert(CAPTAIN.into(), s);
        problems(&p, &lib)
    };

    let mut five = slots(Some(1), &[Some("aaaaaa")]);
    five.slots.pop();
    assert!(check(five).iter().any(|e| matches!(e, Error::SlotCount(_, 5, 6))), "one slot per page key");

    assert!(check(slots(Some(2), &[Some("aaaaaa")])).iter().any(|e| matches!(e, Error::StartNotFilled(_, 2))));
    assert!(check(slots(Some(7), &[Some("aaaaaa")])).iter().any(|e| matches!(e, Error::StartNotFilled(_, 7))));
    assert!(check(slots(None, &[Some("aaaaaa")])).iter().any(|e| matches!(e, Error::NoStartSlot(_))));
    assert!(check(slots(None, &[])).is_empty(), "every slot empty needs no start");

    let mut keyed = slots(Some(1), &[Some("aaaaaa")]);
    keyed.slots[0].as_mut().unwrap().key = Some(serde_json::json!("1L"));
    assert!(check(keyed).iter().any(|e| matches!(e, Error::SlotKeySet(_, 1))));

    let mut p = profile("");
    p.screens.insert("TAKEOFF_PLANEL_2".into(), slots(None, &[]));
    assert!(problems(&p, &lib).iter().any(|e| matches!(e, Error::SlotsWithoutScreen(_))));
}

#[test]
fn a_page_is_checked_against_the_font_of_the_profile_showing_it() {
    // The same page, fine where a font is chosen and not where none is: an
    // aircraft without a CDU of its own draws with the profile's font.
    let mut p = profile("");
    p.screens.insert(CAPTAIN.into(), slots(Some(3), &[None, None, Some("aaaaaa")]));
    assert!(problems(&p, &library()).is_empty());
    p.font = None;
    let found = problems(&p, &library());
    let on_page = found.iter().find(|e| matches!(e, Error::OnPage(..))).expect("refused on the page");
    let text = on_page.to_string();
    assert!(text.contains("\"Radios\"") && text.contains("slot 3"), "{text}");
}

#[test]
fn a_page_file_is_written_without_devices_and_read_back_whole() {
    let dir = std::env::temp_dir().join(format!("dsc-pages-save-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let lib = library();
    lib.save_module(&dir, "TEST").expect("saves");
    let text = std::fs::read_to_string(dir.join("TEST.json")).unwrap();
    assert!(!text.contains("\"device\""), "{text}");
    assert_eq!(text.matches("\"display\"").count(), 2, "only each page names its display: {text}");
    assert!(text.contains("\r\n"), "CRLF, like a profile");
    let back = PageLibrary::load_dir(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    let fuel = back.page_on("TEST", "bbbbbb").expect("the page came back");
    assert_eq!(fuel.fields[0].display, "MCDU", "each field takes the page's display");
    assert_eq!(fuel.fields[0].cells.to_string(), "24-25");
}

#[test]
fn a_page_is_checked_on_its_own_before_it_is_saved() {
    let lib = library();
    let (m, devs, disp) = (module(), devices(), displays());

    let mut taken = page("dddddd", " radios ", vec![field("0-1", "CHAN")]);
    assert!(lib.page_problems(&taken, &m, &devs, &disp).iter().any(|e| matches!(e, Error::PageNameTaken(..))));
    taken.name = "Radios 2".into();
    assert!(lib.page_problems(&taken, &m, &devs, &disp).is_empty());

    let overlap = page("dddddd", "Both", vec![field("0-3", "CHAN"), field("2-5", "CHAN")]);
    assert!(lib.page_problems(&overlap, &m, &devs, &disp).iter().any(|e| matches!(e, Error::CellsOverlap(..))));

    // Every screen takes pages, whatever its glass; only a display nobody
    // has mapped does not.
    for display in ["UFC1", "DED"] {
        let mut other = page("dddddd", "Other", Vec::new());
        other.display = display.into();
        assert!(lib.page_problems(&other, &m, &devs, &disp).is_empty(), "{display}");
    }
    let mut unknown = page("dddddd", "Nowhere", Vec::new());
    unknown.display = "NOPE".into();
    assert!(lib.page_problems(&unknown, &m, &devs, &disp).iter().any(|e| matches!(e, Error::PageOnUnknownDisplay(..))));
}

#[test]
fn names_and_ids_stay_unique() {
    let lib = library();
    assert_eq!(lib.free_name("TEST", "Fuel", None), "Fuel 2");
    assert_eq!(lib.free_name("TEST", "Fuel", Some("bbbbbb")), "Fuel", "a page keeps its own name");
    assert_eq!(lib.free_name("TEST", "Engines", None), "Engines");
    let id = lib.fresh_id();
    assert_eq!(id.len(), 6);
    assert!(lib.find(&id).is_none());

    let mut twice = lib.clone();
    twice.files.get_mut("TEST").unwrap().pages.push(page("aaaaaa", "fuel", Vec::new()));
    let found = twice.problems();
    assert_eq!(found.len(), 2, "{found:?}");
}

#[test]
fn deleting_a_page_empties_its_slots_and_moves_start() {
    let mut s = slots(Some(2), &[Some("aaaaaa"), Some("bbbbbb"), Some("bbbbbb")]);
    assert_eq!(s.clear_page("bbbbbb"), vec![2, 3]);
    assert_eq!(s.start, Some(1));
    assert_eq!(s.clear_page("aaaaaa"), vec![1]);
    assert_eq!(s.start, None, "nothing left to start on");
}

#[test]
fn a_blank_slot_is_the_screen_dark_on_purpose_and_null_is_out_of_use() {
    // Out of use will mean a line select key that does nothing; blank will
    // mean one that takes the screen dark. A blank slot can be the start.
    let mut s = slots(Some(2), &[Some("aaaaaa")]);
    s.slots[1] = Some(Slot::blank());
    let mut p = profile("");
    p.screens.insert(CAPTAIN.into(), s.clone());
    assert!(problems(&p, &library()).is_empty(), "{:?}", problems(&p, &library()));
    assert!(p.with_pages(&library()).readouts.is_empty(), "starting on a blank slot draws nothing");
    assert!(p.slot_notes(&library()).is_empty(), "and it is not a missing page");

    let json = serde_json::to_value(&p).unwrap();
    assert_eq!(json["screens"][CAPTAIN]["slots"][1], serde_json::json!({"page": null, "key": null}));
    assert_eq!(json["screens"][CAPTAIN]["slots"][2], serde_json::Value::Null);

    // A deleted page takes its slot out of use rather than blanking it.
    s.clear_page("aaaaaa");
    assert_eq!(s.slots[0], None);
    assert_eq!(s.start, Some(2), "the blank slot is still in use, and still the start");
}

#[test]
fn slots_are_written_as_the_design_says() {
    let mut p = profile("");
    p.screens.insert(CAPTAIN.into(), slots(Some(1), &[Some("aaaaaa")]));
    let json = serde_json::to_value(&p).unwrap();
    assert_eq!(
        json["screens"][CAPTAIN],
        serde_json::json!({"start": 1, "slots": [{"page": "aaaaaa", "key": null}, null, null, null, null, null]})
    );
    // A profile with no slots writes none.
    assert!(serde_json::to_value(profile("")).unwrap().get("screens").is_none());
}
