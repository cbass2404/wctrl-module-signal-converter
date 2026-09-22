//! Render checks against traffic captured from real hardware.
//!
//! `fixtures/ufc1_captured_states.json` holds display states taken off the wire
//! while SimAppPro drove a CarrierAce UFC in a live Hornet mission. Each state
//! carries the device buffer it produced and what our cell map says that buffer
//! reads. Rendering the second back into the first means agreeing with the
//! hardware rather than with ourselves.

use std::path::Path;

use serde::Deserialize;
use dsc_config::{DisplayCatalogue, Screen};

#[derive(Deserialize)]
struct Fixture {
    states: Vec<State>,
}

#[derive(Deserialize)]
struct State {
    name: String,
    /// What each of the 36 cells reads, in cell order.
    cells: Vec<String>,
    /// The whole device buffer, as hex bytes.
    buffer: String,
}

fn hex(s: &str) -> Vec<u8> {
    s.split_whitespace()
        .map(|b| u8::from_str_radix(b, 16).expect("fixture buffer is hex"))
        .collect()
}

fn load() -> (DisplayCatalogue, Fixture) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let cat = DisplayCatalogue::load_dir(&root.join("data/displays")).expect("displays load");
    let text = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ufc1_captured_states.json"),
    )
    .expect("fixture is readable");
    (cat, serde_json::from_str(&text).expect("fixture parses"))
}

#[test]
fn the_map_loads_and_describes_the_ufc() {
    let (cat, _) = load();
    let ufc = cat.get("UFC1").expect("UFC1 is in data/displays");
    assert_eq!(ufc.part_id, 0xbed0);
    assert_eq!(ufc.cells.len(), 36);
    assert_eq!(ufc.buffer_bytes, 96);
    assert_eq!(ufc.groups(), 24);
    // Found by part id too, which is how the daemon will reach it: a profile
    // names a device, and the device's parts are what carry displays.
    assert_eq!(cat.for_part(0xbed0).map(|d| d.key.as_str()), Some("UFC1"));
}

#[test]
fn rendering_a_captured_state_reproduces_the_bytes_the_device_was_sent() {
    let (cat, fixture) = load();
    let ufc = cat.get("UFC1").unwrap();
    assert!(!fixture.states.is_empty(), "fixture has states");

    for state in &fixture.states {
        let want = hex(&state.buffer);
        assert_eq!(want.len(), ufc.buffer_bytes, "{}: buffer size", state.name);
        assert_eq!(state.cells.len(), ufc.cells.len(), "{}: cell count", state.name);

        let mut screen = Screen::new(ufc);
        for (index, value) in state.cells.iter().enumerate() {
            screen
                .draw(ufc, index, value)
                .unwrap_or_else(|e| panic!("{}: cell {index} = {value:?}: {e}", state.name));
        }

        assert_eq!(
            screen.bytes(),
            want.as_slice(),
            "{}: rendered buffer differs from the captured one",
            state.name
        );
    }
}

#[test]
fn a_cell_only_takes_glyphs_its_shape_can_draw() {
    let (cat, _) = load();
    let ufc = cat.get("UFC1").unwrap();
    let mut screen = Screen::new(ufc);

    // Cell 10 is alphanumeric, cell 2 is a seven-segment digit.
    assert!(screen.draw(ufc, 10, "A").is_ok());
    assert!(
        screen.draw(ufc, 2, "A").is_err(),
        "a seven-segment cell cannot show a letter, and must say so rather \
         than blank itself"
    );
    assert!(screen.draw(ufc, 99, " ").is_err(), "there is no cell 99");
}

#[test]
fn drawing_over_a_cell_leaves_nothing_of_the_old_character() {
    let (cat, _) = load();
    let ufc = cat.get("UFC1").unwrap();

    let mut written = Screen::new(ufc);
    written.draw(ufc, 10, "8").unwrap();
    written.draw(ufc, 10, "1").unwrap();

    let mut fresh = Screen::new(ufc);
    fresh.draw(ufc, 10, "1").unwrap();

    assert_eq!(written, fresh, "a stroke of the 8 survived the overwrite");
}

#[test]
fn only_groups_that_moved_are_reported() {
    let (cat, _) = load();
    let ufc = cat.get("UFC1").unwrap();

    let blank = Screen::new(ufc);
    assert!(blank.changes_from(&blank).is_empty());

    let mut one = blank.clone();
    one.draw(ufc, 34, "1").unwrap();
    let changes = one.changes_from(&blank);
    assert!(!changes.is_empty(), "a drawn character changes something");
    assert!(
        changes.len() < ufc.groups(),
        "one character should not rewrite the whole display"
    );
    for (group, bytes) in &changes {
        assert!((*group as usize) < ufc.groups());
        assert_eq!(bytes.len(), ufc.group_bytes);
    }

    // A full paint is the resync path, and covers everything.
    assert_eq!(one.all_groups().len(), ufc.groups());
}

#[test]
fn a_multi_character_field_on_one_cell_is_looked_up_whole() {
    let (cat, _) = load();
    let ufc = cat.get("UFC1").unwrap();
    let cell = ufc.cell(34).unwrap();
    let table = &ufc.glyphs["alnum16"];

    // Observed live: DCS-BIOS reports UFC_COMM1_DISPLAY as two characters for
    // this one cell, and the pair is a glyph in its own right.
    let mut whole = ufc.glyph(cell, "12").expect("12 draws").clone();
    whole.sort_unstable();

    // SimAppPro's fallback for a pair it cannot find is to OR glyphs[v[0]] with
    // glyphs[' ' + v[1]] (LCDControl.js sendData). Copying that here would be a
    // mistake: the single character keys are the ordinary glyphs for cells 0 to
    // 33, and their slots land on this cell's units digit rather than its tens.
    let mut fused: Vec<u8> = table["1"].iter().chain(&table[" 2"]).copied().collect();
    fused.sort_unstable();
    fused.dedup();

    assert_ne!(
        whole, fused,
        "the pair is one glyph, not the union of its characters"
    );
}

#[test]
fn a_two_digit_comm_preset_draws_its_tens_digit() {
    let (cat, _) = load();
    let ufc = cat.get("UFC1").unwrap();

    // Cells 34 and 35 are two digits on one cell, and the vendor writes the
    // tens as a prefix character. DCS-BIOS writes it as a digit, so "12" fell
    // through the glyph table and blanked the cell: the symptom was a comm
    // preset that worked to 9 and then vanished.
    for (sent, spelled) in [("10", "`0"), ("12", "`2"), ("19", "`9"), ("20", "~0")] {
        let mut from_stream = Screen::new(ufc);
        from_stream.draw(ufc, 34, sent).expect("the stream spelling draws");

        let mut from_table = Screen::new(ufc);
        from_table.draw(ufc, 34, spelled).unwrap();

        assert_eq!(from_stream, from_table, "{sent} should draw as {spelled}");
        assert_ne!(
            from_stream,
            Screen::new(ufc),
            "{sent} must light something; blank is the bug"
        );
    }

    // The tens is what the prefix adds, and nothing else moves.
    let mut ten = Screen::new(ufc);
    ten.draw(ufc, 34, "12").unwrap();
    let mut two = Screen::new(ufc);
    two.draw(ufc, 34, " 2").unwrap();
    assert_ne!(ten, two, "12 and 2 are different readings");
}

