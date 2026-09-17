//! End-to-end tests for the module-load sequence.
//!
//! These run against the real `data/devices.json`, so the LED counts and the
//! dimmer/indicator split are the measured hardware facts rather than a mock.
//! The catalogue is a fixture: `data/catalogue` is generated per machine and
//! gitignored, so a test depending on it would fail on a fresh clone.

use std::path::Path;
use std::time::{Duration, Instant};

use wctrl_bios::Write;
use wctrl_config::{Catalogue, DeviceInventory, Module, Profile};
use wctrl_engine::{Cause, Engine, LedId, ACFT_NAME_LEN};

const PTO2: &str = "TAKEOFF_PLANEL_2";

/// Address of the fixture's caution lamp, and of its panel dimmer.
const CAUTION_ADDR: u16 = 100;
const DIMMER_ADDR: u16 = 102;

/// Every LED the PTO2 declares, read from the inventory rather than written as
/// a literal. Index 3 was found on hardware after these tests were written, and
/// a hardcoded count turned that discovery into four failing tests.

/// Indices the reload tests name, read from the inventory so that a hardware
/// discovery renumbering a lamp does not turn these into silent passes.
fn led_index(name: &str) -> u8 {
    devices()
        .device(PTO2)
        .expect("PTO2 should be in the inventory")
        .led(name)
        .unwrap_or_else(|| panic!("{name} should be a lamp on the PTO2"))
        .1
        .index
}

fn pto2_led_count() -> usize {
    devices()
        .device(PTO2)
        .expect("PTO2 should be in the inventory")
        .leds()
        .count()
}

fn devices() -> DeviceInventory {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/devices.json");
    DeviceInventory::load(&path).expect("data/devices.json should parse")
}

fn catalogue() -> Catalogue {
    let module: Module = serde_json::from_str(
        r#"{
            "module": "TEST",
            "aircraft": ["FA-18C_hornet"],
            "signals": [
                {
                    "id": "MASTER_CAUTION_LT",
                    "control_type": "led",
                    "outputs": [
                        { "address": 100, "mask": 4096, "shift": 12, "max_value": 1 }
                    ]
                },
                {
                    "id": "INST_PNL_DIMMER",
                    "control_type": "analog_dial",
                    "outputs": [
                        { "address": 102, "mask": 65535, "shift": 0, "max_value": 65535 }
                    ]
                },
                {
                    "id": "FLAPS_SWITCH",
                    "control_type": "selector",
                    "outputs": [
                        { "address": 104, "mask": 65535, "shift": 0, "max_value": 2 }
                    ]
                },
                {
                    "id": "FLAP_POS",
                    "control_type": "analog_gauge",
                    "outputs": [
                        { "address": 106, "mask": 65535, "shift": 0, "max_value": 65535 }
                    ]
                }
            ]
        }"#,
    )
    .expect("fixture module should parse");
    Catalogue::from_modules(vec![module])
}

fn profile() -> Profile {
    serde_json::from_str(
        r#"{
            "name": "Hornet",
            "aircraft": ["FA-18C_hornet"],
            "module": "TEST",
            "bindings": [
                {
                    "device": "TAKEOFF_PLANEL_2",
                    "led": "Master_Caution",
                    "conditions": [
                        { "source": "MASTER_CAUTION_LT", "on_when": { "equals": 1 } }
                    ]
                },
                {
                    "device": "TAKEOFF_PLANEL_2",
                    "led": "Backlight",
                    "conditions": [
                        { "source": "INST_PNL_DIMMER", "on_when": { "scale": [0, 65535] } }
                    ]
                }
            ]
        }"#,
    )
    .expect("fixture profile should parse")
}

fn engine_with(profiles: Vec<Profile>) -> Engine {
    let mut e = Engine::new(devices(), catalogue(), profiles);
    e.set_connected(vec![PTO2.to_string()]);
    e
}

/// `_ACFT_NAME` as DCS-BIOS puts it on the wire: 24 bytes, two per word, at
/// even byte addresses starting from zero.
fn acft_name(name: &str) -> Vec<Write> {
    let mut bytes = name.as_bytes().to_vec();
    bytes.resize(ACFT_NAME_LEN as usize, 0);
    (0..ACFT_NAME_LEN / 2)
        .map(|i| {
            let b = i as usize * 2;
            Write {
                address: i * 2,
                value: u16::from_le_bytes([bytes[b], bytes[b + 1]]),
            }
        })
        .collect()
}

