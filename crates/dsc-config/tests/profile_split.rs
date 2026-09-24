//! A shipped profile split in two by a release: DCS-BIOS reports variants
//! under one module that want different signals for the same lamp, so each
//! gets a profile. The aircraft moves with the release only where the user
//! left the list as the last release shipped it, and the new profile arrives
//! on the same start.

use std::path::{Path, PathBuf};

use dsc_config::{DeviceInventory, Profile, Profiles};

struct Scratch(PathBuf);

impl std::ops::Deref for Scratch {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn scratch(name: &str) -> Scratch {
    let dir = std::env::temp_dir().join(format!(
        "dsc-split-{name}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    for sub in ["defaults", "defaults-previous", "active"] {
        std::fs::create_dir_all(dir.join(sub)).unwrap();
    }
    Scratch(dir)
}

fn inventory() -> DeviceInventory {
    DeviceInventory::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/devices.json"))
        .expect("devices.json loads")
}

/// The MFD L backlight at a brightness, so an edit can be told from the default.
fn profile(name: &str, aircraft: &[&str], on: u8) -> String {
    let aircraft = aircraft.iter().map(|a| format!("\"{a}\"")).collect::<Vec<_>>().join(", ");
    format!(
        r#"{{
      "schema_version": 2,
      "name": "{name}",
      "aircraft": [{aircraft}],
      "module": "A-10C",
      "bindings": [{{ "device": "CarrierAce_MFD_L", "led": "Backlight", "always": true, "on": {on} }}]
    }}"#
    )
}

fn write(dir: &Path, sub: &str, file: &str, text: String) {
    std::fs::write(dir.join(sub).join(file), text).unwrap();
}

/// The last release shipped one profile for both variants; this one ships each its own.
fn lay_split(dir: &Path) {
    write(dir, "defaults-previous", "a-10c.json", profile("A-10C", &["A-10C_2", "A-10C"], 200));
    write(dir, "defaults", "a-10c.json", profile("A-10C", &["A-10C"], 200));
    write(dir, "defaults", "a-10c2.json", profile("A-10C II", &["A-10C_2"], 200));
}

/// Seed then merge, in the order the daemon starts.
fn start(dir: &Path) -> Vec<String> {
    let profiles = Profiles::new(dir.join("defaults"), dir.join("active"));
    profiles.seed().expect("seed runs");
    profiles.merge_new(&inventory(), "next").expect("merge runs")
}

fn active(dir: &Path, file: &str) -> Option<Profile> {
    Profile::load(&dir.join("active").join(file)).ok()
}

fn on(p: &Profile) -> Option<u8> {
    p.bindings
        .iter()
        .find(|b| b.device == "CarrierAce_MFD_L" && b.led == "Backlight")
        .and_then(|b| b.on)
}

#[test]
fn an_untouched_list_splits_and_the_new_profile_arrives_on_the_same_start() {
    let dir = scratch("untouched");
    lay_split(&dir);
    // A lamp the user dimmed while both variants shared the profile.
    write(&dir, "active", "a-10c.json", profile("A-10C", &["A-10C_2", "A-10C"], 90));

    let notes = start(&dir);

    let kept = active(&dir, "a-10c.json").expect("still loads");
    assert_eq!(kept.aircraft, ["A-10C"]);
    assert_eq!(on(&kept), Some(90), "the user's edit stays with the profile they made it in");
    let split = active(&dir, "a-10c2.json").expect("the split arrived");
    assert_eq!(split.aircraft, ["A-10C_2"]);
    assert_eq!(on(&split), Some(200), "the new profile comes as shipped");
    assert!(notes.iter().any(|n| n.contains("moved A-10C_2 to a-10c2.json")), "{notes:?}");
    assert!(notes.iter().any(|n| n.starts_with("a-10c2.json: added")), "{notes:?}");
}

#[test]
fn a_list_the_user_changed_is_theirs_and_the_notes_say_what_moved() {
    let dir = scratch("changed");
    lay_split(&dir);
    write(&dir, "active", "a-10c.json", profile("A-10C", &["A-10C_2", "A-10C", "A-10C_3"], 200));

    let notes = start(&dir);

    assert_eq!(active(&dir, "a-10c.json").unwrap().aircraft, ["A-10C_2", "A-10C", "A-10C_3"]);
    assert!(active(&dir, "a-10c2.json").is_none(), "A-10C_2 is still claimed");
    assert!(notes.iter().any(|n| n.contains("kept your own aircraft list")), "{notes:?}");
}

#[test]
fn a_release_seen_once_does_not_split_again() {
    let dir = scratch("once");
    lay_split(&dir);
    write(&dir, "active", "a-10c.json", profile("A-10C", &["A-10C_2", "A-10C"], 200));
    start(&dir);
    // The user puts both variants back on one profile after the split.
    std::fs::remove_file(dir.join("active/a-10c2.json")).unwrap();
    write(&dir, "active", "a-10c.json", profile("A-10C", &["A-10C_2", "A-10C"], 200));

    start(&dir);

    assert_eq!(active(&dir, "a-10c.json").unwrap().aircraft, ["A-10C_2", "A-10C"]);
    assert!(active(&dir, "a-10c2.json").is_none());
}

#[test]
fn an_added_aircraft_is_taken_only_where_nothing_flies_it() {
    let dir = scratch("added");
    write(&dir, "defaults-previous", "a-10c.json", profile("A-10C", &["A-10C"], 200));
    write(&dir, "defaults", "a-10c.json", profile("A-10C", &["A-10C", "A-10C_2"], 200));
    write(&dir, "active", "a-10c.json", profile("A-10C", &["A-10C"], 200));
    write(&dir, "active", "hog.json", profile("Hog", &["A-10C_2"], 200));

    start(&dir);

    assert_eq!(active(&dir, "a-10c.json").unwrap().aircraft, ["A-10C"]);
    assert_eq!(active(&dir, "hog.json").unwrap().aircraft, ["A-10C_2"]);
}

#[test]
fn an_added_aircraft_nothing_flies_is_taken_on() {
    let dir = scratch("joined");
    write(&dir, "defaults-previous", "a-10c.json", profile("A-10C", &["A-10C"], 200));
    write(&dir, "defaults", "a-10c.json", profile("A-10C", &["A-10C", "A-10C_2"], 200));
    write(&dir, "active", "a-10c.json", profile("A-10C", &["A-10C"], 200));

    let notes = start(&dir);

    assert_eq!(active(&dir, "a-10c.json").unwrap().aircraft, ["A-10C", "A-10C_2"]);
    assert!(notes.iter().any(|n| n.contains("now also flies A-10C_2")), "{notes:?}");
}
