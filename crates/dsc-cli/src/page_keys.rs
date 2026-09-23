//! Page keys, read while the converter runs.
//!
//! Every connected panel that lists page keys in `devices.json` gets a thread
//! reading its buttons, and each key going down arrives on one channel with
//! what the keyboard held at that moment. What a key means is decided by the
//! caller, from the device's own `page_keys` and the modifier setting, so
//! nothing here knows a slot or an MCDU.
//!
//! Only reads. Windows gives every open handle its own copy of each input
//! report, so DCS and SimAppPro see every press as before.

use std::sync::mpsc::{self, Receiver};

use dsc_config::settings::Modifier;
use dsc_config::DeviceInventory;

#[cfg(windows)]
use crate::{buttons, keyboard};

pub enum KeyEvent {
    /// A button went down on a device, with the modifiers held at the time.
    Down { device: String, number: u16, held: Vec<Modifier> },
    /// A reader stopped, most likely because the panel was unplugged. Its
    /// keys do nothing until the converter starts again.
    Lost { device: String, why: String },
}

/// Keys are read through Windows' own HID parser, so elsewhere there are none.
#[cfg(not(windows))]
pub fn start(_inventory: &DeviceInventory, _connected: &[String]) -> (Receiver<KeyEvent>, Vec<String>) {
    (mpsc::channel().1, Vec::new())
}

/// Start a reader for each connected device with page keys. Returns the
/// channel keys arrive on and a line per device for the log.
#[cfg(windows)]
pub fn start(inventory: &DeviceInventory, connected: &[String]) -> (Receiver<KeyEvent>, Vec<String>) {
    let (tx, rx) = mpsc::channel();
    let mut lines = Vec::new();
    let with_keys: Vec<_> = inventory
        .devices
        .iter()
        .filter(|d| !d.page_keys.is_empty() && connected.contains(&d.key))
        .collect();
    if with_keys.is_empty() {
        return (rx, lines);
    }
    let api = match hidapi::HidApi::new() {
        Ok(api) => api,
        Err(e) => {
            lines.push(format!("keys     could not list the panels to read page keys: {e}"));
            return (rx, lines);
        }
    };
    let found = wctrl_hid::enumerate(&api);
    for spec in with_keys {
        // The collection that declares buttons: on the MCDU the game
        // controller, one of several interfaces under the one PID.
        let collection = found
            .iter()
            .filter(|d| d.product_id == spec.usb_pid)
            .filter_map(|d| buttons::Collection::open(&d.path).ok())
            .find(|c| !c.buttons.is_empty());
        let Some(collection) = collection else {
            lines.push(format!("keys     {}: no collection declares buttons, so its page keys do nothing", spec.key));
            continue;
        };
        lines.push(format!(
            "keys     {}: reading {} page key(s) on usage page 0x{:04x} usage 0x{:04x}",
            spec.key,
            spec.page_keys.len(),
            collection.usage_page,
            collection.usage
        ));
        let tx = tx.clone();
        let device = spec.key.clone();
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let mut last: Vec<u16> = Vec::new();
            loop {
                if let Err(e) = collection.read(&mut buf) {
                    let _ = tx.send(KeyEvent::Lost { device, why: e.to_string() });
                    return;
                }
                // A report with no buttons in it leaves them as they were.
                // Acting only on keys going down, never on the report
                // changing, is what keeps the MCDU's restless bytes 17 to 24
                // from reading as presses.
                let Some(down) = collection.pressed(&buf) else { continue };
                let new: Vec<u16> = down.iter().copied().filter(|b| !last.contains(b)).collect();
                if !new.is_empty() {
                    let held = keyboard::held_now();
                    for number in new {
                        let event = KeyEvent::Down { device: device.clone(), number, held: held.clone() };
                        if tx.send(event).is_err() {
                            return;
                        }
                    }
                }
                last = down;
            }
        });
    }
    (rx, lines)
}