fn led(index: u8) -> LedId {
    LedId {
        device: PTO2.to_string(),
        part_id: 48901,
        index,
    }
}

fn value_of(batch: &wctrl_engine::Batch, index: u8) -> Option<u8> {
    let want = led(index);
    batch
        .writes
        .iter()
        .find(|w| w.id == want)
        .map(|w| w.value)
}

#[test]
fn an_aircraft_change_is_detected_from_the_name_string() {
    let mut e = engine_with(vec![profile()]);
    assert_eq!(e.aircraft(), None);

    let t0 = Instant::now();
    e.ingest(&acft_name("FA-18C_hornet"), t0);

    assert_eq!(e.aircraft(), Some("FA-18C_hornet"));
    assert_eq!(e.active_profile().map(|p| p.name.as_str()), Some("Hornet"));
}

#[test]
fn nothing_is_written_until_the_post_load_flood_settles() {
    let mut e = engine_with(vec![profile()]);
    let t0 = Instant::now();

    // The name arrives, then the flood. Neither may produce a write: sweeping
    // mid-flood would latch values that are about to be superseded.
    assert!(e.ingest(&acft_name("FA-18C_hornet"), t0).is_empty());
    let mid = t0 + Duration::from_millis(50);
    assert!(e
        .ingest(&[Write { address: CAUTION_ADDR, value: 0x1000 }], mid)
        .is_empty());

    // Quiet for longer than the settle window, and the sweep lands.
    let settled = mid + wctrl_engine::DEFAULT_SETTLE_QUIET;
    let batch = e.tick(settled);
    assert_eq!(batch.cause, Cause::ModuleLoad);
    assert!(!batch.is_empty());
}

#[test]
fn the_sweep_covers_every_led_and_zeroes_the_unbound_ones() {
    let mut e = engine_with(vec![profile()]);
    let t0 = Instant::now();
    e.ingest(&acft_name("FA-18C_hornet"), t0);
    e.ingest(
        &[
            Write { address: CAUTION_ADDR, value: 0x1000 },
            Write { address: DIMMER_ADDR, value: u16::MAX },
        ],
        t0,
    );
    let batch = e.tick(t0 + Duration::from_secs(1));

    // Every LED on the panel is written exactly once - this is the single
    // sweep, not a reset followed by a sync.
    assert_eq!(batch.writes.len(), pto2_led_count());
    let mut seen: Vec<_> = batch.writes.iter().map(|w| w.id.index).collect();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen.len(), pto2_led_count());

    // Bound LEDs take their resolved values...
    assert_eq!(value_of(&batch, 4), Some(1), "Master_Caution is an indicator");
    assert_eq!(value_of(&batch, 0), Some(255), "Backlight scales to full");

    // ...and everything the profile does not mention is driven to zero in the
    // same pass, so a previous module cannot leave a lamp lit.
    assert_eq!(value_of(&batch, 17), Some(0), "HOOK is unbound");
    assert_eq!(value_of(&batch, 2), Some(0), "SL is unbound");
}

#[test]
fn an_unbound_governor_is_swept_to_zero() {
    // Documents a known, accepted consequence rather than asserting it is good:
    // SL gates the PTO2 indicators, so sweeping it to zero leaves bound lamps
    // dark. The decision (2026-09-16) is that the editor warns about this at
    // authoring time and the engine stays dumb. See docs/STATUS.md.
    let mut e = engine_with(vec![profile()]);
    let t0 = Instant::now();
    e.ingest(&acft_name("FA-18C_hornet"), t0);
    e.ingest(&[Write { address: CAUTION_ADDR, value: 0x1000 }], t0);
    let batch = e.tick(t0 + Duration::from_secs(1));

    assert_eq!(value_of(&batch, 4), Some(1), "CAUTION is commanded on");
    assert_eq!(value_of(&batch, 2), Some(0), "but SL, its governor, is not bound");
}