#[test]
fn a_spelling_never_shadows_a_glyph_the_table_already_has() {
    let (cat, _) = load();
    let ufc = cat.get("UFC1").unwrap();
    // " 2" is in the table, so it is drawn as itself whatever the spellings
    // say. A rewrite is a rescue, not a redirection.
    let cell = ufc.cell(34).unwrap();
    assert_eq!(
        ufc.glyph(cell, " 2"),
        ufc.glyphs["alnum16"].get(" 2"),
        "a value the table knows is unaffected"
    );
}

#[test]
fn a_wide_cell_reads_the_same_channel_however_a_module_pads_it() {
    let (cat, _) = load();
    let ufc = cat.get("UFC1").unwrap();
    assert_eq!(ufc.cell(34).unwrap().width, 2, "the comm window holds two characters");
    assert_eq!(ufc.cell(10).unwrap().width, 1, "an option cell holds one");

    // The Hornet sends " 1" from an eight character field, the Hind sends "1"
    // from a one character field and "1 " from a two character one. All three
    // are channel 1, and only the first of them used to draw.
    let mut want = Screen::new(ufc);
    want.draw(ufc, 34, " 1").unwrap();

    for sent in ["1", " 1", "1 ", "  1"] {
        let mut got = Screen::new(ufc);
        got.draw(ufc, 34, sent)
            .unwrap_or_else(|e| panic!("{sent:?} should draw: {e}"));
        assert_eq!(got, want, "{sent:?} is channel 1 like any other spelling of it");
    }

    // Blank, however it is written.
    let blank = Screen::new(ufc);
    for sent in ["", " ", "  "] {
        let mut got = Screen::new(ufc);
        got.draw(ufc, 34, sent).unwrap_or_else(|e| panic!("{sent:?}: {e}"));
        assert_eq!(got, blank, "{sent:?} leaves the window dark");
    }
}

#[test]
fn a_bare_digit_on_a_wide_cell_is_not_taken_from_the_shared_table() {
    let (cat, _) = load();
    let ufc = cat.get("UFC1").unwrap();

    // '1' is a real entry, and on cells 0 to 33 it is the right one. On a comm
    // window its slots land on the units digit, so taking it would draw a
    // legible wrong answer. This is why the width is applied before the lookup
    // rather than as a rescue after it.
    let cell = ufc.cell(34).unwrap();
    assert_eq!(
        ufc.glyph(cell, "1"),
        ufc.glyphs["alnum16"].get(" 1"),
        "a bare digit is fitted to the units position"
    );
    assert_ne!(
        ufc.glyph(cell, "1"),
        ufc.glyphs["alnum16"].get("1"),
        "and is not the shared single character glyph"
    );

    // An ordinary cell reaches the same glyph by the other rule: it prefers
    // the spaced form of a single character rather than fitting to a width.
    let plain = ufc.cell(10).unwrap();
    assert_eq!(ufc.glyph(plain, "1"), ufc.glyphs["alnum16"].get(" 1"));
}

#[test]
fn a_long_value_on_a_wide_cell_keeps_its_last_digits() {
    let (cat, _) = load();
    let ufc = cat.get("UFC1").unwrap();
    // Two cells wide and three characters given. Dropping the leading digit is
    // the only choice that keeps a number readable, and it is what the right
    // aligned scratchpad does across a run of cells.
    let mut got = Screen::new(ufc);
    got.draw(ufc, 34, "112").unwrap();
    let mut want = Screen::new(ufc);
    want.draw(ufc, 34, "12").unwrap();
    assert_eq!(got, want);
}

#[test]
fn an_ordinary_cell_prefers_the_spaced_form_of_a_digit() {
    let (cat, _) = load();
    let ufc = cat.get("UFC1").unwrap();
    let cell = ufc.cell(0).unwrap();
    let table = &ufc.glyphs["alnum16"];

    // A digit has two forms on an ordinary cell and they are different glyphs.
    // The captured COMM page has cell 0 reading " 3", which is the only digit
    // ever seen on such a cell, so the spaced form is what a bare digit means
    // here too.
    assert_ne!(table.get(" 3"), table.get("3"), "two forms, not one");
    assert_eq!(ufc.glyph(cell, " 3"), table.get(" 3"));
    assert_eq!(ufc.glyph(cell, "3"), table.get(" 3"), "a bare digit is spaced");
    assert_eq!(ufc.glyph(cell, "3 "), table.get(" 3"), "whichever side it pads");
}

#[test]
fn a_padded_letter_falls_back_to_the_only_form_there_is() {
    let (cat, _) = load();
    let ufc = cat.get("UFC1").unwrap();
    let cell = ufc.cell(0).unwrap();
    let table = &ufc.glyphs["alnum16"];

    // DCS-BIOS pads a string to its max_length and DCS's own indication does
    // not, so UFC_SCRATCHPAD_STRING_1_DISPLAY arrives as " G" for a cell that
    // holds one character. A letter has no spaced form, so the trim fallback
    // is the only thing standing between this and a dark cell.
    assert!(table.get(" G").is_none(), "there is no spaced letter form");
    assert_eq!(ufc.glyph(cell, " G"), table.get("G"));

    let mut drawn = Screen::new(ufc);
    drawn.draw(ufc, 0, " G").expect("a padded letter draws");
    assert_ne!(drawn, Screen::new(ufc), "and it is not blank");

    // A seven-segment cell gets the same rescue, having no spaced digits.
    let seven = ufc.cell(2).unwrap();
    assert_eq!(ufc.glyph(seven, " 3"), ufc.glyphs["digit7"].get("3"));
}

#[test]
fn a_wide_cell_does_not_get_the_trim_fallback() {
    let (cat, _) = load();
    let ufc = cat.get("UFC1").unwrap();
    let cell = ufc.cell(34).unwrap();
    let table = &ufc.glyphs["alnum16"];

    // The opposite rule, and it has to be the opposite. Trimming "1 " down to
    // "1" would find the ordinary single character glyph, whose slots land on
    // this cell's units digit. Fitting to the width finds " 1" instead.
    assert_eq!(ufc.glyph(cell, "1 "), table.get(" 1"));
    assert_ne!(ufc.glyph(cell, "1 "), table.get("1"));
}

