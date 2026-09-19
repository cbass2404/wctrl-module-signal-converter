//! Where every file lives, in a checkout and once installed.
//!
//! The daemon and the editor share this, so they can never disagree about
//! which profiles or which catalogue are current. A wrong answer here looks
//! like "the editor lost my profiles", so resolution is explicit and in one
//! place rather than guessed at each call site.
//!
//! Two kinds of file, kept apart once installed:
//!
//! * **Shipped, read-only:** the device inventory, display maps, MCDU fonts,
//!   the shipped defaults and the nightly-only list. They sit in `data` beside
//!   the executable and are replaced by each install.
//! * **Written:** the active profiles and the catalogue, with its lock and the
//!   folders a build swaps through. These go to the folder the installer
//!   recorded, by default `Saved Games\DCS Signal Converter`, because the
//!   install folder is the installer's to replace.
//!
//! A checkout keeps both in its own `data` folder, as it always has.

use std::path::{Path, PathBuf};

use crate::Profiles;

/// The product's folder name, under Saved Games by default.
pub const PRODUCT: &str = "DCS Signal Converter";

/// Where the installer records the folders it chose, under HKEY_CURRENT_USER.
/// Kept in step with `editor/src-tauri/installer-hooks.nsh`.
pub const REGISTRY_KEY: &str = "Software\\DCS Signal Converter";
/// The folder profiles and the catalogue are written to.
pub const REGISTRY_DATA: &str = "DataDir";
/// DCS's own Saved Games folder, the one holding `Config` and `Scripts`.
pub const REGISTRY_DCS: &str = "DcsDir";

/// How the paths were found, for reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// `DSC_DATA` named one folder for everything.
    Env,
    /// A development checkout: everything in its `data` folder.
    Checkout,
    /// Installed: shipped files beside the executable, written ones apart.
    Installed,
}

/// Every location the daemon and the editor read or write.
pub struct Paths {
    pub layout: Layout,
    /// Hardware inventory. Read-only.
    pub devices: PathBuf,
    /// Cell and glyph maps for panels with glass. Read-only.
    pub displays: PathBuf,
    /// What the shipped defaults read that only the DCS-BIOS nightly has.
    /// Built at release; absent in a checkout that has not generated one.
    pub nightly_only: PathBuf,
    /// Generated signal catalogue, built from the user's own DCS-BIOS.
    pub catalogue: PathBuf,
    /// Shipped defaults and the active folder the user edits.
    pub profiles: Profiles,
}

impl Paths {
    /// Found in this order:
    ///
    /// 1. `DSC_DATA`, one folder for everything, so a test or a second copy can
    ///    be pointed somewhere else without rebuilding.
    /// 2. `data/devices.json` beside the executable: installed.
    /// 3. A `data/devices.json` in the current directory or any ancestor, then
    ///    the executable's: a checkout. `tauri dev` runs from
    ///    `editor/src-tauri` and cargo from `target/debug`, both below the root.
    ///
    /// Installed is tested before the checkout so an installed copy started
    /// from inside a checkout still uses its own files.
    pub fn resolve() -> Self {
        if let Some(dir) = std::env::var_os("DSC_DATA") {
            return Self::in_one(Layout::Env, PathBuf::from(dir));
        }
        let exe_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf));
        if let Some(dir) = &exe_dir {
            let shipped = dir.join("data");
            if shipped.join("devices.json").is_file() {
                return Self::installed(shipped, writable_dir());
            }
        }
        let cwd = std::env::current_dir().ok();
        for start in cwd.iter().chain(exe_dir.iter()) {
            if let Some(found) = climb(start) {
                return Self::in_one(Layout::Checkout, found);
            }
        }
        Self::in_one(Layout::Checkout, PathBuf::from("data"))
    }

    fn in_one(layout: Layout, root: PathBuf) -> Self {
        Paths {
            layout,
            devices: root.join("devices.json"),
            displays: root.join("displays"),
            nightly_only: root.join("nightly-only.json"),
            catalogue: root.join("catalogue"),
            profiles: Profiles::new(root.join("defaults"), root.join("profiles")),
        }
    }

    /// Shipped files from `shipped`, written ones under `writable`.
    pub fn installed(shipped: PathBuf, writable: PathBuf) -> Self {
        Paths {
            layout: Layout::Installed,
            devices: shipped.join("devices.json"),
            displays: shipped.join("displays"),
            nightly_only: shipped.join("nightly-only.json"),
            catalogue: writable.join("catalogue"),
            profiles: Profiles::new(shipped.join("defaults"), writable.join("profiles")),
        }
    }
}