#[test]
fn after_the_sweep_only_changed_leds_are_written() {
    let mut e = engine_with(vec![profile()]);
    let t0 = Instant::now();
    e.ingest(&acft_name("FA-18C_hornet"), t0);
    e.tick(t0 + Duration::from_secs(1));

    let t1 = t0 + Duration::from_secs(2);

    // Caution comes on: one write, for one LED.
    let batch = e.ingest(&[Write { address: CAUTION_ADDR, value: 0x1000 }], t1);
    assert_eq!(batch.cause, Cause::SignalChange);
    assert_eq!(batch.writes.len(), 1);
    assert_eq!(value_of(&batch, 4), Some(1));

    // The same value again writes nothing. The device latches and there is no
    // host watchdog, so a redundant write is pure bus traffic.
    assert!(e
        .ingest(&[Write { address: CAUTION_ADDR, value: 0x1000 }], t1)
        .is_empty());

    // An address nothing binds writes nothing.
    assert!(e
        .ingest(&[Write { address: 999, value: 1 }], t1)
        .is_empty());

    // And going off writes once more.
    let batch = e.ingest(&[Write { address: CAUTION_ADDR, value: 0 }], t1);
    assert_eq!(value_of(&batch, 4), Some(0));
}

#[test]
fn a_continuous_source_scales_across_its_range() {
    let mut e = engine_with(vec![profile()]);
    let t0 = Instant::now();
    e.ingest(&acft_name("FA-18C_hornet"), t0);
    e.tick(t0 + Duration::from_secs(1));
    let t1 = t0 + Duration::from_secs(2);

    let half = e.ingest(&[Write { address: DIMMER_ADDR, value: 32767 }], t1);
    assert_eq!(value_of(&half, 0), Some(127));

    let full = e.ingest(&[Write { address: DIMMER_ADDR, value: 65535 }], t1);
    assert_eq!(value_of(&full, 0), Some(255));
}

#[test]
fn an_aircraft_with_no_profile_still_clears_the_panel() {
    let mut e = engine_with(vec![profile()]);
    let t0 = Instant::now();

    // Land in the Hornet and light something.
    e.ingest(&acft_name("FA-18C_hornet"), t0);
    e.ingest(&[Write { address: CAUTION_ADDR, value: 0x1000 }], t0);
    let lit = e.tick(t0 + Duration::from_secs(1));
    assert_eq!(value_of(&lit, 4), Some(1));

    // Switch to an aircraft nothing is configured for.
    let t1 = t0 + Duration::from_secs(5);
    e.ingest(&acft_name("A-10C_2"), t1);
    let batch = e.tick(t1 + Duration::from_secs(1));

    assert_eq!(e.aircraft(), Some("A-10C_2"));
    assert!(e.active_profile().is_none());
    assert_eq!(batch.writes.len(), pto2_led_count());
    assert!(
        batch.writes.iter().all(|w| w.value == 0),
        "an unconfigured module must not inherit the previous one's lamps"
    );
}

#[test]
fn shutdown_clears_only_what_is_lit() {
    let mut e = engine_with(vec![profile()]);
    let t0 = Instant::now();
    e.ingest(&acft_name("FA-18C_hornet"), t0);
    e.ingest(
        &[
            Write { address: CAUTION_ADDR, value: 0x1000 },
            Write { address: DIMMER_ADDR, value: u16::MAX },
        ],
        t0,
    );
    e.tick(t0 + Duration::from_secs(1));

    // Only the two lit LEDs need clearing; the other fifteen are already zero.
    let batch = e.shutdown();
    assert_eq!(batch.cause, Cause::Shutdown);
    assert_eq!(batch.writes.len(), 2);
    assert!(batch.writes.iter().all(|w| w.value == 0));

    // Shutting down twice is a no-op, not a second round of writes.
    assert!(e.shutdown().is_empty());
}

#[test]
fn shutdown_does_not_touch_leds_we_never_drove() {
    // Starting and stopping the converter with no mission running must leave
    // the panel exactly as it was. Anything we did not write belongs to
    // whoever did - usually SimAppPro's persisted backlight.
    let mut e = engine_with(vec![profile()]);
    assert!(
        e.shutdown().is_empty(),
        "a converter that never wrote an LED must not blank the panel on exit"
    );
}

