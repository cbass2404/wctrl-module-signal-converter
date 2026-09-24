// Release builds open a window, not a console, so Windows should not attach one.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! The profile editor's backend.
//!
//! Every command here is a thin wrapper over `dsc-config`. The editor and the
//! daemon read and write profiles through exactly the same code, so a profile
//! the editor produces cannot be one the daemon rejects.

mod check;
mod claims;
mod converter;
mod learn;
mod pages;
mod settings;
mod share;
mod update;
mod view;

use dsc_config::paths::Paths;
use view::{DeviceView, ModuleChoice, ProfileSummary, SignalView};
use dsc_config::{divider_rule as rule_for, DeviceInventory, Module, Page, Profile, RuleCell};

/// Commands return a message rather than an error type, because the only useful
/// thing the window can do with a failure is show it to the user.
pub type Reply<T> = Result<T, String>;

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
    let maps = dsc_config::DisplayCatalogue::load_dir(&paths.displays)
        .map_err(|e| format!("loading {}: {e}", paths.displays.display()))?;
    let mut out: Vec<DeviceView> = inv
        .devices
        .iter()
        .map(|d| DeviceView::of(d).with_displays(d, &maps).with_variants(d, &inv.devices))
        .collect();
    out.sort_by(|a, b| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase()));
    Ok(out)
}