#[test]
fn a_letter_reaches_the_glass_on_either_kind_of_cell() {
    let (cat, _) = load();
    let ufc = cat.get("UFC1").unwrap();
    let table = &ufc.glyphs["alnum16"];

    // DCS-BIOS pads to max_length, so a letter arrives spaced and no spaced
    // letter exists. Both kinds of cell fall back to the bare form, and a
    // guard channel on a comm window is the case that needs the wide path to
    // do it: fitting " g" to two characters just gives " g" back.
    assert!(table.get(" g").is_none() && table.get(" G").is_none());
    for (cell, sent, want) in [(0usize, " G", "G"), (34, " g", "G"), (10, " G", "G")] {
        let c = ufc.cell(cell).unwrap();
        assert_eq!(ufc.glyph(c, sent), table.get(want), "cell {cell} sent {sent:?}");
        let mut drawn = Screen::new(ufc);
        drawn.draw(ufc, cell, sent).unwrap_or_else(|e| panic!("cell {cell}: {e}"));
        assert_ne!(drawn, Screen::new(ufc), "cell {cell} must not go dark");
    }

    // And the three forms a module actually sends to a comm window.
    let comm = ufc.cell(34).unwrap();
    assert_eq!(ufc.glyph(comm, " 1"), table.get(" 1"));
    assert_eq!(ufc.glyph(comm, "19"), table.get("`9"));
    assert_eq!(ufc.glyph(comm, " g"), table.get("G"));
}

#[test]
fn a_letter_is_drawn_as_a_capital() {
    let (cat, _) = load();
    let ufc = cat.get("UFC1").unwrap();
    let table = &ufc.glyphs["alnum16"];
    let cell = ufc.cell(10).unwrap();

    // The panel was built for the Hornet and DCS-BIOS reports the Hornet UFC
    // in capitals throughout, so a module that names a guard channel "g"
    // should reach the glass looking like the rest of the panel. The small
    // forms are real and different, which is exactly why this has to be a
    // decision rather than an accident.
    assert!(table.get("g").is_some() && table.get("g") != table.get("G"));
    assert_eq!(ufc.glyph(cell, "g"), table.get("G"));
    assert_eq!(ufc.glyph(cell, " g"), table.get("G"));
    assert_eq!(ufc.glyph(ufc.cell(34).unwrap(), " g"), table.get("G"));

    // Five letters have no small form at all, so this is the only thing that
    // keeps them off a dark cell either way.
    for c in ["r", "u", "w", "y", "z"] {
        assert!(table.get(c).is_none(), "{c} has no small form, or this test is stale");
        assert_eq!(ufc.glyph(cell, c), table.get(&c.to_uppercase()));
    }
}

#[test]
fn a_glyph_with_no_capital_is_still_reachable() {
    let (cat, _) = load();
    let ufc = cat.get("UFC1").unwrap();
    let seven = ufc.cell(2).unwrap();
    let table = &ufc.glyphs["digit7"];

    // digit7 has a 'p' and a 'w' and no letters in capitals at all. Uppercasing
    // without falling back to the value as sent would have taken both off the
    // glass, which is why the second pass exists.
    for c in ["p", "w"] {
        assert!(table.get(&c.to_uppercase()).is_none(), "{c} has no capital");
        assert_eq!(ufc.glyph(seven, c), table.get(c), "{c} still draws");
    }
}

#[test]
fn every_cell_belongs_to_exactly_one_named_region() {
    let (cat, _) = load();
    let ufc = cat.get("UFC1").unwrap();
    assert!(!ufc.regions.is_empty(), "the editor has nothing to offer otherwise");

    let mut owner: Vec<Option<&str>> = vec![None; ufc.cells.len()];
    for region in &ufc.regions {
        let run: CellRange = region
            .cells
            .parse()
            .unwrap_or_else(|e| panic!("region {:?} has cells {:?}: {e}", region.name, region.cells));
        assert!(
            run.last < ufc.cells.len(),
            "region {:?} runs off the end of the display",
            region.name
        );
        for cell in run.first..=run.last {
            assert!(
                owner[cell].is_none(),
                "cell {cell} is in both {:?} and {:?}",
                owner[cell].unwrap(),
                region.name
            );
            owner[cell] = Some(&region.name);
        }
    }

    // A gap is worse than an overlap: it is a cell nobody can reach from the
    // editor at all, and nothing else would ever report it.
    let missing: Vec<usize> = owner
        .iter()
        .enumerate()
        .filter(|(_, o)| o.is_none())
        .map(|(i, _)| i)
        .collect();
    assert!(missing.is_empty(), "cells in no region: {missing:?}");

    // A region is only useful if it says what it is.
    for region in &ufc.regions {
        assert!(!region.name.trim().is_empty(), "a region needs a name");
    }
}

#[test]
fn the_shipped_hornet_fields_land_on_named_regions() {
    let (cat, _) = load();
    let ufc = cat.get("UFC1").unwrap();
    let text = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/defaults/fa-18.json"),
    )
    .expect("the shipped Hornet default is readable");
    let profile: serde_json::Value = serde_json::from_str(&text).expect("it parses");

    let named: Vec<&str> = ufc.regions.iter().map(|r| r.cells.as_str()).collect();
    let readouts = profile["readouts"].as_array().expect("it has readouts");
    assert!(!readouts.is_empty());
    for r in readouts {
        // The UFC's regions, so only the UFC's fields. The Hornet has nothing
        // else on it today, but it has a screen the editor can write to and a
        // row put there would otherwise fail this for having no UFC region.
        if r["display"].as_str() != Some("UFC1") {
            continue;
        }
        let cells = r["cells"].as_str().unwrap();
        assert!(
            named.contains(&cells),
            "shipped field on cells {cells} matches no region, so the editor would show it as a custom range"
        );
    }
}

// ------------------------------------------------------- readouts

use std::collections::BTreeMap;

use dsc_config::{
    divider_rule, divider_text, min_divider_cells, AliasDraw, Align, CellRange, Colour, Reading,
    Readout, Round, Span, ValueBand,
};

fn readout(cells: &str, source: &str) -> Readout {
    Readout::reading(
        "CarrierAce_UFC",
        "UFC1",
        cells.parse::<CellRange>().expect("a cell run parses"),
        source,
    )
}

/// The one span of a field written the ordinary way, to set what shapes it.
fn span(r: &mut Readout) -> &mut Span {
    r.content.first_mut().expect("a field has a span")
}

/// What the field puts on its cells when the signal it reads says `value`.
///
/// Goes through `compose`, so these are the glyphs the panel would be sent
/// rather than the output of a layout step looked at on its own.
fn drawn(r: &Readout, value: &str) -> Vec<String> {
    r.compose(|_| Some(Reading::Text(value.to_string())))
        .expect("the signal has arrived")
        .into_iter()
        .map(|g| g.text)
        .collect()
}