/// Walk up looking for a `data` directory that actually holds the inventory.
/// Testing for `devices.json` rather than the folder avoids matching some
/// unrelated `data` directory on the way up.
fn climb(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .map(|dir| dir.join("data"))
        .find(|candidate| candidate.join("devices.json").is_file())
}

/// Where an installed copy writes: what the installer recorded, else
/// `Saved Games\DCS Signal Converter`.
pub fn writable_dir() -> PathBuf {
    registry_path(REGISTRY_DATA).unwrap_or_else(|| saved_games().join(PRODUCT))
}

/// DCS's own Saved Games folder: what the installer recorded, else
/// `Saved Games\DCS`. DCS-BIOS and the hook live under its `Scripts`.
pub fn dcs_saved_games() -> PathBuf {
    registry_path(REGISTRY_DCS).unwrap_or_else(|| saved_games().join("DCS"))
}

/// The user's Saved Games folder. Asked of Windows rather than assumed under
/// the profile, because users move it, to another drive most often.
pub fn saved_games() -> PathBuf {
    known_saved_games().unwrap_or_else(|| {
        let home = std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default();
        home.join("Saved Games")
    })
}

/// A folder the installer recorded, if it is set and not empty.
fn registry_path(value: &str) -> Option<PathBuf> {
    read_registry(REGISTRY_KEY, value).filter(|s| !s.trim().is_empty()).map(PathBuf::from)
}

#[cfg(windows)]
fn known_saved_games() -> Option<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{FOLDERID_SavedGames, SHGetKnownFolderPath, KF_FLAG_DEFAULT};

    let mut raw: *mut u16 = std::ptr::null_mut();
    // SAFETY: the out pointer is valid; on return it is either null or a
    // null-terminated string the shell allocated, which must be freed with
    // CoTaskMemFree whatever the result.
    let hr = unsafe { SHGetKnownFolderPath(&FOLDERID_SavedGames, KF_FLAG_DEFAULT as _, std::ptr::null_mut(), &mut raw) };
    let path = if hr >= 0 && !raw.is_null() {
        let len = (0..).take_while(|&i| unsafe { *raw.add(i) } != 0).count();
        let wide = unsafe { std::slice::from_raw_parts(raw, len) };
        Some(PathBuf::from(std::ffi::OsString::from_wide(wide)))
    } else {
        None
    };
    unsafe { CoTaskMemFree(raw as _) };
    path
}

#[cfg(not(windows))]
fn known_saved_games() -> Option<PathBuf> {
    None
}

#[cfg(windows)]
fn read_registry(key: &str, value: &str) -> Option<String> {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};

    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let (key, value) = (wide(key), wide(value));
    let mut bytes: u32 = 0;
    // SAFETY: first call with no buffer asks only for the size in bytes.
    let rc = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), std::ptr::null_mut(), &mut bytes)
    };
    if rc != 0 || bytes == 0 {
        return None;
    }
    let mut buf = vec![0u16; (bytes as usize).div_ceil(2)];
    // SAFETY: buf holds `bytes` bytes, as the size says.
    let rc = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), buf.as_mut_ptr() as _, &mut bytes)
    };
    if rc != 0 {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..len]))
}

#[cfg(not(windows))]
fn read_registry(_key: &str, _value: &str) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_splits_shipped_from_written() {
        let p = Paths::installed(PathBuf::from("C:/app/data"), PathBuf::from("D:/sg/DCS Signal Converter"));
        assert_eq!(p.devices, Path::new("C:/app/data/devices.json"));
        assert_eq!(p.profiles.defaults, Path::new("C:/app/data/defaults"));
        assert_eq!(p.profiles.active, Path::new("D:/sg/DCS Signal Converter/profiles"));
        assert_eq!(p.catalogue, Path::new("D:/sg/DCS Signal Converter/catalogue"));
    }

    #[test]
    fn a_checkout_finds_its_data_from_below() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
        let found = climb(&root.join("editor").join("src-tauri")).expect("the repository's data");
        assert!(found.join("devices.json").is_file());
    }

    #[cfg(windows)]
    #[test]
    fn saved_games_comes_from_windows() {
        assert!(known_saved_games().is_some_and(|p| p.is_absolute()));
    }
}
