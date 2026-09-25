//! Typed characters on glass that draws from a glyph table.
//!
//! The UFC and the DED have no font to upload: a cell lights what its table
//! says for a value, and a value the table lacks leaves the cell dark. A text
//! grid refuses a character its font cannot draw. These cannot be that sure,
//! since a value may still reach the glass by another spelling, so the field
//! is flagged and the profile still loads.

use std::path::Path;

use dsc_config::{Catalogue, DeviceInventory, DisplayCatalogue, PageLibrary, Profile, Readout, Span};

fn r(p: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(p)
}

fn fixture(name: &str) -> Profile {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
    Profile::load(&path).expect("fixture profile")
}

/// What the editor shows on the fields, and what the daemon refuses.
fn checks(p: &Profile) -> (Vec<String>, Vec<String>) {
    let devices = DeviceInventory::load(&r("data/devices.json")).unwrap();
    let displays = DisplayCatalogue::load_dir(&r("data/displays")).unwrap();
    let cat = Catalogue::load_dir(&r("data/catalogue")).unwrap();
    let module = cat.module(&p.module).expect("the module");
    let cautions = p
        .field_cautions(module, &devices, &displays)
        .into_iter()
        .map(|(_, c)| c)
        .collect();
    let refusals = p
        .problems(module, &devices, &displays, &PageLibrary::default())
        .iter()
        .map(|e| e.to_string())
        .collect();
    (cautions, refusals)
}

fn flagged(p: &Profile, value: &str) -> bool {
    let (cautions, refusals) = checks(p);
    let says = format!("{value:?} is not something");
    assert!(!refusals.iter().any(|e| e.contains(&says)), "refused, not flagged: {refusals:?}");
    cautions.iter().any(|c| c.contains(&says))
}

/// Swap one of the fixture's fields for typed text on the same cells.
fn typed(mut p: Profile, cells: &str, spans: Vec<Span>) -> Profile {
    let at = p
        .readouts
        .iter()
        .position(|f| f.cells.to_string() == cells)
        .expect("a field on those cells");
    p.readouts[at] = Readout { content: spans, ..p.readouts[at].clone() };
    p
}

fn text(s: &str) -> Span {
    Span { text: s.into(), ..Span::default() }
}

#[test]
fn a_character_the_ded_cannot_draw_is_flagged_and_still_loads() {
    let p = typed(fixture("f-16.json"), "0-23", vec![text("FUEL $")]);
    assert!(flagged(&p, "$"), "{:?}", checks(&p));
}

#[test]
fn lowercase_is_flagged_on_the_ded_except_its_symbols() {
    // The DED looks glyphs up exactly as sent, because DCS-BIOS spells its
    // arrows and degree sign as lowercase letters. So 'a' and 'o' draw and
    // any other lowercase letter does not.
    let p = typed(fixture("f-16.json"), "0-23", vec![text("a 12o fuel")]);
    assert!(flagged(&p, "f"));
    assert!(!flagged(&p, "a"));
    assert!(!flagged(&p, "o"));
}

#[test]
fn everything_the_ded_draws_says_nothing() {
    let p = typed(fixture("f-16.json"), "0-23", vec![text("STPT a 12 <>[]+=|,!?;&_'\"%#@ud")]);
    let (cautions, _) = checks(&p);
    assert!(!cautions.iter().any(|c| c.contains("is not something")), "{cautions:?}");
}

#[test]
fn an_alias_and_a_replacement_are_checked_too() {
    let mut band = text("");
    band.source = "DED_L1".into();
    band.replace = [("-".to_string(), "$".to_string())].into_iter().collect();
    let p = typed(fixture("f-16.json"), "0-23", vec![band]);
    assert!(flagged(&p, "$"));
}

#[test]
fn a_letter_on_the_ufc_scratchpad_digits_is_flagged() {
    // Cells 2 to 8 are seven-segment digits: no letters at all.
    let p = typed(fixture("fa-18.json"), "2-8", vec![text("ABC")]);
    assert!(flagged(&p, "A"), "{:?}", checks(&p));
}

#[test]
fn an_option_window_takes_letters_but_not_a_question_mark() {
    let p = typed(fixture("fa-18.json"), "10-13", vec![text("GPS?")]);
    assert!(flagged(&p, "?"));
    assert!(!flagged(&p, "G"));
}

#[test]
fn a_comm_window_is_checked_as_one_value() {
    // One cell, two characters: "12" is a glyph there by the vendor's
    // spelling, and "AB" is nothing it has.
    let fine = typed(fixture("fa-18.json"), "34", vec![text("12")]);
    let (cautions, _) = checks(&fine);
    assert!(!cautions.iter().any(|c| c.contains("is not something")), "{cautions:?}");
    let dark = typed(fixture("fa-18.json"), "34", vec![text("AB")]);
    assert!(flagged(&dark, "AB"), "{:?}", checks(&dark));
}
