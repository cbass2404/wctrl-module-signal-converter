//! HID transport for WinCtrl / WinWing panels.
//!
//! Frame layout, verified against captured SimAppPro traffic (docs/PROTOCOL.md):
//!
//! ```text
//! byte  0      0x02    report id
//! bytes 1..4   uint32  target part id, little-endian; 1 broadcasts
//! byte  5      len     significant data bytes, 1..8
//! bytes 6..13  data    payload; data[0] is the command
//! ```
//!
//! Replies use the same layout with `PART_REPLY_BIAS` added to the part id.
//! LED state is latched in the device, so there is no keepalive to maintain and
//! writes happen only when a value changes.

use std::time::{Duration, Instant};

use hidapi::{HidApi, HidDevice};

/// Assigned to the vendor, unchanged across the WinWing/WinUSA/WinCtrl rebrands.
pub const VENDOR_ID: u16 = 0x4098;
/// The only output report id the descriptor declares.
pub const REPORT_ID: u8 = 0x02;
/// Addressing this part id makes every sub-part answer.
pub const BROADCAST_PART: u32 = 1;
/// Replies carry the responding part's id with this added.
pub const PART_REPLY_BIAS: u32 = 0x1000;

const FRAME_LEN: usize = 14;
const DATA_OFFSET: usize = 6;
const MAX_DATA: usize = 8;

pub const CMD_ONLINE_HEARTBEAT: u8 = 0x00;
pub const CMD_REQUEST_DEVICE_HW: u8 = 0x01;
pub const CMD_REQUEST_DEVICE_FW: u8 = 0x02;
pub const CMD_REQUEST_DEVICE_SN: u8 = 0x03;
pub const CMD_READ_CFG_DATA: u8 = 0x05;
pub const CMD_SET_LEDX: u8 = 0x49;
const CMD_SET_LCDS: u8 = 0x4c;
pub const CMD_SET_LEDX_WITH_DURATION: u8 = 0x4B;

/// Commands that alter persistent state, calibration, or firmware. This crate
/// refuses to transmit any of them; see docs/PROTOCOL.md for why that matters
/// (`0x40` is the bootloader, one bit from `SET_HIDE_MODE`).
const FORBIDDEN: &[(u8, &str)] = &[
    (0x04, "DEVICE_RESTART"),
    (0x06, "WRITE_CFG_DATA"),
    (0x20, "START_UPDATE"),
    (0x21, "UPDATE_DATA"),
    (0x22, "UPDATE_DATA_LEN"),
    (0x23, "UPDATE_DATA_CRC"),
    (0x25, "READ_UPDATE_OFFSET"),
    (0x40, "ENTER_UPDATA_MODE"),
    (0x43, "SET_USE_COUNTS"),
    (0x47, "CALIBRATION_CMD_START"),
    (0x48, "CALIBRATION_CMD_FINISH"),
    (0x56, "WRITE_PARAM_DATA"),
];

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("hid: {0}")]
    Hid(#[from] hidapi::HidError),
    #[error("payload must be 1..={MAX_DATA} bytes, got {0}")]
    PayloadLen(usize),
    #[error("refusing to transmit {1} (0x{0:02x})")]
    ForbiddenCommand(u8, &'static str),
    #[error("no WinCtrl device with product id 0x{0:04x}")]
    NotFound(u16),
}

pub type Result<T> = std::result::Result<T, Error>;

/// One connected WinCtrl HID interface.
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub product_id: u16,
    pub product: String,
    pub serial: String,
    pub path: String,
}

/// Build a command frame. Rejects forbidden opcodes before anything is sent.
pub fn build_frame(part_id: u32, data: &[u8]) -> Result<[u8; FRAME_LEN]> {
    if data.is_empty() || data.len() > MAX_DATA {
        return Err(Error::PayloadLen(data.len()));
    }
    if let Some((code, name)) = FORBIDDEN.iter().find(|(c, _)| *c == data[0]) {
        return Err(Error::ForbiddenCommand(*code, name));
    }
    let mut frame = [0u8; FRAME_LEN];
    frame[0] = REPORT_ID;
    frame[1..5].copy_from_slice(&part_id.to_le_bytes());
    frame[5] = data.len() as u8;
    frame[DATA_OFFSET..DATA_OFFSET + data.len()].copy_from_slice(data);
    Ok(frame)
}

/// A decoded vendor-channel report.
#[derive(Debug, Clone)]
pub struct Reply {
    /// Part id with `PART_REPLY_BIAS` already removed.
    pub part_id: u32,
    pub data: Vec<u8>,
}

impl Reply {
    fn parse(buf: &[u8]) -> Option<Self> {
        if buf.len() < DATA_OFFSET || buf[0] != REPORT_ID {
            return None; // report 0x01 is joystick state
        }
        let raw = u32::from_le_bytes([buf[1], buf[2], buf[3], buf[4]]);
        let len = (buf[5] as usize).min(MAX_DATA).min(buf.len() - DATA_OFFSET);
        Some(Reply {
            part_id: raw.saturating_sub(PART_REPLY_BIAS),
            data: buf[DATA_OFFSET..DATA_OFFSET + len].to_vec(),
        })
    }
}

