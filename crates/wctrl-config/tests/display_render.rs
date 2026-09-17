//! Render checks against traffic captured from real hardware.
//!
//! `fixtures/ufc1_captured_states.json` holds display states taken off the wire
//! while SimAppPro drove a CarrierAce UFC in a live Hornet mission. Each state
//! carries the device buffer it produced and what our cell map says that buffer
//! reads. Rendering the second back into the first means agreeing with the
//! hardware rather than with ourselves.

use std::path::Path;

use serde::Deserialize;
use wctrl_config::{DisplayCatalogue, Screen};

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

    // Observed live: DCS-BIOS reports UFC_COMM1_DISPLAY as " 2", two characters
    // for the single cell 34, and the hardware is sent the " 2" glyph. It is
    // not the union of " " and "2", and drawing it per character would be wrong
    // rather than merely different.
    let mut whole = Screen::new(ufc);
    whole.draw(ufc, 34, " 2").unwrap();

    let mut per_char = Screen::new(ufc);
    per_char.draw(ufc, 34, "2").unwrap();

    assert_ne!(
        whole, per_char,
        "' 2' and '2' must be different glyphs, or the capture was misread"
    );
}

// ------------------------------------------------------- readouts

use wctrl_config::{Align, CellRange, Readout};

fn readout(cells: &str, source: &str) -> Readout {
    Readout {
        device: "CarrierAce_UFC".into(),
        display: "UFC1".into(),
        cells: cells.parse::<CellRange>().expect("a cell run parses"),
        source: source.into(),
        reads: None,
        decimals: 0,
        align: Align::Left,
        aliases: Default::default(),
        note: String::new(),
    }
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
    r.reads = Some([0.0, 300.0]);
    assert_eq!(r.format_number(0, 65535), "0");
    assert_eq!(r.format_number(65535, 65535), "300");
    assert_eq!(r.format_number(32767, 65535), "150");
}

#[test]
fn a_gauge_that_reads_below_zero_converts_too() {
    // A g meter does not start at zero, and the conversion interpolates
    // between the two ends of the face rather than scaling up from nothing.
    let mut r = readout("2-8", "ACCEL_G");
    r.reads = Some([-10.0, 12.0]);
    r.decimals = 1;
    assert_eq!(r.format_number(0, 65535), "-10.0");
    assert_eq!(r.format_number(65535, 65535), "12.0");
    // Level flight is 1 g, which sits 11/22 of the way up a -10..12 face.
    let one_g = (11.0f64 / 22.0 * 65535.0).round() as u16;
    assert_eq!(r.format_number(one_g, 65535), "1.0");
}

#[test]
fn a_gauge_whose_face_runs_backwards_converts_too() {
    let mut r = readout("2-8", "BACKWARDS");
    r.reads = Some([100.0, 0.0]);
    assert_eq!(r.format_number(0, 65535), "100");
    assert_eq!(r.format_number(65535, 65535), "0");
}

#[test]
fn a_selector_whose_value_is_already_the_number_needs_no_special_case() {
    // A-10C TACAN_1 has max_value 10 and its value is the digit. Giving it its
    // own range makes the conversion an identity, through the same arithmetic
    // a needle uses.
    let mut r = readout("2", "TACAN_1");
    r.reads = Some([0.0, 10.0]);
    for n in 0..=10u16 {
        assert_eq!(r.format_number(n, 10), n.to_string());
    }
}

#[test]
fn a_right_aligned_field_drops_its_leading_pad_not_its_last_digit() {
    let mut r = readout("2-8", "UFC_SCRATCHPAD_NUMBER_DISPLAY");
    r.align = Align::Right;
    // DCS-BIOS gives 8 characters for 7 cells. Observed live.
    assert_eq!(r.lay_out(" 264.000").concat(), "264.000");
    // And a short value is pushed to the right, which is where keypresses land.
    assert_eq!(r.lay_out("1").concat(), "      1");
}

#[test]
fn a_left_aligned_field_pads_on_the_right() {
    let r = readout("10-13", "UFC_OPTION_DISPLAY_1");
    assert_eq!(r.lay_out("AM").concat(), "AM  ");
    assert_eq!(r.lay_out("GRCV").concat(), "GRCV");
    assert_eq!(r.lay_out("TOOLONG").concat(), "TOOL");
}

#[test]
fn a_shrinking_field_blanks_the_cells_it_gives_up() {
    let r = readout("10-13", "UFC_OPTION_DISPLAY_1");
    let now = r.lay_out("AM");
    assert_eq!(now.len(), 4, "every cell is written, not just the used ones");
    assert_eq!(now[2], " ");
    assert_eq!(now[3], " ");
}

#[test]
fn a_single_cell_field_is_laid_out_as_one_whole_glyph() {
    let r = readout("34", "UFC_COMM1_DISPLAY");
    // Not [" ", "2"]: the pair is one glyph on one cell, which is how the
    // hardware was observed being driven.
    assert_eq!(r.lay_out(" 2"), vec![" 2".to_string()]);
}

#[test]
fn an_alias_rewrites_a_value_the_glyph_table_does_not_know() {
    let mut r = readout("1", "UFC_SCRATCHPAD_STRING_2_DISPLAY");
    r.aliases.insert("--".into(), "_".into());
    // DCS-BIOS says "--" where DCS's own indication says "_", and "--" is not
    // a glyph. Without this the cell would fall back to a single dash.
    assert_eq!(r.alias("--"), "_");
    assert_eq!(r.alias(" 2"), " 2", "anything not aliased passes through");
}
