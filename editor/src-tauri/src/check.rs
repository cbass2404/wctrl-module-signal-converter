//! Checking a profile the way the daemon will, while it is still being edited.
//!
//! `dsc-config` already knows every rule; nothing here restates one. The job
//! is to run those rules often enough that a fault is found where it was made,
//! instead of on the ramp, where the daemon's answer to a bad profile is to
//! skip the whole file and leave every lamp in it dark.
//!
//! The window checks after each edit, so this runs on a keystroke. A module is
//! up to 360 KB of JSON and parsing that per keystroke would be felt, so the
//! last one stays loaded. It is keyed by name and replaced when the name
//! changes, because the editor has exactly one profile open at a time and a
//! second entry would only ever be the one just closed.

use std::sync::Mutex;

use dsc_config::catalogue_build::catalogue_version;
use dsc_config::nightly_only::{Change, NightlyOnly};
use dsc_config::{DeviceInventory, DisplayCatalogue, Error, Flag, Module, Page, PageLibrary, Place, Profile, Unsound};

use dsc_config::paths::Paths;

/// The catalogue pieces a check needs, loaded once and reused.
#[derive(Default)]
pub struct Cache {
    module: Mutex<Option<(String, Module)>>,
}

impl Cache {
    /// Every reason the daemon would refuse this profile, in the words it would
    /// use. An empty list means it will load.
    ///
    /// Failing to read the catalogue is reported as a problem rather than
    /// swallowed. A check that silently passes because it could not run is
    /// worse than no check: it is the reassurance without the substance.
    ///
    /// `pages` is the library as the window has it, unsaved edits included,
    /// since the pages are written with the profile. Every page on the module
    /// is checked, slotted here or not, because every one of them is written.
    pub fn problems(&self, paths: &Paths, profile: &Profile, pages: &PageLibrary) -> Vec<String> {
        let devices = match DeviceInventory::load(&paths.devices) {
            Ok(d) => d,
            Err(e) => return vec![format!("cannot check: reading {}: {e}", paths.devices.display())],
        };
        let displays = match DisplayCatalogue::load_dir(&paths.displays) {
            Ok(d) => d,
            Err(e) => {
                return vec![format!("cannot check: loading {}: {e}", paths.displays.display())]
            }
        };

        self.with_module(paths, &profile.module, |module| {
            let mut out: Vec<String> = profile
                .problems(module, &devices, &displays, pages)
                .iter()
                .map(|e| e.to_string())
                .collect();
            let shown = profile.pages_used();
            for page in pages.on_module(&profile.module) {
                for e in pages.page_problems(page, module, &devices, &displays) {
                    // A page in a slot here has its fields checked as they
                    // draw here, by `problems`, so only what is about the page
                    // itself is added.
                    let about_page = matches!(
                        e,
                        Error::PageUnnamed | Error::PageNameTaken(..) | Error::PageOnUnknownDisplay(..)
                    );
                    if shown.contains(&page.id) && !about_page {
                        continue;
                    }
                    let line = format!("the page {:?}: {e}", page.name.trim());
                    if !out.contains(&line) {
                        out.push(line);
                    }
                }
            }
            out
        })
        .unwrap_or_else(|e| vec![format!("cannot check: {e}")])
    }

    /// Why a page could not be saved, shown on `device` of `profile`: what is
    /// wrong with the page wherever it goes, and what is wrong with it drawn
    /// in this profile's font. `lib` is the library as saved, which is what
    /// its name must not clash with.
    pub fn page_problems(
        &self,
        paths: &Paths,
        lib: &PageLibrary,
        profile: &Profile,
        page: &Page,
        device: &str,
    ) -> Vec<String> {
        let (devices, displays) = match (DeviceInventory::load(&paths.devices), DisplayCatalogue::load_dir(&paths.displays)) {
            (Ok(d), Ok(m)) => (d, m),
            (Err(e), _) => return vec![format!("cannot check: reading {}: {e}", paths.devices.display())],
            (_, Err(e)) => return vec![format!("cannot check: loading {}: {e}", paths.displays.display())],
        };
        self.with_module(paths, &profile.module, |m| {
            let mut out: Vec<String> =
                lib.page_problems(page, m, &devices, &displays).iter().map(|e| e.to_string()).collect();
            let mut here = Vec::new();
            profile.page_view(device, page).page_field_problems(m, &devices, &displays, &mut here);
            for e in here.into_iter().filter(|e| !e.is_advisory()) {
                let line = e.to_string();
                if !out.contains(&line) {
                    out.push(line);
                }
            }
            out
        })
        .unwrap_or_else(|e| vec![format!("cannot check: {e}")])
    }

