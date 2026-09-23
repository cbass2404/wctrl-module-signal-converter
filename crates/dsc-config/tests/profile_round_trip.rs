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

use dsc_config::{AliasDraw, Align, Colour, Profile, Span, ValueBand};

/// A profile of flat, single-piece fields: one A-10C CDU page, frozen.
///
/// Not a shipped default. The promise these tests are about is that adding a
/// key moves nothing that does not use it, and holding anybody to that needs a
/// fixture that uses none of them. A shipped profile is a living document: the
/// A-10C's own fields have since grown chains and boxes, which is the feature
/// working rather than the promise breaking, and every such edit would land
/// here as a failing build.
fn flat_fields() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/a-10c-cdu-page.json")
}

#[test]
fn an_alias_with_no_colour_is_written_as_bare_characters() {
    // The object shape exists for a colour, so an alias without one has to go
    // back as the string it arrived as. Built in code with an explicit
    // `colour: None` as well as parsed, because the editor builds them that
    // way and a row it touched must not change shape under it.
    let parsed: Profile = serde_json::from_str(
        r#"{"schema_version": 2, "name": "T", "aircraft": [], "module": "A-10C",
             "readouts": [{"device": "MCDU_Captain", "display": "MCDU", "cells": "0-3",
                           "source": "KNOB", "value_aliases": {"0": "OFF"}}]}"#,
    )
    .expect("it parses");
    let written = serde_json::to_string(&parsed).expect("it serializes");
    assert!(written.contains(r#""0":"OFF""#), "{written}");

    let mut built = parsed.clone();
    built.readouts[0].content[0].value_aliases.insert(
        ValueBand::One(1.0),
        AliasDraw {
            text: "ON".into(),
            colour: None,
            inverse: false,
        },
    );
    let written = serde_json::to_string(&built).expect("it serializes");
    assert!(written.contains(r#""1":"ON""#), "{written}");

    built.readouts[0].content[0].value_aliases.insert(
        ValueBand::One(2.0),
        AliasDraw {
            text: "WARN".into(),
            colour: Some(Colour::Red),
            inverse: false,
        },
    );
    let written = serde_json::to_string(&built).expect("it serializes");
    assert!(
        written.contains(r#""2":{"text":"WARN","colour":"red"}"#),
        "a colour grows the object: {written}"
    );

    built.readouts[0].content[0].value_aliases.insert(
        ValueBand::One(3.0),
        AliasDraw {
            text: " ".into(),
            colour: None,
            inverse: true,
        },
    );
    let written = serde_json::to_string(&built).expect("it serializes");
    assert!(
        written.contains(r#""3":{"text":" ","inverse":true}"#),
        "and so does inverse: {written}"
    );
    let back: Profile = serde_json::from_str(&written).expect("it parses back");
    assert_eq!(
        back.readouts[0].content[0].value_aliases,
        built.readouts[0].content[0].value_aliases
    );
}

#[test]
fn a_band_naming_a_whole_number_is_written_without_a_decimal_point() {
    // `3` and `3.0` are the same reading and a different JSON key, and the
    // merge compares rows as JSON.
    assert_eq!(ValueBand::One(3.0).to_string(), "3");
    assert_eq!(ValueBand::One(-1.5).to_string(), "-1.5");
    assert_eq!(ValueBand::List(vec![0.0, 1.0, 2.0]).to_string(), "0,1,2");
    assert_eq!(
        ValueBand::Range { lo: -1.5, hi: -0.1 }.to_string(),
        "-1.5..-0.1"
    );
    for written in ["3", "0,1,2", "-1.5..-0.1", "0.5..1"] {
        let band: ValueBand = written.parse().expect("it parses");
        assert_eq!(band.to_string(), written, "it goes back as it came");
    }
    // Said the way it is spoken, written the one canonical way.
    let band: ValueBand = "-1.5 to -0.1".parse().expect("it parses");
    assert_eq!(band.to_string(), "-1.5..-0.1");
}

#[test]
fn a_rule_keeps_its_label_and_the_label_its_own_colour() {
    // A divider goes back through the flat shape along with everything else,
    // and its colour is the one key on it that means something different there
    // than on a field. The label rides beside it and has to survive the same
    // trip, with its own colour kept apart from the rule's.
    let path = flat_fields();
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
    let path = flat_fields();
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
    let path = flat_fields();
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
    let path = flat_fields();
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
    let path = flat_fields();
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
    let path = flat_fields();
    let mut profile = Profile::load(&path).expect("the CDU page fixture loads");
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
    let path = flat_fields();
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
        let path = flat_fields();
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
    let path = flat_fields();
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
    let profile = Profile::load(&flat_fields()).expect("the A-10C default");
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
    let path = flat_fields();
    let mut profile = Profile::load(&path).expect("the A-10C default loads");
    let field = profile
        .readouts
        .iter_mut()
        .find(|r| !r.divider)
        .expect("a field to alias");
    field.content[0].value_aliases = [(0.0, "OFF"), (3.0, "SEMI")]
        .into_iter()
        .map(|(v, a)| (ValueBand::One(v), a.into()))
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
    let drawn = field.content[0]
        .value_aliases
        .get(&ValueBand::One(3.0))
        .map(|a| a.text.as_str());
    assert_eq!(drawn, Some("SEMI"));
}

#[test]
fn a_piece_with_no_aliases_writes_no_aliases_key() {
    // A key written for nothing would change every row that does not use it,
    // so the absence is asserted on a piece that has none rather than on a
    // whole default: the F-16 ships aliased rows now, and the promise was
    // never that nothing uses the key.
    let path = flat_fields();
    let profile = Profile::load(&path).expect("the A-10C default loads");
    for field in &profile.readouts {
        for span in &field.content {
            assert!(
                span.value_aliases.is_empty(),
                "the fixture is meant to have none"
            );
        }
    }
    let written = serde_json::to_string_pretty(&profile).expect("it serializes");
    assert!(!written.contains("value_aliases"), "{}", path.display());
}
