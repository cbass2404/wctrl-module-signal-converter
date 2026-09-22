//! A field's content is a chain of pieces, and which font draws it.
//!
//! Two things are pinned here. A field can hold characters the user typed
//! beside the readings, each piece with its own colour and size, which is what
//! makes `RALT 250M` one field rather than three. And an aircraft with a CDU of
//! its own takes that aircraft's font whatever the profile says, while one
//! without takes the font the profile picked, which is the only reason its
//! screen can be used at all.
//!
//! These go through the whole daemon path, the same as `text_paint.rs`: a
//! profile, a live stream, and the bytes that would reach the panel.

use std::path::Path;
use std::time::{Duration, Instant};

use dsc_bios::Write as BiosWrite;
use dsc_config::{
    text_cells, Catalogue, Colour, DeviceInventory, DisplayCatalogue, Profile, Readout, Span,
};
use dsc_engine::{Batch, Engine, LcdWrite};

const MCDU: &str = "MCDU_Captain";
const A10C_FONT: &str = "../mcdu/a10c-font-21x31.json";
const F14BU_FONT: &str = "../mcdu/f14bu-font-21x31.json";

fn r(p: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(p)
}

fn bytes_at(address: u16, b: &[u8]) -> Vec<BiosWrite> {
    (0..b.len().div_ceil(2))
        .map(|i| BiosWrite {
            address: address + (i as u16) * 2,
            value: u16::from(b[i * 2]) | (u16::from(*b.get(i * 2 + 1).unwrap_or(&0)) << 8),
        })
        .collect()
}

/// A real profile to hang a test field on: one A-10C CDU page, frozen.
///
/// Not a shipped default. These tests put their field on MCDU row 1, and a
/// shipped profile is a living document whose author is free to fill that row,
/// which would fail every test here over cells rather than over anything the
/// test is about. The fixture holds the page's shape, with rows 1 to 3 left
/// free on purpose.
fn profile() -> Profile {
    Profile::load(&fixture("a-10c-cdu-page.json")).expect("the CDU page fixture")
}

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

fn engine(p: Profile) -> Engine {
    let devices = DeviceInventory::load(&r("data/devices.json")).expect("devices");
    let cat = Catalogue::load_dir(&r("data/catalogue")).expect("catalogue");
    let displays = DisplayCatalogue::load_dir(&r("data/displays")).expect("displays");
    let mut e = Engine::new(devices, cat, vec![p]).with_displays(displays);
    e.set_connected(vec![MCDU.into()]);
    e
}

fn line(e: &Engine, n: usize) -> u16 {
    e.catalogue()
        .module("A-10C")
        .and_then(|m| m.signal(&format!("CDU_LINE{n}")))
        .and_then(|s| s.primary())
        .expect("a CDU line")
        .address
}

fn fly(e: &mut Engine, aircraft: &str, lines: &[(usize, &[u8])]) -> Batch {
    let mut name = aircraft.as_bytes().to_vec();
    name.resize(24, 0);
    let mut writes = bytes_at(0, &name);
    for (n, text) in lines {
        let mut padded = text.to_vec();
        padded.resize(24, b' ');
        writes.extend(bytes_at(line(e, *n), &padded));
    }
    let t0 = Instant::now();
    let batch = e.ingest(&writes, t0);
    if batch.lcd.is_empty() {
        return e.tick(t0 + Duration::from_secs(5));
    }
    batch
}

fn screen(batch: &Batch) -> &LcdWrite {
    batch
        .lcd
        .iter()
        .find(|w| w.display == "MCDU")
        .expect("the MCDU was painted")
}

fn row(w: &LcdWrite, n: usize) -> String {
    text_cells(&w.bytes)[(n - 1) * 24..n * 24]
        .iter()
        .map(|c| c.ch)
        .collect()
}

/// Everything the daemon would refuse this profile for, in its own words.
/// What the editor would show on the fields themselves: everything that loads
/// but wants a second look.
fn cautions(p: &Profile) -> Vec<String> {
    let e = engine(p.clone());
    let devices = DeviceInventory::load(&r("data/devices.json")).unwrap();
    let displays = DisplayCatalogue::load_dir(&r("data/displays")).unwrap();
    let module = e.catalogue().module(&p.module).expect("the module");
    p.field_cautions(module, &devices, &displays).into_iter().map(|(_, c)| c).collect()
}