#[test]
fn the_settle_window_has_an_upper_bound() {
    // A cockpit that never goes quiet must still get its sweep.
    let mut e = engine_with(vec![profile()]);
    e.set_settle(Duration::from_millis(250), Duration::from_millis(1000));

    let t0 = Instant::now();
    e.ingest(&acft_name("FA-18C_hornet"), t0);

    // Continuous traffic, 100 ms apart, never satisfying the quiet window.
    // Runs past the 1000 ms cap so the bound is what fires the sweep, not luck.
    let mut now = t0;
    for _ in 0..15 {
        now += Duration::from_millis(100);
        let batch = e.ingest(&[Write { address: DIMMER_ADDR, value: 1 }], now);
        if !batch.is_empty() {
            assert_eq!(batch.cause, Cause::ModuleLoad);
            assert_eq!(batch.writes.len(), pto2_led_count());
            return;
        }
    }
    panic!("the sweep never fired within the maximum settle window");
}

#[test]
fn an_unassigned_lamp_is_swept_off_and_never_driven() {
    // A half-configured profile is a normal state, not an error: the user fills
    // lamps in one at a time. An unassigned row must load, sweep off, and stay
    // out of the incremental path entirely.
    let mut p = profile();
    p.bindings.push(wctrl_config::Binding {
        device: PTO2.to_string(),
        led: "HOOK".to_string(),
        conditions: Vec::new(),
        always: false,
        any_of: Vec::new(),
        same_as: None,
        on: None,
        off: 0,
        note: "not decided yet".to_string(),
    });
    assert!(p.bindings.last().unwrap().is_placeholder());

    let mut e = engine_with(vec![p]);
    let t0 = Instant::now();
    e.ingest(&acft_name("FA-18C_hornet"), t0);
    e.ingest(&[Write { address: CAUTION_ADDR, value: 0x1000 }], t0);
    let batch = e.tick(t0 + Duration::from_secs(1));

    assert_eq!(batch.writes.len(), pto2_led_count(), "the sweep still covers every lamp");
    assert_eq!(value_of(&batch, 17), Some(0), "an unassigned lamp is off");

    // Nothing the profile does not actually bind should ever produce a write.
    let t1 = t0 + Duration::from_secs(2);
    assert!(e.ingest(&[Write { address: 999, value: 1 }], t1).is_empty());
}

#[test]
fn a_stub_profile_covers_every_lamp_and_binds_none_of_them() {
    let devs = devices();
    let stub = Profile::stub("A-10C II", "A-10C_2", "TEST", &devs);

    let lamps: usize = devs.devices.iter().flat_map(|d| d.leds()).count();
    assert_eq!(stub.bindings.len(), lamps, "one row per lamp on the hardware");
    assert!(stub.bindings.iter().all(|b| b.is_placeholder()));

    // And it must survive the same validation a hand-written profile gets,
    // or auto-generating one would produce a file the loader then rejects.
    let cat = catalogue();
    let module = cat.module("TEST").expect("fixture module");
    stub.validate(module, &devs).expect("a stub must validate");
}

const LEVER_ADDR: u16 = 104;
const GAUGE_ADDR: u16 = 106;

/// A profile whose HALF lamp needs the lever at MVR *and* the flaps off the
/// stops, which is the A-10C mapping that motivated multi-condition bindings.
fn flap_profile() -> Profile {
    serde_json::from_str(
        r#"{
            "name": "Flaps",
            "aircraft": ["FA-18C_hornet"],
            "module": "TEST",
            "bindings": [
                {
                    "device": "TAKEOFF_PLANEL_2",
                    "led": "HALF",
                    "conditions": [
                        { "source": "FLAPS_SWITCH", "on_when": { "equals": 1 } },
                        { "source": "FLAP_POS", "on_when": { "gte": 300 } }
                    ]
                }
            ]
        }"#,
    )
    .expect("flap fixture should parse")
}

#[test]
fn every_condition_must_hold_for_the_lamp_to_light() {
    let mut e = engine_with(vec![flap_profile()]);
    let t0 = Instant::now();
    e.ingest(&acft_name("FA-18C_hornet"), t0);
    e.ingest(
        &[
            Write { address: LEVER_ADDR, value: 1 },
            Write { address: GAUGE_ADDR, value: 20000 },
        ],
        t0,
    );
    let batch = e.tick(t0 + Duration::from_secs(1));
    assert_eq!(value_of(&batch, 16), Some(1), "lever at MVR and flaps deployed");

    let t1 = t0 + Duration::from_secs(2);

    // Lever moves away: the lamp goes out even though the gauge still reads
    // deployed. Either condition failing is enough.
    let batch = e.ingest(&[Write { address: LEVER_ADDR, value: 0 }], t1);
    assert_eq!(value_of(&batch, 16), Some(0));

    // Gauge alone cannot bring it back.
    let batch = e.ingest(&[Write { address: GAUGE_ADDR, value: 65535 }], t1);
    assert_eq!(value_of(&batch, 16), None, "no write: it is already off");
}

