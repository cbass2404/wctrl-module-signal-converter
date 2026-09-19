//! Learn mode against a synthetic export stream.
//!
//! Everything here is the situation the feature exists for: a cockpit that is
//! never still, where one thing the user just did has to be picked out of a
//! stream that is re-sending its entire address space several times a second.
//!
//! Built on a fixture module rather than `data/catalogue`, which is generated
//! per machine and not committed.

use std::time::{Duration, Instant};

use dsc_bios::Write as BiosWrite;
use dsc_config::Module;
use dsc_engine::learn::{Watcher, DEFAULT_BASELINE_QUIET};

/// A word, a neighbour sharing that word, a gauge and a string field.
///
/// The two signals on address 100 are the case that produces most of the noise
/// in a real module: DCS-BIOS packs several controls into one word, so the word
/// moves whenever any of them does.
fn module() -> Module {
    serde_json::from_str(
        r#"{
          "module": "TEST_LEARN",
          "aircraft": ["TEST_LEARN"],
          "signals": [
            {
              "id": "GEAR_LEVER",
              "control_type": "selector",
              "description": "Landing gear lever",
              "outputs": [{"address": 100, "mask": 3, "shift": 0, "max_value": 2, "max_length": null}]
            },
            {
              "id": "CANOPY_LIGHT",
              "control_type": "led",
              "description": "Canopy unlocked",
              "outputs": [{"address": 100, "mask": 4, "shift": 2, "max_value": 1, "max_length": null}]
            },
            {
              "id": "ALTIMETER",
              "control_type": "analog_gauge",
              "description": "Barometric altitude",
              "outputs": [{"address": 200, "mask": 65535, "shift": 0, "max_value": 65535, "max_length": null}]
            },
            {
              "id": "SCRATCHPAD",
              "control_type": "display",
              "description": "UFC scratchpad",
              "outputs": [{"address": 300, "mask": null, "shift": 0, "max_value": null, "max_length": 6, "type": "string"}]
            },
            {
              "id": "NO_LENGTH",
              "control_type": "display",
              "description": "A string the catalogue could not size",
              "outputs": [{"address": 400, "mask": null, "shift": 0, "max_value": null, "max_length": null, "type": "string"}]
            }
          ]
        }"#,
    )
    .expect("the fixture module parses")
}

fn w(address: u16, value: u16) -> BiosWrite {
    BiosWrite { address, value }
}

/// Pack an ASCII field the way DCS-BIOS does, two bytes per word, low first.
fn text_at(address: u16, s: &str) -> Vec<BiosWrite> {
    let b = s.as_bytes();
    (0..b.len().div_ceil(2))
        .map(|i| {
            w(
                address + (i as u16) * 2,
                u16::from(b[i * 2]) | (u16::from(*b.get(i * 2 + 1).unwrap_or(&b' ')) << 8),
            )
        })
        .collect()
}

/// One full export cycle: everything the module publishes, whether it moved or
/// not. This is what the real stream does about three times a second.
fn sweep(word100: u16, altimeter: u16, scratchpad: &str) -> Vec<BiosWrite> {
    let mut out = text_at(0, "TEST_LEARN              ");
    out.push(w(100, word100));
    out.push(w(200, altimeter));
    out.extend(text_at(300, scratchpad));
    out
}

/// Arm a watcher and let the first sweep settle, which is what the editor waits
/// for before it tells the user to go ahead and flip something.
fn armed(t0: Instant) -> Watcher {
    let mut watcher = Watcher::new(&module(), t0);
    watcher.ingest(&sweep(0, 5000, "  12  "), t0);
    watcher.ingest(&sweep(0, 5000, "  12  "), t0 + DEFAULT_BASELINE_QUIET);
    assert!(watcher.ready(), "one quiet cycle is enough to have a baseline");
    watcher
}

#[test]
fn the_first_sighting_of_a_signal_is_not_a_movement() {
    let t0 = Instant::now();
    let watcher = armed(t0);
    // Arming during a flood of the whole address space is the normal case: the
    // editor is opened mid-flight. Reporting all of it would drown the one
    // thing the user is about to do.
    assert!(watcher.changes().is_empty(), "arriving is not moving");
}

