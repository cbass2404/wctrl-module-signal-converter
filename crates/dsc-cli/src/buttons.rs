//! A panel's buttons, numbered the way Windows numbers them.
//!
//! Keys arrive on a panel's game controller collection, which DCS and SimAppPro
//! read as well. Windows gives every open handle its own copy of each input
//! report, so reading here takes nothing away from them. The numbers come from
//! Windows' own HID parser, the same one behind the numbers SimAppPro lights
//! and DCS binds, so there is no descriptor walk of ours to disagree with them.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

use anyhow::{bail, Result};
use windows_sys::Win32::Devices::HumanInterfaceDevice::{
    HidD_FreePreparsedData, HidD_GetPreparsedData, HidP_GetButtonCaps, HidP_GetCaps, HidP_GetUsages, HidP_Input,
    HidP_MaxUsageListLength, HIDP_BUTTON_CAPS, HIDP_CAPS, HIDP_STATUS_SUCCESS, PHIDP_PREPARSED_DATA,
};
use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_READ, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{CreateFileW, ReadFile, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING};

/// The HID usage page every button is on. A button's usage on it is its number.
const BUTTON_PAGE: u16 = 0x09;

/// One top level collection of a panel, opened for reading.
pub struct Collection {
    handle: HANDLE,
    preparsed: PHIDP_PREPARSED_DATA,
    pub usage_page: u16,
    pub usage: u16,
    /// Every input report is this long, report id byte included.
    pub report_len: usize,
    /// The buttons it declares, as (report id, first number, last number).
    pub buttons: Vec<(u8, u16, u16)>,
}

// The handle and the parsed descriptor are only ever used by one thread at a
// time; the reader moves the whole collection into its own.
unsafe impl Send for Collection {}

impl Collection {
    /// Open the collection at a HID interface path, as hidapi lists it.
    pub fn open(path: &str) -> Result<Collection> {
        let wide: Vec<u16> = OsStr::new(path).encode_wide().chain(Some(0)).collect();
        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                GENERIC_READ,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            bail!(std::io::Error::last_os_error());
        }
        let mut preparsed = 0;
        if unsafe { HidD_GetPreparsedData(handle, &mut preparsed) } == 0 {
            let e = std::io::Error::last_os_error();
            unsafe { CloseHandle(handle) };
            bail!(e);
        }
        // From here Drop closes both, whatever goes wrong.
        let mut collection = Collection {
            handle,
            preparsed,
            usage_page: 0,
            usage: 0,
            report_len: 0,
            buttons: Vec::new(),
        };
        let mut caps: HIDP_CAPS = unsafe { std::mem::zeroed() };
        if unsafe { HidP_GetCaps(preparsed, &mut caps) } != HIDP_STATUS_SUCCESS {
            bail!("Windows could not read this collection's descriptor");
        }
        collection.usage_page = caps.UsagePage;
        collection.usage = caps.Usage;
        collection.report_len = caps.InputReportByteLength as usize;

        let mut count = caps.NumberInputButtonCaps;
        if count > 0 {
            let mut list: Vec<HIDP_BUTTON_CAPS> = vec![unsafe { std::mem::zeroed() }; count as usize];
            if unsafe { HidP_GetButtonCaps(HidP_Input, list.as_mut_ptr(), &mut count, preparsed) } == HIDP_STATUS_SUCCESS {
                for c in &list[..count as usize] {
                    if c.UsagePage != BUTTON_PAGE {
                        continue;
                    }
                    let (first, last) = unsafe {
                        if c.IsRange != 0 {
                            (c.Anonymous.Range.UsageMin, c.Anonymous.Range.UsageMax)
                        } else {
                            (c.Anonymous.NotRange.Usage, c.Anonymous.NotRange.Usage)
                        }
                    };
                    collection.buttons.push((c.ReportID, first, last));
                }
            }
        }
        Ok(collection)
    }

    /// Wait for the next input report. Blocks until the panel sends one.
    pub fn read(&self, buf: &mut Vec<u8>) -> Result<()> {
        buf.resize(self.report_len, 0);
        let mut read = 0u32;
        let ok = unsafe { ReadFile(self.handle, buf.as_mut_ptr(), buf.len() as u32, &mut read, std::ptr::null_mut()) };
        if ok == 0 {
            bail!(std::io::Error::last_os_error());
        }
        buf.truncate(read as usize);
        Ok(())
    }

    /// The buttons a report says are down, by number, lowest first. `None`
    /// when the report carries no buttons, such as one with another report id.
    pub fn pressed(&self, report: &[u8]) -> Option<Vec<u16>> {
        let mut report = report.to_vec();
        let mut len = unsafe { HidP_MaxUsageListLength(HidP_Input, BUTTON_PAGE, self.preparsed) };
        if len == 0 {
            return None;
        }
        let mut usages = vec![0u16; len as usize];
        let status = unsafe {
            HidP_GetUsages(
                HidP_Input,
                BUTTON_PAGE,
                0,
                usages.as_mut_ptr(),
                &mut len,
                self.preparsed,
                report.as_mut_ptr(),
                report.len() as u32,
            )
        };
        if status != HIDP_STATUS_SUCCESS {
            return None;
        }
        usages.truncate(len as usize);
        usages.sort_unstable();
        Some(usages)
    }
}

impl Drop for Collection {
    fn drop(&mut self) {
        unsafe {
            HidD_FreePreparsedData(self.preparsed);
            CloseHandle(self.handle);
        }
    }
}