/// List every connected WinCtrl HID interface.
pub fn enumerate(api: &HidApi) -> Vec<DeviceInfo> {
    api.device_list()
        .filter(|d| d.vendor_id() == VENDOR_ID)
        .map(|d| DeviceInfo {
            product_id: d.product_id(),
            product: d.product_string().unwrap_or_default().to_string(),
            serial: d.serial_number().unwrap_or_default().to_string(),
            path: d.path().to_string_lossy().into_owned(),
        })
        .collect()
}

pub struct Device {
    handle: HidDevice,
    pub info: DeviceInfo,
}

impl Device {
    pub fn open(api: &HidApi, product_id: u16) -> Result<Self> {
        let info = enumerate(api)
            .into_iter()
            .find(|d| d.product_id == product_id)
            .ok_or(Error::NotFound(product_id))?;
        let path = std::ffi::CString::new(info.path.clone()).expect("hid path has no interior nul");
        let handle = api.open_path(&path)?;
        Ok(Device { handle, info })
    }

    fn send(&self, part_id: u32, data: &[u8]) -> Result<()> {
        let frame = build_frame(part_id, data)?;
        self.handle.write(&frame)?;
        Ok(())
    }

    /// Set one LED. `value` is 0..=255; lamps that are not dimmable treat any
    /// non-zero as on (see `data/devices.json` for per-LED ranges).
    pub fn set_led(&self, part_id: u32, index: u8, value: u8) -> Result<()> {
        self.send(part_id, &[CMD_SET_LEDX, index, value])
    }

    /// Write one group of a segment display's buffer.
    ///
    /// Unlike `set_led`, this is never acknowledged: 24 frames to a UFC drew no
    /// replies at all, from the same read window that had just taken an echo
    /// from an LED write. So a display write cannot be confirmed, and a failed
    /// one is corrected by the next full repaint rather than by a retry.
    pub fn set_lcd(&self, part_id: u32, group: u8, bytes: &[u8]) -> Result<()> {
        let mut data = Vec::with_capacity(bytes.len() + 2);
        data.push(CMD_SET_LCDS);
        data.push(group);
        data.extend_from_slice(bytes);
        self.send(part_id, &data)
    }

    /// Collect vendor-channel replies until `window` elapses with nothing new.
    pub fn drain_replies(&self, window: Duration) -> Vec<Reply> {
        let mut out = Vec::new();
        let deadline = Instant::now() + window;
        let mut buf = [0u8; 64];
        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let ms = remaining.as_millis().min(i32::MAX as u128) as i32;
            match self.handle.read_timeout(&mut buf, ms.max(1)) {
                Ok(0) => break,
                Ok(n) => {
                    if let Some(reply) = Reply::parse(&buf[..n]) {
                        out.push(reply);
                    }
                }
                Err(_) => break,
            }
        }
        out
    }

    /// Broadcast a heartbeat; every sub-part answers with its own id.
    ///
    /// A part id is not a USB product id: the Orion II enumerates as `0xbd64`
    /// but answers as part `0xbe60`, with its handles as separate parts.
    pub fn discover_parts(&self, window: Duration) -> Result<Vec<u32>> {
        self.send(BROADCAST_PART, &[CMD_ONLINE_HEARTBEAT])?;
        let mut parts: Vec<u32> = self
            .drain_replies(window)
            .into_iter()
            .map(|r| r.part_id)
            .collect();
        parts.sort_unstable();
        parts.dedup();
        Ok(parts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_matches_captured_traffic() {
        // Captured from SimAppPro setting the PTO2 gear lamp to 137.
        let frame = build_frame(0xbf05, &[CMD_SET_LEDX, 1, 137]).unwrap();
        assert_eq!(
            frame,
            [0x02, 0x05, 0xbf, 0x00, 0x00, 0x03, 0x49, 0x01, 0x89, 0, 0, 0, 0, 0]
        );
    }

    #[test]
    fn broadcast_heartbeat_matches() {
        let frame = build_frame(BROADCAST_PART, &[CMD_ONLINE_HEARTBEAT]).unwrap();
        assert_eq!(
            frame,
            [0x02, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0, 0, 0, 0, 0, 0, 0]
        );
    }

    #[test]
    fn forbidden_commands_are_refused() {
        for (code, _) in FORBIDDEN {
            assert!(build_frame(0xbf05, &[*code, 0, 0]).is_err(), "0x{code:02x}");
        }
    }

    #[test]
    fn reply_strips_the_bias() {
        let raw = [0x02, 0x05, 0xcf, 0x00, 0x00, 0x03, 0x49, 0x01, 0xff, 0, 0, 0, 0, 0];
        let reply = Reply::parse(&raw).unwrap();
        assert_eq!(reply.part_id, 0xbf05);
        assert_eq!(reply.data, vec![0x49, 0x01, 0xff]);
    }

    #[test]
    fn joystick_reports_are_not_replies() {
        let raw = [0x01, 0x44, 0x2b, 0x02, 0xa0, 0x08, 0, 0, 0, 0, 0, 0, 0, 0];
        assert!(Reply::parse(&raw).is_none());
    }
}
