//! Correcting a display field an update changed, without touching one the
//! user made their own.
//!
//! This is the only code in the project that removes content from a file the
//! user owns, so the rule is narrow and every branch of it is pinned here. A
//! field is ours to correct only while it is still byte for byte what the last
//! release shipped, which is what `data/defaults-previous` records. Anything
//! else, including a field that is simply missing, is a decision somebody made
//! and is left exactly as it is.
//!
//! The version argument is the seam that makes this testable: reconciling runs
//! once per version, and a test drives an upgrade by passing a different one.

use std::path::{Path, PathBuf};

use dsc_config::{DeviceInventory, Profile, Profiles, ValueBand};

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

/// Three folders: what ships now, what shipped last time, and what runs.
fn scratch(name: &str) -> Scratch {
    let dir = std::env::temp_dir().join(format!(
        "dsc-reconcile-{name}-{}",
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

/// A profile holding exactly the readouts given, as JSON text.
fn profile_with(readouts: &str) -> String {
    format!(
        r#"{{
      "schema_version": 2,
      "name": "Hog",
      "aircraft": ["A-10C"],
      "module": "A-10C",
      "bindings": [],
      "readouts": [{readouts}]
    }}"#
    )
}

/// The divider as it shipped last time.
const OLD: &str = r#"{ "device": "MCDU_Captain", "display": "MCDU", "cells": "72-95",
                       "divider": true, "colour": "green" }"#;

/// The same field, recoloured by a later release.
const NEW: &str = r#"{ "device": "MCDU_Captain", "display": "MCDU", "cells": "72-95",
                       "divider": true, "colour": "amber" }"#;

/// Write all three files. `previous` may be None to leave no snapshot at all.
fn lay(dir: &Path, previous: Option<&str>, shipped: &str, mine: &str) {
    if let Some(previous) = previous {
        std::fs::write(dir.join("defaults-previous/a-10c.json"), profile_with(previous)).unwrap();
    }
    std::fs::write(dir.join("defaults/a-10c.json"), profile_with(shipped)).unwrap();
    std::fs::write(dir.join("active/a-10c.json"), profile_with(mine)).unwrap();
}

fn merge(dir: &Path, version: &str) -> Vec<String> {
    Profiles::new(dir.join("defaults"), dir.join("active"))
        .merge_new(&inventory(), version)
        .expect("merge runs")
}

fn mine(dir: &Path) -> Profile {
    Profile::load(&dir.join("active/a-10c.json")).expect("still loads")
}

/// Whether the merge claims to have done anything to a field.
///
/// These fixtures carry no bindings, so every merge also adds a blank lamp row
/// for each LED on each connected device. That is the every-start behaviour and
/// is not what any of this is about.
fn touched_a_field(notes: &[String]) -> bool {
    notes.iter().any(|n| n.contains("display field"))
}

// ---------------------------------------------------------------------------
// still ours
// ---------------------------------------------------------------------------

#[test]
fn a_field_still_matching_the_last_release_takes_the_new_one() {
    let dir = scratch("takes-new");
    lay(&dir, Some(OLD), NEW, OLD);

    let notes = merge(&dir, "alpha.004");

    let after = mine(&dir);
    assert_eq!(after.readouts.len(), 1);
    assert_eq!(
        after.readouts[0].colour.map(|c| format!("{c:?}")),
        Some("Amber".to_string()),
        "the correction reached the file"
    );
    assert!(
        notes.iter().any(|n| n.contains("updated 1")),
        "and it was reported: {notes:?}"
    );
}

