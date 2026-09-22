//! The picture of a cell, which is what the editor previews a field with.
//!
//! Glass with no font of its own draws whatever its glyph table says, and a
//! table entry is a set of slots rather than a character. The editor can only
//! show what a field will look like if every slot a glyph names has something
//! drawn for it, and if the slots it is handed are the ones the panel lights.
//! Both are checked here against the shipped displays.

use std::path::Path;

use dsc_config::{Display, DisplayCatalogue, ShapeArt};

fn catalogue() -> DisplayCatalogue {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    DisplayCatalogue::load_dir(&root.join("data/displays")).expect("displays load")
}

#[test]
fn the_ufc_draws_its_segments_as_strokes() {
    let cat = catalogue();
    let ufc = cat.get("UFC1").expect("UFC1 is in data/displays");
    let art = ufc.shape_art();
    for (shape, slots) in [("alnum16", 16usize), ("digit7", 7), ("single", 1)] {
        match art.get(shape) {
            Some(ShapeArt::Strokes(drawn)) => {
                assert_eq!(drawn.slots.len(), slots, "{shape} slots");
                assert!(drawn.width > 0.0 && drawn.height > 0.0, "{shape} has a box");
                assert!(drawn.stroke > 0.0, "{shape} segments have a thickness");
            }
            other => panic!("{shape} is drawn as {other:?}"),
        }
    }
}

#[test]
fn every_glyph_the_ufc_has_is_drawable() {
    // A glyph naming a slot the art does not cover would preview as a
    // character with a piece missing, and the missing piece would be ours
    // rather than the panel's.
    let cat = catalogue();
    let ufc = cat.get("UFC1").unwrap();
    let art = ufc.shape_art();
    for (shape, table) in &ufc.glyphs {
        let Some(ShapeArt::Strokes(drawn)) = art.get(shape) else {
            panic!("{shape} has no art");
        };
        for (value, lit) in table {
            for &slot in lit {
                let strokes = drawn
                    .slots
                    .get(slot as usize)
                    .unwrap_or_else(|| panic!("{shape} {value:?} lights slot {slot}"));
                assert!(!strokes.is_empty(), "{shape} slot {slot} draws nothing");
            }
        }
    }
}

#[test]
fn the_ded_draws_its_cells_as_the_pixels_they_are() {
    let cat = catalogue();
    let ded = cat.get("DED").expect("DED is in data/displays");
    match ded.shape_art().get("ded") {
        Some(ShapeArt::Pixels { width, height }) => assert_eq!((*width, *height), (8, 13)),
        other => panic!("the DED is drawn as {other:?}"),
    }
    // A pixel screen writes no art down, so what it hands over has to be the
    // same reading of the font that made the cells: slot `n` is the pixel
    // `n % 8`, `n / 8`, and the rows start `ink_top` down the cell.
    let cell = ded.cell(0).unwrap();
    let rows = &ded.fonts["ded"]["A"];
    let want: Vec<u8> = rows
        .iter()
        .enumerate()
        .flat_map(|(y, row)| {
            row.chars()
                .enumerate()
                .filter(|(_, c)| *c == '#')
                .map(move |(x, _)| ((y + 2) * 8 + x) as u8)
                .collect::<Vec<u8>>()
        })
        .collect();
    assert_eq!(ded.lit(cell, "A", false), Some(want));
}

#[test]
fn an_inverse_ded_cell_is_the_block_with_the_glyph_knocked_out() {
    // How the DED highlights, and the one thing a preview of it has to get
    // right: drawing the glyph over a filled cell rather than out of it puts
    // the wrong half of a TCN page in light.
    let cat = catalogue();
    let ded = cat.get("DED").unwrap();
    let cell = ded.cell(0).unwrap();
    let plain = ded.lit(cell, "A", false).unwrap();
    let inverse = ded.lit(cell, "A", true).unwrap();
    let block: Vec<u8> = (8u8..96).filter(|s| !plain.contains(s)).collect();
    assert_eq!(inverse, block, "rows 1 to 11, less the glyph");
}

#[test]
fn a_value_this_glass_cannot_draw_lights_nothing() {
    // The editor marks the cell instead of leaving a blank that reads as a
    // space, so it needs the difference between a space and a refusal.
    let cat = catalogue();
    let ufc = cat.get("UFC1").unwrap();
    let seven = ufc.cell(2).expect("a scratchpad digit");
    assert!(ufc.lit(seven, "8", false).is_some());
    assert_eq!(ufc.lit(seven, "Q", false), None, "a digit cell has no letters");
}

/// One display on its own, for the faults a shipped file does not have.
fn made(art: &str) -> Result<Display, String> {
    let json = format!(
        r#"{{
          "key": "TEST", "part_id": 1, "buffer_bytes": 4, "group_bytes": 4,
          "cells": [{{ "index": 0, "shape": "two", "segments": [0, 1] }}],
          "glyphs": {{ "two": {{ "-": [0] }} }},
          "art": {art}
        }}"#
    );
    let mut display: Display = serde_json::from_str(&json).expect("the test display parses");
    display.expand().map_err(|e| e.to_string())?;
    Ok(display)
}

#[test]
fn art_with_a_slot_missing_is_refused() {
    let one = r#"{ "two": { "width": 4, "height": 8, "stroke": 1, "slots": [[[1, 4, 3, 4]]] } }"#;
    let err = made(one).expect_err("one slot for a two slot cell");
    assert!(err.contains("draws 1 slots and a two cell has 2"), "{err}");
}

#[test]
fn art_for_a_shape_no_cell_draws_is_refused() {
    let other = r#"{ "three": { "width": 4, "height": 8, "stroke": 1, "slots": [] } }"#;
    let err = made(other).expect_err("a shape this display does not have");
    assert!(err.contains("draws three, which no cell is"), "{err}");
}

#[test]
fn a_stroke_that_is_not_pairs_of_x_and_y_is_refused() {
    let odd = r#"{ "two": { "width": 4, "height": 8, "stroke": 1,
                  "slots": [[[1, 4, 3]], [[1, 6, 3, 6]]] } }"#;
    let err = made(odd).expect_err("three numbers is not a stroke");
    assert!(err.contains("stroke of 3 numbers"), "{err}");
}

#[test]
fn a_shape_with_art_and_one_without_are_both_reported_honestly() {
    // A shape nothing can picture is left out rather than guessed at, so the
    // window can say so instead of drawing something in particular.
    let fine = r#"{ "two": { "width": 4, "height": 8, "stroke": 1,
                   "slots": [[[1, 4, 3, 4]], [[1, 6, 3, 6]]] } }"#;
    let display = made(fine).expect("art that matches its cells");
    assert!(matches!(display.shape_art().get("two"), Some(ShapeArt::Strokes(_))));
    let mut bare = display.clone();
    bare.art.clear();
    assert!(bare.shape_art().is_empty(), "no art, no picture");
}
