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
//!
//! Panels with a pixel screen add a second channel, report `0xf0`, carrying a
//! longer frame split across 64-byte reports. See [`pixel_write_frame`].

use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant};

use hidapi::{HidApi, HidDevice};

/// Assigned to the vendor, unchanged across the WinWing/WinUSA/WinCtrl rebrands.
pub const VENDOR_ID: u16 = 0x4098;
/// The command channel's report id, and the only one on the 14-byte panels.
pub const REPORT_ID: u8 = 0x02;
/// The pixel screen channel, declared only by panels with 64-byte reports.
pub const PIXEL_REPORT_ID: u8 = 0xf0;
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
    #[error("report 0x{0:02x} is not a screen report")]
    NotAScreenReport(u8),
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

// ------------------------------------------------------------- pixel channel
//
// Confirmed on a ViperAce ICP 2026-09-18 (docs/PROTOCOL.md, "Driving a pixel
// display"). A logical frame is a structured command:
//
//   part id u32 | function u32 | clock u32 ms | respond u8 | payload len u32 | payload
//
// and goes out as consecutive 64-byte reports, each `f0 00 <seq> <n>` and then
// n (1..=60) bytes of the frame, zero padded. WWTHID.log prints the frame
// without those four header bytes, which is why the first attempt to replay it
// was acknowledged and drew nothing. Several commands can share one stream:
// the device reassembles it, and a command may straddle two reports.
//
// The function ids and their names are SimAppPro's, as WwDevicesDotnet
// documents them (Winctrl/README.md, "Structured Commands").

const PIXEL_REPORT_LEN: usize = 64;
const PIXEL_REPORT_DATA: usize = 60;
const PIXEL_WRITE: u32 = 0x102;
const PIXEL_COMMIT: u32 = 0x103; // refreshLCD
const SET_SCREEN_INFO: u32 = 0x118;
const SET_FEATURE_INFO: u32 = 0x119;
const SET_COMPOSITE_INDEX_BYTES: u32 = 0x11a;
const BUILD_FORMAT_TABLE: u32 = 0x11c;
const CLEAR_FEATURE_INFO: u32 = 0x11e;

/// The text-grid channel of the CDU-style panels (the MCDU).
pub const GRID_REPORT_ID: u8 = 0xf2;

/// The most data bytes put in one pixel write.
///
/// 225 is nine rows of a 200 pixel screen, the largest write confirmed on
/// hardware. SimAppPro was seen sending up to 270, so this is a margin rather
/// than the device's limit, which is not known.
pub const PIXEL_WRITE_MAX: usize = 225;

/// One structured command. Only the functions named above are built here;
/// `0x104`, the vendor's self-test pattern, has no use.
fn structured(part_id: u32, function: u32, clock_ms: u32, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(17 + payload.len());
    out.extend_from_slice(&part_id.to_le_bytes());
    out.extend_from_slice(&function.to_le_bytes());
    // SimAppPro's millisecond clock. The device does not check its continuity.
    out.extend_from_slice(&clock_ms.to_le_bytes());
    // Whether to answer; nothing sent here needs to.
    out.push(0x00);
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    out
}

/// A write into the screen's framebuffer, not shown until a commit.
///
/// `address` is in pixels, `y * width + x`, and must be a multiple of 8,
/// because each byte is 8 pixels with the least significant bit leftmost. The
/// bytes run on across the end of a row.
pub fn pixel_write_frame(part_id: u32, clock_ms: u32, address: u32, bytes: &[u8]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(4 + bytes.len());
    payload.extend_from_slice(&address.to_le_bytes());
    payload.extend_from_slice(bytes);
    structured(part_id, PIXEL_WRITE, clock_ms, &payload)
}

/// Show what the writes since the last commit put in the framebuffer.
pub fn pixel_commit_frame(part_id: u32, clock_ms: u32) -> Vec<u8> {
    structured(part_id, PIXEL_COMMIT, clock_ms, &[0x00])
}

