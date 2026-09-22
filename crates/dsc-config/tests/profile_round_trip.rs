//! Loading a shipped profile and saving it back must change nothing.
//!
//! A field is a chain of spans in memory, and a chain of one is written flat,
//! with its source and styling beside the cells, exactly as every profile
//! written before chains existed. That is not tidiness. An update never
//! rewrites a row the user has changed, so a profile that came back from a
//! save reordered, or with a `content` array where a `source` used to be,
//! would turn every row into a row the user owns and freeze it against every
//! later fix.
//!
//! These compare the serialized text against the file on disk, because that is
//! the thing that has to be unchanged.

use std::path::{Path, PathBuf};

use dsc_config::{Align, Colour, Profile, Span};

fn r(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(rel)
}

fn defaults() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(r("data/defaults"))
        .expect("the defaults are in the repository")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    out.sort();
    assert!(!out.is_empty(), "there are shipped defaults to check");
    out
}

/// The file's own text, with line endings normalised.
///
/// The working tree is CRLF and `to_string_pretty` writes LF, which is a
/// difference in how the file is stored rather than in what it says.
fn on_disk(path: &Path) -> String {
    std::fs::read_to_string(path)
        .expect("the file reads")
        .replace("\r\n", "\n")
}

#[test]
fn every_shipped_default_saves_back_unchanged() {
    // Compared as JSON rather than as text, because `replace` and `aliases`
    // are hash maps and have always come back out in whatever order the map
    // felt like. That is noise in a diff, not a change to the profile, and it
    // predates fields having more than one piece.
    for path in defaults() {
        let profile = Profile::load(&path).expect("a shipped default loads");
        let written = serde_json::to_string_pretty(&profile).expect("it serializes");
        let mine: serde_json::Value = serde_json::from_str(&written).expect("it parses");
        let theirs: serde_json::Value =
            serde_json::from_str(&on_disk(&path)).expect("the file parses");
        assert_eq!(
            mine,
            theirs,
            "{} changed on a load and save",
            path.display()
        );
    }
}

#[test]
fn every_shipped_default_keeps_the_shape_of_its_fields() {
    // The keys each field is written with, which is what a diff against a
    // later release compares. A field growing a `content` array here would
    // mean every row read as one the user had edited.
    for path in defaults() {
        let profile = Profile::load(&path).expect("a shipped default loads");
        let written = serde_json::to_string_pretty(&profile).expect("it serializes");
        let mine: serde_json::Value = serde_json::from_str(&written).expect("it parses");
        let theirs: serde_json::Value =
            serde_json::from_str(&on_disk(&path)).expect("the file parses");
        let keys = |v: &serde_json::Value| -> Vec<Vec<String>> {
            v["readouts"]
                .as_array()
                .map(|rs| {
                    rs.iter()
                        .map(|r| {
                            r.as_object()
                                .map(|o| o.keys().cloned().collect())
                                .unwrap_or_default()
                        })
                        .collect()
                })
                .unwrap_or_default()
        };
        assert_eq!(keys(&mine), keys(&theirs), "{}", path.display());
    }
}

#[test]
fn a_rule_keeps_its_label_and_the_label_its_own_colour() {
    // A divider goes back through the flat shape along with everything else,
    // and its colour is the one key on it that means something different there
    // than on a field. The label rides beside it and has to survive the same
    // trip, with its own colour kept apart from the rule's.
    let path = r("data/defaults/a-10c.json");
    let mut profile = Profile::load(&path).expect("the A-10C default loads");
    let rule = profile
        .readouts
        .iter_mut()
        .find(|r| r.divider)
        .expect("the A-10C ships a rule");
    rule.colour = Some(Colour::Green);
    rule.label = "FUEL".into();
    rule.label_colour = Some(Colour::Amber);

    let written = serde_json::to_string_pretty(&profile).expect("it serializes");
    let back: Profile = serde_json::from_str(&written).expect("it reads back");
    let rule = back.readouts.iter().find(|r| r.divider).expect("the rule came back");
    assert_eq!(rule.label, "FUEL");
    assert_eq!(rule.label_colour, Some(Colour::Amber));
    assert_eq!(rule.colour, Some(Colour::Green), "the rule keeps its own");
}