/// The same field written the long way round. A field of one piece is stored
/// flat, but a hand-edited file may spell it as a chain of one, and the two
/// mean the same thing. Compared as values rather than text so that spelling,
/// and the arbitrary key order of `replace` and `aliases`, are not mistaken
/// for somebody's edit.
#[test]
fn a_field_spelled_differently_still_counts_as_untouched() {
    let dir = scratch("spelling");
    let flat = r#"{ "device": "MCDU_Captain", "display": "MCDU", "cells": "0-23",
                    "source": "CDU_LINE0", "colour": "green" }"#;
    let chained = r#"{ "device": "MCDU_Captain", "display": "MCDU", "cells": "0-23",
                       "content": [{ "source": "CDU_LINE0", "colour": "green" }] }"#;
    let recoloured = r#"{ "device": "MCDU_Captain", "display": "MCDU", "cells": "0-23",
                          "source": "CDU_LINE0", "colour": "amber" }"#;
    lay(&dir, Some(flat), recoloured, chained);

    merge(&dir, "alpha.004");

    let after = mine(&dir);
    assert_eq!(
        after.readouts[0].content[0].colour.map(|c| format!("{c:?}")),
        Some("Amber".to_string()),
        "the chain of one was recognised as the flat field we shipped"
    );
}

// ---------------------------------------------------------------------------
// theirs
// ---------------------------------------------------------------------------

#[test]
fn a_field_the_user_changed_is_left_alone() {
    let dir = scratch("theirs");
    let theirs = r#"{ "device": "MCDU_Captain", "display": "MCDU", "cells": "72-95",
                      "divider": true, "colour": "white" }"#;
    lay(&dir, Some(OLD), NEW, theirs);

    let notes = merge(&dir, "alpha.004");

    let after = mine(&dir);
    assert_eq!(
        after.readouts[0].colour.map(|c| format!("{c:?}")),
        Some("White".to_string()),
        "their colour survived an update that changed ours"
    );
    assert!(!touched_a_field(&notes), "and nothing was claimed: {notes:?}");
}

#[test]
fn a_row_the_user_owns_takes_the_sensible_default_for_a_setting_added_later() {
    // What a row edited before a setting existed should do with it: nothing,
    // and go on working. Every key on a field is optional and absent means its
    // default, so the row reads as the setting turned off, which is the only
    // sensible value for a row that was built without it and looked right.
    //
    // Written the long way, through a merge and a reload, because the promise
    // is about what survives an update and not about what serde does.
    let dir = scratch("settings-added-later");
    let old = r#"{ "device": "MCDU_Captain", "display": "MCDU", "cells": "0-3",
                   "source": "CDU_BRT" }"#;
    // The new default uses this release's settings on its own row.
    let new = r#"{ "device": "MCDU_Captain", "display": "MCDU", "cells": "0-3",
                   "source": "CDU_BRT", "reads": [-1.5, 1.5], "decimals": 1, "abs": true,
                   "value_aliases": { "-1.5..-0.1": "ND" } }"#;
    // Theirs, edited back when a reading was a number and nothing else.
    let theirs = r#"{ "device": "MCDU_Captain", "display": "MCDU", "cells": "0-3",
                      "source": "CDU_BRT", "colour": "green" }"#;
    lay(&dir, Some(old), new, theirs);

    let notes = merge(&dir, "alpha.004");

    let after = mine(&dir);
    let span = &after.readouts[0].content[0];
    assert_eq!(span.colour.map(|c| format!("{c:?}")), Some("Green".into()), "theirs");
    assert!(!span.abs, "the setting is off, which is what their row meant");
    assert!(span.value_aliases.is_empty(), "and no bands arrived");
    assert_eq!(span.reads, None);
    assert!(!touched_a_field(&notes), "nothing was claimed: {notes:?}");
}

