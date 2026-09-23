//! Sharing a profile with its pages: what an export carries, how an import
//! settles each page against the library already here, and how a merge takes
//! page slots. See docs/CONFIG.md "Sharing pages".

use std::path::Path;

use dsc_config::bundle::{self, Bundle, Fate, PageTake};
use dsc_config::merge::{self, Pick, SlotPick};
use dsc_config::{DeviceInventory, DisplayCatalogue, Page, PageFile, PageLibrary, PageSlots, Profile, Readout, Slot};

const CAPTAIN: &str = "MCDU_Captain";

fn r(p: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(p)
}

fn field(cells: &str) -> Readout {
    let mut f = Readout::reading("", "MCDU", cells.parse().unwrap(), "CHAN");
    f.device.clear();
    f
}

fn page(id: &str, name: &str, cells: &str) -> Page {
    Page { id: id.into(), name: name.into(), display: "MCDU".into(), fields: vec![field(cells)] }
}

fn profile(slots: &[Option<&str>], start: usize) -> Profile {
    let mut p: Profile = serde_json::from_str(
        r#"{"schema_version": 2, "name": "T", "aircraft": ["TEST"], "module": "TEST"}"#,
    )
    .unwrap();
    let mut s = PageSlots::empty(6);
    for (i, id) in slots.iter().enumerate() {
        s.slots[i] = id.map(Slot::new);
    }
    s.start = Some(start);
    p.screens.insert(CAPTAIN.into(), s);
    p
}

fn shown(p: &Profile) -> Vec<Option<String>> {
    p.screens[CAPTAIN].slots.iter().map(|s| s.as_ref().and_then(|s| s.page.clone())).collect()
}

fn here() -> PageLibrary {
    let mut lib = PageLibrary::of("TEST", vec![page("same01", "Radios", "0-1"), page("diff01", "Fuel", "24-25")]);
    lib.files.insert(
        "OTHER".into(),
        PageFile { module: "OTHER".into(), pages: vec![page("othr01", "Elsewhere", "0-1")] },
    );
    lib
}

#[test]
fn an_export_carries_the_pages_its_slots_show_and_those_ticked() {
    let lib = PageLibrary::of(
        "TEST",
        vec![page("a", "One", "0-1"), page("b", "Two", "0-1"), page("c", "Three", "0-1")],
    );
    let p = profile(&[Some("a")], 1);
    let ids = |b: &Bundle| b.pages.iter().map(|p| p.id.clone()).collect::<Vec<_>>();
    assert_eq!(ids(&Bundle::of(&p, &lib, &[])), vec!["a"]);
    assert_eq!(ids(&Bundle::of(&p, &lib, &["c".into()])), vec!["a", "c"]);
}

