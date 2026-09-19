//! Replays a real A-10C flap cycle through the engine.
//!
//! Captured from DCS on 2026-09-16 with `FLAP_POS` and `FLAPS_SWITCH` watched on
//! one timeline, cycling UP, MVR, DN, MVR, UP with a pause at each detent. The
//! lever values here are measured, not reconstructed from the shape of the
//! gauge, which matters because an earlier reading of this data got the lever
//! ordering backwards.
//!
//! Three properties of the real gauge drive the thresholds, and all three would
//! be easy to miss from an idealised model:
//!
//! * **It overshoots and rings** rather than sliding to a stop.
//! * **MVR settles differently by direction**, 22726 arriving from retracted and
//!   23411 arriving from DN.
//! * **Retracted does not rest at zero.** It undershoots to 34, rebounds to 462
//!   and settles near 138. A second capture settled at 0. The rebound is higher
//!   than the first sample of real travel (283), so those regions overlap.

use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

use dsc_bios::Write;
use dsc_config::{Catalogue, DeviceInventory, Module, Profile};
use dsc_engine::{Engine, ACFT_NAME_LEN};

const PTO2: &str = "TAKEOFF_PLANEL_2";
const LEVER: u16 = 104;
const GAUGE: u16 = 106;

const FLAPS: u8 = 11;
const FULL: u8 = 13;
const HALF: u8 = 16;

/// Lever detents as measured. NOT the order the DCS-BIOS description implies.
const UP: u16 = 0;
const MVR: u16 = 1;
const DN: u16 = 2;

/// Lever to MVR: rises, overshoots to 23264, rings, settles at 22726.
const TO_MVR: &[u16] = &[
    283, 1174, 2565, 4379, 6234, 8027, 9539, 10915, 12136, 13430, 14756, 16232, 17700, 19216,
    20623, 21889, 22827, 23264, 23232, 22946, 22705, 22535, 22415, 22478, 22551, 22602, 22639,
    22664, 22691, 22701, 22709, 22714, 22718, 22720, 22722, 22723, 22724, 22725, 22726,
];

/// Lever to DN: the travel that would flash HALF without the lever condition.
const TO_DN: &[u16] = &[
    22895, 23558, 24781, 26446, 28301, 30103, 31716, 33128, 34418, 35689, 37018, 38427, 39898,
    41384, 42842, 44247, 45599, 46917, 48233, 49565, 50915, 52272, 53620, 54949, 56263, 57568,
    58869, 60168, 61463, 62752, 64005, 65011, 65535, 65483, 65398, 65338, 65296, 65266, 65244,
    65229, 65219, 65211, 65206, 65276, 65398, 65485, 65535,
];

/// Lever back to MVR: undershoots to 22778, settles at 23411.
const BACK_TO_MVR: &[u16] = &[
    64963, 63845, 62272, 60467, 58666, 57020, 55565, 54245, 52964, 51643, 50251, 48794, 47311,
    45849, 44434, 43071, 41746, 40428, 39098, 37750, 36395, 35046, 33715, 32398, 31089, 29786,
    28486, 27191, 25902, 24647, 23636, 23009, 22778, 22851, 23015, 23131, 23213, 23271, 23312,
    23341, 23362, 23376, 23386, 23392, 23397, 23401, 23404, 23406, 23408, 23409, 23410, 23411,
];

/// Lever to UP: undershoots to 34, rebounds to 462, settles at 138.
const TO_UP: &[u16] = &[
    23375, 23070, 22280, 21043, 19376, 17608, 15828, 14277, 12840, 11560, 10230, 8905, 7453, 6019,
    4525, 3113, 1706, 583, 34, 241, 387, 462, 367, 300, 252, 219, 195, 178, 166, 158, 152, 148,
    145, 143, 141, 140, 139, 138,
];

fn devices() -> DeviceInventory {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/devices.json");
    DeviceInventory::load(&path).expect("data/devices.json should parse")
}

