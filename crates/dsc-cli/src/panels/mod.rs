//! The seam between what the engine decides and what goes on the wire.
//!
//! The engine knows nothing about transports: it emits [`LedWrite`]s and
//! [`LcdWrite`]s naming a device, a part and a value. Everything below turns
//! those into bytes for one brand of hardware, and every brand-specific fact
//! lives on this side of the line: frame layout, which commands are refused,
//! whether a screen has to be committed, whether a font has to be uploaded
//! first.
//!
//! Each device in `data/devices.json` names its protocol, and [`all`] builds
//! the ones this release can drive. Adding a brand means a new module here and
//! a new name in that list; nothing above this module changes.
//!
//! **The diagnostics are deliberately outside this.** `parts`, `led`, `blink`,
//! `sweep`, `probe-brightness` and `mcdu-test` take a raw part id and index and
//! poke one protocol on purpose. There is no honest generic version of that, so
//! they call [`wctrl_hid`] directly and a second brand gets its own commands
//! rather than a shared vocabulary that fits neither.

use anyhow::Result;
use dsc_config::{DeviceSpec, DisplayCatalogue};
use dsc_engine::{LcdWrite, LedWrite};

mod wctrl;

/// Every protocol this build can drive.
///
/// Built once per run, because a protocol may hold an open bus: the WinCtrl
/// one keeps the HID API it enumerates and opens through.
pub fn all() -> Result<Vec<Box<dyn Protocol>>> {
    Ok(vec![Box::new(wctrl::Wctrl::new()?)])
}

/// One device of a brand that is plugged in, whether or not this build knows
/// what it is. Only for the log: a panel missing from the inventory and a panel
/// nobody plugged in look identical from a profile's side, and this is what
/// separates them.
pub struct Found {
    /// How the protocol names this device on its own bus, such as
    /// `pid 0xbd64`. Opaque above this module.
    pub ident: String,
    pub product: String,
    pub serial: String,
}

/// One brand's transport: how its devices are found and opened.
pub trait Protocol {
    /// The name devices use to ask for this protocol.
    fn name(&self) -> &'static str;

    /// Everything of this brand's that is plugged in.
    fn present(&self) -> Result<Vec<Found>>;

    /// Whether `spec` is one of them. The protocol decides what identity
    /// means, because a USB product id is only one brand's answer.
    fn is_connected(&self, spec: &DeviceSpec) -> Result<bool>;

    /// Open a device [`is_connected`](Protocol::is_connected) has just found.
    ///
    /// The display catalogue comes in here because a screen's geometry and its
    /// fonts are needed to drive it, and which of that the wire needs is the
    /// protocol's business rather than the caller's.
    fn open(&self, spec: &DeviceSpec, displays: &DisplayCatalogue) -> Result<Box<dyn Panel>>;
}

/// One opened device, for as long as the converter runs.
///
/// Three methods on purpose. Anything a brand needs beyond them, such as
/// uploading a font or remembering which screens are waiting to be shown, is
/// state the implementation keeps for itself: the caller has no way to know
/// about it and no reason to.
pub trait Panel {
    /// Set one lamp to one value.
    fn set_lamp(&mut self, w: &LedWrite) -> Result<()>;

    /// Put a piece of a display's buffer on the glass. Whether it is visible
    /// before [`flush`](Panel::flush) is the protocol's business.
    fn write_display(&mut self, w: &LcdWrite) -> Result<()>;

    /// End of a batch: show anything written but not yet shown.
    ///
    /// Called once per batch on every panel that was written to, and a
    /// protocol whose writes land immediately does nothing here.
    fn flush(&mut self) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::all;
    use dsc_config::DeviceInventory;
    use std::path::Path;

    /// A device whose protocol nothing here can build is skipped at startup
    /// with a warning, which is right for an inventory from a newer release
    /// and wrong for the one we ship. This is the difference between the two.
    #[test]
    fn every_shipped_device_asks_for_a_protocol_this_build_can_drive() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/devices.json");
        let inventory = DeviceInventory::load(&path).expect("data/devices.json parses");
        let protocols = all().expect("the protocols start");
        let names: Vec<&str> = protocols.iter().map(|p| p.name()).collect();
        assert!(!inventory.devices.is_empty(), "an empty inventory proves nothing");
        for device in &inventory.devices {
            assert!(
                names.contains(&device.protocol.as_str()),
                "{} asks for {:?}, and this build has {names:?}",
                device.display_name,
                device.protocol
            );
        }
    }
}