// -------------------------------------------------------------- grid channel
//
// The MCDU's screen as a text grid, ported from WwDevicesDotnet (BSD-3-Clause,
// Andrew Whewell and Laurent André; Winctrl/CommonWinctrlPanel.cs and
// Winctrl/ScreenWriter.cs; see THIRD_PARTY_NOTICES.md). docs/PROTOCOL.md,
// "Driving a text grid", has the detail. Before the device takes grid data it has to be told what a cell can
// look like: four features, the values each may take, and the device indexes
// the cartesian product. A cell then names its look by that index.

/// Every value of the four features, in the order the index is built from:
/// font slot, foreground colour, background colour, and the first/last cell
/// marker. Each is `feature u16 | value u32 | format id u64`, little-endian,
/// and the colours are `B G R A`. Taken verbatim from WwDevicesDotnet, which
/// took them from SimAppPro.
const GRID_FEATURES: [&str; 27] = [
    "0100050000000200000000000000", "0100060000000300000000000000",
    "0200000000ff0400000000000000", "020000a5ffff0500000000000000",
    "0200ffffffff0600000000000000", "0200ffff00ff0700000000000000",
    "02003dff00ff0800000000000000", "0200ff63ffff0900000000000000",
    "02000000ffff0a00000000000000", "020000ffffff0b00000000000000",
    "0200425c61ff0c00000000000000", "0200777777ff0d00000000000000",
    "02005e7379ff0e00000000000000", "0300000000ff0f00000000000000",
    "030000a5ffff1000000000000000", "0300ffffffff1100000000000000",
    "0300ffff00ff1200000000000000", "03003dff00ff1300000000000000",
    "0300ff63ffff1400000000000000", "03000000ffff1500000000000000",
    "030000ffffff1600000000000000", "0300425c61ff1700000000000000",
    "0300777777ff1800000000000000", "03005e7379ff1900000000000000",
    "0400000000001a00000000000000", "0400010000001b00000000000000",
    "0400020000001c00000000000000",
];

fn hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("constant hex"))
        .collect()
}

/// The stream that declares the grid and its format table: where the grid
/// starts on the 640x480 surface, its size, and every look a cell can take.
pub fn grid_format_table(
    part_id: u32,
    clock_ms: u32,
    origin: (u16, u16),
    rows: u16,
    columns: u16,
) -> Vec<u8> {
    let mut out = structured(part_id, CLEAR_FEATURE_INFO, clock_ms, &[]);
    let mut info = Vec::with_capacity(8);
    for v in [origin.0, origin.1, rows, columns] {
        info.extend_from_slice(&v.to_le_bytes());
    }
    out.extend(structured(part_id, SET_SCREEN_INFO, clock_ms, &info));
    for feature in GRID_FEATURES {
        out.extend(structured(part_id, SET_FEATURE_INFO, clock_ms, &hex(feature)));
    }
    out.extend(structured(part_id, SET_COMPOSITE_INDEX_BYTES, clock_ms, &[2]));
    out.extend(structured(part_id, BUILD_FORMAT_TABLE, clock_ms, &[]));
    out
}

/// One cell of a grid screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridCell {
    pub ch: char,
    /// Colour ordinal in the table above: 0 black, 1 amber, 2 white, 3 cyan,
    /// 4 green, 5 magenta, 6 red, 7 yellow, 8 brown, 9 grey, 10 khaki.
    pub fg: u8,
    pub bg: u8,
    pub small: bool,
}

impl GridCell {
    pub const BLANK: GridCell = GridCell { ch: ' ', fg: 2, bg: 0, small: false };

    /// The cell's index into the format table. The strides are the product's:
    /// 363 per font, 33 per foreground, 3 per background, then 1 for the first
    /// cell of the screen and 2 for the last.
    fn format(&self, first: bool, last: bool) -> u16 {
        let mut code = u16::from(self.fg) * 0x21 + u16::from(self.bg) * 3;
        if self.small {
            code += 0x16b;
        }
        if first {
            code += 1;
        } else if last {
            code += 2;
        }
        code
    }
}

