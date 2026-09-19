//! Every shipped profile has a row for every lamp we support.
//!
//! A profile ships as a product, so a panel added to `data/devices.json` has to
//! arrive in `data/defaults` at the same time, bound or deliberately left
//! empty. `merge_new` would paper over a gap at startup with a blank row, which
//! is exactly how the ICP went missing from eight defaults without anyone
//! noticing: nothing failed, the lamp just never lit.

use std::path::{Path, PathBuf};

use dsc_config::{Binding, Catalogue, DeviceInventory, DisplayCatalogue, Profile, Profiles};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn defaults() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(root().join("data/defaults"))
        .expect("data/defaults exists")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect();
    files.sort();
    files
}

fn inventory() -> DeviceInventory {
    DeviceInventory::load(&root().join("data/devices.json")).expect("devices.json loads")
}

#[test]
fn every_default_has_a_row_for_every_profile_lamp() {
    let inventory = inventory();
    let mut gaps = Vec::new();
    for path in defaults() {
        let profile = Profile::load(&path).expect("shipped profile loads");
        for device in &inventory.devices {
            for (_, led) in device.leds() {
                let found = profile
                    .bindings
                    .iter()
                    .any(|b| b.device == device.key && b.led == led.name);
                if !found {
                    gaps.push(format!(
                        "{}: {} {}",
                        path.file_name().unwrap().to_string_lossy(),
                        device.key,
                        led.name
                    ));
                }
            }
        }
    }
    assert!(gaps.is_empty(), "shipped defaults are missing rows:\n  {}", gaps.join("\n  "));
}

/// The stronger form: the startup merge, run over a copy of the defaults with
/// nothing else to draw on, finds nothing to add and nothing to reorder. So a
/// shipped file is already exactly what a user's copy would become.
#[test]
fn the_startup_merge_leaves_every_default_untouched() {
    let dir = std::env::temp_dir().join(format!(
        "dsc-shipped-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(dir.join("none")).unwrap();
    std::fs::create_dir_all(dir.join("active")).unwrap();
    for path in defaults() {
        std::fs::copy(&path, dir.join("active").join(path.file_name().unwrap())).unwrap();
    }

    let notes = Profiles::new(dir.join("none"), dir.join("active"))
        .merge_new(&inventory())
        .expect("merge runs");
    assert!(notes.is_empty(), "the merge would change shipped defaults: {notes:#?}");
}

/// Defaults whose backlights are not on one knob yet, each with its reason.
/// Remove an entry once the knob is known; the test then holds it too.
const ONE_KNOB_EXEMPT: &[(&str, &str)] = &[(
    "no-aircraft.json",
    "no aircraft is loaded, so there is no cockpit knob to follow (2026-09-18)",
)];

/// A shipped default drives every panel backlight from the same cockpit
/// source, so the whole pit dims together until the user decides otherwise.
/// A `same_as` is followed to the lamp it names before comparing.
#[test]
fn every_default_drives_all_backlights_from_one_source() {
    let inventory = inventory();
    let backlights: Vec<(String, String)> = inventory
        .devices
        .iter()
        .flat_map(|d| d.leds().filter(|(_, l)| l.backlight).map(|(_, l)| (d.key.clone(), l.name.clone())))
        .collect();
    assert!(backlights.len() > 1, "devices.json marks no backlights");

    let mut split = Vec::new();
    for path in defaults() {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if ONE_KNOB_EXEMPT.iter().any(|(n, _)| *n == name) {
            continue;
        }
        let profile = Profile::load(&path).expect("shipped profile loads");
        let find = |device: &str, led: &str| {
            profile.bindings.iter().find(|b| b.device == device && b.led == led)
        };
        let mut sources: Vec<(String, serde_json::Value)> = Vec::new();
        for (device, led) in &backlights {
            let mut b: &Binding = find(device, led).expect("every lamp has a row");
            let mut hops = 0;
            while let Some(target) = &b.same_as {
                b = find(device, target).expect("same_as names a lamp with a row");
                hops += 1;
                assert!(hops < 8, "{name}: same_as loop at {device} {led}");
            }
            // Unbound is not a source. Without this, a default whose
            // backlights are all left empty matches itself and passes.
            if b.is_placeholder() {
                split.push(format!("{name}: {device} {led} is not bound"));
                continue;
            }
            let mut plain = b.clone();
            plain.device.clear();
            plain.led.clear();
            plain.note.clear();
            sources.push((format!("{device} {led}"), serde_json::to_value(&plain).unwrap()));
        }
        let Some((_, first)) = sources.first() else {
            continue; // nothing bound, already reported lamp by lamp
        };
        for (lamp, source) in &sources[1..] {
            if source != first {
                split.push(format!("{name}: {lamp} differs from {}", sources[0].0));
            }
        }
    }
    assert!(split.is_empty(), "backlights on more than one source:
  {}", split.join("
  "));
}

/// A shipped default passes the same checks the editor puts a user's profile
/// through, and against the nightly it is written for, flags nothing: every
/// signal exists in its module and every value is in range. A user's DCS-BIOS
/// may flag some, which is what flags are for; the defaults' own may not.
/// Skipped where `data/catalogue` has not been generated.
#[test]
fn every_default_passes_the_editors_checks() {
    let Ok(catalogue) = Catalogue::load_dir(&root().join("data/catalogue")) else {
        return;
    };
    let inventory = inventory();
    let displays = DisplayCatalogue::load_dir(&root().join("data/displays")).expect("displays load");
    let mut found = Vec::new();
    for path in defaults() {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let profile = Profile::load(&path).expect("shipped profile loads");
        let Some(module) = catalogue.module(&profile.module) else {
            found.push(format!("{name}: module {} is not in the catalogue", profile.module));
            continue;
        };
        for problem in profile.problems(module, &inventory, &displays) {
            found.push(format!("{name}: {problem}"));
        }
        for flag in profile.flags(module) {
            found.push(format!("{name}: {} {} reads {} ({:?})", flag.device, flag.target, flag.source, flag.why));
        }
    }
    assert!(found.is_empty(), "shipped defaults fail the editor's checks:\n  {}", found.join("\n  "));
}
