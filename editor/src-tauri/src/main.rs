// Release builds open a window, not a console, so Windows should not attach one.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! The profile editor's backend.
//!
//! Every command here is a thin wrapper over `wctrl-config`. The editor and the
//! daemon read and write profiles through exactly the same code, so a profile
//! the editor produces cannot be one the daemon rejects.

mod check;
mod learn;
mod paths;
mod view;

use paths::Paths;
use view::{DeviceView, ModuleChoice, ProfileSummary, SignalView};
use wctrl_config::{DeviceInventory, Module, Profile};

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
    let inv = inventory(&paths)?;
    let maps = wctrl_config::DisplayCatalogue::load_dir(&paths.displays)
        .map_err(|e| format!("loading {}: {e}", paths.displays.display()))?;
    let mut out: Vec<DeviceView> = inv
        .devices
        .iter()
        .map(|d| DeviceView::of(d).with_displays(d, &maps))
        .collect();
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

/// Every bindable signal in one module, for the typeahead and the hint box.
#[tauri::command]
fn signals(module: String) -> Reply<Vec<SignalView>> {
    let paths = Paths::resolve();
    SignalView::of_module(&paths.catalogue.join(format!("{module}.json")))
}

#[tauri::command]
fn open_profile(file: String) -> Reply<Profile> {
    let paths = Paths::resolve();
    let path = paths.profiles.active.join(&file);
    Profile::load(&path).map_err(|e| fail(&format!("reading {file}"), e))
}