/// The reports that paint a whole grid screen, row by row.
///
/// Each cell is its format index, low byte first, then the character in
/// UTF-8. The cells run on across report boundaries, and the last report is
/// zero padded. Cells cannot be addressed singly: every write is the whole
/// screen.
pub fn grid_reports(cells: &[GridCell]) -> Vec<[u8; PIXEL_REPORT_LEN]> {
    let mut stream = Vec::with_capacity(cells.len() * 3);
    let last = cells.len().saturating_sub(1);
    for (i, cell) in cells.iter().enumerate() {
        stream.extend_from_slice(&cell.format(i == 0, i == last).to_le_bytes());
        let mut utf8 = [0u8; 4];
        stream.extend_from_slice(cell.ch.encode_utf8(&mut utf8).as_bytes());
    }
    stream
        .chunks(PIXEL_REPORT_LEN - 1)
        .map(|chunk| {
            let mut report = [0u8; PIXEL_REPORT_LEN];
            report[0] = GRID_REPORT_ID;
            report[1..1 + chunk.len()].copy_from_slice(chunk);
            report
        })
        .collect()
}

/// Split a logical frame into the reports that carry it.
///
/// `seq` is the host's report counter, advanced once per report and wrapping.
/// The device acknowledges each report with its own counter, not ours.
pub fn pixel_reports(frame: &[u8], seq: &mut u8) -> Vec<[u8; PIXEL_REPORT_LEN]> {
    frame
        .chunks(PIXEL_REPORT_DATA)
        .map(|chunk| {
            *seq = seq.wrapping_add(1);
            let mut report = [0u8; PIXEL_REPORT_LEN];
            report[0] = PIXEL_REPORT_ID;
            report[2] = *seq;
            report[3] = chunk.len() as u8;
            report[4..4 + chunk.len()].copy_from_slice(chunk);
            report
        })
        .collect()
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

/// Whether a report descriptor declares an Output main item, which is what
/// makes an interface writable. Walks items by their size bits rather than
/// scanning bytes, so a data byte that happens to look like a prefix is never
/// read as one.
fn declares_output(desc: &[u8]) -> bool {
    let mut i = 0;
    while i < desc.len() {
        let prefix = desc[i];
        if prefix == 0xfe {
            // Long item: bDataSize follows the prefix, then bLongItemTag.
            let size = desc.get(i + 1).copied().unwrap_or(0) as usize;
            i += 3 + size;
            continue;
        }
        if prefix & 0xfc == 0x90 {
            return true;
        }
        i += 1 + [0, 1, 2, 4][(prefix & 0x03) as usize];
    }
    false
}

pub struct Device {
    handle: HidDevice,
    pub info: DeviceInfo,
    /// Pixel channel report counter.
    seq: AtomicU8,
    /// Zero point of the pixel channel's millisecond clock.
    opened: Instant,
}

impl Device {
    /// Open the interface of `product_id` that can take a command.
    ///
    /// A PID can enumerate as several collections. The CarrierAce MFD in
    /// "1 Split 3" mode is three joysticks under one PID, and only the first
    /// declares the vendor channel; the other two have no output report, so a
    /// write to them fails. Windows' listing order is not a promise, so the
    /// collection with an Output item is chosen on purpose. When no descriptor
    /// can be read, the first interface is kept, which is what every
    /// single-collection panel gets either way.
    pub fn open(api: &HidApi, product_id: u16) -> Result<Self> {
        let candidates: Vec<DeviceInfo> = enumerate(api)
            .into_iter()
            .filter(|d| d.product_id == product_id)
            .collect();
        let mut fallback = None;
        let mut failed = None;
        for info in candidates {
            let path = std::ffi::CString::new(info.path.clone()).expect("hid path has no interior nul");
            let handle = match api.open_path(&path) {
                Ok(h) => h,
                Err(e) => {
                    failed.get_or_insert(e);
                    continue;
                }
            };
            let mut desc = [0u8; 4096];
            if let Ok(n) = handle.get_report_descriptor(&mut desc) {
                if declares_output(&desc[..n]) {
                    return Ok(Self::wrap(handle, info));
                }
            }
            fallback.get_or_insert((handle, info));
        }
        match (fallback, failed) {
            (Some((handle, info)), _) => Ok(Self::wrap(handle, info)),
            (None, Some(e)) => Err(e.into()),
            (None, None) => Err(Error::NotFound(product_id)),
        }
    }

    fn wrap(handle: HidDevice, info: DeviceInfo) -> Self {
        Device {
            handle,
            info,
            seq: AtomicU8::new(0),
            opened: Instant::now(),
        }
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

    /// Write bytes into a pixel screen's framebuffer, starting `offset` bytes
    /// in. Nothing changes on the glass until [`commit_pixels`](Self::commit_pixels).
    ///
    /// Split into writes of at most [`PIXEL_WRITE_MAX`] bytes. Each report is
    /// acknowledged, but the acknowledgements are not read: as with a segment
    /// display, a bad write is corrected by the next repaint, not a retry.
    pub fn write_pixels(&self, part_id: u32, offset: usize, bytes: &[u8]) -> Result<()> {
        for (i, run) in bytes.chunks(PIXEL_WRITE_MAX).enumerate() {
            let address = ((offset + i * PIXEL_WRITE_MAX) * 8) as u32;
            self.send_pixel_frame(&pixel_write_frame(part_id, self.clock_ms(), address, run))?;
        }
        Ok(())
    }

    /// Show the framebuffer. The screen keeps it after the process exits.
    pub fn commit_pixels(&self, part_id: u32) -> Result<()> {
        self.send_pixel_frame(&pixel_commit_frame(part_id, self.clock_ms()))
    }

    /// Declare a grid screen and its format table. The grid draws nothing
    /// until this has been sent, and a font upload resets the grid to 24x14.
    pub fn declare_grid(&self, part_id: u32, origin: (u16, u16), rows: u16, columns: u16) -> Result<()> {
        self.send_pixel_frame(&grid_format_table(part_id, self.clock_ms(), origin, rows, columns))
    }

    /// Paint a whole grid screen. The screen keeps it after the process exits.
    pub fn paint_grid(&self, cells: &[GridCell]) -> Result<()> {
        grid_reports(cells)
            .iter()
            .try_for_each(|report| self.handle.write(report).map(|_| ()))?;
        Ok(())
    }

    /// Send reports built elsewhere, such as a font upload. Only the two screen
    /// channels pass: anything on the command channel has to go through
    /// [`build_frame`] and its forbidden list.
    pub fn send_screen_reports(&self, reports: &[Vec<u8>]) -> Result<()> {
        for report in reports {
            match report.first() {
                Some(&PIXEL_REPORT_ID | &GRID_REPORT_ID) => {
                    self.handle.write(report)?;
                }
                Some(&id) => return Err(Error::NotAScreenReport(id)),
                None => {}
            }
        }
        Ok(())
    }

    fn clock_ms(&self) -> u32 {
        self.opened.elapsed().as_millis() as u32
    }

    fn send_pixel_frame(&self, frame: &[u8]) -> Result<()> {
        let mut seq = self.seq.load(Ordering::Relaxed);
        let result = pixel_reports(frame, &mut seq)
            .iter()
            .try_for_each(|report| self.handle.write(report).map(|_| ()));
        self.seq.store(seq, Ordering::Relaxed);
        Ok(result?)
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

    /// One line of SimAppPro's ICP traffic as WWTHID.log prints it: the report
    /// id, then the logical frame with the rest of the report header dropped.
    fn captured(line: &str) -> Vec<u8> {
        let open = line.find("[f0 ").expect("an f0 frame") + 1;
        let close = open + line[open..].find(']').unwrap();
        line[open..close]
            .split_whitespace()
            .map(|h| u8::from_str_radix(h, 16).unwrap())
            .collect()
    }

    #[test]
    fn pixel_frames_match_every_captured_write() {
        let log = include_str!("../../wctrl-config/tests/fixtures/ded_simapppro_frames.txt");
        let mut writes = 0;
        for line in log.lines().filter(|l| l.contains("[f0 ")) {
            let raw = captured(line);
            let frame = &raw[1..];
            let part = u32::from_le_bytes(frame[0..4].try_into().unwrap());
            let clock = u32::from_le_bytes(frame[8..12].try_into().unwrap());
            let rebuilt = match u32::from_le_bytes(frame[4..8].try_into().unwrap()) {
                PIXEL_WRITE => {
                    writes += 1;
                    let address = u32::from_le_bytes(frame[17..21].try_into().unwrap());
                    pixel_write_frame(part, clock, address, &frame[21..])
                }
                PIXEL_COMMIT => pixel_commit_frame(part, clock),
                other => panic!("unexpected function 0x{other:03x}"),
            };
            assert_eq!(rebuilt, frame, "{line}");
        }
        assert!(writes > 50, "only {writes} writes in the fixture");
    }

    #[test]
    fn a_frame_is_split_across_reports_with_a_header_each() {
        let frame: Vec<u8> = (0..247u32).map(|i| i as u8).collect();
        let mut seq = 0xfe;
        let reports = pixel_reports(&frame, &mut seq);
        let lens: Vec<u8> = reports.iter().map(|r| r[3]).collect();
        assert_eq!(lens, [60, 60, 60, 60, 7]);
        let seqs: Vec<u8> = reports.iter().map(|r| r[2]).collect();
        assert_eq!(seqs, [0xff, 0x00, 0x01, 0x02, 0x03], "the counter wraps");
        assert_eq!(seq, 0x03);
        for (i, r) in reports.iter().enumerate() {
            assert_eq!(&r[..2], &[0xf0, 0x00]);
            let n = r[3] as usize;
            assert_eq!(&r[4..4 + n], &frame[i * 60..i * 60 + n]);
            assert!(r[4 + n..].iter().all(|b| *b == 0), "zero padded");
        }
    }

    #[test]
    fn a_commit_is_one_report() {
        let mut seq = 0;
        let reports = pixel_reports(&pixel_commit_frame(0xbf06, 0x0003_ad2b), &mut seq);
        assert_eq!(reports.len(), 1);
        assert_eq!(
            &reports[0][..24],
            &[
                0xf0, 0x00, 0x01, 18, // report header
                0x06, 0xbf, 0x00, 0x00, 0x03, 0x01, 0x00, 0x00, // part, commit
                0x2b, 0xad, 0x03, 0x00, 0x00, // clock
                0x01, 0x00, 0x00, 0x00, 0x00, // one byte of payload, zero
                0x00, 0x00, // padding
            ]
        );
    }

    /// The commands in a stream, as (function, payload), timestamps dropped.
    fn commands(stream: &[u8]) -> Vec<(u32, Vec<u8>)> {
        let mut out = Vec::new();
        let mut i = 0;
        while i + 17 <= stream.len() {
            let function = u32::from_le_bytes(stream[i + 4..i + 8].try_into().unwrap());
            let len = u32::from_le_bytes(stream[i + 13..i + 17].try_into().unwrap()) as usize;
            out.push((function, stream[i + 17..i + 17 + len].to_vec()));
            i += 17 + len;
        }
        out
    }

    #[test]
    fn the_format_table_is_the_one_simapppro_declares() {
        // SimAppPro's font upload, as WwDevicesDotnet recorded it, declares
        // the grid and its format table at the end. Ours has to say the same.
        let map: serde_json::Value = serde_json::from_str(include_str!(
            "../../../data/mcdu/font-packet-map-3x31.json"
        ))
        .unwrap();
        let mut stream = Vec::new();
        for p in map["Packets"].as_array().unwrap() {
            let p = p.as_str().unwrap().replace("{CP}", "32bb");
            if !p.starts_with("f0") {
                continue;
            }
            let bytes: Vec<u8> = p
                .chars()
                .map(|c| if "_HLWXY".contains(c) { '0' } else { c })
                .collect::<String>()
                .as_bytes()
                .chunks(2)
                .map(|h| u8::from_str_radix(std::str::from_utf8(h).unwrap(), 16).unwrap())
                .collect();
            stream.extend_from_slice(&bytes[4..4 + bytes[3] as usize]);
        }
        let theirs: Vec<_> = commands(&stream)
            .into_iter()
            .skip_while(|(f, _)| *f != CLEAR_FEATURE_INFO)
            .filter(|(f, _)| *f != 0x105)
            .collect();
        let ours = commands(&grid_format_table(0xbb32, 0, (0, 0), 14, 24));
        assert_eq!(ours.len(), theirs.len());
        for ((f, a), (g, b)) in ours.iter().zip(&theirs) {
            assert_eq!(f, g);
            if *f == SET_SCREEN_INFO {
                // The origin is a placeholder in theirs; rows and columns are not.
                assert_eq!(&a[4..], &b[4..], "14 rows of 24");
            } else {
                assert_eq!(a, b, "function 0x{f:03x}");
            }
        }
    }

    #[test]
    fn a_blank_screen_marks_its_first_and_last_cell() {
        let reports = grid_reports(&[GridCell::BLANK; 24 * 14]);
        // 336 cells of three bytes, 63 to a report after the report id.
        assert_eq!(reports.len(), 16);
        assert!(reports.iter().all(|r| r[0] == GRID_REPORT_ID));
        // White on black is 2 * 0x21 = 0x42; the first cell adds 1, the last 2.
        assert_eq!(&reports[0][1..7], &[0x43, 0x00, b' ', 0x42, 0x00, b' ']);
        assert_eq!(&reports[15][61..64], &[0x44, 0x00, b' ']);
    }

    #[test]
    fn a_small_coloured_glyph_carries_its_utf8_whole() {
        let cell = GridCell { ch: '☐', fg: 4, bg: 0, small: true };
        let reports = grid_reports(&[GridCell::BLANK, cell, GridCell::BLANK]);
        // Green is 4 * 0x21 = 0x84, small adds 0x16b: 0x1ef.
        assert_eq!(&reports[0][4..9], &[0xef, 0x01, 0xe2, 0x98, 0x90]);
        assert!(reports[0][12..].iter().all(|b| *b == 0), "zero padded");
    }

    #[test]
    fn joystick_reports_are_not_replies() {
        let raw = [0x01, 0x44, 0x2b, 0x02, 0xa0, 0x08, 0, 0, 0, 0, 0, 0, 0, 0];
        assert!(Reply::parse(&raw).is_none());
    }

    fn hex(s: &str) -> Vec<u8> {
        s.split_whitespace().map(|b| u8::from_str_radix(b, 16).unwrap()).collect()
    }

    /// The three collections a CarrierAce MFD (PID 0xbee2) presents in
    /// "1 Split 3" mode, read from Windows 2026-09-18. Only col01 carries the
    /// vendor channel, report 2 on page 0xff, and only it can be written.
    #[test]
    fn only_the_split_mfd_command_collection_declares_output() {
        let col01 = hex(
            "05 01 09 04 a1 01 85 01 05 09 19 01 29 32 15 00 25 01 75 01 95 32 81 02 75 06 95 01 81 03 \
             05 01 09 36 15 00 27 ff ff 00 00 35 00 47 ff ff 00 00 75 10 95 01 81 02 85 02 05 ff 09 01 \
             15 00 26 ff 00 35 00 46 ff 00 75 08 95 0d 81 02 09 02 15 00 26 ff 00 75 08 95 0d 91 02 c0",
        );
        let joystick = |id: &str| {
            hex(&format!(
                "05 01 09 04 a1 01 85 {id} 05 09 19 01 29 32 15 00 25 01 75 01 95 32 81 02 75 06 95 01 \
                 81 03 05 01 09 36 15 00 27 ff ff 00 00 35 00 47 ff ff 00 00 75 10 95 01 81 02 c0"
            ))
        };
        assert!(declares_output(&col01));
        assert!(!declares_output(&joystick("03")));
        assert!(!declares_output(&joystick("04")));
    }

    #[test]
    fn a_data_byte_that_looks_like_output_is_not_one() {
        // Logical Maximum 0x91, then Input. Scanning bytes would see 0x91.
        assert!(!declares_output(&hex("25 91 81 02")));
        // Long item whose payload holds 0x91.
        assert!(!declares_output(&hex("fe 02 00 91 91 81 02")));
    }
}