fn catalogue() -> Catalogue {
    let module: Module = serde_json::from_str(
        r#"{
            "module": "A-10C",
            "aircraft": ["A-10C_2"],
            "signals": [
                {
                    "id": "FLAPS_SWITCH",
                    "control_type": "selector",
                    "outputs": [{ "address": 104, "mask": 65535, "shift": 0, "max_value": 2 }]
                },
                {
                    "id": "FLAP_POS",
                    "control_type": "analog_gauge",
                    "outputs": [{ "address": 106, "mask": 65535, "shift": 0, "max_value": 65535 }]
                }
            ]
        }"#,
    )
    .expect("fixture module should parse");
    Catalogue::from_modules(vec![module])
}

/// The three flap bindings exactly as `data/defaults/a-10c.json` carries them.
fn profile() -> Profile {
    serde_json::from_str(
        r#"{
            "name": "A-10C II",
            "aircraft": ["A-10C_2"],
            "module": "A-10C",
            "bindings": [
                {
                    "device": "TAKEOFF_PLANEL_2",
                    "led": "FLAPS",
                    "conditions": [
                        { "source": "FLAP_POS", "on_when": { "gte": 1000 } }
                    ]
                },
                {
                    "device": "TAKEOFF_PLANEL_2",
                    "led": "HALF",
                    "conditions": [
                        { "source": "FLAPS_SWITCH", "on_when": { "equals": 1 } },
                        { "source": "FLAP_POS", "on_when": { "between": [21000, 25000] } }
                    ]
                },
                {
                    "device": "TAKEOFF_PLANEL_2",
                    "led": "FULL",
                    "conditions": [
                        { "source": "FLAP_POS", "on_when": { "gte": 64000 } }
                    ]
                }
            ]
        }"#,
    )
    .expect("fixture profile should parse")
}

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

/// Drives the engine and tracks what each lamp is currently showing.
struct Panel {
    engine: Engine,
    lamps: HashMap<u8, u8>,
    now: Instant,
}

impl Panel {
    fn new() -> Self {
        let mut engine = Engine::new(devices(), catalogue(), vec![profile()]);
        engine.set_connected(vec![PTO2.to_string()]);
        let mut panel = Panel {
            engine,
            lamps: HashMap::new(),
            now: Instant::now(),
        };
        panel.feed(&acft_name("A-10C_2"));
        panel.now += Duration::from_secs(1);
        let batch = panel.engine.tick(panel.now);
        panel.record(&batch);
        panel
    }

    fn record(&mut self, batch: &dsc_engine::Batch) {
        for w in &batch.writes {
            self.lamps.insert(w.id.index, w.value);
        }
    }

    fn feed(&mut self, writes: &[Write]) {
        self.now += Duration::from_millis(50);
        let batch = self.engine.ingest(writes, self.now);
        self.record(&batch);
    }

    fn lever(&mut self, value: u16) {
        self.feed(&[Write { address: LEVER, value }]);
    }

    fn gauge(&mut self, value: u16) {
        self.feed(&[Write { address: GAUGE, value }]);
    }

    fn lit(&self, index: u8) -> bool {
        self.lamps.get(&index).copied().unwrap_or(0) != 0
    }
}