#[test]
fn a_cell_run_reads_and_writes_the_way_it_is_written() {
    let one: CellRange = "34".parse().unwrap();
    assert_eq!((one.first, one.last, one.len()), (34, 34, 1));
    assert_eq!(one.to_string(), "34");

    let run: CellRange = "2-8".parse().unwrap();
    assert_eq!((run.first, run.last, run.len()), (2, 8, 7));
    assert_eq!(run.to_string(), "2-8");

    assert!("8-2".parse::<CellRange>().is_err(), "a run cannot end before it starts");
    assert!("two".parse::<CellRange>().is_err());
}

#[test]
fn a_divider_rules_every_cell_of_its_run() {
    // The shape the panel gets: an unbroken line, corner to corner. Spaced
    // dashes read as a dotted line on the glass. It was inset by a blank at
    // each end until it was flown beside real CDU lines, which start in the
    // first cell of their run and left the rule the one thing out of line.
    assert_eq!(divider_text(9, ""), "---------");
    assert_eq!(divider_text(1, ""), "-");
    assert_eq!(divider_text(24, ""), "------------------------");
    // Every rule is exactly as wide as the run it was asked for, so a divider
    // can never spill into the field beside it.
    for width in 1..40 {
        let rule = divider_rule(width, "");
        assert_eq!(rule.len(), width);
        assert!(rule.iter().all(|c| c.text == "-"), "{width} cells");
        assert!(rule.iter().all(|c| !c.label), "{width} cells: nothing is a label");
    }
}

#[test]
fn a_divider_draws_its_own_run_whatever_the_field_says() {
    let mut r = readout("2-10", "IGNORED");
    r.divider = true;
    assert_eq!(r.divider_cells().into_iter().map(|c| c.text).collect::<String>(), "---------");
}

// --------------------------------------------------------------- rule labels

#[test]
fn a_label_sits_in_the_middle_of_the_rule_with_a_blank_each_side() {
    // The blanks are what keep the label from reading as part of the line.
    // They are the only blanks a rule draws: the line itself runs to the edge.
    assert_eq!(divider_text(14, "FUEL"), "---- FUEL ----");
    // An odd number of dashes puts the extra one on the left, the way a gap
    // gives its remainder to the earlier side.
    assert_eq!(divider_text(15, "FUEL"), "----- FUEL ----");
    // The narrowest a label can be drawn in: one dash each side.
    assert_eq!(divider_text(min_divider_cells("FUEL"), "FUEL"), "- FUEL -");
}

#[test]
fn a_labelled_rule_is_still_exactly_as_wide_as_its_run() {
    // The one thing a rule may never do is spill into the field beside it.
    for label in ["A", "FUEL", "WAYPOINT"] {
        for width in min_divider_cells(label)..48 {
            let rule = divider_rule(width, label);
            assert_eq!(rule.len(), width, "{label} in {width} cells");
            assert_eq!(rule[0].text, "-", "{label} in {width}: the line starts at the edge");
            assert_eq!(rule[width - 1].text, "-", "{label} in {width}: and ends at it");
            let drawn: String = rule.iter().filter(|c| c.label).map(|c| c.text.as_str()).collect();
            assert_eq!(drawn, label, "{label} in {width} cells is drawn whole");
            // The label is one run, not scattered, and has a blank each side.
            let at = rule.iter().position(|c| c.label).expect("the label is drawn");
            assert_eq!(rule[at - 1].text, " ", "{label} in {width}: a blank before it");
            assert_eq!(rule[at + label.chars().count()].text, " ", "{label} in {width}: a blank after it");
            assert!(rule[..at - 1].iter().all(|c| c.text == "-"), "{label} in {width}");
            let after = at + label.chars().count() + 1;
            assert!(rule[after..].iter().all(|c| c.text == "-"), "{label} in {width}");
            assert!(after < width, "{label} in {width}: a dash is left on the right");
        }
    }
}

#[test]
fn a_label_with_no_room_leaves_a_plain_rule() {
    // Refused by `problems`, which is where the user is told. Drawing it anyway
    // would mean crowding the line or running past the run, and a rule that
    // quietly loses its margins looks like a fault on the glass.
    for width in 0..min_divider_cells("FUEL") {
        let rule = divider_rule(width, "FUEL");
        assert_eq!(rule.len(), width, "{width} cells");
        assert!(rule.iter().all(|c| !c.label), "{width} cells: no label is drawn");
    }
}

#[test]
fn a_label_takes_its_own_colour_and_leaves_the_rule_its_own() {
    // The point of a label is that it is not the line, so a label drawn in the
    // line's colour is the one thing this should not quietly do.
    let mut r = readout("0-13", "IGNORED");
    r.divider = true;
    r.colour = Some(Colour::Green);
    r.label = "FUEL".to_string();
    r.label_colour = Some(Colour::Amber);
    let drawn = r.compose(|_| None).expect("a rule reads nothing and always draws");
    let colours: Vec<Option<Colour>> = drawn.iter().map(|g| g.colour).collect();
    let label: Vec<Option<Colour>> = drawn
        .iter()
        .filter(|g| g.text.chars().all(|c| c.is_ascii_alphabetic()) && !g.text.is_empty())
        .map(|g| g.colour)
        .collect();
    assert_eq!(label, vec![Some(Colour::Amber); 4], "the label is drawn in its own colour");
    assert!(
        colours.iter().filter(|c| **c == Some(Colour::Green)).count() > 4,
        "the rest of the rule keeps the rule's colour"
    );
}

#[test]
fn a_label_nobody_coloured_follows_the_rule() {
    // A label added to a rule that already had a colour should look like part
    // of the same thing until somebody says otherwise.
    let mut r = readout("0-13", "IGNORED");
    r.divider = true;
    r.colour = Some(Colour::Green);
    r.label = "FUEL".to_string();
    let drawn = r.compose(|_| None).expect("a rule always draws");
    assert!(
        drawn.iter().all(|g| g.colour == Some(Colour::Green)),
        "every cell of it is the rule's colour"
    );
}

#[test]
fn runs_overlap_only_when_they_share_a_cell() {
    let a: CellRange = "2-8".parse().unwrap();
    assert!(a.overlaps(&"8-9".parse().unwrap()), "they share cell 8");
    assert!(a.overlaps(&"0-2".parse().unwrap()), "they share cell 2");
    assert!(!a.overlaps(&"9-13".parse().unwrap()));
    assert!(!a.overlaps(&"0-1".parse().unwrap()));
}

#[test]
fn a_gauge_is_converted_between_the_values_its_face_is_marked_with() {
    let mut r = readout("2-8", "OIL_TEMP");
    span(&mut r).reads = Some([0.0, 300.0]);
    let s = span(&mut r).clone();
    assert_eq!(s.format_number(0, 65535), "0");
    assert_eq!(s.format_number(65535, 65535), "300");
    assert_eq!(s.format_number(32767, 65535), "150");
}