#[test]
fn a_switch_thrown_once_outranks_the_gauges_that_never_stop() {
    let t0 = Instant::now();
    let mut watcher = armed(t0);

    // Thirty cycles of flight with the altimeter unwinding, and the gear lever
    // pulled during one of them.
    let mut at = t0 + Duration::from_secs(1);
    for step in 0..30u16 {
        let gear = if step >= 10 { 1 } else { 0 };
        watcher.ingest(&sweep(gear, 5000 - step, "  12  "), at);
        at += Duration::from_millis(300);
    }

    let changes = watcher.changes();
    assert_eq!(changes[0].id, "GEAR_LEVER", "the thing the user just did");
    assert_eq!(changes[0].moves, 1);
    assert_eq!(changes[0].from.as_deref(), Some("0"));
    assert_eq!(changes[0].to, "1");

    // The gauge is still listed. Somebody mapping an altimeter to a display
    // field needs to find it, and it is obvious from the count what it is.
    let gauge = changes.iter().find(|c| c.id == "ALTIMETER").expect("listed");
    assert_eq!(gauge.moves, 29);
    assert!(
        changes.iter().position(|c| c.id == "ALTIMETER") > Some(0),
        "but it does not sit on top of the answer"
    );
}

#[test]
fn a_signal_sharing_a_word_with_the_one_that_moved_is_not_reported() {
    let t0 = Instant::now();
    let mut watcher = armed(t0);

    // Word 100 carries the gear lever in its low two bits and the canopy lamp
    // in bit 2. Moving the lever moves the word, and a watcher that stopped at
    // "this word changed" would name both.
    watcher.ingest(&sweep(2, 5000, "  12  "), t0 + Duration::from_secs(1));

    let changes = watcher.changes();
    assert_eq!(changes.len(), 1, "one signal moved, not one word");
    assert_eq!(changes[0].id, "GEAR_LEVER");
    assert_eq!(changes[0].to, "2");
}

#[test]
fn a_string_spanning_several_words_counts_as_one_movement() {
    let t0 = Instant::now();
    let mut watcher = armed(t0);

    // Six characters is three words, all of them changing at once. Counting per
    // word would rank a scratchpad edit as three times busier than it is, and
    // the count is what the ordering is built on.
    watcher.ingest(&sweep(0, 5000, "ABCDEF"), t0 + Duration::from_secs(1));

    let changes = watcher.changes();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].id, "SCRATCHPAD");
    assert_eq!(changes[0].moves, 1);
    assert_eq!(changes[0].from.as_deref(), Some("  12  "));
    assert_eq!(changes[0].to, "ABCDEF");
    assert!(changes[0].text, "so the caller can quote it and keep the padding");
}

#[test]
fn a_run_of_movements_reports_where_it_started_and_where_it_is_now() {
    let t0 = Instant::now();
    let mut watcher = armed(t0);

    // A rotary walked up through its detents. What the user wants to read is
    // 0 to 2, not the last step of it.
    for (step, at) in [1u16, 2].iter().zip([1u64, 2]) {
        watcher.ingest(&sweep(*step, 5000, "  12  "), t0 + Duration::from_secs(at));
    }

    let changes = watcher.changes();
    assert_eq!(changes[0].from.as_deref(), Some("0"));
    assert_eq!(changes[0].to, "2");
    assert_eq!(changes[0].moves, 2);
}

#[test]
fn the_baseline_is_not_ready_until_the_stream_stops_showing_new_addresses() {
    let t0 = Instant::now();
    let mut watcher = Watcher::new(&module(), t0);
    assert!(!watcher.ready(), "nothing has arrived at all yet");

    watcher.ingest(&sweep(0, 5000, "  12  "), t0);
    // Every address here is new, so the map is still filling. An empty list at
    // this point means "still listening", and saying "nothing moved" would send
    // the user off to check their wiring.
    assert!(!watcher.ready());

    watcher.ingest(&sweep(0, 5000, "  12  "), t0 + Duration::from_millis(100));
    assert!(!watcher.ready(), "not quiet for long enough");

    watcher.ingest(&sweep(0, 5000, "  12  "), t0 + DEFAULT_BASELINE_QUIET);
    assert!(watcher.ready());
}