#[test]
fn a_bundle_round_trips_and_a_bare_profile_reads_as_one() {
    let dir = std::env::temp_dir().join(format!("dsc-bundle-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let lib = PageLibrary::of("TEST", vec![page("a", "One", "0-1")]);
    let out = dir.join("shared.json");
    Bundle::of(&profile(&[Some("a")], 1), &lib, &[]).save(&out).unwrap();
    let text = std::fs::read_to_string(&out).unwrap();
    let back = Bundle::load(&out).unwrap();
    std::fs::write(dir.join("bare.json"), serde_json::to_string(&profile(&[], 1)).unwrap()).unwrap();
    let bare = Bundle::load(&dir.join("bare.json")).unwrap();
    let _ = std::fs::remove_dir_all(&dir);

    assert!(text.contains("\"profile\"") && text.contains("\r\n"), "{text}");
    assert_eq!(back.pages[0].fields[0].display, "MCDU", "fields take their page's display");
    assert_eq!(back.profile.name, "T");
    assert!(bare.pages.is_empty());
}

#[test]
fn each_page_is_settled_against_the_library() {
    let incoming = vec![
        page("same01", "Radios", "0-1"),   // here, unchanged
        page("diff01", "Fuel", "48-49"),   // id here, drawing something else
        page("othr01", "Other", "0-1"),    // id taken on another module
        page("new001", "Radios", "72-73"), // new, with a name already taken
        page("new002", "Radios", "96-97"), // new, and the same name again
    ];
    let p = profile(&[Some("same01"), Some("diff01")], 1);
    let plan = bundle::plan(&here(), &p, &incoming);
    let fates: Vec<Fate> = plan.iter().map(|p| p.fate).collect();
    assert_eq!(fates, vec![Fate::Same, Fate::NewId, Fate::NewId, Fate::New, Fate::New]);
    let names: Vec<&str> = plan.iter().map(|p| p.name_after.as_str()).collect();
    assert_eq!(names, vec!["Radios", "Fuel 2", "Other", "Radios 2", "Radios 3"]);
    assert!(plan[0].used && plan[1].used && !plan[3].used);
}

#[test]
fn bringing_pages_in_moves_slots_with_them() {
    let incoming = vec![page("same01", "Radios", "0-1"), page("diff01", "Fuel", "48-49"), page("new001", "Nav", "72-73")];
    let mut p = profile(&[Some("same01"), Some("diff01"), Some("new001")], 1);
    let take = vec![
        PageTake { id: "same01".into(), name: "Radios".into() },
        PageTake { id: "diff01".into(), name: "Fuel 2".into() },
    ];
    let added = bundle::bring_in(&here(), &mut p, &incoming, &take).unwrap();

    assert_eq!(added.len(), 1, "an unchanged page adds nothing: {added:?}");
    let renumbered = &added[0];
    assert_ne!(renumbered.id, "diff01");
    assert_eq!(renumbered.name, "Fuel 2");
    let slots = shown(&p);
    assert_eq!(slots[0].as_deref(), Some("same01"));
    assert_eq!(slots[1].as_deref(), Some(renumbered.id.as_str()), "the slot follows the new id");
    assert_eq!(slots[2], None, "a page left unticked comes in as an empty slot");
}

#[test]
fn a_name_already_taken_is_refused() {
    let mut p = profile(&[Some("new001")], 1);
    let err = bundle::bring_in(
        &here(),
        &mut p,
        &[page("new001", "Nav", "0-1")],
        &[PageTake { id: "new001".into(), name: " fuel ".into() }],
    )
    .unwrap_err();
    assert!(err.contains("already called"), "{err}");
}

#[test]
fn a_merge_takes_slot_n_for_slot_n() {
    let devices = DeviceInventory::load(&r("data/devices.json")).unwrap();
    let displays = DisplayCatalogue::load_dir(&r("data/displays")).unwrap();
    let source = profile(&[Some("a"), None, Some("c")], 3);
    let target = profile(&[Some("x"), Some("y"), None], 2);
    let pick = Pick {
        slots: vec![
            SlotPick { device: CAPTAIN.into(), slot: 1 },
            SlotPick { device: CAPTAIN.into(), slot: 2 },
            SlotPick { device: CAPTAIN.into(), slot: 3 },
        ],
        ..Pick::default()
    };
    let merged = merge::merge(&target, &source, &pick, &devices, &displays).unwrap();
    let slots = shown(&merged.profile);
    assert_eq!(slots[..3], [Some("a".to_string()), None, Some("c".to_string())]);
    let s = &merged.profile.screens[CAPTAIN];
    assert_eq!(s.start, Some(1), "start moves off the slot the merge emptied");
    let counts: Vec<(usize, usize, usize)> = merged.changes.iter().map(|c| (c.added, c.replaced, c.removed)).collect();
    assert_eq!(counts, vec![(0, 1, 0), (0, 0, 1), (1, 0, 0)]);
    assert!(merged.changes.iter().all(|c| c.pages));

    let parts = merge::slot_parts(&source, &devices, |id| (id == "a").then(|| "Alpha".to_string()));
    let named: Vec<(usize, &str, bool)> = parts.iter().map(|s| (s.slot, s.page.as_str(), s.start)).collect();
    assert_eq!(named, vec![(1, "Alpha", false), (3, "c", true)]);
}
