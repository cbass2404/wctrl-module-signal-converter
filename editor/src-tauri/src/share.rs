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
//!
//! An import can instead be merged into a profile already here, taking only
//! the panels' lamps and screen lines the user ticks. So can another profile
//! on the same module, which is how the F-14 and F-14BU share a change
//! without it being made twice. See `dsc_config::merge`.
//!
//! A profile travels with its pages, which live in a library apart from
//! it: an export writes the pages its slots show and any others ticked, and an
//! import brings in the pages ticked, settling each against the library here.
//! See `dsc_config::bundle`.

use std::path::{Path, PathBuf};

use dsc_config::bundle::{self, Bundle, PagePlan, PageTake};
use dsc_config::merge::{self, Change, Parts, Pick};
use dsc_config::paths::Paths;
use dsc_config::{DeviceInventory, DisplayCatalogue, Page, PageLibrary, Profile};
use serde::{Deserialize, Serialize};
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
    /// What it could give a profile already here, for a merge.
    pub parts: Parts,
    /// The pages it brings, and what becomes of each here.
    pub pages: Vec<PagePlan>,
}

/// A page on the module, for the export dialog to offer.
#[derive(Serialize)]
pub struct ExportPage {
    pub id: String,
    pub name: String,
    /// A slot in the profile shows it, so it goes whether ticked or not.
    pub used: bool,
}

/// Where a merge takes from: a file picked for import, or a profile here.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Source {
    File { path: String },
    Profile { file: String },
}

/// What a merge would do, for the window to put to the user before it is done.
#[derive(Serialize)]
pub struct MergeReport {
    /// Every panel and line picked, including any it would leave as they are.
    pub changes: Vec<Change>,
    pub notes: Vec<String>,
}

pub fn maps(paths: &Paths) -> Result<(DeviceInventory, DisplayCatalogue), String> {
    let devices = DeviceInventory::load(&paths.devices).map_err(|e| format!("reading {}: {e}", paths.devices.display()))?;
    let displays =
        DisplayCatalogue::load_dir(&paths.displays).map_err(|e| format!("loading {}: {e}", paths.displays.display()))?;
    Ok((devices, displays))
}

impl Source {
    /// The profile to take from, checked the way an import is when it comes
    /// from outside, with the pages it brings: a file's own, or none for a
    /// profile here, whose pages are already in the library.
    fn load(&self, paths: &Paths, cache: &Cache) -> Result<Bundle, String> {
        match self {
            Source::File { path } => read(paths, cache, Path::new(path)),
            Source::Profile { file } => {
                let profile =
                    Profile::load(&paths.profiles.active.join(file)).map_err(|e| format!("reading {file}: {e}"))?;
                Ok(Bundle { schema_version: profile.schema_version, profile, pages: Vec::new() })
            }
        }
    }
}

/// Every page in `bundle` ticked, under the name the plan gives it.
fn take_all(lib: &PageLibrary, bundle: &Bundle) -> Vec<PageTake> {
    bundle::plan(lib, &bundle.profile, &bundle.pages)
        .into_iter()
        .map(|p| PageTake { id: p.id, name: p.name_after })
        .collect()
}

/// `profile` as it would arrive with the pages in `take`, and the library it
/// would arrive into.
fn arriving(
    lib: &PageLibrary,
    profile: &Profile,
    pages: &[Page],
    take: &[PageTake],
) -> Result<(Profile, Vec<Page>, PageLibrary), String> {
    let mut profile = profile.clone();
    let added = bundle::bring_in(lib, &mut profile, pages, take)?;
    let lib = bundle::with_added(lib, &profile.module, &added);
    Ok((profile, added, lib))
}

/// Write `added` into the library's file for `module`.
fn write_pages(paths: &Paths, module: &str, added: &[Page]) -> Result<(), String> {
    if added.is_empty() {
        return Ok(());
    }
    let lib = paths.pages.library();
    if let Some(why) = lib.broken(module) {
        return Err(format!("the page file for {module} would not load, so no page could be added to it: {why}"));
    }
    bundle::with_added(&lib, module, added)
        .save_module(&paths.pages.active, module)
        .map_err(|e| format!("writing the pages for {module}: {e}"))
}

