// Release builds open a window, not a console, so Windows should not attach one.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! The profile editor's backend.
//!
//! Every command here is a thin wrapper over `wctrl-config`. The editor and the
//! daemon read and write profiles through exactly the same code, so a profile
//! the editor produces cannot be one the daemon rejects.

mod paths;
mod view;

use paths::Paths;
use view::{DeviceView, ModuleChoice, ProfileSummary};
use wctrl_config::{DeviceInventory, Profile};

/// Commands return a message rather than an error type, because the only useful
/// thing the window can do with a failure is show it to the user.
type Reply<T> = Result<T, String>;

fn fail(context: &str, e: impl std::fmt::Display) -> String {
    format!("{context}: {e}")
}

fn inventory(paths: &Paths) -> Reply<DeviceInventory> {
    DeviceInventory::load(&paths.devices)
        .map_err(|e| fail(&format!("reading {}", paths.devices.display()), e))
}

/// Every device we have mapped, ordered by the name the user actually sees.
///
/// Sorted once here rather than in `data/devices.json`, which is edited by hand
/// and would drift the first time a device was appended at the bottom, and by
/// `display_name` rather than `key`, because `PTO2` and `TAKEOFF_PLANEL_2` do
/// not sort the same way and only one of them is ever on screen.
///
/// Unconnected devices are listed too. A profile has to be editable with the
/// panels unplugged, which is most of the time.
#[tauri::command]
fn devices() -> Reply<Vec<DeviceView>> {
    let paths = Paths::resolve();
    let mut out: Vec<DeviceView> = inventory(&paths)?.devices.iter().map(DeviceView::of).collect();
    out.sort_by(|a, b| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase()));
    Ok(out)
}

/// The modules a new profile can be built for.
///
/// Read from the catalogue's `index.json` rather than by loading the catalogue,
/// which is about 11 MB across fifty files and holds nothing this list needs.
/// The list is therefore whatever the user's own DCS-BIOS supports, which is the
/// point: the dropdown cannot offer a module they do not have.
#[tauri::command]
fn modules() -> Reply<Vec<ModuleChoice>> {
    let paths = Paths::resolve();
    ModuleChoice::read_index(&paths.catalogue.join("index.json"))
}

/// Profiles in the active folder, seeding from the shipped defaults first.
///
/// Seeding on every listing rather than once at install means a user who empties
/// the folder, or who installs an update, arrives at the same place without
/// having to be told to do anything.
#[tauri::command]
fn profiles() -> Reply<Vec<ProfileSummary>> {
    let paths = Paths::resolve();
    paths
        .profiles
        .seed()
        .map_err(|e| fail("copying in the shipped profiles", e))?;

    let dir = &paths.profiles.active;
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        // No folder is the same as no profiles. The user makes one and it
        // appears; there is nothing here worth interrupting them over.
        Err(_) => return Ok(out),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let file = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let has_default = paths.profiles.has_default(&file);
        // A profile that will not parse is still listed, carrying its error.
        // Hiding it would leave the user looking for a file they can see on disk.
        out.push(match Profile::load(&path) {
            Ok(p) => ProfileSummary::of(&p, file, has_default),
            Err(e) => ProfileSummary::broken(file, has_default, e.to_string()),
        });
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(out)
}

#[tauri::command]
fn open_profile(file: String) -> Reply<Profile> {
    let paths = Paths::resolve();
    let path = paths.profiles.active.join(&file);
    Profile::load(&path).map_err(|e| fail(&format!("reading {file}"), e))
}

/// Create a profile for one module, populated with every lamp and none assigned.
///
/// The name and the aircraft list come from the catalogue rather than from the
/// user: a module can serve several runtime aircraft names, `A-10C` covering
/// both `A-10C_2` and `A-10C`, and a typed list would be a silent way to build a
/// profile DCS never matches.
#[tauri::command]
fn create_profile(module: String) -> Reply<String> {
    let paths = Paths::resolve();
    let choices = ModuleChoice::read_index(&paths.catalogue.join("index.json"))?;
    let choice = choices
        .iter()
        .find(|m| m.key == module)
        .ok_or_else(|| format!("{module} is not in the catalogue"))?;

    let inv = inventory(&paths)?;
    let first = choice.aircraft.first().map(String::as_str).unwrap_or(&module);
    let mut profile = Profile::stub(&module, first, &module, &inv);
    profile.aircraft = choice.aircraft.clone();

    let file = format!("{}.json", slug(&module));
    let path = paths.profiles.active.join(&file);
    if path.exists() {
        return Err(format!("{file} already exists"));
    }
    std::fs::create_dir_all(&paths.profiles.active).map_err(|e| fail("creating the profile folder", e))?;
    profile.save(&path).map_err(|e| fail(&format!("writing {file}"), e))?;
    Ok(file)
}

#[tauri::command]
fn save_profile(file: String, profile: Profile) -> Reply<()> {
    let paths = Paths::resolve();
    profile
        .save(&paths.profiles.active.join(&file))
        .map_err(|e| fail(&format!("writing {file}"), e))
}

/// Replace one profile with its shipped default. The only call that discards
/// the user's work, so nothing reaches it except the reset button.
#[tauri::command]
fn reset_profile(file: String) -> Reply<()> {
    let paths = Paths::resolve();
    paths
        .profiles
        .reset_to_default(&file)
        .map_err(|e| fail(&format!("resetting {file}"), e))
}

/// Lowercase, non-alphanumerics collapsed to single dashes: `FA-18C_hornet`
/// becomes `fa-18c-hornet`, matching the profiles already shipped.
fn slug(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            devices,
            modules,
            profiles,
            open_profile,
            create_profile,
            save_profile,
            reset_profile
        ])
        .run(tauri::generate_context!())
        .expect("starting the editor window");
}

#[cfg(test)]
mod tests {
    use super::slug;

    #[test]
    fn module_keys_slug_to_the_names_already_shipped() {
        assert_eq!(slug("FA-18C_hornet"), "fa-18c-hornet");
        assert_eq!(slug("A-10C_2"), "a-10c-2");
        assert_eq!(slug("Christen Eagle II"), "christen-eagle-ii");
    }
}