fn refusals(p: &Profile) -> Vec<String> {
    let e = engine(p.clone());
    let devices = DeviceInventory::load(&r("data/devices.json")).unwrap();
    let displays = DisplayCatalogue::load_dir(&r("data/displays")).unwrap();
    let module = e.catalogue().module(&p.module).expect("the module");
    p.problems(module, &devices, &displays)
        .iter()
        .map(|e| e.to_string())
        .collect()
}

/// A field on one of the A-10C's three free rows, which is the case this whole
/// feature exists for: the CDU is ten lines on a screen of fourteen.
fn on_row_one(content: Vec<Span>) -> Readout {
    Readout {
        device: MCDU.into(),
        display: "MCDU".into(),
        cells: "0-23".parse().unwrap(),
        content,
        ..Readout::default()
    }
}

fn text(s: &str) -> Span {
    Span { text: s.into(), ..Span::default() }
}

fn reading(source: &str) -> Span {
    Span { source: source.into(), ..Span::default() }
}

// --- drawing a chain ---------------------------------------------------------

#[test]
fn typed_characters_and_a_reading_are_drawn_on_one_row() {
    // The A-10C uses rows 4 to 14, so rows 1 to 3 are the user's. This is what
    // they are for.
    let mut p = profile();
    p.readouts
        .push(on_row_one(vec![text("PAGE "), reading("CDU_LINE1"), text(" END")]));
    let mut e = engine(p);
    let batch = fly(&mut e, "A-10C", &[(1, b"ALPHA")]);
    let w = screen(&batch);
    // `PAGE `, then the whole 24 character CDU line, which is `ALPHA` and
    // nineteen spaces. That is already past the end of the row, so ` END`
    // never lands: exactly the loss the width caution is there to warn about,
    // and exactly as quiet on the panel.
    assert_eq!(row(w, 1).trim_end(), "PAGE ALPHA");
}

#[test]
fn each_piece_keeps_its_own_colour_and_size() {
    let mut p = profile();
    p.readouts.push(on_row_one(vec![
        Span { text: "RALT".into(), colour: Some(Colour::Red), small: true, ..Span::default() },
        Span { text: "250".into(), colour: Some(Colour::Green), ..Span::default() },
    ]));
    let mut e = engine(p);
    let batch = fly(&mut e, "A-10C", &[]);
    let cells = text_cells(&screen(&batch).bytes);
    assert_eq!(cells[0].ch, 'R');
    assert_eq!(cells[0].fg, Colour::Red.ordinal());
    assert!(cells[0].small, "the label is small");
    assert_eq!(cells[4].ch, '2');
    assert_eq!(cells[4].fg, Colour::Green.ordinal());
    assert!(!cells[4].small, "the reading beside it is not");
}

#[test]
fn a_typed_piece_is_on_the_glass_before_its_reading_arrives() {
    // A label should not wait on the signal beside it. The row is drawn as
    // soon as the aircraft loads, with the reading's cells still blank.
    let mut p = profile();
    p.readouts
        .push(on_row_one(vec![text("WIND "), reading("CDU_LINE1")]));
    let mut e = engine(p);
    let batch = fly(&mut e, "A-10C", &[]);
    assert_eq!(row(screen(&batch), 1).trim_end(), "WIND");
}

// --- which font ---------------------------------------------------------------

#[test]
fn an_aircraft_with_its_own_cdu_keeps_its_font_whatever_the_profile_says() {
    // The A-10C font was drawn to match what the module sends. Overriding it
    // would not draw the same page in another hand, it would draw the wrong
    // symbols.
    let mut p = profile();
    p.font = Some(F14BU_FONT.into());
    let mut e = engine(p);
    let batch = fly(&mut e, "A-10C", &[(2, b"STEERPOINT")]);
    assert_eq!(screen(&batch).font.as_deref(), Some(A10C_FONT));
}

