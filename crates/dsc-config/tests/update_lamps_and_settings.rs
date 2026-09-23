//! Lamp rows and profile settings an update changed, taken only where the user
//! left them as the last release shipped them.
//!
//! The same rule `profile_reconcile.rs` pins for display fields. It exists
//! because alpha.004 shipped `follows`, `same_as_device` and an MCDU `font`,
//! and an upgrade from alpha.003 merged none of them: every panel kept "its
//! own setup", and the Hornet took the new MCDU fields without the font they
//! are drawn in, which the daemon refuses to load.

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
        "dsc-update-{name}-{}",
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

/// A profile from its settings and one lamp row, as JSON text.
fn profile(settings: &str, lamp: &str) -> String {
    format!(
        r#"{{
      "schema_version": 2,
      "name": "Hog",
      "aircraft": ["A-10C"],
      "module": "A-10C",
      {settings}
      "bindings": [{lamp}]
    }}"#
    )
}

/// The MFD L backlight as alpha.003 shipped it: its own condition.
const OWN: &str = r#"{ "device": "CarrierAce_MFD_L", "led": "Backlight",
                       "always": true, "on": 200 }"#;

/// The same lamp as alpha.004 shipped it: matching the PTO2's backlight.
const MATCHED: &str = r#"{ "device": "CarrierAce_MFD_L", "led": "Backlight",
                           "same_as": "Backlight", "same_as_device": "TAKEOFF_PLANEL_2" }"#;

/// The same lamp as a user set it.
const THEIRS: &str = r#"{ "device": "CarrierAce_MFD_L", "led": "Backlight",
                          "always": true, "on": 90 }"#;

const FOLLOWS: &str = r#""follows": { "MCDU_CoPilot": "MCDU_Captain" },"#;
const FONT: &str = r#""font": "../mcdu/f14bu-font-21x31.json","#;

/// Write all three files: last release, this release, and the user's.
fn lay(dir: &Path, previous: (&str, &str), shipped: (&str, &str), mine: (&str, &str)) {
    let write = |sub: &str, (settings, lamp): (&str, &str)| {
        std::fs::write(dir.join(sub).join("a-10c.json"), profile(settings, lamp)).unwrap();
    };
    write("defaults-previous", previous);
    write("defaults", shipped);
    write("active", mine);
}

fn merge(dir: &Path, version: &str) -> Vec<String> {
    Profiles::new(dir.join("defaults"), dir.join("active"))
        .merge_new(&inventory(), version)
        .expect("merge runs")
}

fn mine(dir: &Path) -> Profile {
    Profile::load(&dir.join("active/a-10c.json")).expect("still loads")
}

fn mfd_l(p: &Profile) -> &dsc_config::Binding {
    p.bindings
        .iter()
        .find(|b| b.device == "CarrierAce_MFD_L" && b.led == "Backlight")
        .expect("the row is still there")
}

#[test]
fn a_lamp_row_still_as_shipped_takes_the_new_one() {
    let dir = scratch("lamp-new");
    lay(&dir, ("", OWN), ("", MATCHED), ("", OWN));

    let notes = merge(&dir, "alpha.004");

    let after = mine(&dir);
    assert_eq!(mfd_l(&after).same_as_device.as_deref(), Some("TAKEOFF_PLANEL_2"));
    assert!(notes.iter().any(|n| n.contains("1 unchanged lamp row")), "{notes:?}");
}

#[test]
fn a_lamp_row_the_user_changed_is_theirs() {
    let dir = scratch("lamp-theirs");
    lay(&dir, ("", OWN), ("", MATCHED), ("", THEIRS));

    merge(&dir, "alpha.004");

    let after = mine(&dir);
    assert_eq!(mfd_l(&after).same_as_device, None);
    assert_eq!(mfd_l(&after).on, Some(90));
}