#[test]
fn an_aliased_row_nobody_touched_is_still_brought_up_to_the_new_default() {
    // The other half of the promise, and the one that fails silently. Whether
    // a row is still as it shipped is decided by comparing rows as JSON, so an
    // alias that came back in a different shape than it went in would make
    // every aliased row in every profile read as one the user had edited, and
    // it would never be updated again.
    //
    // The row here is byte for byte the last release's, so the new default's
    // wording has to reach it.
    let dir = scratch("aliases-untouched");
    let shipped_before = r#"{ "device": "MCDU_Captain", "display": "MCDU", "cells": "0-3",
                              "source": "CDU_BRT", "value_aliases": { "0": "OFF", "1": "ON" } }"#;
    let shipped_now = r#"{ "device": "MCDU_Captain", "display": "MCDU", "cells": "0-3",
                           "source": "CDU_BRT", "value_aliases": { "0": "OFF", "1": "BRT" } }"#;
    lay(&dir, Some(shipped_before), shipped_now, shipped_before);

    let notes = merge(&dir, "alpha.004");

    let after = mine(&dir);
    let span = &after.readouts[0].content[0];
    let drawn = |v: f64| {
        span.value_aliases
            .get(&ValueBand::One(v))
            .map(|a| a.text.as_str())
    };
    assert_eq!(drawn(1.0), Some("BRT"), "the new wording landed: {notes:?}");
    assert!(touched_a_field(&notes), "and it was reported: {notes:?}");
}

#[test]
fn a_field_the_user_gave_aliases_is_left_alone() {
    // Aliases are the user shortening names to fit their cells, which is as
    // much an edit as a colour. An update that changed the field must not
    // take them back to the numbers.
    let dir = scratch("aliased");
    let old = r#"{ "device": "MCDU_Captain", "display": "MCDU", "cells": "0-3",
                   "source": "CDU_BRT" }"#;
    let new = r#"{ "device": "MCDU_Captain", "display": "MCDU", "cells": "0-3",
                   "source": "CDU_BRT", "colour": "amber" }"#;
    let theirs = r#"{ "device": "MCDU_Captain", "display": "MCDU", "cells": "0-3",
                      "source": "CDU_BRT", "value_aliases": { "0": "OFF", "1": "ON" } }"#;
    lay(&dir, Some(old), new, theirs);

    let notes = merge(&dir, "alpha.004");

    let after = mine(&dir);
    let span = &after.readouts[0].content[0];
    let drawn = |v: f64| {
        span.value_aliases
            .get(&ValueBand::One(v))
            .map(|a| a.text.as_str())
    };
    assert_eq!(drawn(0.0), Some("OFF"));
    assert_eq!(drawn(1.0), Some("ON"));
    assert_eq!(span.colour, None, "the shipped change did not land on their field");
    assert!(!touched_a_field(&notes), "and nothing was claimed: {notes:?}");
}

/// A drum digit as it shipped before `wrap` and `round` existed.
const DRUM_OLD: &str = r#"{ "device": "MCDU_Captain", "display": "MCDU", "cells": "30",
                            "source": "CDU_BRT", "reads": [0, 9] }"#;

/// The same drum, drawn as the digit it has clicked over to.
const DRUM_NEW: &str = r#"{ "device": "MCDU_Captain", "display": "MCDU", "cells": "30",
                            "source": "CDU_BRT", "reads": [0, 10], "wrap": 10, "round": "down" }"#;

#[test]
fn a_drum_the_user_left_alone_takes_the_wrap_and_rounding() {
    // A profile written before the two keys existed has neither, and has to
    // read as untouched rather than as somebody's edit, or no drum anyone
    // already has would ever get the fix.
    let dir = scratch("drum-takes-new");
    lay(&dir, Some(DRUM_OLD), DRUM_NEW, DRUM_OLD);

    let notes = merge(&dir, "alpha.004");

    let span = &mine(&dir).readouts[0].content[0];
    assert_eq!(span.reads, Some([0.0, 10.0]));
    assert_eq!(span.wrap, Some(10.0));
    assert_eq!(span.round, dsc_config::Round::Down);
    assert!(touched_a_field(&notes), "and it was reported: {notes:?}");
}