#[test]
fn an_aircraft_without_a_cdu_cannot_use_the_screen_until_a_font_is_picked() {
    let mut p = profile();
    p.aircraft = vec!["Mi-24P".into()];
    p.readouts.push(on_row_one(vec![text("HELLO")]));
    assert!(
        refusals(&p).iter().any(|e| e.contains("has no font for")),
        "{:?}",
        refusals(&p)
    );
}

#[test]
fn picking_a_font_opens_the_screen_to_an_aircraft_without_a_cdu() {
    let mut p = profile();
    p.aircraft = vec!["Mi-24P".into()];
    p.readouts.push(on_row_one(vec![text("HELLO")]));
    p.font = Some(F14BU_FONT.into());
    assert!(
        !refusals(&p).iter().any(|e| e.contains("has no font for")),
        "{:?}",
        refusals(&p)
    );
}

#[test]
fn the_chosen_font_is_the_one_uploaded() {
    let mut p = profile();
    p.aircraft = vec!["Mi-24P".into()];
    p.font = Some(F14BU_FONT.into());
    p.readouts.push(on_row_one(vec![text("HELLO")]));
    let mut e = engine(p);
    let batch = fly(&mut e, "Mi-24P", &[]);
    assert_eq!(screen(&batch).font.as_deref(), Some(F14BU_FONT));
}

// --- what a font can draw ------------------------------------------------------

#[test]
fn a_character_the_font_does_not_draw_is_refused() {
    // Only the F-14BU font has lowercase. Typing it against the A-10C's would
    // leave blank cells on the panel with nothing saying why, so it is refused
    // here instead.
    let mut p = profile();
    p.readouts.push(on_row_one(vec![text("Fuel")]));
    assert!(
        refusals(&p).iter().any(|e| e.contains("is not a character the font")),
        "{:?}",
        refusals(&p)
    );
}

#[test]
fn the_same_characters_are_fine_in_a_font_that_draws_them() {
    let mut p = profile();
    p.aircraft = vec!["Mi-24P".into()];
    p.font = Some(F14BU_FONT.into());
    p.readouts.push(on_row_one(vec![text("Fuel")]));
    assert!(
        !refusals(&p).iter().any(|e| e.contains("is not a character the font")),
        "{:?}",
        refusals(&p)
    );
}

#[test]
fn a_piece_marked_small_is_checked_against_the_small_alphabet() {
    // Every font here draws fewer characters small than large, so marking a
    // piece small can take away a character that was fine at full size. The
    // A-10C font draws `^` large and not small.
    let mut p = profile();
    p.readouts.push(on_row_one(vec![text("^")]));
    assert!(refusals(&p).is_empty(), "{:?}", refusals(&p));

    let mut p = profile();
    p.readouts
        .push(on_row_one(vec![Span { text: "^".into(), small: true, ..Span::default() }]));
    assert!(
        refusals(&p).iter().any(|e| e.contains("is not a character the font")),
        "{:?}",
        refusals(&p)
    );
}

// --- a piece that says two things ------------------------------------------------

#[test]
fn a_piece_that_both_reads_and_writes_is_refused() {
    let mut p = profile();
    p.readouts.push(on_row_one(vec![Span {
        text: "RALT".into(),
        source: "CDU_LINE1".into(),
        ..Span::default()
    }]));
    assert!(
        refusals(&p).iter().any(|e| e.contains("it can have one")),
        "{:?}",
        refusals(&p)
    );
}

#[test]
fn a_piece_with_nothing_in_it_is_unfinished_rather_than_ignored() {
    let mut p = profile();
    p.readouts
        .push(on_row_one(vec![text("RALT"), Span::default()]));
    assert!(
        refusals(&p).iter().any(|e| e.contains("a piece with nothing in it")),
        "{:?}",
        refusals(&p)
    );
}

// --- running out of room ---------------------------------------------------------

#[test]
fn content_too_wide_for_its_cells_is_a_caution_and_not_a_refusal() {
    // Nothing on the panel says the tail was cut, so it is said here. It stays
    // a caution: whether the aircraft ever sends a reading that wide is the
    // user's to judge.
    let mut p = profile();
    let mut field = on_row_one(vec![text("A VERY LONG LABEL INDEED"), reading("CDU_LINE1")]);
    field.cells = "0-23".parse().unwrap();
    p.readouts.push(field);
    let e = engine(p.clone());
    let module = e.catalogue().module(&p.module).expect("the module");
    let cautions = p.width_cautions(module);
    assert!(
        cautions.iter().any(|c| c.contains("would be dropped")),
        "{cautions:?}"
    );
    assert!(refusals(&p).is_empty(), "{:?}", refusals(&p));
}

