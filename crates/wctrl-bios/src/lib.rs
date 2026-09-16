//! Decoder for the DCS-BIOS export stream.
//!
//! DCS-BIOS broadcasts a byte stream of write accesses into a flat 16-bit
//! address space:
//!
//! ```text
//! 55 55 55 55 | addr(u16 LE) count(u16 LE) data… | addr count data… | …
//! ```
//!
//! `55 55 55 55` is the frame sync. Only changed values are sent, so a client
//! that connects mid-flight sees an incomplete picture until each value next
//! changes. On aircraft change DCS-BIOS marks every entry dirty
//! (`BIOSStateMachine` calls `memoryMap:clearValues()`), so the whole module
//! state arrives immediately after load  that flood is what the engine syncs
//! against.

use std::collections::HashMap;
use std::io;
use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};

/// Default multicast group and port from `BIOSConfig.lua`.
pub const MULTICAST_GROUP: Ipv4Addr = Ipv4Addr::new(239, 255, 50, 10);
pub const MULTICAST_PORT: u16 = 5010;

const SYNC_BYTE: u8 = 0x55;
const SYNC_RUN: u8 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Sync,
    AddressLow,
    AddressHigh,
    CountLow,
    CountHigh,
    DataLow,
    DataHigh,
}

/// One 16-bit word written to the address space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Write {
    pub address: u16,
    pub value: u16,
}

/// Byte-at-a-time state machine over the export stream.
///
/// Resynchronises on any run of four `0x55` bytes, so a dropped datagram costs
/// at most the remainder of one frame.
#[derive(Debug)]
pub struct Decoder {
    state: State,
    sync_run: u8,
    address: u16,
    count: u16,
    data: u16,
}

impl Default for Decoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Decoder {
    pub fn new() -> Self {
        Decoder {
            state: State::Sync,
            sync_run: 0,
            address: 0,
            count: 0,
            data: 0,
        }
    }

    /// Feed one byte; returns a completed write, if this byte finished one.
    pub fn push(&mut self, byte: u8) -> Option<Write> {
        // Sync detection runs in parallel with decoding, exactly as the
        // reference clients do: four 0x55 bytes always restart the frame,
        // wherever they appear.
        if byte == SYNC_BYTE {
            self.sync_run += 1;
            if self.sync_run == SYNC_RUN {
                self.state = State::AddressLow;
                self.sync_run = 0;
                return None;
            }
        } else {
            self.sync_run = 0;
        }

        match self.state {
            State::Sync => None,
            State::AddressLow => {
                self.address = byte as u16;
                self.state = State::AddressHigh;
                None
            }
            State::AddressHigh => {
                self.address |= (byte as u16) << 8;
                self.state = State::CountLow;
                None
            }
            State::CountLow => {
                self.count = byte as u16;
                self.state = State::CountHigh;
                None
            }
            State::CountHigh => {
                self.count |= (byte as u16) << 8;
                self.state = if self.count == 0 {
                    State::AddressLow
                } else {
                    State::DataLow
                };
                None
            }
            State::DataLow => {
                self.data = byte as u16;
                self.state = State::DataHigh;
                None
            }
            State::DataHigh => {
                self.data |= (byte as u16) << 8;
                let write = Write {
                    address: self.address,
                    value: self.data,
                };
                self.address = self.address.wrapping_add(2);
                self.count = self.count.saturating_sub(2);
                self.state = if self.count == 0 {
                    State::AddressLow
                } else {
                    State::DataLow
                };
                Some(write)
            }
        }
    }

    /// Feed a datagram, collecting every completed write.
    pub fn push_slice(&mut self, bytes: &[u8], out: &mut Vec<Write>) {
        for &b in bytes {
            if let Some(w) = self.push(b) {
                out.push(w);
            }
        }
    }
}

/// The address space as last seen.
#[derive(Debug, Default)]
pub struct BiosState {
    words: HashMap<u16, u16>,
}