#[test]
fn the_lever_condition_stops_half_flashing_during_travel_to_full() {
    // The reason this feature exists. Selecting DN from MVR sweeps FLAP_POS
    // through the half range; without the lever condition HALF would blink.
    let mut e = engine_with(vec![flap_profile()]);
    let t0 = Instant::now();
    e.ingest(&acft_name("FA-18C_hornet"), t0);
    e.ingest(
        &[
            Write { address: LEVER_ADDR, value: 1 },
            Write { address: GAUGE_ADDR, value: 20000 },
        ],
        t0,
    );
    e.tick(t0 + Duration::from_secs(1));

    let t1 = t0 + Duration::from_secs(2);
    // Lever to DN first, exactly as the aircraft reports it.
    let batch = e.ingest(&[Write { address: LEVER_ADDR, value: 0 }], t1);
    assert_eq!(value_of(&batch, 16), Some(0), "HALF goes out with the lever");

    // Now the flaps travel from MVR to DN, passing right through the half
    // range. Not one write, because the lever condition is already false.
    for step in [25000u16, 35000, 45000, 55000, 65535] {
        let batch = e.ingest(&[Write { address: GAUGE_ADDR, value: step }], t1);
        assert!(
            batch.is_empty(),
            "HALF flickered at FLAP_POS={step} during travel to DN"
        );
    }
}

#[test]
fn a_continuous_source_can_be_gated_by_a_switch() {
    // The combining rule is "dimmest value any condition asks for", which has to
    // leave a scaled value intact while the gate is satisfied.
    let p: Profile = serde_json::from_str(
        r#"{
            "name": "Gated backlight",
            "aircraft": ["FA-18C_hornet"],
            "module": "TEST",
            "bindings": [
                {
                    "device": "TAKEOFF_PLANEL_2",
                    "led": "Backlight",
                    "conditions": [
                        { "source": "FLAPS_SWITCH", "on_when": { "equals": 1 } },
                        { "source": "INST_PNL_DIMMER", "on_when": { "scale": [0, 65535] } }
                    ]
                }
            ]
        }"#,
    )
    .expect("fixture should parse");

    let mut e = engine_with(vec![p]);
    let t0 = Instant::now();
    e.ingest(&acft_name("FA-18C_hornet"), t0);
    e.ingest(
        &[
            Write { address: LEVER_ADDR, value: 1 },
            Write { address: DIMMER_ADDR, value: 32767 },
        ],
        t0,
    );
    let batch = e.tick(t0 + Duration::from_secs(1));
    assert_eq!(value_of(&batch, 0), Some(127), "gate open: the scaled value survives");

    let t1 = t0 + Duration::from_secs(2);
    let batch = e.ingest(&[Write { address: LEVER_ADDR, value: 2 }], t1);
    assert_eq!(value_of(&batch, 0), Some(0), "gate shut: dark regardless of the dial");
}

/// A profile edited while the daemon runs, from the engine's side.
///
/// Same aircraft, one binding changed: the caution lamp now follows the dimmer
/// instead of the caution light.
fn edited_profile() -> Profile {
    serde_json::from_str(
        r#"{
            "name": "Hornet",
            "aircraft": ["FA-18C_hornet"],
            "module": "TEST",
            "bindings": [
                {
                    "device": "TAKEOFF_PLANEL_2",
                    "led": "Master_Caution",
                    "conditions": [
                        { "source": "INST_PNL_DIMMER", "on_when": { "gte": 1 } }
                    ]
                }
            ]
        }"#,
    )
    .expect("fixture profile should parse")
}

/// Drive an engine to the state a running daemon is in: aircraft detected,
/// flood settled, lamps swept.
fn flying() -> Engine {
    let mut e = engine_with(vec![profile()]);
    let mut now = Instant::now();
    e.ingest(&acft_name("FA-18C_hornet"), now);
    e.ingest(
        &[
            // 0x1000 because the fixture masks the caution bit at 4096.
            Write { address: CAUTION_ADDR, value: 0x1000 },
            Write { address: DIMMER_ADDR, value: 65535 },
        ],
        now,
    );
    now += Duration::from_secs(1);
    e.tick(now);
    e
}