#[test]
fn content_that_fits_says_nothing() {
    let mut p = profile();
    p.readouts.push(on_row_one(vec![text("SHORT")]));
    let e = engine(p.clone());
    let module = e.catalogue().module(&p.module).expect("the module");
    assert!(p.width_cautions(module).is_empty());
}

#[test]
fn a_gauge_shown_as_sent_is_measured_by_its_maximum() {
    // With no range the needle is drawn as the number DCS-BIOS sends, 0 to
    // 65535, so five cells is known and four is known to be one short.
    let mut p = profile();
    let mut field = on_row_one(vec![reading("FLAP_POS")]);
    field.cells = "0-3".parse().unwrap();
    p.readouts.push(field);
    let e = engine(p.clone());
    let module = e.catalogue().module(&p.module).expect("the module");
    let cautions = p.width_cautions(module);
    assert!(
        cautions.iter().any(|c| c.contains("needs up to 5 cells and has 4")),
        "{cautions:?}"
    );
}

#[test]
fn a_run_of_one_cell_is_never_measured_by_character() {
    // The Hornet UFC is the case this exists for. Four of its fields are one
    // cell reading a two character signal, and the panel draws each as a
    // single glyph: the comm windows carry `width: 2` in the display map, and
    // a scratchpad mark arrives from DCS-BIOS as `" G"` and is looked up
    // whole. Counted by character they all read as a field about to lose its
    // last character, and the editor showed four warnings on a screen that
    // draws exactly what it was built to draw.
    let p = Profile::load(&r("crates/dsc-engine/tests/fixtures/fa-18.json")).expect("the Hornet fixture");
    let cat = Catalogue::load_dir(&r("data/catalogue")).expect("catalogue");
    let module = cat.module(&p.module).expect("the module");
    // The whole screen rather than the four fields, because a profile that
    // warns on sight teaches people to scroll past the warnings.
    assert!(
        p.width_cautions(module).is_empty(),
        "the Hornet draws what it was built to draw: {:?}",
        p.width_cautions(module)
    );
}

#[test]
fn a_run_of_two_cells_still_counts_characters() {
    // The boundary the one cell rule sits on, and the half of it that is easy
    // to take too far. One cell holds whatever it is handed; two hold one
    // character each, and a third character is gone with nothing said.
    let mut p = profile();
    let mut field = on_row_one(vec![text("ABC")]);
    field.cells = "0-1".parse().unwrap();
    p.readouts.push(field);
    let e = engine(p.clone());
    let module = e.catalogue().module(&p.module).expect("the module");
    assert!(
        p.width_cautions(module).iter().any(|c| c.contains("would be dropped")),
        "{:?}",
        p.width_cautions(module)
    );
}

// --- gaps: pushing content to both ends -------------------------------------

fn gap() -> Span {
    Span { gap: true, ..Span::default() }
}

#[test]
fn a_gap_pushes_what_follows_to_the_far_end() {
    // The thing a CDU page does constantly: a label at the left and its value
    // hard against the right, with the blank between them worked out rather
    // than counted by hand.
    let mut p = profile();
    p.readouts
        .push(on_row_one(vec![text("FUEL"), gap(), text("2450")]));
    let mut e = engine(p);
    let batch = fly(&mut e, "A-10C", &[]);
    let drawn = row(screen(&batch), 1);
    assert_eq!(drawn, "FUEL                2450");
    assert_eq!(drawn.chars().count(), 24, "the row is filled exactly");
}

#[test]
fn two_gaps_space_three_pieces_across_the_line() {
    let mut p = profile();
    p.readouts
        .push(on_row_one(vec![text("A"), gap(), text("B"), gap(), text("C")]));
    let mut e = engine(p);
    let batch = fly(&mut e, "A-10C", &[]);
    // 21 cells spare across two gaps: 11 then 10, the remainder going to the
    // earlier one.
    assert_eq!(row(screen(&batch), 1), "A           B          C");
}

