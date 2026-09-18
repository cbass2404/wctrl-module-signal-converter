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

use wctrl_config::{DeviceInventory, DisplayCatalogue, Module, Profile};

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

        let mut slot = match self.module.lock() {
            Ok(slot) => slot,
            // A poisoned lock means a previous check panicked mid-load. The
            // profile is still checkable; only the cached module is suspect.
            Err(poisoned) => poisoned.into_inner(),
        };
        if slot.as_ref().map(|(name, _)| name.as_str()) != Some(profile.module.as_str()) {
            let path = paths.catalogue.join(format!("{}.json", profile.module));
            match Module::load(&path) {
                Ok(m) => *slot = Some((profile.module.clone(), m)),
                Err(e) => {
                    *slot = None;
                    return vec![format!("cannot check: reading {}: {e}", path.display())];
                }
            }
        }
        let Some((_, module)) = slot.as_ref() else {
            return vec![format!("cannot check: no catalogue for {}", profile.module)];
        };

        profile
            .problems(module, &devices, &displays)
            .iter()
            .map(|e| e.to_string())
            .collect()
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
}