#[test]
fn watching_again_forgets_what_moved_but_not_where_things_are() {
    let t0 = Instant::now();
    let mut watcher = armed(t0);

    watcher.ingest(&sweep(1, 5000, "  12  "), t0 + Duration::from_secs(1));
    assert_eq!(watcher.changes().len(), 1);

    let again = t0 + Duration::from_secs(2);
    watcher.rearm(again);
    assert!(watcher.changes().is_empty(), "a clean sheet for the next flip");
    assert!(
        watcher.ready(),
        "and no second wait for the map, which is the point of keeping it"
    );

    watcher.ingest(&sweep(2, 5000, "  12  "), again + Duration::from_secs(1));
    let changes = watcher.changes();
    assert_eq!(changes.len(), 1);
    assert_eq!(
        changes[0].from.as_deref(),
        Some("1"),
        "from where it was when we re-armed, not where it started"
    );
    assert_eq!(changes[0].to, "2");
}

#[test]
fn the_aircraft_in_the_stream_is_reported() {
    let t0 = Instant::now();
    let watcher = armed(t0);
    // Editing a Hornet profile while sitting in a Hind is a thing that will
    // happen, and nothing the user flips will ever show up. The name is how the
    // window can say so instead of looking broken.
    assert_eq!(watcher.aircraft().as_deref(), Some("TEST_LEARN"));

    let empty = Watcher::new(&module(), t0);
    assert_eq!(empty.aircraft(), None, "before the stream has said anything");
}

#[test]
fn a_string_the_catalogue_could_not_size_is_left_alone() {
    let t0 = Instant::now();
    let mut watcher = armed(t0);

    // Without max_length there is no way to know where the field ends. Reading
    // a guessed length would report movement that belongs to whatever signal
    // sits after it, which is worse than not offering it.
    watcher.ingest(&[w(400, 0x4241)], t0 + Duration::from_secs(1));
    assert!(watcher.changes().is_empty());
}

#[test]
fn a_signal_shifted_past_its_word_is_watched_without_panicking() {
    // DCS-BIOS ships entries like this: the Mi-24P's wiper selectors are
    // mask 0, shift 16. Reading one took the listener thread down with it, and
    // with it the editor, the moment the Hind's stream arrived.
    let module: Module = serde_json::from_str(
        r#"{
          "module": "TEST_SHIFT",
          "aircraft": ["TEST_SHIFT"],
          "signals": [
            {
              "id": "PLT_WIPER_OFF",
              "control_type": "selector",
              "outputs": [{"address": 100, "mask": 0, "shift": 16, "max_value": 0, "max_length": null}]
            }
          ]
        }"#,
    )
    .expect("the fixture module parses");
    let t0 = Instant::now();
    let mut watcher = Watcher::new(&module, t0);
    watcher.ingest(&[w(100, 0)], t0);
    watcher.ingest(&[w(100, 0xffff)], t0 + Duration::from_secs(1));
    // It can never read anything but 0, so it never moves.
    assert!(watcher.changes().is_empty());
}

/// Every module in the local catalogue, fed a stream that moves every word it
/// publishes. Learn mode reads every signal a module has, so one bad entry
/// anywhere in it is enough to crash the editor. Skipped where the catalogue
/// has not been generated, except in the pipeline (`DSC_REQUIRE_CATALOGUE`).
#[test]
fn every_catalogue_module_survives_being_watched() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/catalogue");
    let catalogue = match dsc_config::Catalogue::load_dir(&dir) {
        Ok(catalogue) => catalogue,
        Err(e) if std::env::var_os("DSC_REQUIRE_CATALOGUE").is_some() => {
            panic!("DSC_REQUIRE_CATALOGUE is set and data/catalogue does not load: {e}")
        }
        Err(_) => return,
    };
    let t0 = Instant::now();
    for module in catalogue.modules() {
        let addresses: Vec<u16> = module
            .signals
            .iter()
            .flat_map(|s| &s.outputs)
            .flat_map(|o| {
                let words = o.max_length.map_or(1, |len| len.div_ceil(2).max(1));
                (0..words).map(move |i| o.address.wrapping_add(i * 2))
            })
            .collect();
        let mut watcher = Watcher::new(module, t0);
        for (step, value) in [0u16, 0xffff, 0x5a5a, 0].into_iter().enumerate() {
            let writes: Vec<BiosWrite> = addresses.iter().map(|&a| w(a, value)).collect();
            watcher.ingest(&writes, t0 + Duration::from_secs(step as u64));
        }
        let _ = (watcher.changes(), watcher.aircraft(), watcher.ready());
    }
}