#[test]
fn a_gap_gives_its_room_back_as_a_reading_grows() {
    // The whole point of measuring it rather than typing spaces: the ends stay
    // put when the value in the middle changes width.
    let mut p = profile();
    p.readouts
        .push(on_row_one(vec![text("WP"), gap(), reading("CDU_LINE1")]));
    let mut e = engine(p);
    let batch = fly(&mut e, "A-10C", &[(1, b"X")]);
    let drawn = row(screen(&batch), 1);
    assert!(drawn.starts_with("WP"), "{drawn:?}");
    assert_eq!(drawn.chars().count(), 24);
    // The CDU line is 24 characters of its own, so it fills what is left and
    // the gap closes to nothing. A shorter source would open it back up.
    assert!(drawn.contains('X'), "{drawn:?}");
}

#[test]
fn a_gap_with_nothing_spare_draws_nothing() {
    // A full line is a line that has filled up, not a reason to push anything
    // off the end.
    let mut p = profile();
    let long = "ABCDEFGHIJKL";
    p.readouts
        .push(on_row_one(vec![text(long), gap(), text(long)]));
    let mut e = engine(p);
    let batch = fly(&mut e, "A-10C", &[]);
    assert_eq!(row(screen(&batch), 1), format!("{long}{long}"));
}

#[test]
fn a_gap_asks_for_no_room_of_its_own() {
    // It can never be the reason content will not fit, so it must not show up
    // in the width warning.
    let mut p = profile();
    p.readouts
        .push(on_row_one(vec![text("LEFT"), gap(), text("RIGHT")]));
    let e = engine(p.clone());
    let module = e.catalogue().module(&p.module).expect("the module");
    assert!(p.width_cautions(module).is_empty(), "{:?}", p.width_cautions(module));
}

#[test]
fn a_gap_that_also_has_something_to_draw_is_refused() {
    let mut p = profile();
    p.readouts.push(on_row_one(vec![Span {
        gap: true,
        text: "X".into(),
        ..Span::default()
    }]));
    assert!(
        refusals(&p).iter().any(|e| e.contains("draws nothing of its own")),
        "{:?}",
        refusals(&p)
    );
}

#[test]
fn a_field_of_nothing_but_gaps_is_refused() {
    // Gaps space out what is around them. With nothing around them they are an
    // elaborate way of writing blanks.
    let mut p = profile();
    p.readouts.push(on_row_one(vec![gap(), gap()]));
    assert!(
        refusals(&p).iter().any(|e| e.contains("nothing but gaps")),
        "{:?}",
        refusals(&p)
    );
}

#[test]
fn a_gap_keeps_each_side_in_its_own_colour() {
    let mut p = profile();
    p.readouts.push(on_row_one(vec![
        Span { text: "L".into(), colour: Some(Colour::Green), ..Span::default() },
        gap(),
        Span { text: "R".into(), colour: Some(Colour::Amber), ..Span::default() },
    ]));
    let mut e = engine(p);
    let batch = fly(&mut e, "A-10C", &[]);
    let cells = text_cells(&screen(&batch).bytes);
    assert_eq!(cells[0].ch, 'L');
    assert_eq!(cells[0].fg, Colour::Green.ordinal());
    assert_eq!(cells[23].ch, 'R');
    assert_eq!(cells[23].fg, Colour::Amber.ordinal());
}

// --- boxes: holding a piece to a width --------------------------------------

#[test]
fn a_boxed_piece_keeps_what_follows_it_in_the_same_cells() {
    // Without the box the unit would sit wherever the reading happened to end,
    // and move every time it changed width. The whole CDU line is 24
    // characters, so the box is also what stops it filling the row.
    let mut p = profile();
    p.readouts.push(on_row_one(vec![
        text("W "),
        Span { source: "CDU_LINE1".into(), width: 8, ..Span::default() },
        text("KT"),
    ]));
    let mut e = engine(p);
    let batch = fly(&mut e, "A-10C", &[(1, b"ALPHA")]);
    let w = screen(&batch);
    assert_eq!(row(w, 1).trim_end(), "W ALPHA   KT");
    let cells = text_cells(&w.bytes);
    assert_eq!(cells[10].ch, 'K', "the unit starts where the box ends");
}