#[test]
fn a_gauge_that_reads_below_zero_converts_too() {
    // A g meter does not start at zero, and the conversion interpolates
    // between the two ends of the face rather than scaling up from nothing.
    let mut r = readout("2-8", "ACCEL_G");
    span(&mut r).reads = Some([-10.0, 12.0]);
    span(&mut r).decimals = 1;
    let s = span(&mut r).clone();
    assert_eq!(s.format_number(0, 65535), "-10.0");
    assert_eq!(s.format_number(65535, 65535), "12.0");
    // Level flight is 1 g, which sits 11/22 of the way up a -10..12 face.
    let one_g = (11.0f64 / 22.0 * 65535.0).round() as u16;
    assert_eq!(s.format_number(one_g, 65535), "1.0");
}

#[test]
fn a_gauge_whose_face_runs_backwards_converts_too() {
    let mut r = readout("2-8", "BACKWARDS");
    span(&mut r).reads = Some([100.0, 0.0]);
    let s = span(&mut r).clone();
    assert_eq!(s.format_number(0, 65535), "100");
    assert_eq!(s.format_number(65535, 65535), "0");
}

#[test]
fn a_selector_whose_value_is_already_the_number_needs_no_special_case() {
    // A-10C TACAN_1 has max_value 10 and its value is the digit. Giving it its
    // own range makes the conversion an identity, through the same arithmetic
    // a needle uses.
    let mut r = readout("2", "TACAN_1");
    span(&mut r).reads = Some([0.0, 10.0]);
    let s = span(&mut r).clone();
    for n in 0..=10u16 {
        assert_eq!(s.format_number(n, 10), n.to_string());
    }
}

/// Where DCS-BIOS puts a float of 0 to 1, as the Lua export does: the nearest
/// step, not the exact position.
fn raw(position: f64) -> u16 {
    (position * 65535.0).round() as u16
}

/// One odometer drum: a full turn is the digits 0 to 9, drawn as they click
/// over.
fn drum(r: &mut Readout) -> Span {
    span(r).reads = Some([0.0, 10.0]);
    span(r).wrap = Some(10.0);
    span(r).round = Round::Down;
    span(r).clone()
}

#[test]
fn a_drum_draws_every_digit_it_sits_on_including_zero() {
    // The F-16 fuel totalizer drums, among many. Sitting on digit 7 is 0.7 of
    // a turn, which DCS-BIOS sends as 45875 and which converts to a hair under
    // 7. Rounding down without allowing for that drew a 6.
    let mut r = readout("2", "FUELTOTALIZER_1K");
    let s = drum(&mut r);
    for digit in 0..10u16 {
        assert_eq!(
            s.format_number(raw(f64::from(digit) / 10.0), 65535),
            digit.to_string(),
            "digit {digit}"
        );
    }
    // A full turn is back on 0, drawn as 0 rather than 10 or nothing.
    assert_eq!(s.format_number(65535, 65535), "0");
}

#[test]
fn a_drum_between_digits_draws_the_one_it_has_left() {
    let mut r = readout("2", "FUELTOTALIZER_1K");
    let s = drum(&mut r);
    // Most of the way from 4 to 5 is still 4 on a drum. Rounded to the
    // nearest it read 5 while the drum below it still read 9.
    assert_eq!(s.format_number(raw(0.48), 65535), "4");
    assert_eq!(s.format_number(raw(0.99), 65535), "9");
}

#[test]
fn a_reading_that_goes_round_more_than_once_draws_where_it_is_in_the_turn() {
    // Twelve turns of 0 to 999 on one signal: the three digits a needle or a
    // counter shows, not the running total behind them.
    let mut r = readout("2-4", "MULTI_TURN");
    span(&mut r).reads = Some([0.0, 12000.0]);
    span(&mut r).wrap = Some(1000.0);
    let s = span(&mut r).clone();
    assert_eq!(s.format_number(0, 65535), "0");
    assert_eq!(s.format_number(raw(0.5), 65535), "0", "6000 is a whole turn");
    assert_eq!(s.format_number(raw(7250.0 / 12000.0), 65535), "250");
    assert_eq!(span(&mut r).widest(None, Some(65535)), Some(3));
}

#[test]
fn a_full_scale_wraps_to_zero_at_the_top_after_rounding() {
    let mut r = readout("2-4", "HEADING");
    span(&mut r).reads = Some([0.0, 360.0]);
    span(&mut r).wrap = Some(360.0);
    let s = span(&mut r).clone();
    assert_eq!(s.format_number(65535, 65535), "0");
    // 359.7 rounds to 360 first, which is 0, rather than drawing 360.
    assert_eq!(s.format_number(raw(359.7 / 360.0), 65535), "0");
    assert_eq!(s.format_number(raw(0.5), 65535), "180");
    assert_eq!(span(&mut r).widest(None, Some(65535)), Some(3));
}

#[test]
fn zero_is_never_drawn_as_minus_zero() {
    let mut r = readout("2-8", "ACCEL_G");
    span(&mut r).reads = Some([-10.0, 12.0]);
    span(&mut r).decimals = 1;
    let s = span(&mut r).clone();
    // -0.01 g, which the formatter alone drew as "-0.0".
    let near_zero = raw(9.99 / 22.0);
    assert_eq!(s.format_number(near_zero, 65535), "0.0");
}

#[test]
fn a_drum_round_trips_flat_and_a_plain_reading_gains_no_keys() {
    let mut r = readout("2", "FUELTOTALIZER_1K");
    drum(&mut r);
    let json = serde_json::to_value(&r).unwrap();
    assert_eq!(json["wrap"], 10.0);
    assert_eq!(json["round"], "down");
    assert!(json.get("content").is_none(), "one drum stays in the flat shape");
    let back: Readout = serde_json::from_value(json).unwrap();
    assert_eq!(back.content[0].wrap, Some(10.0));
    assert_eq!(back.content[0].round, Round::Down);

    let plain = serde_json::to_value(readout("2", "OIL_TEMP")).unwrap();
    assert!(plain.get("wrap").is_none() && plain.get("round").is_none());
}

#[test]
fn a_right_aligned_field_drops_its_leading_pad_not_its_last_digit() {
    let mut r = readout("2-8", "UFC_SCRATCHPAD_NUMBER_DISPLAY");
    r.align = Align::Right;
    // DCS-BIOS gives 8 characters for 7 cells. Observed live.
    assert_eq!(drawn(&r, " 264.000").concat(), "264.000");
    // And a short value is pushed to the right, which is where keypresses land.
    assert_eq!(drawn(&r, "1").concat(), "      1");
}