/// The shipped version of one profile, if it has one.
///
/// Loaded alongside the profile so a single lamp can be put back the way it
/// shipped without discarding every other edit in the file. Profiles the user
/// created themselves have no default, which is not an error: the answer is
/// simply that there is nothing to revert to.
#[tauri::command]
fn default_profile(file: String) -> Reply<Option<Profile>> {
    let paths = Paths::resolve();
    if !paths.profiles.has_default(&file) {
        return Ok(None);
    }
    Profile::load(&paths.profiles.defaults.join(&file))
        .map(Some)
        .map_err(|e| fail(&format!("reading the shipped {file}"), e))
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

/// Copy an existing profile to a new aircraft.
///
/// The case this exists for is a module that reuses another's DCS-BIOS
/// definitions, where the work is already done and only the name and the
/// aircraft list differ: the Super Hornet community mod reads the Hornet
/// catalogue, so the Hornet profile drives it unchanged. Doing that by hand
/// means copying a file, editing two fields, and getting the third one wrong.
///
/// `module` is deliberately carried over rather than asked for. A copy whose
/// signal ids resolve against a different catalogue is not a copy, it is a
/// profile full of unknown signals.
#[tauri::command]
fn clone_profile(file: String, name: String, aircraft: Vec<String>) -> Reply<String> {
    let paths = Paths::resolve();
    let name = name.trim();
    if name.is_empty() {
        return Err("a profile needs a name".into());
    }
    let aircraft: Vec<String> = aircraft
        .into_iter()
        .map(|a| a.trim().to_string())
        .filter(|a| !a.is_empty())
        .collect();
    if aircraft.is_empty() {
        return Err("a profile needs at least one aircraft name, as DCS reports it".into());
    }

    let mut profile = Profile::load(&paths.profiles.active.join(&file))
        .map_err(|e| fail(&format!("reading {file}"), e))?;
    profile.name = name.to_string();
    profile.aircraft = aircraft;

    let out = format!("{}.json", slug(name));
    let path = paths.profiles.active.join(&out);
    if path.exists() {
        return Err(format!("{out} already exists"));
    }
    std::fs::create_dir_all(&paths.profiles.active)
        .map_err(|e| fail("creating the profile folder", e))?;
    profile.save(&path).map_err(|e| fail(&format!("writing {out}"), e))?;
    Ok(out)
}

/// Start watching the export stream for one module.
///
/// Called every time the learn panel is opened, and cheap to repeat: a session
/// already on this module is left running rather than rebuilt, because its
/// baseline took a full export cycle to gather and throwing it away would make
/// the user wait again for nothing.
#[tauri::command]
fn learn_start(module: String, learn: tauri::State<learn::State>) -> Reply<()> {
    let paths = Paths::resolve();
    let path = paths.catalogue.join(format!("{module}.json"));
    let module = Module::load(&path).map_err(|e| fail(&format!("reading {}", path.display()), e))?;
    learn.start(&module);
    Ok(())
}

/// What has moved since the panel was opened, most switch-like first.
///
/// Polled rather than pushed. The window wants the same four facts on every
/// tick whether or not anything changed, so there is nothing an event would
/// save, and a poll cannot leave the panel stale if a message is missed.
#[tauri::command]
fn learn_poll(learn: tauri::State<learn::State>) -> Reply<learn::Report> {
    Ok(learn.with(|s| s.report()).unwrap_or_else(learn::Report::idle))
}

/// Clear the list and watch again, keeping the map of the cockpit.
#[tauri::command]
fn learn_again(learn: tauri::State<learn::State>) -> Reply<()> {
    learn.with(|s| s.rearm());
    Ok(())
}

/// Close the socket. The panel calls this on the way out, and the session
/// would be dropped with the window in any case.
#[tauri::command]
fn learn_stop(learn: tauri::State<learn::State>) -> Reply<()> {
    learn.stop();
    Ok(())
}

/// Every reason the daemon would refuse this profile, for the window to show.
///
/// Called after each edit rather than on save. A fault found where it was made
/// costs one click to undo; the same fault found by the daemon costs a flight,
/// because it skips the whole profile and every lamp in it stays dark.
#[tauri::command]
fn check_profile(profile: Profile, cache: tauri::State<check::Cache>) -> Reply<Vec<String>> {
    let paths = Paths::resolve();
    Ok(cache.problems(&paths, &profile))
}

/// Write a profile, refusing one the daemon would not load.
///
/// The window checks as the user types and will not offer Save while anything
/// is outstanding, so this is the guarantee rather than the message: the file
/// on disk is one that runs. Refusing also protects what is already there,
/// since the previous save is very likely a profile that flies.
#[tauri::command]
fn save_profile(file: String, profile: Profile, cache: tauri::State<check::Cache>) -> Reply<()> {
    let paths = Paths::resolve();
    let problems = cache.problems(&paths, &profile);
    if !problems.is_empty() {
        return Err(format!(
            "{file} was not written, because the daemon would refuse it:
{}",
            problems.join("
")
        ));
    }
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
        .manage(learn::State::default())
        .manage(check::Cache::default())
        .invoke_handler(tauri::generate_handler![
            devices,
            modules,
            profiles,
            signals,
            open_profile,
            default_profile,
            create_profile,
            clone_profile,
            check_profile,
            save_profile,
            reset_profile,
            learn_start,
            learn_poll,
            learn_again,
            learn_stop
        ])
        .run(tauri::generate_context!())
        .expect("starting the editor window");
}

#[cfg(test)]
mod tests {
    use super::slug;

    #[test]
    fn a_copied_profile_is_named_after_what_it_was_called() {
        // The file name comes from the profile name, not the module, because a
        // copy is by definition a second profile on the same module: naming it
        // after the module would collide with the one it came from.
        assert_eq!(slug("FA-18E"), "fa-18e");
        assert_eq!(slug("F/A-18C Hornet copy"), "f-a-18c-hornet-copy");
        // A name that slugs to nothing would write ".json", so the command
        // rejects an empty name before it reaches here.
        assert_eq!(slug("   "), "");
    }

    #[test]
    fn module_keys_slug_to_the_names_already_shipped() {
        assert_eq!(slug("FA-18C_hornet"), "fa-18c-hornet");
        assert_eq!(slug("A-10C_2"), "a-10c-2");
        assert_eq!(slug("Christen Eagle II"), "christen-eagle-ii");
    }
}