/// The page slots `profile` has, named from `lib`.
fn slot_parts(profile: &Profile, devices: &DeviceInventory, lib: &PageLibrary) -> Vec<merge::SlotPart> {
    merge::slot_parts(profile, devices, |id| lib.page_on(&profile.module, id).map(|p| p.name.clone()))
}

/// The profile at `path`, with the pages it brings, if it is one this
/// install would load.
///
/// Refused when it will not parse, when it reads a module the installed
/// DCS-BIOS does not have, or when the daemon would refuse it with every page
/// it brings taken. A module that is missing is said plainly rather than left
/// to the check, whose answer would be every signal in the file, one by one.
fn read(paths: &Paths, cache: &Cache, path: &Path) -> Result<Bundle, String> {
    let shown = path.file_name().unwrap_or_default().to_string_lossy();
    let bundle = Bundle::load(path).map_err(|e| format!("{shown} is not a profile this editor can read: {e}"))?;
    let profile = &bundle.profile;
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
    let lib = paths.pages.library();
    let (arrived, _, lib) = arriving(&lib, profile, &bundle.pages, &take_all(&lib, &bundle))?;
    let problems = cache.problems(paths, &arrived, &lib);
    if !problems.is_empty() {
        return Err(format!(
            "{} was not imported, because the daemon would refuse it:\n{}",
            profile.name,
            problems.join("\n")
        ));
    }
    Ok(bundle)
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

/// Every page on a profile's module, for the export dialog: the ones its
/// slots show, which always go, and the rest, which go if ticked.
#[tauri::command]
pub fn export_pages(file: String) -> Result<Vec<ExportPage>, String> {
    let paths = Paths::resolve();
    let profile = Profile::load(&paths.profiles.active.join(&file)).map_err(|e| format!("reading {file}: {e}"))?;
    let used = profile.pages_used();
    Ok(paths
        .pages
        .library()
        .on_module(&profile.module)
        .iter()
        .map(|p| ExportPage { id: p.id.clone(), name: p.name.clone(), used: used.contains(&p.id) })
        .collect())
}

/// Save one profile from the active folder wherever the user chooses, with
/// every page its slots show and the pages in `also`. Returns where it went,
/// or nothing if the dialog was cancelled.
///
/// The profile is written as it is on disk, so what is shared is exactly what
/// flies here. Async because the dialog blocks, and a blocking dialog on the
/// main thread would hang the window it belongs to.
#[tauri::command]
pub async fn export_profile(app: tauri::AppHandle, file: String, also: Vec<String>) -> Result<Option<String>, String> {
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
    // An export is a bundle, not a profile, so writing one over the profile
    // it came from would leave the active folder holding a file it cannot fly.
    let same = matches!(
        (std::fs::canonicalize(&from), std::fs::canonicalize(&to)),
        (Ok(a), Ok(b)) if a == b
    );
    if same {
        return Err(format!("{} cannot be exported over itself; choose another place", profile.name));
    }
    Bundle::of(&profile, &paths.pages.library(), &also)
        .save(&to)
        .map_err(|e| format!("writing {}: {e}", to.display()))?;
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
    let bundle = read(&paths, &cache, &path)?;
    let saved = paths.pages.library();
    let pages = bundle::plan(&saved, &bundle.profile, &bundle.pages);
    let (profile, _, lib) = arriving(&saved, &bundle.profile, &bundle.pages, &take_all(&saved, &bundle))?;
    let (flags, _) = cache.flags(&paths, &profile, &lib);
    let (devices, displays) = maps(&paths)?;
    let mut parts = merge::parts(&profile, &devices, &displays);
    parts.slots = slot_parts(&profile, &devices, &lib);
    Ok(Some(Preview {
        parts,
        pages,
        path: path.display().to_string(),
        name: profile.name.clone(),
        author: profile.author.clone(),
        module: profile.module.clone(),
        aircraft: profile.aircraft.clone(),
        bound: profile.bindings.iter().filter(|b| !b.is_placeholder()).count(),
        total: profile.bindings.len(),
        flagged: flags.len(),
        cautions: cache.all_cautions(&paths, &profile, &lib),
    }))
}

/// Write the profile at `path` into the active folder under `name`, for the
/// chosen `aircraft`, with the pages in `pages` under the names given them.
/// Returns the new file name. Aircraft another profile flies move to it, as
/// they do for Copy to...; see `claims`. A profile the move leaves with no
/// aircraft is deleted if `delete` names it, which the window does only after
/// the user confirms, and refused otherwise. A slot showing a page left out
/// comes in empty.
///
/// The file is read and checked again rather than trusted from the preview,
/// since it may have changed while the dialog was open. The profile is written
/// before its pages, so a name that is refused leaves the library untouched.
#[tauri::command]
pub fn import_profile(
    path: String,
    name: String,
    aircraft: Vec<String>,
    delete: Vec<String>,
    pages: Vec<PageTake>,
    cache: tauri::State<'_, Cache>,
) -> Result<String, String> {
    let paths = Paths::resolve();
    let bundle = read(&paths, &cache, Path::new(&path))?;
    let (profile, added, lib) = arriving(&paths.pages.library(), &bundle.profile, &bundle.pages, &pages)?;
    let problems = cache.problems(&paths, &profile, &lib);
    if !problems.is_empty() {
        return Err(format!(
            "{} was not imported, because the daemon would refuse it with these pages:\n{}",
            profile.name,
            problems.join("\n")
        ));
    }
    let module = profile.module.clone();
    let file = claims::write_new_deleting(&paths.profiles.active, settle(profile, name, aircraft)?, &delete)?;
    write_pages(&paths, &module, &added)?;
    Ok(file)
}

/// What a profile here could give another, for Merge from....
#[tauri::command]
pub fn merge_parts(file: String) -> Result<Parts, String> {
    let paths = Paths::resolve();
    let profile = Profile::load(&paths.profiles.active.join(&file)).map_err(|e| format!("reading {file}: {e}"))?;
    let (devices, displays) = maps(&paths)?;
    let mut parts = merge::parts(&profile, &devices, &displays);
    parts.slots = slot_parts(&profile, &devices, &paths.pages.library());
    Ok(parts)
}

/// Take the picked lamps and lines from `from` into the profile `into`, and
/// say what that did. With `write` false nothing is saved, which is how the
/// window asks what a merge would do before asking the user.
///
/// Refused, either way, when the result would not load: a lamp merged in that
/// matches one on a panel that was not, or a field that now shares cells with
/// one on a line left alone.
///
/// A page slot merged from a file brings its page into the library, settled
/// as an import settles it; one merged from a profile here shows a page the
/// library already has.
#[tauri::command]
pub fn merge_profile(
    from: Source,
    into: String,
    pick: Pick,
    write: bool,
    cache: tauri::State<'_, Cache>,
) -> Result<MergeReport, String> {
    let paths = Paths::resolve();
    let source = from.load(&paths, &cache)?;
    let path = paths.profiles.active.join(&into);
    let target = Profile::load(&path).map_err(|e| format!("reading {into}: {e}"))?;
    let (devices, displays) = maps(&paths)?;
    // Only the pages the picked slots show come in, under the names the plan
    // gives them, and the source's slots follow any that take a new id.
    let saved = paths.pages.library();
    let wanted: std::collections::BTreeSet<String> = pick
        .slots
        .iter()
        .filter_map(|s| source.profile.screens.get(&s.device)?.slots.get(s.slot.checked_sub(1)?)?.as_ref())
        .filter_map(|slot| slot.page.clone())
        .collect();
    let take: Vec<PageTake> = take_all(&saved, &source).into_iter().filter(|t| wanted.contains(&t.id)).collect();
    let (source, added, lib) = arriving(&saved, &source.profile, &source.pages, &take)?;
    let merged = merge::merge(&target, &source, &pick, &devices, &displays)?;
    let problems = cache.problems(&paths, &merged.profile, &lib);
    if !problems.is_empty() {
        return Err(format!(
            "{} would not load with this merged in, so nothing was changed:
{}",
            target.name,
            problems.join("
")
        ));
    }
    if write && merged.changes.iter().any(Change::changes_anything) {
        write_pages(&paths, &target.module, &added)?;
        merged.profile.save(&path).map_err(|e| format!("writing {into}: {e}"))?;
    }
    Ok(MergeReport { changes: merged.changes, notes: merged.notes })
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