#[test]
fn a_colour_for_a_label_that_is_not_there_is_dropped() {
    // The editor keeps it on the object while the text is being edited, so
    // clearing the label to retype does not throw the colour away. On the way
    // to disk it goes: found in a shipped default after an evening of testing,
    // which is exactly how it would reach somebody's profile.
    let path = r("data/defaults/a-10c.json");
    let mut profile = Profile::load(&path).expect("the A-10C default loads");
    let rule = profile
        .readouts
        .iter_mut()
        .find(|r| r.divider)
        .expect("the A-10C ships a rule");
    rule.label = String::new();
    rule.label_colour = Some(Colour::Green);

    let written = serde_json::to_string_pretty(&profile).expect("it serializes");
    assert!(!written.contains("label_colour"), "no label, no colour for one");
    let back: Profile = serde_json::from_str(&written).expect("it reads back");
    let rule = back.readouts.iter().find(|r| r.divider).expect("the rule came back");
    assert_eq!(rule.label_colour, None);
}

#[test]
fn a_label_written_on_anything_but_a_rule_is_dropped() {
    // The same treatment the rule's colour gets. Kept, it would be a setting
    // the window never shows and nothing ever draws, sitting in the file
    // waiting to be believed.
    let path = r("data/defaults/a-10c.json");
    let mut profile = Profile::load(&path).expect("the A-10C default loads");
    let field = profile
        .readouts
        .iter_mut()
        .find(|r| !r.divider)
        .expect("a field that is not a rule");
    field.label = "FUEL".into();
    field.label_colour = Some(Colour::Amber);

    let written = serde_json::to_string_pretty(&profile).expect("it serializes");
    let back: Profile = serde_json::from_str(&written).expect("it reads back");
    assert!(
        back.readouts.iter().all(|r| r.divider || r.label.is_empty()),
        "no field but a rule carries a label"
    );
}

#[test]
fn a_rule_with_no_label_writes_no_label_key() {
    // An empty label has to leave nothing behind. Written out, every rule in
    // every default would gain a key at once, which an update would read as a
    // change and the reconcile would then decline to touch.
    let path = r("data/defaults/a-10c.json");
    let mut profile = Profile::load(&path).expect("the A-10C default loads");
    for r in &mut profile.readouts {
        r.label = String::new();
        r.label_colour = None;
    }
    let written = serde_json::to_string_pretty(&profile).expect("it serializes");
    assert!(
        !written.contains("\"label\":") && !written.contains("\"label_colour\":"),
        "an unlabelled rule writes neither key"
    );
}

#[test]
fn a_field_of_one_piece_is_written_flat_not_as_a_chain() {
    // The shape on disk is what an update diffs against, so a field with one
    // piece has to go back the way it came rather than growing an array.
    let path = r("data/defaults/a-10c.json");
    let profile = Profile::load(&path).expect("the A-10C default loads");
    let written = serde_json::to_string_pretty(&profile).expect("it serializes");
    assert!(
        written.contains("\"source\": \"CDU_LINE0\""),
        "a single source stays beside its cells"
    );
    assert!(
        !written.contains("\"content\""),
        "nothing here is a chain, so nothing gets a content array"
    );
}

#[test]
fn a_field_of_two_pieces_is_written_as_a_chain() {
    let path = r("data/defaults/a-10c.json");
    let mut profile = Profile::load(&path).expect("the A-10C default loads");
    let field = profile
        .readouts
        .iter_mut()
        .find(|r| !r.divider)
        .expect("a field to lengthen");
    field.content.push(Span {
        text: "M".into(),
        ..Span::default()
    });
    let written = serde_json::to_string_pretty(&profile).expect("it serializes");
    assert!(
        written.contains("\"content\""),
        "two pieces cannot be written beside the cells, so they become a chain"
    );
}

#[test]
fn a_chain_survives_a_save_and_a_load() {
    let path = r("data/defaults/a-10c.json");
    let mut profile = Profile::load(&path).expect("the A-10C default loads");
    let field = profile
        .readouts
        .iter_mut()
        .find(|r| !r.divider)
        .expect("a field to lengthen");
    field.content = vec![
        Span {
            text: "RALT".into(),
            small: true,
            ..Span::default()
        },
        Span {
            source: "CDU_LINE0".into(),
            ..Span::default()
        },
        Span {
            text: "M".into(),
            ..Span::default()
        },
    ];
    let written = serde_json::to_string_pretty(&profile).expect("it serializes");
    let back: Profile = serde_json::from_str(&written).expect("it reads back");
    let field = back
        .readouts
        .iter()
        .find(|r| r.content.len() == 3)
        .expect("the chain came back whole");
    assert_eq!(field.content[0].text, "RALT");
    assert!(field.content[0].small);
    assert_eq!(field.content[1].source, "CDU_LINE0");
    assert_eq!(field.content[2].text, "M");
}