#[test]
fn a_wrap_the_user_set_is_left_alone() {
    let dir = scratch("drum-theirs");
    let theirs = r#"{ "device": "MCDU_Captain", "display": "MCDU", "cells": "30",
                      "source": "CDU_BRT", "reads": [0, 9], "wrap": 5 }"#;
    lay(&dir, Some(DRUM_OLD), DRUM_NEW, theirs);

    let notes = merge(&dir, "alpha.004");

    let span = &mine(&dir).readouts[0].content[0];
    assert_eq!(span.wrap, Some(5.0), "their wrap survived");
    assert_eq!(span.round, dsc_config::Round::Nearest, "and ours did not land beside it");
    assert!(!touched_a_field(&notes), "and nothing was claimed: {notes:?}");
}

// ---------------------------------------------------------------------------
// removal, the branch worth distrusting
// ---------------------------------------------------------------------------

#[test]
fn a_field_the_default_no_longer_ships_is_taken_out() {
    let dir = scratch("retired");
    lay(&dir, Some(OLD), "", OLD);

    let notes = merge(&dir, "alpha.004");

    assert!(mine(&dir).readouts.is_empty(), "the retired field is gone");
    assert!(
        notes.iter().any(|n| n.contains("removed 1")),
        "and it was reported: {notes:?}"
    );
}

/// The same retirement, against a field somebody had edited. Removing is the
/// one operation that cannot be undone by the next start, so it has to be at
/// least as careful as replacing.
#[test]
fn a_field_the_user_changed_survives_the_default_dropping_it() {
    let dir = scratch("retired-theirs");
    let theirs = r#"{ "device": "MCDU_Captain", "display": "MCDU", "cells": "72-95",
                      "divider": true, "colour": "white" }"#;
    lay(&dir, Some(OLD), "", theirs);

    merge(&dir, "alpha.004");

    assert_eq!(mine(&dir).readouts.len(), 1, "their field was not swept up");
}

#[test]
fn nothing_is_removed_without_a_snapshot_to_judge_by() {
    let dir = scratch("no-snapshot");
    lay(&dir, None, "", OLD);
    std::fs::remove_dir_all(dir.join("defaults-previous")).unwrap();

    merge(&dir, "alpha.004");

    assert_eq!(
        mine(&dir).readouts.len(),
        1,
        "with nothing to compare against, everything is the user's"
    );
}

// ---------------------------------------------------------------------------
// a field that moved
// ---------------------------------------------------------------------------

/// The duplicate this whole scheme exists to avoid: a field keeps its meaning
/// and changes cells. Adding the new one and leaving the old one draws both.
#[test]
fn a_field_that_moved_is_not_drawn_twice() {
    let dir = scratch("moved");
    let moved = r#"{ "device": "MCDU_Captain", "display": "MCDU", "cells": "96-119",
                     "divider": true, "colour": "green" }"#;
    lay(&dir, Some(OLD), moved, OLD);

    merge(&dir, "alpha.004");

    let after = mine(&dir);
    assert_eq!(after.readouts.len(), 1, "one divider, not two");
    assert_eq!(after.readouts[0].cells.to_string(), "96-119", "at the new row");
}

// ---------------------------------------------------------------------------
// a field the user deleted
// ---------------------------------------------------------------------------

/// Deleting a field is as much a decision as editing one, and used to be
/// undone on the next start because nothing could tell "deleted" from "never
/// had it". The snapshot is what tells them apart.
#[test]
fn a_field_the_user_deleted_does_not_come_back() {
    let dir = scratch("deleted");
    lay(&dir, Some(OLD), OLD, "");

    let notes = merge(&dir, "alpha.004");

    assert!(mine(&dir).readouts.is_empty(), "it stayed deleted");
    assert!(!touched_a_field(&notes), "and nothing was claimed: {notes:?}");
}

#[test]
fn a_field_we_never_shipped_before_still_arrives() {
    let dir = scratch("new-field");
    lay(&dir, Some(""), OLD, "");

    merge(&dir, "alpha.004");

    assert_eq!(mine(&dir).readouts.len(), 1, "a genuinely new field lands");
}