#[test]
fn a_left_aligned_field_pads_on_the_right() {
    let r = readout("10-13", "UFC_OPTION_DISPLAY_1");
    assert_eq!(drawn(&r, "AM").concat(), "AM  ");
    assert_eq!(drawn(&r, "GRCV").concat(), "GRCV");
    assert_eq!(drawn(&r, "TOOLONG").concat(), "TOOL");
}

#[test]
fn a_shrinking_field_blanks_the_cells_it_gives_up() {
    let r = readout("10-13", "UFC_OPTION_DISPLAY_1");
    let now = drawn(&r, "AM");
    assert_eq!(now.len(), 4, "every cell is written, not just the used ones");
    assert_eq!(now[2], " ");
    assert_eq!(now[3], " ");
}

#[test]
fn a_single_cell_field_is_laid_out_as_one_whole_glyph() {
    let r = readout("34", "UFC_COMM1_DISPLAY");
    // Not [" ", "2"]: the pair is one glyph on one cell, which is how the
    // hardware was observed being driven.
    assert_eq!(drawn(&r, " 2"), vec![" 2".to_string()]);
}

#[test]
fn an_alias_rewrites_a_value_the_glyph_table_does_not_know() {
    let mut r = readout("1", "UFC_SCRATCHPAD_STRING_2_DISPLAY");
    span(&mut r).aliases.insert("--".into(), "_".into());
    // DCS-BIOS says "--" where DCS's own indication says "_", and "--" is not
    // a glyph. Without this the cell would fall back to a single dash.
    assert_eq!(drawn(&r, "--"), vec!["_".to_string()]);
    assert_eq!(
        drawn(&r, " 2"),
        vec![" 2".to_string()],
        "anything not aliased passes through"
    );
}

#[test]
fn a_chain_draws_its_pieces_end_to_end() {
    // The case the chain exists for: a label the user typed, the reading
    // beside it, and a unit after it, all on one run of cells.
    let mut r = readout("10-13", "RALT");
    r.content = vec![
        Span { text: "R".into(), ..Span::default() },
        Span { source: "RALT".into(), ..Span::default() },
        Span { text: "M".into(), ..Span::default() },
    ];
    assert_eq!(drawn(&r, "25").concat(), "R25M");
}

#[test]
fn a_chain_longer_than_its_run_loses_the_end_and_says_nothing() {
    // Nothing refuses this and nothing on the panel shows it happened, which
    // is why the editor works the width out ahead of time.
    let mut r = readout("10-13", "RALT");
    r.content = vec![
        Span { text: "RALT".into(), ..Span::default() },
        Span { source: "RALT".into(), ..Span::default() },
    ];
    assert_eq!(drawn(&r, "250").concat(), "RALT");
}

#[test]
fn a_chain_draws_what_has_arrived_while_the_rest_is_still_coming() {
    // A label belongs on the glass before the reading beside it, so one span
    // still waiting does not hold back the ones that are ready.
    let mut r = readout("10-13", "RALT");
    r.content = vec![
        Span { text: "R".into(), ..Span::default() },
        Span { source: "RALT".into(), ..Span::default() },
    ];
    let glyphs = r.compose(|_| None).expect("the literal piece is ready");
    let text: String = glyphs.iter().map(|g| g.text.as_str()).collect();
    assert_eq!(text, "R   ", "the label is drawn and the rest is blank");
}

#[test]
fn a_field_with_nothing_but_an_unread_signal_leaves_its_cells_alone() {
    // None rather than a run of blanks: writing spaces over a cell is not the
    // same as not writing it, and a field that has never read anything has
    // nothing to say about what is there.
    let r = readout("10-13", "RALT");
    assert!(r.compose(|_| None).is_none());
}

#[test]
fn each_piece_of_a_chain_keeps_its_own_colour_and_size() {
    let mut r = readout("10-13", "RALT");
    r.content = vec![
        Span {
            text: "R".into(),
            colour: Some(dsc_config::Colour::Red),
            small: true,
            ..Span::default()
        },
        Span {
            source: "RALT".into(),
            colour: Some(dsc_config::Colour::Green),
            ..Span::default()
        },
    ];
    let glyphs = r.compose(|_| Some(Reading::Text("25".into()))).unwrap();
    assert_eq!(glyphs[0].colour, Some(dsc_config::Colour::Red));
    assert!(glyphs[0].small, "the label is small and the reading is not");
    assert_eq!(glyphs[1].colour, Some(dsc_config::Colour::Green));
    assert!(!glyphs[1].small);
}

// ------------------------------------------------------- boxes

/// A field of one boxed reading, to watch what the box does on its own.
fn boxed(cells: &str, width: usize, align: Align) -> Readout {
    let mut r = readout(cells, "RALT");
    let s = span(&mut r);
    s.width = width;
    s.align = align;
    r
}

#[test]
fn a_centred_box_keeps_the_value_in_the_middle_as_it_shrinks() {
    // The whole point, in the shape it was asked for: a reading counting down
    // through four widths, drawn in the same eight cells every time.
    let r = boxed("10-17", 8, Align::Centre);
    assert_eq!(drawn(&r, "1000").concat(), "  1000  ");
    assert_eq!(drawn(&r, "900").concat(), "   900  ");
    assert_eq!(drawn(&r, "90").concat(), "   90   ");
    assert_eq!(drawn(&r, "9").concat(), "    9   ");
}

#[test]
fn a_right_aligned_box_pins_the_digits_and_grows_the_blanks_in_front() {
    // What a number actually wants. Centring moves both edges in half a cell
    // at a time, which is right for a label and reads as drift on a reading.
    let r = boxed("10-17", 8, Align::Right);
    assert_eq!(drawn(&r, "1000").concat(), "    1000");
    assert_eq!(drawn(&r, "900").concat(), "     900");
    assert_eq!(drawn(&r, "90").concat(), "      90");
    assert_eq!(drawn(&r, "9").concat(), "       9");
}

#[test]
fn a_box_keeps_what_follows_it_from_moving() {
    // The reason a box beats the field's own alignment: this piece is in the
    // middle of a chain, so without one the unit would walk left every time
    // the reading lost a character.
    let mut r = readout("10-19", "RALT");
    r.content = vec![
        Span { text: "R".into(), ..Span::default() },
        Span { source: "RALT".into(), width: 8, align: Align::Centre, ..Span::default() },
        Span { text: "M".into(), ..Span::default() },
    ];
    for value in ["1000", "900", "90", "9"] {
        let cells = drawn(&r, value);
        assert_eq!(cells[0], "R", "{value}");
        assert_eq!(cells[9], "M", "{value}: the unit has not moved");
    }
}