    /// Run `f` over the named module, loading it only if the last check was
    /// for another one.
    fn with_module<R>(&self, paths: &Paths, name: &str, f: impl FnOnce(&Module) -> R) -> Result<R, String> {
        let mut slot = match self.module.lock() {
            Ok(slot) => slot,
            // A poisoned lock means a previous check panicked mid-load. The
            // profile is still checkable; only the cached module is suspect.
            Err(poisoned) => poisoned.into_inner(),
        };
        if slot.as_ref().map(|(n, _)| n.as_str()) != Some(name) {
            let path = paths.catalogue.join(format!("{name}.json"));
            match Module::load(&path) {
                Ok(m) => *slot = Some((name.to_string(), m)),
                Err(e) => {
                    *slot = None;
                    return Err(format!("reading {}: {e}", path.display()));
                }
            }
        }
        match slot.as_ref() {
            Some((_, module)) => Ok(f(module)),
            None => Err(format!("no catalogue for {name}")),
        }
    }

    /// Every condition and field this DCS-BIOS cannot back, each placed so
    /// the window can mark it, and one line for the top of the page when any
    /// of them is something the DCS-BIOS nightly has.
    ///
    /// None of this withholds Save. The daemon runs the rest of the profile
    /// and turns these rows off, and the file keeps them so they work again
    /// once DCS-BIOS is updated.
    ///
    /// Empty when the module cannot be read. `problems` already says so.
    pub fn flags(&self, paths: &Paths, profile: &Profile, pages: &PageLibrary) -> (Vec<FlagView>, Option<String>) {
        let devices = DeviceInventory::load(&paths.devices).ok();
        let Ok(flags) = self.with_module(paths, &profile.module, |m| {
            let mut all: Vec<(Option<String>, Flag)> = profile.flags(m).into_iter().map(|f| (None, f)).collect();
            if let Some(devices) = &devices {
                for (page, view) in page_views(profile, pages, devices) {
                    all.extend(view.flags(m).into_iter().map(|f| (Some(page.id.clone()), f)));
                }
            }
            all
        }) else {
            return (Vec::new(), None);
        };
        // A list that cannot be read costs only the reasons, not the marks.
        let nightly = NightlyOnly::load(&paths.nightly_only).unwrap_or_default();
        let views = flags
            .iter()
            .map(|(page, f)| FlagView { page: page.clone(), ..FlagView::of(f, &profile.module, &nightly) })
            .collect();
        let flags: Vec<Flag> = flags.into_iter().map(|(_, f)| f).collect();

        let mut listed: Vec<&str> = flags
            .iter()
            .filter(|f| nightly.get(&profile.module, &f.source).is_some())
            .map(|f| f.source.as_str())
            .collect();
        listed.sort_unstable();
        listed.dedup();
        let notice = (!listed.is_empty()).then(|| {
            let installed = catalogue_version(&paths.catalogue).unwrap_or_else(|| "here".to_string());
            let n = listed.len();
            format!(
                "DCS-BIOS {installed} lacks {n} signal{} this profile reads that the DCS-BIOS nightly has, so the rows marked below stay off. Everything else runs.",
                if n == 1 { "" } else { "s" }
            )
        });
        (views, notice)
    }

    /// What will load but probably not do what was meant, such as a gate that
    /// follows the console to 0 and hides its lamps by day. Shown without
    /// withholding Save, because the profile runs and may be exactly what the
    /// user wants.
    ///
    /// Only what is about the profile as a whole. What a display field will
    /// draw is said on the field, by `field_cautions`.
    ///
    /// Empty when the inventory cannot be read. `problems` already says so, and
    /// saying it twice would only lengthen the list.
    pub fn cautions(&self, paths: &Paths, profile: &Profile) -> Vec<String> {
        DeviceInventory::load(&paths.devices)
            .map(|devices| profile.cautions(&devices))
            .unwrap_or_default()
    }