#[test]
fn a_new_field_does_not_land_on_cells_the_user_claimed() {
    let dir = scratch("new-clash");
    let theirs = r#"{ "device": "MCDU_Captain", "display": "MCDU", "cells": "80-90",
                      "source": "CDU_LINE9" }"#;
    lay(&dir, Some(""), OLD, theirs);

    merge(&dir, "alpha.004");

    let after = mine(&dir);
    assert_eq!(after.readouts.len(), 1);
    assert_eq!(after.readouts[0].sources(), vec!["CDU_LINE9"]);
}

// ---------------------------------------------------------------------------
// once per version
// ---------------------------------------------------------------------------

/// The user putting the old field back, through the profile rather than over
/// the file, so the lamp rows the first merge added are not thrown away with
/// it.
fn restore_old_field(dir: &Path) {
    let path = dir.join("active/a-10c.json");
    let mut p = Profile::load(&path).expect("loads");
    let was: Profile = serde_json::from_str(&profile_with(OLD)).expect("fixture parses");
    p.readouts = was.readouts;
    p.save(&path).expect("saves");
}

/// Reconciling on every start would take a field away from somebody who put it
/// back the way they liked it, and would do it again at the next start, and
/// every start after that. Once per version means their change is the last
/// word until a release actually has something new to say.
#[test]
fn putting_a_field_back_sticks_until_the_next_version() {
    let dir = scratch("put-back");
    lay(&dir, Some(OLD), NEW, OLD);

    merge(&dir, "alpha.004");
    restore_old_field(&dir);
    let notes = merge(&dir, "alpha.004");

    assert!(
        !touched_a_field(&notes),
        "this version already had its turn: {notes:?}"
    );
    assert_eq!(
        mine(&dir).readouts[0].colour.map(|c| format!("{c:?}")),
        Some("Green".to_string()),
        "their restored field was left alone"
    );
}

#[test]
fn a_new_version_reconciles_again() {
    let dir = scratch("next-version");
    lay(&dir, Some(OLD), NEW, OLD);

    merge(&dir, "alpha.004");
    restore_old_field(&dir);
    merge(&dir, "alpha.005");

    assert_eq!(
        mine(&dir).readouts[0].colour.map(|c| format!("{c:?}")),
        Some("Amber".to_string()),
        "a release with something to say gets its turn"
    );
}

#[test]
fn the_version_is_recorded_even_when_there_was_nothing_to_do() {
    let dir = scratch("marker");
    lay(&dir, Some(OLD), OLD, OLD);

    merge(&dir, "alpha.004");

    let marker = std::fs::read_to_string(dir.join("active/.updated")).expect("marker written");
    assert_eq!(marker.trim(), "alpha.004");
}

/// A development checkout, where the default being edited and the profile
/// being flown are the same file. There is nothing to reconcile against, and
/// writing a marker into a tracked folder would show up in every diff.
#[test]
fn a_checkout_is_left_alone() {
    let dir = scratch("checkout");
    std::fs::write(dir.join("defaults/a-10c.json"), profile_with(OLD)).unwrap();
    std::fs::write(dir.join("defaults-previous/a-10c.json"), profile_with(NEW)).unwrap();

    Profiles::new(dir.join("defaults"), dir.join("defaults"))
        .merge_new(&inventory(), "alpha.004")
        .expect("merge runs");

    assert!(
        !dir.join("defaults/.updated").exists(),
        "no marker in the tracked folder"
    );
    let after = Profile::load(&dir.join("defaults/a-10c.json")).expect("still loads");
    assert_eq!(
        after.readouts[0].colour.map(|c| format!("{c:?}")),
        Some("Green".to_string()),
        "and the tracked default was not rewritten from the snapshot"
    );
}

/// The snapshot names the folder beside the defaults, so pointing `--defaults`
/// somewhere else takes its snapshot with it rather than reading the install's.
#[test]
fn the_snapshot_follows_the_defaults_folder() {
    let dir = scratch("sibling");
    let profiles = Profiles::new(dir.join("defaults"), dir.join("active"));
    assert_eq!(profiles.previous, dir.join("defaults-previous"));
}