#[test]
fn follows_and_font_still_as_shipped_take_the_new_ones() {
    let dir = scratch("settings-new");
    lay(&dir, ("", OWN), (&format!("{FOLLOWS}{FONT}"), OWN), ("", OWN));

    let notes = merge(&dir, "alpha.004");

    let after = mine(&dir);
    assert_eq!(after.follows.get("MCDU_CoPilot").map(String::as_str), Some("MCDU_Captain"));
    assert_eq!(after.font.as_deref(), Some("../mcdu/f14bu-font-21x31.json"));
    assert!(notes.iter().any(|n| n.contains("2 unchanged profile setting")), "{notes:?}");
}

/// Kept apart on purpose is a decision, and so is a font of their own.
#[test]
fn follows_and_font_the_user_set_are_theirs() {
    let dir = scratch("settings-theirs");
    let own = r#""follows": { "MCDU_CoPilot": "MCDU_Observer" }, "font": "../mcdu/a10c-font-21x31.json","#;
    lay(&dir, ("", OWN), (&format!("{FOLLOWS}{FONT}"), OWN), (own, OWN));

    merge(&dir, "alpha.004");

    let after = mine(&dir);
    assert_eq!(after.follows.get("MCDU_CoPilot").map(String::as_str), Some("MCDU_Observer"));
    assert_eq!(after.font.as_deref(), Some("../mcdu/a10c-font-21x31.json"));
}

/// A user who stopped a panel following keeps it stopped, even when the
/// release still has it following.
#[test]
fn a_follow_the_user_removed_stays_removed() {
    let dir = scratch("follow-removed");
    lay(&dir, (FOLLOWS, OWN), (FOLLOWS, OWN), ("", OWN));

    merge(&dir, "alpha.004");

    assert!(mine(&dir).follows.is_empty());
}

#[test]
fn a_device_disabled_by_the_release_is_disabled_unless_the_user_decided() {
    let dir = scratch("disabled");
    let icp = r#""disabled_devices": ["ViperAce_ICP"],"#;
    let both = r#""disabled_devices": ["ViperAce_ICP", "MCDU_Observer"],"#;
    // ICP: shipped disabled, now enabled, user left it: enabled.
    // Observer: newly disabled, user never touched it: disabled.
    lay(&dir, (icp, OWN), (r#""disabled_devices": ["MCDU_Observer"],"#, OWN), (icp, OWN));
    merge(&dir, "alpha.004");
    assert_eq!(mine(&dir).disabled_devices, vec!["MCDU_Observer".to_string()]);

    // The user had already enabled the ICP and disabled the Observer
    // themselves: nothing to do.
    let dir = scratch("disabled-theirs");
    lay(&dir, (icp, OWN), (both, OWN), (r#""disabled_devices": ["MCDU_Observer"],"#, OWN));
    merge(&dir, "alpha.004");
    assert_eq!(mine(&dir).disabled_devices, vec!["MCDU_Observer".to_string()]);
}

/// With no snapshot there is no telling an untouched row from an edited one,
/// so nothing that already exists is rewritten.
#[test]
fn with_no_snapshot_lamps_and_settings_stay_put() {
    let dir = scratch("no-snapshot");
    lay(&dir, ("", OWN), (&format!("{FOLLOWS}{FONT}"), MATCHED), ("", OWN));
    std::fs::remove_dir_all(dir.join("defaults-previous")).unwrap();

    merge(&dir, "alpha.004");

    let after = mine(&dir);
    assert_eq!(mfd_l(&after).same_as_device, None);
    assert!(after.follows.is_empty());
    assert_eq!(after.font, None);
}

/// Once per version, as fields are: a user who puts a row back after the
/// update keeps it at the next start.
#[test]
fn it_runs_once_per_version() {
    let dir = scratch("once");
    lay(&dir, ("", OWN), ("", MATCHED), ("", OWN));
    merge(&dir, "alpha.004");

    std::fs::write(dir.join("active/a-10c.json"), profile("", OWN)).unwrap();
    merge(&dir, "alpha.004");

    assert_eq!(mfd_l(&mine(&dir)).same_as_device, None);
}