impl BiosState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply(&mut self, write: Write) {
        self.words.insert(write.address, write.value);
    }

    pub fn word(&self, address: u16) -> Option<u16> {
        self.words.get(&address).copied()
    }

    /// Read one signal, as the catalogue describes it.
    pub fn value(&self, address: u16, mask: u16, shift: u8) -> Option<u16> {
        self.word(address).map(|w| (w & mask) >> shift)
    }

    /// Read a string signal: `len` bytes starting at `address`, two per word,
    /// trimmed at the first NUL.
    pub fn string(&self, address: u16, len: u16) -> Option<String> {
        let mut bytes = Vec::with_capacity(len as usize);
        for i in 0..len.div_ceil(2) {
            let word = self.word(address.wrapping_add(i * 2))?;
            bytes.push((word & 0xff) as u8);
            bytes.push((word >> 8) as u8);
        }
        bytes.truncate(len as usize);
        if let Some(nul) = bytes.iter().position(|&b| b == 0) {
            bytes.truncate(nul);
        }
        Some(String::from_utf8_lossy(&bytes).trim_end().to_string())
    }

    pub fn len(&self) -> usize {
        self.words.len()
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    pub fn clear(&mut self) {
        self.words.clear();
    }
}

/// UDP multicast listener for the export stream.
pub struct Listener {
    socket: UdpSocket,
    decoder: Decoder,
    buf: Vec<u8>,
}

impl Listener {
    /// Join the DCS-BIOS multicast group on the given interface
    /// (`Ipv4Addr::UNSPECIFIED` for the default route).
    pub fn bind(interface: Ipv4Addr) -> io::Result<Self> {
        let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, MULTICAST_PORT))?;
        socket.join_multicast_v4(&MULTICAST_GROUP, &interface)?;
        Ok(Listener {
            socket,
            decoder: Decoder::new(),
            buf: vec![0u8; 8192],
        })
    }

    pub fn set_read_timeout(&self, timeout: Option<std::time::Duration>) -> io::Result<()> {
        self.socket.set_read_timeout(timeout)
    }

    /// Receive one datagram and append the writes it carried.
    pub fn recv(&mut self, out: &mut Vec<Write>) -> io::Result<usize> {
        let n = self.socket.recv(&mut self.buf)?;
        let before = out.len();
        let buf = std::mem::take(&mut self.buf);
        self.decoder.push_slice(&buf[..n], out);
        self.buf = buf;
        Ok(out.len() - before)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(bytes: &[u8]) -> Vec<Write> {
        let mut d = Decoder::new();
        let mut out = Vec::new();
        d.push_slice(bytes, &mut out);
        out
    }

    #[test]
    fn decodes_a_single_write() {
        // sync, address 0x2af8, 2 bytes, value 0x0001
        let bytes = [0x55, 0x55, 0x55, 0x55, 0xf8, 0x2a, 0x02, 0x00, 0x01, 0x00];
        assert_eq!(
            decode(&bytes),
            vec![Write {
                address: 0x2af8,
                value: 1
            }]
        );
    }

    #[test]
    fn decodes_a_run_of_consecutive_words() {
        // one write access of 4 bytes covers two consecutive addresses
        let bytes = [
            0x55, 0x55, 0x55, 0x55, 0x00, 0x10, 0x04, 0x00, 0xaa, 0xbb, 0xcc, 0xdd,
        ];
        assert_eq!(
            decode(&bytes),
            vec![
                Write {
                    address: 0x1000,
                    value: 0xbbaa
                },
                Write {
                    address: 0x1002,
                    value: 0xddcc
                },
            ]
        );
    }

    #[test]
    fn resyncs_mid_stream() {
        // garbage, then a sync and a valid write
        let bytes = [
            0x12, 0x34, 0x56, 0x55, 0x55, 0x55, 0x55, 0xf8, 0x2a, 0x02, 0x00, 0x07, 0x00,
        ];
        assert_eq!(
            decode(&bytes),
            vec![Write {
                address: 0x2af8,
                value: 7
            }]
        );
    }

    #[test]
    fn extracts_masked_signal() {
        let mut state = BiosState::new();
        // AH-64D PLT_GROUND_OVERRIDE_BTN: mask 0x4000, shift 14
        state.apply(Write {
            address: 34548,
            value: 0x4000,
        });
        assert_eq!(state.value(34548, 0x4000, 14), Some(1));
        state.apply(Write {
            address: 34548,
            value: 0x0000,
        });
        assert_eq!(state.value(34548, 0x4000, 14), Some(0));
    }

    #[test]
    fn unknown_address_reads_none() {
        let state = BiosState::new();
        assert_eq!(state.value(0x1234, 0xffff, 0), None);
    }
}
