//! A lamp that follows whichever of two knobs was turned last.
//!
//! The case is any two-seat aircraft with a lighting knob per seat and no
//! signal saying which seat the player is in. Nothing here names an aircraft:
//! the module is a fixture with two knobs and a neighbour sharing one of their
//! words.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use wctrl_bios::Write;
use wctrl_config::{Catalogue, DeviceInventory, Module, Profile};
use wctrl_engine::{Batch, Engine, ACFT_NAME_LEN};

const PTO2: &str = "TAKEOFF_PLANEL_2";
const BACKLIGHT: u8 = 0;
const FRONT: u16 = 200;
/// The rear knob's word, whose high byte belongs to an unrelated switch.
const REAR: u16 = 202;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn catalogue() -> Catalogue {
    let json = format!(
        r#"{{
            "module": "TEST_TWO_SEAT",
            "aircraft": ["TEST_TWO_SEAT"],
            "signals": [
                {{ "id": "FRONT_KNOB", "control_type": "selector",
                   "outputs": [{{ "address": {FRONT}, "mask": 255, "shift": 0, "max_value": 8 }}] }},
                {{ "id": "REAR_KNOB", "control_type": "selector",
                   "outputs": [{{ "address": {REAR}, "mask": 255, "shift": 0, "max_value": 8 }}] }},
                {{ "id": "REAR_OTHER", "control_type": "selector",
                   "outputs": [{{ "address": {REAR}, "mask": 65280, "shift": 8, "max_value": 1 }}] }}
            ]
        }}"#
    );
    Catalogue::from_modules(vec![serde_json::from_str::<Module>(&json).expect("fixture parses")])
}

fn profile() -> Profile {
    serde_json::from_str(
        r#"{
            "name": "Two seats",
            "aircraft": ["TEST_TWO_SEAT"],
            "module": "TEST_TWO_SEAT",
            "bindings": [
                {
                    "device": "TAKEOFF_PLANEL_2",
                    "led": "Backlight",
                    "any_of": [
                        { "conditions": [ { "source": "FRONT_KNOB", "on_when": { "scale": [0, 8] } } ] },
                        { "conditions": [ { "source": "REAR_KNOB", "on_when": { "scale": [0, 8] } } ] }
                    ],
                    "pick": "latest"
                }
            ]
        }"#,
    )
    .expect("fixture profile parses")
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

struct Pit {
    engine: Engine,
    now: Instant,
    backlight: Option<u8>,
}

impl Pit {
    /// Loaded, settled, with the front knob at 2 and the rear at 8.
    fn loaded() -> Self {
        let devices = DeviceInventory::load(&root().join("data/devices.json")).expect("devices");
        let mut engine = Engine::new(devices, catalogue(), vec![profile()]);
        engine.set_connected(vec![PTO2.to_string()]);
        let mut pit = Pit {
            engine,
            now: Instant::now(),
            backlight: None,
        };
        let name = acft_name("TEST_TWO_SEAT");
        pit.feed(&name);
        pit.feed(&[w(FRONT, 2), w(REAR, 8)]);
        pit.now += Duration::from_secs(1);
        let sweep = pit.engine.tick(pit.now);
        pit.take(&sweep);
        pit
    }

    fn feed(&mut self, writes: &[Write]) {
        self.now += Duration::from_millis(50);
        let batch = self.engine.ingest(writes, self.now);
        self.take(&batch);
    }

    fn take(&mut self, batch: &Batch) {
        for write in &batch.writes {
            if write.id.device == PTO2 && write.id.index == BACKLIGHT {
                self.backlight = Some(write.value);
            }
        }
    }
}

fn w(address: u16, value: u16) -> Write {
    Write { address, value }
}

#[test]
fn before_either_knob_moves_the_brighter_one_lights_the_panel() {
    // The load flood set both knobs, but nobody has turned anything. A lit
    // panel is the safer guess than a dark one.
    assert_eq!(Pit::loaded().backlight, Some(255));
}

#[test]
fn the_knob_turned_last_drives_the_panel_even_downwards() {
    let mut pit = Pit::loaded();
    // Front seat turns theirs from 2 to 3. Brightest would stay at the rear's
    // 8 and ignore them.
    pit.feed(&[w(FRONT, 3)]);
    assert_eq!(pit.backlight, Some(95));
    // Rear seat takes it back by turning theirs.
    pit.feed(&[w(REAR, 6)]);
    assert_eq!(pit.backlight, Some(191));
    // And the front again, down to 1.
    pit.feed(&[w(FRONT, 1)]);
    assert_eq!(pit.backlight, Some(31));
}

#[test]
fn a_neighbour_sharing_the_word_is_not_the_knob_moving() {
    let mut pit = Pit::loaded();
    pit.feed(&[w(FRONT, 1)]);
    assert_eq!(pit.backlight, Some(31));
    // A switch in the high byte of the rear knob's word flips. The word moved,
    // the rear knob did not, so the front knob still has the panel.
    pit.feed(&[w(REAR, 8 | (1 << 8))]);
    assert_eq!(pit.backlight, Some(31));
}

#[test]
fn saving_the_profile_keeps_which_knob_was_turned_last() {
    let mut pit = Pit::loaded();
    pit.feed(&[w(FRONT, 1)]);
    // The editor saves, and the daemon reloads its profiles. Forgetting here
    // would jump the panel to the brighter knob for no reason.
    let batch = pit.engine.set_profiles(vec![profile()]);
    pit.take(&batch);
    assert_eq!(pit.backlight, Some(31));
}