#[test]
fn the_captured_flap_cycle_drives_the_lamps_correctly() {
    let mut p = Panel::new();

    // Retracted, lever UP.
    p.lever(UP);
    p.gauge(0);
    assert!(!p.lit(FLAPS), "retracted: FLAPS dark");
    assert!(!p.lit(HALF));
    assert!(!p.lit(FULL));

    // UP to MVR.
    p.lever(MVR);
    for &v in TO_MVR {
        p.gauge(v);
    }
    assert!(p.lit(FLAPS), "at MVR: FLAPS lit");
    assert!(p.lit(HALF), "at MVR: HALF lit at the 22726 plateau");
    assert!(!p.lit(FULL), "at MVR: FULL dark");

    // MVR to DN. The flaps pass straight through the half window on the way, so
    // this is the travel the lever condition exists for.
    p.lever(DN);
    assert!(!p.lit(HALF), "HALF goes out with the lever, before any travel");
    let mut crossed_window = false;
    for &v in TO_DN {
        p.gauge(v);
        if (21000..=25000).contains(&v) {
            crossed_window = true;
        }
        assert!(!p.lit(HALF), "HALF flashed at FLAP_POS={v} on the way to DN");
    }
    assert!(
        crossed_window,
        "this capture must actually cross the half window, or it proves nothing"
    );
    assert!(p.lit(FULL), "at DN: FULL lit at the 65535 plateau");
    assert!(p.lit(FLAPS), "at DN: FLAPS still lit");

    // DN back to MVR.
    p.lever(MVR);
    for &v in BACK_TO_MVR {
        p.gauge(v);
    }
    assert!(
        p.lit(HALF),
        "back at MVR: HALF lit at the 23411 plateau, which differs from 22726"
    );
    assert!(!p.lit(FULL), "back at MVR: FULL dark");

    // MVR to UP, including the rebound at the end.
    p.lever(UP);
    assert!(!p.lit(HALF), "HALF goes out with the lever");
    for &v in TO_UP {
        p.gauge(v);
    }
    assert!(
        !p.lit(FLAPS),
        "retracted: FLAPS must be dark even though the gauge rests near 138, not 0"
    );
    assert!(!p.lit(HALF));
    assert!(!p.lit(FULL));
}

#[test]
fn the_retracted_rebound_does_not_light_flaps() {
    // The bug this replaced: a floor of 100 came from a capture that happened to
    // reach exactly 0. Retracted actually rests near 138 and rebounds to 462 on
    // the way there, so that floor left FLAPS lit with the flaps fully up.
    for value in [34u16, 138, 241, 387, 462] {
        let mut p = Panel::new();
        p.lever(UP);
        p.gauge(value);
        assert!(
            !p.lit(FLAPS),
            "FLAPS lit at FLAP_POS={value}, which is a measured retracted reading"
        );
    }
}

#[test]
fn both_measured_mvr_plateaus_fall_inside_the_half_window() {
    // Guards the window against being tightened around one direction only.
    // 22726 is where MVR settles arriving from retracted, 23411 arriving from
    // DN, and the ringing reaches 22415 and 23476 across two captures.
    for value in [22415u16, 22726, 23264, 23411, 23476] {
        let mut p = Panel::new();
        p.lever(MVR);
        p.gauge(value);
        assert!(p.lit(HALF), "HALF should be lit at a measured MVR value {value}");
    }
}

#[test]
fn the_full_threshold_clears_the_ringing_at_dn() {
    // DN rings down to 65206 before settling at 65535. A threshold above that
    // would make FULL flicker once after the flaps had already arrived.
    for value in [65206u16, 65219, 65535] {
        let mut p = Panel::new();
        p.lever(DN);
        p.gauge(value);
        assert!(p.lit(FULL), "FULL should stay lit at a measured DN value {value}");
    }
}

#[test]
fn the_lever_ordering_is_the_reverse_of_the_dcs_bios_description() {
    // DCS-BIOS describes FLAPS_SWITCH as "Flaps Setting DN - MVR - UP", which
    // reads as 0=DN. Measurement says otherwise: 0=UP, 2=DN. Pinned here so
    // nobody re-derives it from the definition and silently inverts the lamps.
    let mut p = Panel::new();
    p.lever(UP);
    p.gauge(65535);
    assert!(!p.lit(HALF), "lever 0 is UP, not a half-flaps setting");

    let mut p = Panel::new();
    p.lever(DN);
    p.gauge(22726);
    assert!(!p.lit(HALF), "lever 2 is DN, not a half-flaps setting");

    let mut p = Panel::new();
    p.lever(MVR);
    p.gauge(22726);
    assert!(p.lit(HALF), "lever 1 is MVR");
}