#[test]
fn a_right_aligned_box_pins_its_reading_to_the_end_of_the_box() {
    let mut p = profile();
    p.readouts.push(on_row_one(vec![
        text("W "),
        Span {
            source: "CDU_LINE1".into(),
            width: 8,
            align: dsc_config::Align::Right,
            ..Span::default()
        },
        text("KT"),
    ]));
    let mut e = engine(p);
    let batch = fly(&mut e, "A-10C", &[(1, b"ALPHA")]);
    // The reading is the whole 24 character line, so the box crops it from the
    // front: what a right aligned run keeps is its tail.
    let drawn = row(screen(&batch), 1);
    assert_eq!(&drawn[2..10], "        ", "the line's trailing blanks are its end");
    assert_eq!(&drawn[10..12], "KT");
}

#[test]
fn a_box_wider_than_the_field_is_refused() {
    let mut p = profile();
    p.readouts.push(on_row_one(vec![Span {
        source: "CDU_LINE1".into(),
        width: 40,
        ..Span::default()
    }]));
    assert!(
        refusals(&p).iter().any(|e| e.contains("wider than the 24 cells")),
        "{:?}",
        refusals(&p)
    );
}

// --- a rule inside a chain ---------------------------------------------------

#[test]
fn a_rule_fills_the_room_between_two_pieces_on_the_panel() {
    // What this replaces is three fields with hand counted cells, where the
    // rule could not move and a reading one character wider than planned
    // overran into it.
    let mut p = profile();
    p.readouts.push(on_row_one(vec![
        text("NAV"),
        Span { gap: true, rule: true, ..Span::default() },
        text("END"),
    ]));
    let mut e = engine(p);
    let batch = fly(&mut e, "A-10C", &[]);
    assert_eq!(row(screen(&batch), 1), "NAV------------------END");
}

#[test]
fn a_boxed_rule_carries_a_label_and_its_own_colour() {
    let mut p = profile();
    p.readouts.push(on_row_one(vec![
        text("A"),
        Span {
            gap: true,
            rule: true,
            width: 23,
            label: "FUEL".into(),
            colour: Some(Colour::Green),
            label_colour: Some(Colour::Amber),
            ..Span::default()
        },
    ]));
    let mut e = engine(p);
    let batch = fly(&mut e, "A-10C", &[]);
    let w = screen(&batch);
    assert_eq!(row(w, 1), format!("A{}", dsc_config::divider_text(23, "FUEL")));
    let cells = text_cells(&w.bytes);
    let label: Vec<usize> = (0..24).filter(|&i| cells[i].ch == 'F' || cells[i].ch == 'U').collect();
    assert!(!label.is_empty(), "the label is on the glass");
    for i in label {
        assert_eq!(cells[i].fg, Colour::Amber.ordinal(), "cell {i} is the label");
    }
    assert_eq!(cells[1].fg, Colour::Green.ordinal(), "the line keeps its own");
}

#[test]
fn a_label_on_a_rule_a_reading_can_squeeze_is_cautioned_not_refused() {
    // An elastic rule beside a reading is as wide as that reading leaves it,
    // so the label fits at one reading and is dropped at the next. Said on the
    // field rather than refused: whether the reading ever really gets that
    // wide is the user's to judge, and the rule draws either way.
    let mut p = profile();
    p.readouts.push(on_row_one(vec![
        text("NAV"),
        Span { gap: true, rule: true, label: "FUEL".into(), ..Span::default() },
        reading("CDU_LINE1"),
    ]));
    assert!(refusals(&p).is_empty(), "{:?}", refusals(&p));
    assert!(
        cautions(&p).iter().any(|c| c.contains("may come and go")),
        "{:?}",
        cautions(&p)
    );
}

#[test]
fn a_rule_with_the_line_to_itself_carries_a_label_without_a_box() {
    // Nothing else on the row is taking cells off it, so it is the whole run
    // in every frame: a width that holds still without anybody writing one
    // down, which is all a label ever needed.
    let mut p = profile();
    p.readouts
        .push(on_row_one(vec![Span { gap: true, rule: true, label: "FUEL".into(), ..Span::default() }]));
    assert!(refusals(&p).is_empty(), "{:?}", refusals(&p));
    assert!(cautions(&p).is_empty(), "{:?}", cautions(&p));
    let mut e = engine(p);
    let batch = fly(&mut e, "A-10C", &[]);
    assert_eq!(row(screen(&batch), 1), dsc_config::divider_text(24, "FUEL"));
}