#[test]
fn a_piece_with_a_box_is_written_as_a_chain_even_on_its_own() {
    // The flat shape's `align` is the field's, so a piece aligned inside its
    // own box has no flat spelling: written there, a box aligned right in a
    // field aligned left would come back as both aligned right. A chain of one
    // is the honest shape for it.
    for shaped in [
        Span { source: "CDU_LINE0".into(), width: 8, ..Span::default() },
        Span { source: "CDU_LINE0".into(), align: Align::Centre, ..Span::default() },
        Span { gap: true, rule: true, ..Span::default() },
    ] {
        let path = r("data/defaults/a-10c.json");
        let mut profile = Profile::load(&path).expect("the A-10C default loads");
        let field = profile
            .readouts
            .iter_mut()
            .find(|r| !r.divider)
            .expect("a field to shape");
        field.content = vec![shaped.clone()];
        let written = serde_json::to_string_pretty(&profile).expect("it serializes");
        assert!(
            written.contains("\"content\""),
            "a piece carrying {shaped:?} cannot be written beside the cells"
        );
        let back: Profile = serde_json::from_str(&written).expect("it reads back");
        let field = back
            .readouts
            .iter()
            .find(|r| r.content.len() == 1 && r.content[0].needs_chain())
            .expect("the piece came back shaped");
        assert_eq!(field.content[0].width, shaped.width);
        assert_eq!(field.content[0].align, shaped.align);
        assert_eq!(field.content[0].rule, shaped.rule);
    }
}

#[test]
fn a_piece_carrying_none_of_it_is_still_written_flat() {
    // The other half of the same promise: adding these keys must not move a
    // single field that does not use them, or every shipped row would read as
    // one the user had changed.
    let path = r("data/defaults/a-10c.json");
    let profile = Profile::load(&path).expect("the A-10C default loads");
    for field in &profile.readouts {
        for span in &field.content {
            assert!(
                !span.needs_chain(),
                "nothing shipped is boxed, so nothing shipped changes shape"
            );
        }
    }
}

#[test]
fn a_saved_profile_is_written_with_windows_line_endings() {
    // These are Windows files that people open and hand edit, and every one
    // of them was committed with CRLF. `to_string_pretty` writes LF, so a
    // save from the editor turned the whole file over and buried the one row
    // that had actually changed. Checked on the bytes, because that is the
    // thing that was wrong.
    let profile = Profile::load(&r("data/defaults/a-10c.json")).expect("the A-10C default");
    let dir = std::env::temp_dir().join(format!("dsc-endings-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a place to write");
    let path = dir.join("a-10c.json");
    profile.save(&path).expect("it saves");
    let bytes = std::fs::read(&path).expect("it reads back");
    let _ = std::fs::remove_dir_all(&dir);

    let lone = bytes
        .iter()
        .enumerate()
        .filter(|(i, &c)| c == b'\n' && *i > 0 && bytes[i - 1] != b'\r')
        .count();
    assert_eq!(lone, 0, "every line ends with the pair");
    assert!(bytes.windows(2).any(|w| w == b"\r\n"), "and there are lines");
    // No trailing newline, the way the shipped defaults are written, so the
    // last row of a file is not a change every time it is saved.
    assert_ne!(bytes.last(), Some(&b'\n'));
}

#[test]
fn a_piece_with_aliases_is_still_written_flat() {
    // Aliases belong to the one reading, not the field, so they have a flat
    // spelling beside the source. Growing a chain for them would make every
    // aliased row read as a changed one on the next update.
    let path = r("data/defaults/a-10c.json");
    let mut profile = Profile::load(&path).expect("the A-10C default loads");
    let field = profile
        .readouts
        .iter_mut()
        .find(|r| !r.divider)
        .expect("a field to alias");
    field.content[0].value_aliases = [(0, "OFF"), (3, "SEMI")]
        .into_iter()
        .map(|(v, a)| (v, a.to_string()))
        .collect();
    let written = serde_json::to_string_pretty(&profile).expect("it serializes");
    assert!(!written.contains("\"content\""), "one piece stays flat");
    assert!(written.contains("\"value_aliases\""), "and carries its aliases");
    let back: Profile = serde_json::from_str(&written).expect("it reads back");
    let field = back
        .readouts
        .iter()
        .find(|r| !r.content[0].value_aliases.is_empty())
        .expect("the aliases came back");
    assert_eq!(field.content[0].value_aliases.get(&3).map(String::as_str), Some("SEMI"));
}

#[test]
fn a_piece_with_no_aliases_writes_no_aliases_key() {
    // The other half: every shipped field has none, and a key written for
    // nothing would change every row in every default at once.
    for path in defaults() {
        let profile = Profile::load(&path).expect("a shipped default loads");
        let written = serde_json::to_string_pretty(&profile).expect("it serializes");
        assert!(!written.contains("value_aliases"), "{}", path.display());
    }
}