#[test]
fn a_box_crops_from_the_end_its_alignment_anchors_away_from() {
    // The same bargain the field makes with its run. A right aligned box is
    // what a scratchpad wants: it drops its leading pad, not its last digit.
    let left = boxed("10-13", 4, Align::Left);
    assert_eq!(drawn(&left, "123456").concat(), "1234");
    let right = boxed("10-13", 4, Align::Right);
    assert_eq!(drawn(&right, "123456").concat(), "3456");
    // Centred, the odd cell comes off the front, the way its padding is added.
    let centre = boxed("10-13", 4, Align::Centre);
    assert_eq!(drawn(&centre, "123456").concat(), "2345");
    assert_eq!(drawn(&centre, "1234567").concat(), "3456");
}

#[test]
fn a_box_holds_its_cells_before_the_reading_has_arrived() {
    // Room held, not content drawn. The piece beside it is on the glass from
    // the first frame and does not jump when the signal turns up.
    let mut r = readout("10-19", "RALT");
    r.content = vec![
        Span { source: "RALT".into(), width: 8, align: Align::Right, ..Span::default() },
        Span { text: "M".into(), ..Span::default() },
    ];
    let glyphs = r.compose(|s| (s != "RALT").then(|| Reading::Text(String::new())));
    let text: String = glyphs.expect("the unit is ready").iter().map(|g| g.text.as_str()).collect();
    assert_eq!(text, "        M ");
}

#[test]
fn a_field_of_nothing_but_held_room_still_leaves_its_cells_alone() {
    // Blanks from a box are not a reason to write over the glass: a field with
    // nothing else on it has said nothing yet, box or no box.
    let r = boxed("10-17", 8, Align::Right);
    assert!(r.compose(|_| None).is_none());
}

#[test]
fn a_centred_field_sits_in_the_middle_of_its_run() {
    // The same arithmetic one level up, so a field and a box put the odd cell
    // on the same side and the screen lines up with itself.
    let mut r = readout("10-17", "RALT");
    r.align = Align::Centre;
    assert_eq!(drawn(&r, "1000").concat(), "  1000  ");
    assert_eq!(drawn(&r, "900").concat(), "   900  ");
}

// ------------------------------------------------------- rules in a chain

#[test]
fn a_rule_fills_the_room_between_two_pieces() {
    // What used to be three fields with hand counted cells. The rule is a gap,
    // so it is measured last and takes whatever the two ends leave.
    let mut r = readout("10-19", "RALT");
    r.content = vec![
        Span { text: "NAV".into(), ..Span::default() },
        Span { gap: true, rule: true, ..Span::default() },
        Span { source: "RALT".into(), ..Span::default() },
    ];
    assert_eq!(drawn(&r, "250").concat(), "NAV----250");
    // And the rule is what gives way when the reading grows, rather than the
    // reading being pushed off the end.
    assert_eq!(drawn(&r, "2500").concat(), "NAV---2500");
}

#[test]
fn a_boxed_rule_carries_a_label_the_way_a_divider_does() {
    // The same function draws both, so a rule in a chain cannot drift from the
    // one a whole field draws.
    let mut r = readout("10-18", "RALT");
    r.content = vec![
        Span { text: "A".into(), ..Span::default() },
        Span { gap: true, rule: true, width: 8, label: "FUEL".into(), ..Span::default() },
    ];
    assert_eq!(drawn(&r, "").concat(), "A- FUEL -");
    assert_eq!(divider_text(8, "FUEL"), "- FUEL -");
}

#[test]
fn a_rule_in_a_chain_is_drawn_before_anything_has_been_read() {
    // A rule reads nothing, so it belongs on the glass from the moment the
    // aircraft loads, exactly like the divider it is a piece of.
    let mut r = readout("10-19", "RALT");
    r.content = vec![
        Span { gap: true, rule: true, width: 4, ..Span::default() },
        Span { source: "RALT".into(), ..Span::default() },
    ];
    let glyphs = r.compose(|_| None).expect("the rule is ready");
    let text: String = glyphs.iter().map(|g| g.text.as_str()).collect();
    assert_eq!(text, "----      ");
}

#[test]
fn a_rule_and_its_label_keep_their_own_colours() {
    // A label drawn in the line's colour reads as part of the line, and unset
    // it follows the rule rather than arriving white.
    let mut r = readout("10-18", "RALT");
    r.content = vec![Span {
        gap: true,
        rule: true,
        width: 9,
        label: "FUEL".into(),
        colour: Some(Colour::Green),
        label_colour: Some(Colour::Red),
        ..Span::default()
    }];
    let glyphs = r.compose(|_| None).expect("a rule needs nothing to arrive");
    let text: String = glyphs.iter().map(|g| g.text.as_str()).collect();
    assert_eq!(text, "-- FUEL -");
    for (n, g) in glyphs.iter().enumerate() {
        let want = if g.text == "F" || g.text == "U" || g.text == "E" || g.text == "L" {
            Some(Colour::Red)
        } else {
            Some(Colour::Green)
        };
        assert_eq!(g.colour, want, "cell {n} draws {:?}", g.text);
    }
}

/// Aliases naming one reading each, which is how every alias written before
/// bands existed reads.
fn naming<const N: usize>(pairs: [(f64, &str); N]) -> BTreeMap<ValueBand, AliasDraw> {
    pairs
        .into_iter()
        .map(|(v, a)| (ValueBand::One(v), a.into()))
        .collect()
}

#[test]
fn a_knob_draws_the_alias_for_its_position() {
    let mut r = readout("2-5", "CMDS_MODE_KNB");
    span(&mut r).value_aliases = naming([(0.0, "OFF"), (3.0, "SEMI")]);
    let at = |value: u16| -> String {
        r.compose(|_| Some(Reading::Number { value, max: 5 }))
            .expect("the signal has arrived")
            .into_iter()
            .map(|g| g.text)
            .collect()
    };
    assert_eq!(at(3), "SEMI");
    assert_eq!(at(0), "OFF ");
    // A position with no alias still draws, as the number it is.
    assert_eq!(at(4), "4   ");
}

#[test]
fn a_knob_is_as_wide_as_its_longest_alias() {
    let mut r = readout("2-5", "CMDS_MODE_KNB");
    let names = ["OFF", "STBY", "MAN", "SEMI", "AUTO", "BYP"];
    span(&mut r).value_aliases = names
        .iter()
        .enumerate()
        .map(|(v, a)| (ValueBand::One(v as f64), (*a).into()))
        .collect();
    assert_eq!(span(&mut r).widest(None, Some(5)), Some(4));
    // With one position left as a number, that number still counts.
    span(&mut r).value_aliases = naming([
        (0.0, "O"),
        (1.0, "S"),
        (2.0, "M"),
        (3.0, "S"),
        (4.0, "A"),
    ]);
    assert_eq!(span(&mut r).widest(None, Some(500)), Some(3));
}