    /// Cautions about what each display field will draw: content that may not
    /// fit its cells, and settings that lean on what DCS-BIOS says a signal
    /// is. Shown on the field they are about.
    ///
    /// Empty when the inventory, the displays or the module cannot be read.
    /// `problems` already says so.
    ///
    /// A page's fields are cautioned as they draw in this profile, since the
    /// font is the profile's, and carry the page's id.
    pub fn field_cautions(&self, paths: &Paths, profile: &Profile, pages: &PageLibrary) -> Vec<FieldCaution> {
        let (Ok(devices), Ok(displays)) = (
            DeviceInventory::load(&paths.devices),
            DisplayCatalogue::load_dir(&paths.displays),
        ) else {
            return Vec::new();
        };
        self.with_module(paths, &profile.module, |m| {
            let mut out: Vec<FieldCaution> = profile
                .field_cautions(m, &devices, &displays)
                .into_iter()
                .map(|(readout, text)| FieldCaution { page: None, readout, text })
                .collect();
            for (page, view) in page_views(profile, pages, &devices) {
                out.extend(
                    view.field_cautions(m, &devices, &displays)
                        .into_iter()
                        .map(|(readout, text)| FieldCaution { page: Some(page.id.clone()), readout, text }),
                );
            }
            out
        })
        .unwrap_or_default()
    }

    /// Every caution as one list, for a profile that is not open: an import
    /// has no fields on screen to put them beside, so each says where it is.
    pub fn all_cautions(&self, paths: &Paths, profile: &Profile, pages: &PageLibrary) -> Vec<String> {
        let mut out = self.cautions(paths, profile);
        out.extend(self.field_cautions(paths, profile, pages).into_iter().filter_map(|c| {
            let (r, on) = match &c.page {
                None => (profile.readouts.get(c.readout)?, String::new()),
                Some(id) => {
                    let page = pages.page_on(&profile.module, id)?;
                    (page.fields.get(c.readout)?, format!("page {:?}, ", page.name.trim()))
                }
            };
            Some(format!("{on}{} cells {}: {}", r.display, r.cells, c.text))
        }));
        out
    }
}