#[test]
fn reloading_profiles_resweeps_against_the_state_already_known() {
    let mut e = flying();

    // The caution lamp was lit by MASTER_CAUTION_LT, which has not moved. The
    // new profile ties it to the dimmer instead, and the dimmer is at full, so
    // the lamp stays lit for a different reason. Nothing re-sent a signal, so
    // only a full sweep could have noticed.
    let batch = e.set_profiles(vec![edited_profile()]);
    assert_eq!(batch.cause, Cause::ProfileReload);
    assert_eq!(value_of(&batch, led_index("Master_Caution")), Some(1));

    // Backlight is gone from the profile entirely, so it must be driven off
    // rather than left where the previous sweep put it.
    assert_eq!(value_of(&batch, led_index("Backlight")), Some(0));
}

#[test]
fn a_reload_keeps_the_signal_state() {
    // The reload must not clear what DCS-BIOS has already told us. Dropping it
    // would blank the panel until the next re-export refilled it, which is the
    // exact flicker a reload exists to avoid.
    let mut e = flying();
    let batch = e.set_profiles(vec![profile()]);
    assert_eq!(
        value_of(&batch, led_index("Master_Caution")),
        Some(1),
        "the caution signal was seen before the reload and is still known after it"
    );
}

#[test]
fn reloading_before_an_aircraft_is_known_writes_nothing() {
    // Editing a profile with DCS closed is normal. There is no cockpit to sync
    // to, so there is nothing to write, and the new profiles are simply used
    // when an aircraft next appears.
    let mut e = engine_with(vec![profile()]);
    let batch = e.set_profiles(vec![edited_profile()]);
    assert!(batch.writes.is_empty());

    let batch = e.ingest(&acft_name("FA-18C_hornet"), Instant::now());
    assert!(batch.writes.is_empty(), "still settling");
    assert_eq!(
        e.active_profile().map(|p| p.bindings.len()),
        Some(1),
        "the edited profile is the one that got selected"
    );
}

#[test]
fn a_reload_while_still_settling_defers_to_the_pending_sweep() {
    // A reload mid-flood must not sweep early: the flood has not finished
    // describing the cockpit, so a sweep now would write half-known values.
    let mut e = engine_with(vec![profile()]);
    let now = Instant::now();
    e.ingest(&acft_name("FA-18C_hornet"), now);

    let batch = e.set_profiles(vec![edited_profile()]);
    assert!(batch.writes.is_empty(), "nothing is written while settling");

    // The sweep that was already coming uses the profile that just arrived.
    e.ingest(&[Write { address: DIMMER_ADDR, value: 65535 }], now);
    let batch = e.tick(now + Duration::from_secs(1));
    assert_eq!(batch.cause, Cause::ModuleLoad);
    assert_eq!(value_of(&batch, led_index("Master_Caution")), Some(1));
}

/// A mission ending with DCS still running.
///
/// The panels clear, and the *same* aircraft loading again is treated as a
/// fresh load. This is the case that fails silently if the aircraft name is
/// left set: nothing looks like a change, no sweep runs, and the panels simply
/// stay dark for the rest of the session.
#[test]
fn a_mission_ending_forgets_the_cockpit_so_the_next_one_sweeps() {
    let mut e = flying();

    let batch = e.mission_ended();
    assert_eq!(batch.cause, Cause::Shutdown);
    assert_eq!(
        value_of(&batch, led_index("Master_Caution")),
        Some(0),
        "the caution lamp was lit, so it must be cleared"
    );
    assert_eq!(e.aircraft(), None, "the cockpit is forgotten");

    // The same aircraft again. Without forgetting, this is not a change.
    let mut now = Instant::now();
    e.ingest(&acft_name("FA-18C_hornet"), now);
    e.ingest(
        &[Write { address: CAUTION_ADDR, value: 0x1000 }],
        now,
    );
    now += Duration::from_secs(1);
    let batch = e.tick(now);
    assert_eq!(batch.cause, Cause::ModuleLoad);
    assert_eq!(
        value_of(&batch, led_index("Master_Caution")),
        Some(1),
        "the second mission must sweep the panels back to life"
    );
}