/// The keys of the devices plugged in now, for grouping the profile page.
///
/// Listed, never opened, so asking while the converter holds the panels takes
/// nothing from it. Asked again every few seconds while a profile is open, so
/// a panel plugged in mid-edit moves into place without a reload. A new
/// `HidApi` each time, since one caches the list it was built with.
///
/// Only devices on a protocol this build can list are ever reported; any other
/// reads as not found, which is true as far as the editor can tell.
#[tauri::command]
fn connected_devices() -> Reply<Vec<String>> {
    let paths = Paths::resolve();
    let inv = inventory(&paths)?;
    let api = hidapi::HidApi::new().map_err(|e| format!("looking for connected panels: {e}"))?;
    let pids: Vec<u16> = wctrl_hid::enumerate(&api).iter().map(|d| d.product_id).collect();
    Ok(inv
        .devices
        .iter()
        .filter(|d| d.protocol == dsc_config::DEFAULT_PROTOCOL && pids.contains(&d.usb_pid))
        .map(|d| d.key.clone())
        .collect())
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
///
/// The same goes for rows a release adds for new hardware: the daemon merges
/// them in when it starts, and so does this, because after an update the editor
/// is often opened before anything has been flown.
#[tauri::command]
fn profiles() -> Reply<Vec<ProfileSummary>> {
    let paths = Paths::resolve();
    paths
        .profiles
        .seed()
        .map_err(|e| fail("copying in the shipped profiles", e))?;
    paths
        .profiles
        .merge_new(&inventory(&paths)?, env!("CARGO_PKG_VERSION"))
        .map_err(|e| fail("adding new hardware to the profiles", e))?;
    // The pages the same way, apart from the profiles; see `Pages::merge_new`.
    paths.pages.seed().map_err(|e| fail("copying in the shipped pages", e))?;
    paths
        .pages
        .merge_new(env!("CARGO_PKG_VERSION"))
        .map_err(|e| fail("updating the pages", e))?;

    let dir = &paths.profiles.active;
    let families = paths.profiles.families();
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
        out.push(match Profile::load(&path).and_then(current) {
            Ok(p) => ProfileSummary::of(&p, file, has_default, &families),
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

/// What a divider of this many cells will draw, for the window to show.
///
/// Asked of the backend rather than worked out again in TypeScript, so there is
/// one rule for where the dashes fall and where a label sits between them, and
/// the preview cannot drift from what the panel gets. One cell at a time rather
/// than one string, because the label is drawn in its own colour and the window
/// has to know which cells are it.
#[tauri::command]
fn divider_rule(cells: usize, label: String) -> Reply<Vec<RuleCell>> {
    Ok(rule_for(cells, &label))
}

/// What each cell of a field would light, for the preview of glass that draws
/// from a glyph table rather than from a font.
///
/// Asked of the backend for the same reason a divider is. Which glyph a value
/// lands on is decided by the cell it is drawn in, not by the value alone: a
/// wide cell takes a two character value whole, a digit cell has a spaced form
/// and a bare one, and the DED is the only display that never uppercases. A
/// second copy of that in TypeScript would be a preview that agrees with the
/// user and disagrees with the glass.
#[tauri::command]
fn cell_ink(display: String, cells: Vec<view::CellDraw>) -> Reply<Vec<view::CellInk>> {
    let paths = Paths::resolve();
    let maps = dsc_config::DisplayCatalogue::load_dir(&paths.displays)
        .map_err(|e| format!("loading {}: {e}", paths.displays.display()))?;
    let map = maps.get(&display).ok_or_else(|| format!("no display named {display}"))?;
    Ok(view::CellInk::of(map, &cells))
}

#[tauri::command]
fn open_profile(file: String) -> Reply<Profile> {
    let paths = Paths::resolve();
    let path = paths.profiles.active.join(&file);
    Profile::load(&path).and_then(current).map_err(|e| fail(&format!("reading {file}"), e))
}

/// A profile of the version this editor writes, or why not.
///
/// One from before pages is refused rather than opened: the daemon will not
/// load it, and there is no migration, so nothing edited in it could run.
fn current(p: Profile) -> dsc_config::Result<Profile> {
    use dsc_config::{Error, SCHEMA_VERSION};
    match p.schema_version {
        v if v < SCHEMA_VERSION => Err(Error::MadeBeforePages(v)),
        v if v > SCHEMA_VERSION => Err(Error::NewerSchema(v)),
        _ => Ok(p),
    }
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

/// Create a profile for some of one module's aircraft, blank or copied.
///
/// The aircraft come from the catalogue rather than from typing: a module can
/// serve several runtime names, `A-10C` covering both `A-10C_2` and `A-10C`,
/// and a typed list would be a silent way to build a profile DCS never matches.
/// Which of them this profile is for is the user's choice, because sharing
/// DCS-BIOS outputs does not mean wanting the same lamps.
///
/// `from` names an existing profile to copy instead of starting blank. Any
/// chosen aircraft another profile claims moves to this one; see `claims`.
#[tauri::command]
fn create_profile(
    module: String,
    name: String,
    aircraft: Vec<String>,
    from: Option<String>,
) -> Reply<String> {
    let paths = Paths::resolve();
    let choices = ModuleChoice::read_index(&paths.catalogue.join("index.json"))?;
    let choice = choices
        .iter()
        .find(|m| m.key == module)
        .ok_or_else(|| format!("{module} is not in the catalogue"))?;
    // A module with no runtime name is offered under its key, which is then
    // the one thing it can be chosen for.
    let known: Vec<String> = if choice.aircraft.is_empty() {
        vec![module.clone()]
    } else {
        choice.aircraft.clone()
    };
    if let Some(stray) = aircraft.iter().find(|a| !known.contains(a)) {
        return Err(format!("{stray} is not an aircraft {module} covers"));
    }

    let mut profile = match &from {
        Some(file) => {
            let p = Profile::load(&paths.profiles.active.join(file))
                .map_err(|e| fail(&format!("reading {file}"), e))?;
            // Copied bindings name signals by id, so they only mean something
            // against the catalogue they were written for.
            if p.module != module {
                return Err(format!("{} reads {}, not {module}, so it cannot be copied here", p.name, p.module));
            }
            p
        }
        None => {
            let inv = inventory(&paths)?;
            let first = aircraft.first().map(String::as_str).unwrap_or(&module);
            Profile::stub(&name, first, &module, &inv)
        }
    };
    profile.name = name;
    profile.aircraft = aircraft;
    claims::write_new(&paths.profiles.active, profile)
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
/// profile full of unknown signals. Aircraft another profile claims move to
/// the copy, as they do for a new profile; see `claims`.
#[tauri::command]
fn clone_profile(file: String, name: String, aircraft: Vec<String>) -> Reply<String> {
    let paths = Paths::resolve();
    let aircraft: Vec<String> = aircraft
        .into_iter()
        .map(|a| a.trim().to_string())
        .filter(|a| !a.is_empty())
        .collect();
    let mut profile = Profile::load(&paths.profiles.active.join(&file))
        .map_err(|e| fail(&format!("reading {file}"), e))?;
    profile.name = name;
    profile.aircraft = aircraft;
    claims::write_new(&paths.profiles.active, profile)
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

/// What a check found: faults that stop the profile loading, cautions about
/// ones that load but probably do not do what was meant, and rows this
/// DCS-BIOS cannot back, which load and stay off.
#[derive(serde::Serialize)]
struct Findings {
    problems: Vec<String>,
    cautions: Vec<String>,
    /// Cautions about what one display field will draw, shown on that field.
    field_cautions: Vec<check::FieldCaution>,
    flags: Vec<check::FlagView>,
    /// One line for the top of the page, only when a flagged row reads
    /// something the DCS-BIOS nightly has.
    notice: Option<String>,
    /// Why the page being edited could not be saved. Kept apart from
    /// `problems`, since the profile saves whatever state the page is in.
    page_problems: Vec<String>,
}

/// Every reason the daemon would refuse this profile, for the window to show,
/// and every caution it would log.
///
/// Called after each edit rather than on save. A fault found where it was made
/// costs one click to undo; the same fault found by the daemon costs a flight,
/// because it skips the whole profile and every lamp in it stays dark.
///
/// The profile is checked against the pages as saved, which is what it will
/// show. `working` is the page open for editing on `device`, if one is: its
/// fields are flagged and cautioned where they sit, and why it could not be
/// saved is said apart. A slot's notes, such as a page gone from the
/// library, are listed with the cautions.
#[tauri::command]
fn check_profile(
    profile: Profile,
    working: Option<Page>,
    device: Option<String>,
    cache: tauri::State<check::Cache>,
) -> Reply<Findings> {
    let paths = Paths::resolve();
    let saved = paths.pages.library();
    let (shown, page_problems) = match (&working, &device) {
        (Some(page), Some(device)) => (
            pages::library_with(&paths, &profile.module, page),
            cache.page_problems(&paths, &saved, &profile, page, device),
        ),
        _ => (saved.clone(), Vec::new()),
    };
    let (flags, notice) = cache.flags(&paths, &profile, &shown);
    let mut cautions = cache.cautions(&paths, &profile);
    cautions.extend(profile.slot_notes(&saved).into_iter().map(|n| n.text));
    Ok(Findings {
        problems: cache.problems(&paths, &profile, &saved),
        cautions,
        field_cautions: cache.field_cautions(&paths, &profile, &shown),
        flags,
        notice,
        page_problems,
    })
}

/// Write a profile, refusing one the daemon would not load.
///
/// The window checks as the user types and will not offer Save while anything
/// is outstanding, so this is the guarantee rather than the message: the file
/// on disk is one that runs. Refusing also protects what is already there,
/// since the previous save is very likely a profile that flies.
///
/// Checked against the pages as saved. A page is saved on its own, so one
/// still being edited is not what this profile will show.
#[tauri::command]
fn save_profile(file: String, profile: Profile, cache: tauri::State<check::Cache>) -> Reply<()> {
    let paths = Paths::resolve();
    // The name is only what the list shows, never the file, so renaming
    // changes nothing else. It still has to be something to read.
    if profile.name.trim().is_empty() {
        return Err(format!("{file} was not written, because a profile needs a name"));
    }
    // Renaming is the one way a duplicate name could be made: every path that
    // writes a new file goes through `claims::write_new`, which refuses one.
    if let Some(taken) = paths.profiles.name_taken(&file, &profile.name) {
        return Err(format!(
            "{file} was not written, because another profile is already called {taken}."
        ));
    }
    let problems = cache.problems(&paths, &profile, &paths.pages.library());
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

/// Delete a profile, giving its aircraft to the profile `give_to` first if one
/// is named. A shipped profile whose aircraft would go nowhere is refused,
/// since seeding would bring it straight back; Reset is that.
#[tauri::command]
fn delete_profile(file: String, give_to: Option<String>) -> Reply<()> {
    let paths = Paths::resolve();
    claims::delete_giving(&paths.profiles, &file, give_to.as_deref())
}

/// What the startup check on the catalogue found, for the profiles page.
///
/// Shown in the window because nobody installing this will ever see a
/// console. Without it a catalogue that did not match, or a DCS-BIOS that
/// was not found, would only show as lamps that do nothing.
#[derive(Clone, serde::Serialize)]
struct CatalogueStatus {
    /// `"ok"`, `"caution"` or `"error"`: how loudly the window says it.
    level: &'static str,
    text: String,
}

/// Bring the catalogue up to date with the installed DCS-BIOS before the
/// window reads any of it.
///
/// The daemon does the same on its own start. Whichever runs first rebuilds,
/// and the other finds the catalogue already matching, so the two never build
/// twice or read each other's half-written files.
fn refresh_catalogue(paths: &Paths) -> CatalogueStatus {
    use dsc_config::catalogue_build::{self, Freshness};
    let bios_json = catalogue_build::locate_bios_json(&paths.catalogue, None);
    let fresh = catalogue_build::ensure(&bios_json, &paths.catalogue);
    match &fresh {
        Ok(f) => eprintln!("{f}"),
        Err(e) => eprintln!("could not update the catalogue: {e}"),
    }
    let (level, text) = match fresh {
        Ok(Freshness::Current { version }) => ("ok", format!("Signals from DCS-BIOS {version}, up to date.")),
        Ok(Freshness::Built { was: None, now, .. }) => ("ok", format!("Signals built from DCS-BIOS {now}.")),
        Ok(Freshness::Built { was: Some(was), now, .. }) if was == now => (
            "ok",
            format!("Signals rebuilt from DCS-BIOS {now}, whose files changed since the last build."),
        ),
        Ok(Freshness::Built { was: Some(was), now, .. }) => {
            ("ok", format!("Signals rebuilt for DCS-BIOS {now}, replacing {was}."))
        }
        Ok(Freshness::NoBios { bios_json, have_catalogue: true }) => (
            "caution",
            format!(
                "DCS-BIOS was not found at {}. Using the signals from the last time it was, which may not match what DCS runs.",
                bios_json.display()
            ),
        ),
        Ok(Freshness::NoBios { bios_json, have_catalogue: false }) => (
            "error",
            format!(
                "DCS-BIOS was not found at {}, so there are no signals to assign. Install DCS-BIOS, then restart the editor.",
                bios_json.display()
            ),
        ),
        Err(e) => ("error", format!("The signals could not be updated from DCS-BIOS: {e}")),
    };
    CatalogueStatus { level, text }
}

/// What `refresh_catalogue` found when the editor started.
#[tauri::command]
fn catalogue_status(status: tauri::State<CatalogueStatus>) -> Reply<CatalogueStatus> {
    Ok(status.inner().clone())
}

/// One font's glyphs, so the window can draw a line the way the panel will.
///
/// Checking the typed characters against the alphabet is not enough. These
/// fonts reuse slots: in the A-10C font `%` draws a question mark, and a
/// window that showed the typed string would agree with the user and disagree
/// with the panel. So the real bitmaps go up and the preview is drawn from
/// them.
///
/// Asked for per font rather than sent with the displays, because four fonts
/// of glyph bitmaps is a great deal of data to hand over for a screen nobody
/// may open.
#[tauri::command]
fn font_glyphs(display: String, font: String) -> Reply<view::FontGlyphs> {
    let paths = Paths::resolve();
    let maps = dsc_config::DisplayCatalogue::load_dir(&paths.displays)
        .map_err(|e| format!("loading {}: {e}", paths.displays.display()))?;
    let map = maps.get(&display).ok_or_else(|| format!("no display named {display}"))?;
    let text = map
        .text
        .as_ref()
        .ok_or_else(|| format!("{display} draws from a glyph table, not from a font"))?;
    view::FontGlyphs::load(text, &font)
}

fn main() {
    let status = refresh_catalogue(&Paths::resolve());
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(status)
        .manage(learn::State::default())
        .manage(check::Cache::default())
        .manage(update::Found::default())
        .invoke_handler(tauri::generate_handler![
            catalogue_status,
            devices,
            connected_devices,
            modules,
            profiles,
            signals,
            divider_rule,
            cell_ink,
            font_glyphs,
            converter::converter_state,
            converter::converter_restart,
            converter::converter_kill,
            settings::settings_read,
            settings::settings_save,
            open_profile,
            default_profile,
            create_profile,
            clone_profile,
            check_profile,
            save_profile,
            pages::open_pages,
            pages::new_page_id,
            pages::save_page,
            pages::delete_page,
            reset_profile,
            delete_profile,
            share::export_profile,
            share::export_pages,
            share::import_pick,
            share::import_profile,
            share::merge_parts,
            share::merge_profile,
            learn_start,
            learn_poll,
            learn_again,
            learn_stop,
            update::update_check,
            update::open_update
        ])
        .run(tauri::generate_context!())
        .expect("starting the editor window");
}