/// Each page on the profile's module, shown on the device that shows it here,
/// or on the first that could when no slot here does.
fn page_views<'a>(
    profile: &'a Profile,
    pages: &'a PageLibrary,
    devices: &'a DeviceInventory,
) -> impl Iterator<Item = (&'a Page, Profile)> + 'a {
    pages.on_module(&profile.module).iter().filter_map(move |page| {
        let slotted = profile
            .screens
            .iter()
            .find(|(_, s)| s.pages().any(|(_, id)| id == page.id))
            .map(|(d, _)| d.clone());
        let device = slotted.or_else(|| {
            devices
                .devices
                .iter()
                .find(|d| d.part_with_display(&page.display).is_some())
                .map(|d| d.key.clone())
        })?;
        Some((page, profile.page_view(&device, page)))
    })
}

/// One caution about a display field, for the window to show on it.
#[derive(Debug, serde::Serialize)]
pub struct FieldCaution {
    /// The page the field is on, or none for one of the profile's own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    /// The field's index in `readouts`, or in the page's `fields`.
    pub readout: usize,
    pub text: String,
}

/// One flagged condition or field, for the window to mark where it sits.
#[derive(Debug, serde::Serialize)]
pub struct FlagView {
    #[serde(flatten)]
    pub place: Place,
    /// The page a flagged field is on, when it is on one; its index is then
    /// into the page's fields.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    /// Why, where anything more than the name is known, then what it costs.
    /// Written to follow what the row already says, which for a missing
    /// signal is that the module does not have it.
    pub text: String,
}

impl FlagView {
    fn of(f: &Flag, module: &str, nightly: &NightlyOnly) -> Self {
        let why = match (&f.why, nightly.get(module, &f.source)) {
            (Unsound::Missing, Some(_)) => {
                format!("Needs the DCS-BIOS nightly; stable {} does not have it. ", nightly.stable)
            }
            (Unsound::Missing, None) => String::new(),
            (Unsound::AboveRange { value, max }, Some(Change::Range { nightly: Some(n), .. })) => {
                format!("Tests for {value}, above its highest here, {max}. The DCS-BIOS nightly goes to {n}. ")
            }
            (Unsound::AboveRange { value, max }, _) => format!("Tests for {value}, above its highest, {max}. "),
        };
        let cost = match f.place {
            Place::Condition { .. } => "The lamp stays off.",
            Place::Branch { .. } => "This alternative is left out; the others still work.",
            Place::Field { .. } => "The field stays blank.",
        };
        FlagView { place: f.place, page: None, text: format!("{why}{cost}") }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A profile for a module the catalogue does not have.
    fn orphan() -> Profile {
        serde_json::from_str(
            r#"{"name": "T", "aircraft": ["NOPE"], "module": "NOT_A_MODULE", "bindings": []}"#,
        )
        .expect("the fixture parses")
    }

    #[test]
    fn a_check_that_cannot_run_says_so_rather_than_passing() {
        // The failure mode this guards against is the quiet one: a missing
        // catalogue producing an empty problem list, which the window would
        // show as a profile with nothing wrong and a Save button ready.
        let paths = Paths::resolve();
        let cache = Cache::default();
        let problems = cache.problems(&paths, &orphan(), &PageLibrary::default());
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].starts_with("cannot check"), "{:?}", problems[0]);
    }

    #[test]
    fn a_failed_load_is_not_cached_as_an_answer() {
        // Asking twice must ask twice. Holding a failure would mean a user who
        // rebuilds the catalogue while the window is open never gets told it
        // worked, and the check that comes back clean is the one that matters.
        let paths = Paths::resolve();
        let cache = Cache::default();
        assert_eq!(cache.problems(&paths, &orphan(), &PageLibrary::default()).len(), 1);
        assert_eq!(cache.problems(&paths, &orphan(), &PageLibrary::default()).len(), 1);
    }

    fn nightly() -> NightlyOnly {
        serde_json::from_str(
            r#"{"stable": "0.11.7", "nightly": "2026.09.18-nightly", "signals": {"TEST": {
                "NEW": {"change": "missing"},
                "SEL": {"change": "range", "stable": 2, "nightly": 3}
            }}}"#,
        )
        .expect("the fixture list parses")
    }

    fn flag(place: Place, source: &str, why: Unsound) -> Flag {
        Flag { device: "D".into(), target: "L".into(), place, source: source.into(), why }
    }

    #[test]
    fn a_flag_says_what_it_costs_and_names_the_nightly_where_the_list_does() {
        let lamp = Place::Condition { binding: 0, index: 0 };
        let listed = FlagView::of(&flag(lamp, "NEW", Unsound::Missing), "TEST", &nightly());
        assert_eq!(listed.text, "Needs the DCS-BIOS nightly; stable 0.11.7 does not have it. The lamp stays off.");

        // Not on the list: the row already says the module lacks it.
        let typo = FlagView::of(&flag(Place::Field { readout: 2 }, "TYPO", Unsound::Missing), "TEST", &nightly());
        assert_eq!(typo.text, "The field stays blank.");

        let branch = Place::Branch { binding: 0, branch: 1, index: 0 };
        let range = FlagView::of(&flag(branch, "SEL", Unsound::AboveRange { value: 3, max: 2 }), "TEST", &nightly());
        assert_eq!(
            range.text,
            "Tests for 3, above its highest here, 2. The DCS-BIOS nightly goes to 3. This alternative is left out; the others still work."
        );
    }

    #[test]
    fn a_flag_serialises_with_its_place_beside_the_text() {
        // The window finds the row from these fields, so their names matter.
        let view = FlagView::of(
            &flag(Place::Branch { binding: 4, branch: 1, index: 0 }, "NEW", Unsound::Missing),
            "TEST",
            &nightly(),
        );
        let json = serde_json::to_value(&view).expect("serialises");
        assert_eq!(json["at"], "branch");
        assert_eq!(json["binding"], 4);
        assert_eq!(json["branch"], 1);
        assert_eq!(json["index"], 0);
    }
}