#[test]
fn two_rules_on_a_settled_line_split_the_leftover_and_the_odd_cell_goes_left() {
    // Both rules hold still here, so both labels are measured rather than
    // cautioned, and against different widths: the leftover is split evenly
    // and the remainder goes to the earlier gap. A label needing exactly the
    // wider share is the test of it, since it fits the first rule and not the
    // second.
    let label = "STEERPNT"; // eight characters, so twelve cells with its dashes and blanks
    let mut p = profile();
    p.readouts.push(on_row_one(vec![
        text("A"),
        Span { gap: true, rule: true, label: label.into(), ..Span::default() },
        Span { gap: true, rule: true, ..Span::default() },
    ]));
    assert!(refusals(&p).is_empty(), "{:?}", refusals(&p));
    let mut e = engine(p.clone());
    let batch = fly(&mut e, "A-10C", &[]);
    assert_eq!(
        row(screen(&batch), 1),
        format!("A{}{}", dsc_config::divider_text(12, label), dsc_config::divider_text(11, ""))
    );

    let field = p.readouts.last_mut().expect("the field just pushed");
    field.content[1].label = String::new();
    field.content[2].label = label.into();
    assert!(
        refusals(&p).iter().any(|e| e.contains("does not fit")),
        "{:?}",
        refusals(&p)
    );
}

#[test]
fn a_label_too_wide_for_the_cells_a_settled_rule_gets_is_refused() {
    // The leftover is certain here: typed characters and a box each side, so
    // the rule is the same six cells in every frame and the label will never
    // fit. Refused like a boxed rule that is too narrow, because the width it
    // is measured against cannot change.
    let mut p = profile();
    p.readouts.push(on_row_one(vec![
        text("NAV"),
        Span { gap: true, rule: true, label: "STEERPOINT".into(), ..Span::default() },
        Span { source: "CDU_LINE1".into(), width: 15, ..Span::default() },
    ]));
    assert!(
        refusals(&p).iter().any(|e| e.contains("does not fit")),
        "{:?}",
        refusals(&p)
    );
}

#[test]
fn a_label_too_wide_for_its_rule_is_refused() {
    let mut p = profile();
    p.readouts.push(on_row_one(vec![
        Span { gap: true, rule: true, width: 6, label: "STEERPOINT".into(), ..Span::default() },
        text("X"),
    ]));
    assert!(
        refusals(&p).iter().any(|e| e.contains("does not fit")),
        "{:?}",
        refusals(&p)
    );
}

#[test]
fn a_rule_written_on_a_piece_that_draws_its_own_content_is_refused() {
    let mut p = profile();
    p.readouts
        .push(on_row_one(vec![Span { text: "NAV".into(), rule: true, ..Span::default() }]));
    assert!(
        refusals(&p).iter().any(|e| e.contains("only a gap can be a rule")),
        "{:?}",
        refusals(&p)
    );
}

#[test]
fn a_label_written_on_a_piece_that_is_not_a_rule_is_refused() {
    let mut p = profile();
    p.readouts
        .push(on_row_one(vec![Span { text: "NAV".into(), label: "FUEL".into(), ..Span::default() }]));
    assert!(
        refusals(&p).iter().any(|e| e.contains("not a rule")),
        "{:?}",
        refusals(&p)
    );
}

#[test]
fn a_field_of_nothing_but_a_rule_is_a_divider_written_the_long_way() {
    // Gaps with nothing around them are refused as an elaborate way of writing
    // blanks. A rule is not blanks, so drawing it is the right answer.
    let mut p = profile();
    p.readouts.push(on_row_one(vec![Span {
        gap: true,
        rule: true,
        ..Span::default()
    }]));
    assert_eq!(refusals(&p), Vec::<String>::new());
    let mut e = engine(p);
    let batch = fly(&mut e, "A-10C", &[]);
    assert_eq!(row(screen(&batch), 1), "------------------------");
}
