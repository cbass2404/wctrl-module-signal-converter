//! Where the editor finds data, in development and once installed.
//!
//! Development runs from a checkout and installed runs from Program Files, so
//! neither a relative path nor the executable's own folder works on its own.
//! Resolution is explicit and in one place rather than guessed at each call
//! site, because a wrong answer here looks like "the editor lost my profiles".

use std::path::{Path, PathBuf};

use wctrl_config::Profiles;

/// Every location the editor reads or writes.
pub struct Paths {
    /// Hardware inventory. Read-only.
    pub devices: PathBuf,
    /// Generated signal catalogue, built from the user's own DCS-BIOS.
    pub catalogue: PathBuf,
    /// Shipped defaults and the active folder the user edits.
    pub profiles: Profiles,
}

impl Paths {
    pub fn resolve() -> Self {
        let root = data_root();
        Paths {
            devices: root.join("devices.json"),
            catalogue: root.join("catalogue"),
            profiles: Profiles::new(root.join("defaults"), root.join("profiles")),
        }
    }
}

/// The `data` directory, found in this order:
///
/// 1. `WCTRL_DATA`, which exists so a test or a second install can be pointed
///    somewhere else without rebuilding.
/// 2. A `data/devices.json` in the current directory or any ancestor. This is
///    the development case: `tauri dev` runs the binary from `editor/src-tauri`,
///    several levels below the repository root.
/// 3. `data` beside the executable, which is how the installed app is laid out.
///
/// Falling back rather than failing means a missing catalogue surfaces as
/// "no modules to choose from" in the window, which a user can act on, instead
/// of a startup crash they cannot.
fn data_root() -> PathBuf {
    if let Some(dir) = std::env::var_os("WCTRL_DATA") {
        return PathBuf::from(dir);
    }
    if let Ok(cwd) = std::env::current_dir() {
        if let Some(found) = climb(&cwd) {
            return found;
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            return dir.join("data");
        }
    }
    PathBuf::from("data")
}

/// Walk up looking for a `data` directory that actually holds the inventory.
/// Testing for `devices.json` rather than the folder avoids matching some
/// unrelated `data` directory on the way up.
fn climb(start: &Path) -> Option<PathBuf> {
    for dir in start.ancestors() {
        let candidate = dir.join("data");
        if candidate.join("devices.json").is_file() {
            return Some(candidate);
        }
    }
    None
}
