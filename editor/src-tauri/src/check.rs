//! Checking a profile the way the daemon will, while it is still being edited.
//!
//! `wctrl-config` already knows every rule; nothing here restates one. The job
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

use wctrl_config::catalogue_build::catalogue_version;
use wctrl_config::nightly_only::{Change, NightlyOnly};
use wctrl_config::{DeviceInventory, DisplayCatalogue, Flag, Module, Place, Profile, Unsound};

use crate::paths::Paths;

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
    pub fn problems(&self, paths: &Paths, profile: &Profile) -> Vec<String> {
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
            profile
                .problems(module, &devices, &displays)
                .iter()
                .map(|e| e.to_string())
                .collect()
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
    pub fn flags(&self, paths: &Paths, profile: &Profile) -> (Vec<FlagView>, Option<String>) {
        let Ok(flags) = self.with_module(paths, &profile.module, |m| profile.flags(m)) else {
            return (Vec::new(), None);
        };
        // A list that cannot be read costs only the reasons, not the marks.
        let nightly = NightlyOnly::load(&paths.nightly_only).unwrap_or_default();
        let views = flags.iter().map(|f| FlagView::of(f, &profile.module, &nightly)).collect();

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
    /// Empty when the inventory cannot be read. `problems` already says so, and
    /// saying it twice would only lengthen the list.
    pub fn cautions(&self, paths: &Paths, profile: &Profile) -> Vec<String> {
        DeviceInventory::load(&paths.devices)
            .map(|devices| profile.cautions(&devices))
            .unwrap_or_default()
    }
}

/// One flagged condition or field, for the window to mark where it sits.
#[derive(Debug, serde::Serialize)]
pub struct FlagView {
    #[serde(flatten)]
    pub place: Place,
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
        FlagView { place: f.place, text: format!("{why}{cost}") }
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
        let problems = cache.problems(&paths, &orphan());
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
        assert_eq!(cache.problems(&paths, &orphan()).len(), 1);
        assert_eq!(cache.problems(&paths, &orphan()).len(), 1);
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