/// Bands as a profile writes them, parsed the way the file is read.
fn banded<const N: usize>(pairs: [(&str, &str); N]) -> BTreeMap<ValueBand, AliasDraw> {
    pairs
        .into_iter()
        .map(|(band, drawn)| (band.parse().expect("a band parses"), drawn.into()))
        .collect()
}

/// The F-16 trim indicator: a needle at 0 to 65535, a face marked -1.5 to 1.5,
/// and the bands written in the units the face is marked with.
fn trim_face(r: &mut Readout) -> Span {
    let s = span(r);
    s.reads = Some([-1.5, 1.5]);
    s.decimals = 1;
    s.value_aliases = banded([("-1.5..-0.1", "ND"), ("0", " "), ("0.1..1.5", "NU")]);
    s.clone()
}

#[test]
fn a_band_is_matched_against_what_the_face_reads() {
    // Not against the raw count. Bands written for a converted face have to
    // hold when the face is retuned, and 32768 means nothing to anybody.
    let mut r = readout("2-5", "PITCHTRIMIND");
    let s = trim_face(&mut r);
    assert_eq!(s.format_number(0, 65535), "ND", "hard nose down");
    assert_eq!(s.format_number(65535, 65535), "NU", "hard nose up");
    assert_eq!(s.format_number(32768, 65535), " ", "dead centre draws blank");
}

#[test]
fn a_needle_between_two_bands_lands_in_one_of_them() {
    // The reading is rounded to its decimal places before a band is looked
    // for, so the gap between -0.1 and 0 is not a gap the needle can sit in.
    let mut r = readout("2-5", "PITCHTRIMIND");
    let s = trim_face(&mut r);
    // A hair below centre rounds to -0.1 and is nose down.
    assert_eq!(s.format_number(raw((1.5 - 0.06) / 3.0), 65535), "ND");
    // A hair closer still rounds to 0 and is centred.
    assert_eq!(s.format_number(raw((1.5 - 0.04) / 3.0), 65535), " ");
}

#[test]
fn abs_draws_the_magnitude_and_still_lets_a_band_see_the_sign() {
    // `abs` is the last thing that happens, so a band written for negative
    // readings still matches on a piece that draws magnitudes.
    let mut r = readout("2-5", "PITCHTRIMIND");
    let s = span(&mut r);
    s.reads = Some([-1.5, 1.5]);
    s.decimals = 1;
    s.abs = true;
    let plain = s.clone();
    assert_eq!(plain.format_number(0, 65535), "1.5", "the sign is dropped");
    assert_eq!(plain.format_number(65535, 65535), "1.5");

    span(&mut r).value_aliases = banded([("-1.5..-0.1", "ND")]);
    let s = span(&mut r).clone();
    assert_eq!(s.format_number(0, 65535), "ND", "the band saw a negative");
    assert_eq!(s.format_number(65535, 65535), "1.5", "and nose up is a number");
}

#[test]
fn a_band_carries_its_own_colour() {
    let mut r = readout("2-5", "PITCHTRIMIND");
    let s = span(&mut r);
    s.reads = Some([-1.5, 1.5]);
    s.decimals = 1;
    s.colour = Some(Colour::Amber);
    s.value_aliases = [(
        "0.1..1.5".parse().expect("a band"),
        AliasDraw {
            text: "NU".into(),
            colour: Some(Colour::Green),
            inverse: false,
        },
    )]
    .into_iter()
    .collect();
    let s = s.clone();
    let (text, band) = s.format_reading(65535, 65535);
    assert_eq!(text, "NU");
    assert_eq!(band.and_then(|b| b.colour), Some(Colour::Green));
    // A reading no band claims takes no colour of its own, so the piece's
    // stands.
    let (text, band) = s.format_reading(0, 65535);
    assert_eq!(text, "-1.5");
    assert!(band.is_none());
}

#[test]
fn a_band_can_draw_inverse() {
    // A blank drawn inverse is a solid block, which is the cursor on glass with
    // no block glyph of its own.
    let mut r = readout("2-3", "KNOB");
    span(&mut r).value_aliases = [(
        ValueBand::One(1.0),
        AliasDraw {
            text: " ".into(),
            colour: None,
            inverse: true,
        },
    )]
    .into_iter()
    .collect();
    let number = |value: u16| move |_: &str| Some(Reading::Number { value, max: 1 });
    let glyphs = r.compose(number(1)).unwrap();
    assert!(glyphs[0].inverse, "the band claimed it");
    let glyphs = r.compose(number(0)).unwrap();
    assert!(glyphs.iter().all(|g| !g.inverse), "a reading no band claims draws plainly");
}

#[test]
fn a_band_can_name_a_list_of_readings() {
    let mut r = readout("2-5", "YAW_TRIM");
    let s = span(&mut r);
    s.reads = Some([-1.5, 1.5]);
    s.decimals = 1;
    s.value_aliases = banded([("-1.5,-1.4,-1.3", "L3")]);
    let s = s.clone();
    assert_eq!(s.format_number(raw(0.0), 65535), "L3");
    assert_eq!(s.format_number(raw(0.1 / 3.0), 65535), "L3", "-1.4");
    // The run between the named readings is not named: a list is its values.
    assert_eq!(s.format_number(raw(0.3 / 3.0), 65535), "-1.2");
}

#[test]
fn the_lower_of_two_overlapping_bands_draws() {
    // Overlapping bands are a caution rather than a refusal, so something has
    // to draw, and which one cannot be left to how the file happened to be
    // written. Bands sort by where they start, so it is always the lower.
    let mut r = readout("2-5", "PITCHTRIMIND");
    let s = span(&mut r);
    s.reads = Some([-1.5, 1.5]);
    s.decimals = 1;
    s.value_aliases = banded([("-0.5..0.5", "NEAR"), ("-1.5..0", "ND")]);
    let s = s.clone();
    // -0.3 is claimed by both. The band starting at -1.5 is the lower one.
    assert_eq!(s.format_number(raw((1.5 - 0.3) / 3.0), 65535), "ND");
}

#[test]
fn bands_covering_the_whole_face_are_the_whole_width() {
    // No number is ever drawn, so the number's width is not allowed for. The
    // same rule the aliased knob above gets, generalised to bands.
    let mut r = readout("2-5", "PITCHTRIMIND");
    let s = trim_face(&mut r);
    assert_eq!(s.widest(None, Some(65535)), Some(2), "ND and NU are two");

    // Leave the nose-up half to the number and the number counts again: -1.5
    // to 1.5 at one decimal place is four characters.
    span(&mut r).value_aliases = banded([("-1.5..-0.1", "ND"), ("0", " ")]);
    assert_eq!(span(&mut r).widest(None, Some(65535)), Some(4));

    // With the sign dropped the widest it draws is three.
    span(&mut r).abs = true;
    assert_eq!(span(&mut r).widest(None, Some(65535)), Some(3));
}
