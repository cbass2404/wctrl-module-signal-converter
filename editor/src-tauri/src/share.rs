//! Export and import a profile, so a setup can be shared.
//!
//! Both file dialogs are run here, never by the window, which has no
//! permission to open one. The window asks for an export or an import and is
//! handed back what came of it.
//!
//! An import is checked the way a save is, and settles its aircraft the way
//! Copy to... does: a profile that would not load is refused, and an aircraft
//! another profile already flies moves to the import only if the user ticks
//! it and confirms the move. Unlike Copy to..., a move may take every aircraft
//! a profile has, and that profile is then deleted, but only once the user has
//! confirmed deleting it. The file it came from is never written.

use std::path::{Path, PathBuf};

use dsc_config::paths::Paths;
use dsc_config::Profile;
use serde::Serialize;
use tauri_plugin_dialog::DialogExt;

use crate::check::Cache;
use crate::claims;
use crate::view::ModuleChoice;

/// A profile picked for import, as the import dialog needs it.
#[derive(Serialize)]
pub struct Preview {
    /// Where it was picked from, handed back to `import_profile`.
    pub path: String,
    pub name: String,
    pub author: String,
    pub module: String,
    pub aircraft: Vec<String>,
    pub bound: usize,
    pub total: usize,
    /// Rows reading something this DCS-BIOS cannot back. They load and stay off.
    pub flagged: usize,
    pub cautions: Vec<String>,
}

/// The profile at `path`, if it is one this install would load.
///
/// Refused when it will not parse, when it reads a module the installed
/// DCS-BIOS does not have, or when the daemon would refuse it. A module that is
/// missing is said plainly rather than left to the check, whose answer would be
/// every signal in the file, one by one.
fn read(paths: &Paths, cache: &Cache, path: &Path) -> Result<Profile, String> {
    let shown = path.file_name().unwrap_or_default().to_string_lossy();
    let profile = Profile::load(path).map_err(|e| format!("{shown} is not a profile this editor can read: {e}"))?;
    let modules = ModuleChoice::read_index(&paths.catalogue.join("index.json"))?;
    if !modules.iter().any(|m| m.key == profile.module) {
        return Err(format!(
            "{} reads {}, which the DCS-BIOS installed here does not have, so none of its signals could be found.",
            profile.name, profile.module
        ));
    }
    if profile.aircraft.is_empty() {
        return Err(format!("{} names no aircraft, so DCS would never fly it.", profile.name));
    }
    let problems = cache.problems(paths, &profile);
    if !problems.is_empty() {
        return Err(format!(
            "{} was not imported, because the daemon would refuse it:\n{}",
            profile.name,
            problems.join("\n")
        ));
    }
    Ok(profile)
}

/// Give an imported profile its name and the aircraft chosen for it, which
/// must be some of those it came with. Anything else is not the profile the
/// dialog showed.
fn settle(mut profile: Profile, name: String, aircraft: Vec<String>) -> Result<Profile, String> {
    if let Some(stray) = aircraft.iter().find(|a| !profile.aircraft.contains(a)) {
        return Err(format!("{stray} is not one of the aircraft {} was made for", profile.name));
    }
    profile.name = name;
    profile.aircraft = aircraft;
    Ok(profile)
}

fn chosen(path: Option<tauri_plugin_dialog::FilePath>) -> Result<Option<PathBuf>, String> {
    path.map(|p| p.into_path().map_err(|e| format!("reading the chosen path: {e}")))
        .transpose()
}

/// Save one profile from the active folder wherever the user chooses. Returns
/// where it went, or nothing if the dialog was cancelled.
///
/// The file is copied as it is on disk, so what is shared is exactly what flies
/// here. Async because the dialog blocks, and a blocking dialog on the main
/// thread would hang the window it belongs to.
#[tauri::command]
pub async fn export_profile(app: tauri::AppHandle, file: String) -> Result<Option<String>, String> {
    let paths = Paths::resolve();
    let from = paths.profiles.active.join(&file);
    let profile = Profile::load(&from).map_err(|e| format!("reading {file}: {e}"))?;
    let picked = app
        .dialog()
        .file()
        .set_title(format!("Export {}", profile.name))
        .set_file_name(&file)
        .add_filter("Profile", &["json"])
        .blocking_save_file();
    let Some(to) = chosen(picked)? else { return Ok(None) };
    // Saving over the very file being exported would truncate it first.
    let same = matches!(
        (std::fs::canonicalize(&from), std::fs::canonicalize(&to)),
        (Ok(a), Ok(b)) if a == b
    );
    if !same {
        std::fs::copy(&from, &to).map_err(|e| format!("writing {}: {e}", to.display()))?;
    }
    Ok(Some(to.display().to_string()))
}

/// Ask for a profile to import and say what it holds. Nothing is written yet;
/// the window shows this and `import_profile` does the work.
#[tauri::command]
pub async fn import_pick(app: tauri::AppHandle, cache: tauri::State<'_, Cache>) -> Result<Option<Preview>, String> {
    let picked = app
        .dialog()
        .file()
        .set_title("Import a profile")
        .add_filter("Profile", &["json"])
        .blocking_pick_file();
    let Some(path) = chosen(picked)? else { return Ok(None) };
    let paths = Paths::resolve();
    let profile = read(&paths, &cache, &path)?;
    let (flags, _) = cache.flags(&paths, &profile);
    Ok(Some(Preview {
        path: path.display().to_string(),
        name: profile.name.clone(),
        author: profile.author.clone(),
        module: profile.module.clone(),
        aircraft: profile.aircraft.clone(),
        bound: profile.bindings.iter().filter(|b| !b.is_placeholder()).count(),
        total: profile.bindings.len(),
        flagged: flags.len(),
        cautions: cache.all_cautions(&paths, &profile),
    }))
}

/// Write the profile at `path` into the active folder under `name`, for the
/// chosen `aircraft`. Returns the new file name. Aircraft another profile
/// flies move to it, as they do for Copy to...; see `claims`. A profile the
/// move leaves with no aircraft is deleted if `delete` names it, which the
/// window does only after the user confirms, and refused otherwise.
///
/// The file is read and checked again rather than trusted from the preview,
/// since it may have changed while the dialog was open.
#[tauri::command]
pub fn import_profile(
    path: String,
    name: String,
    aircraft: Vec<String>,
    delete: Vec<String>,
    cache: tauri::State<'_, Cache>,
) -> Result<String, String> {
    let paths = Paths::resolve();
    let profile = read(&paths, &cache, Path::new(&path))?;
    claims::write_new_deleting(&paths.profiles.active, settle(profile, name, aircraft)?, &delete)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> Profile {
        serde_json::from_str(
            r#"{"name": "Shared", "author": "someone", "aircraft": ["A-10C_2", "A-10C"], "module": "A-10C", "bindings": []}"#,
        )
        .unwrap()
    }

    #[test]
    fn an_import_keeps_what_it_came_with_under_its_new_name() {
        let p = settle(profile(), "Mine".into(), vec!["A-10C".into()]).unwrap();
        assert_eq!(p.name, "Mine");
        assert_eq!(p.aircraft, vec!["A-10C".to_string()]);
        assert_eq!(p.author, "someone", "the author travels with it");
    }

    #[test]
    fn an_import_cannot_be_given_an_aircraft_it_was_not_made_for() {
        let err = settle(profile(), "Mine".into(), vec!["F-16C_50".into()]).unwrap_err();
        assert!(err.contains("F-16C_50"), "{err}");
    }
}
