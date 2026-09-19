//! One aircraft, one profile.
//!
//! A new profile, made blank or copied, may be for an aircraft another profile
//! already claims. Leaving the claim where it was would give that aircraft two
//! profiles, and the aircraft is the thing DCS reports, so there would be no
//! telling which one flies. So the claim moves: the new profile takes the
//! aircraft and the old one gives it up.
//!
//! What a move must not do is leave a profile claiming nothing, because a
//! profile with no aircraft can never be flown and would sit in the list
//! looking fine. That is refused before anything is written.

use std::path::Path;

use dsc_config::{file_stem, Profile, Profiles};

/// A profile that gives up some of its aircraft to a new one.
pub struct Release {
    pub file: String,
    pub profile: Profile,
}

/// Every active profile that would give up one of `aircraft`, with its list
/// already trimmed. Refused, naming each one, if any would be left empty.
pub fn plan(active: &Path, aircraft: &[String]) -> Result<Vec<Release>, String> {
    let mut out = Vec::new();
    let mut emptied = Vec::new();
    let Ok(entries) = std::fs::read_dir(active) else {
        return Ok(out);
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(mut profile) = Profile::load(&path) else { continue };
        if !profile.aircraft.iter().any(|a| aircraft.contains(a)) {
            continue;
        }
        profile.aircraft.retain(|a| !aircraft.contains(a));
        if profile.aircraft.is_empty() {
            emptied.push(profile.name.clone());
            continue;
        }
        let file = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
        out.push(Release { file, profile });
    }
    if !emptied.is_empty() {
        return Err(format!(
            "{} would be left with no aircraft. Leave at least one of its aircraft unselected, or edit that profile instead.",
            emptied.join(", ")
        ));
    }
    Ok(out)
}

/// Write `profile` as a new file named after it, then move its aircraft out of
/// every profile that claimed them. Returns the new file name.
///
/// The new file is written first. If a release then fails, the aircraft is
/// claimed twice until the user fixes it, which the error says; the other
/// order would drop the aircraft from its old profile with nowhere to go.
pub fn write_new(active: &Path, mut profile: Profile) -> Result<String, String> {
    profile.name = profile.name.trim().to_string();
    if profile.name.is_empty() {
        return Err("a profile needs a name".into());
    }
    if profile.aircraft.is_empty() {
        return Err("a profile needs at least one aircraft, as DCS reports it".into());
    }
    let stem = file_stem(&profile.name);
    if stem.is_empty() {
        return Err(format!("{:?} has nothing to name a file after", profile.name));
    }
    let file = format!("{stem}.json");
    let path = active.join(&file);
    if path.exists() {
        return Err(format!("{file} already exists. Give the profile another name."));
    }

    let releases = plan(active, &profile.aircraft)?;
    std::fs::create_dir_all(active).map_err(|e| format!("creating the profile folder: {e}"))?;
    profile.save(&path).map_err(|e| format!("writing {file}: {e}"))?;
    for r in releases {
        r.profile.save(&active.join(&r.file)).map_err(|e| {
            format!(
                "{file} was written, but {} could not give up its aircraft ({e}). Remove them from it by hand.",
                r.file
            )
        })?;
    }
    Ok(file)
}

/// Delete `file`, first giving the aircraft only it flies to the profile `to`,
/// if one is named. Returns nothing; the list is reread afterwards.
///
/// The one case this is for: a profile split in two with Copy to..., where
/// deleting either half would otherwise leave its aircraft with no profile.
/// `to` must be able to fly each of them, which is a same-module profile
/// already flying an aircraft of the same family (see `Families`): the F-14BU
/// is not handed to the F-14, nor "No aircraft" to FC3. Aircraft another
/// profile already flies stay there, so nothing ends up claimed twice.
///
/// A shipped profile must hand its aircraft on. Left with nowhere to go, they
/// would have it seeded straight back as shipped, which is Reset.
///
/// The target is written before the file is removed. If the removal then
/// fails, both claim the aircraft until the user deletes it again, which the
/// error says; the other order could drop the aircraft with nowhere to go.
pub fn delete_giving(profiles: &Profiles, file: &str, to: Option<&str>) -> Result<(), String> {
    if to == Some(file) {
        return Err("a profile cannot take its own aircraft".into());
    }
    let gone = Profile::load(&profiles.active.join(file)).map_err(|e| format!("reading {file}: {e}"))?;
    let others = profiles.claimed_except(Some(file));
    let orphans: Vec<String> = gone.aircraft.iter().filter(|a| !others.contains_key(*a)).cloned().collect();

    match to {
        None if profiles.has_default(file) && !orphans.is_empty() => {
            return Err(format!(
                "{} shipped with DCS Signal Converter and would come straight back for {}. Give them to a profile for the same aircraft, or use Reset.",
                gone.name,
                orphans.join(", ")
            ));
        }
        None => {}
        Some(to) => {
            let path = profiles.active.join(to);
            let mut target = Profile::load(&path).map_err(|e| format!("reading {to}: {e}"))?;
            let families = profiles.families();
            let misfits: Vec<&str> = orphans
                .iter()
                .filter(|a| !families.fits(a, &gone.module, &target))
                .map(String::as_str)
                .collect();
            if !misfits.is_empty() {
                return Err(format!("{} cannot fly {}", target.name, misfits.join(", ")));
            }
            target.aircraft.extend(orphans);
            target.save(&path).map_err(|e| format!("writing {to}: {e}"))?;
        }
    }
    profiles.delete(file).map_err(|e| match to {
        Some(to) => format!("{to} took the aircraft, but {file} could not be deleted ({e}). Delete it again."),
        None => format!("deleting {file}: {e}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "dsc-claims-{name}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn profile(name: &str, aircraft: &[&str]) -> Profile {
        let list: Vec<String> = aircraft.iter().map(|a| format!("{a:?}")).collect();
        serde_json::from_str(&format!(
            r#"{{"name": "{name}", "aircraft": [{}], "module": "A-10C", "bindings": []}}"#,
            list.join(", ")
        ))
        .unwrap()
    }

    #[test]
    fn a_claimed_aircraft_moves_to_the_new_profile() {
        let dir = scratch("move");
        profile("A-10C II", &["A-10C_2", "A-10C"]).save(&dir.join("a-10c-2.json")).unwrap();

        let file = write_new(&dir, profile("A-10C", &["A-10C"])).unwrap();
        assert_eq!(file, "a-10c.json");
        let old = Profile::load(&dir.join("a-10c-2.json")).unwrap();
        assert_eq!(old.aircraft, vec!["A-10C_2".to_string()], "the old profile gave it up");
    }

    #[test]
    fn a_move_that_would_empty_a_profile_writes_nothing() {
        // Copy to... prefills the source's own aircraft, so taking them all is
        // the easy mistake. It must fail before any file changes.
        let dir = scratch("empty");
        profile("A-10C II", &["A-10C_2"]).save(&dir.join("a-10c-2.json")).unwrap();

        let err = write_new(&dir, profile("Copy", &["A-10C_2"])).unwrap_err();
        assert!(err.contains("A-10C II"), "{err}");
        assert!(!dir.join("copy.json").exists(), "nothing was written");
        let old = Profile::load(&dir.join("a-10c-2.json")).unwrap();
        assert_eq!(old.aircraft, vec!["A-10C_2".to_string()], "and nothing was taken");
    }

    #[test]
    fn an_unclaimed_aircraft_touches_no_other_profile() {
        let dir = scratch("free");
        profile("A-10C II", &["A-10C_2"]).save(&dir.join("a-10c-2.json")).unwrap();
        let before = std::fs::read(dir.join("a-10c-2.json")).unwrap();

        write_new(&dir, profile("A-10C", &["A-10C"])).unwrap();
        assert_eq!(std::fs::read(dir.join("a-10c-2.json")).unwrap(), before);
    }

    #[test]
    fn deleting_half_of_a_split_gives_its_aircraft_back() {
        let dir = scratch("give");
        profile("A-10C", &["A-10C"]).save(&dir.join("a-10c.json")).unwrap();
        profile("A-10C II", &["A-10C_2"]).save(&dir.join("a-10c-ii.json")).unwrap();
        let profiles = Profiles::new(dir.join("none"), &dir);

        delete_giving(&profiles, "a-10c-ii.json", Some("a-10c.json")).unwrap();
        assert!(!dir.join("a-10c-ii.json").exists());
        let kept = Profile::load(&dir.join("a-10c.json")).unwrap();
        assert_eq!(kept.aircraft, vec!["A-10C".to_string(), "A-10C_2".to_string()]);
    }

    #[test]
    fn a_shipped_profile_is_deleted_only_once_its_aircraft_have_a_home() {
        let dir = scratch("shipped");
        let (defaults, active) = (dir.join("defaults"), dir.join("active"));
        std::fs::create_dir_all(&defaults).unwrap();
        std::fs::create_dir_all(&active).unwrap();
        profile("A-10C", &["A-10C_2", "A-10C"]).save(&defaults.join("a-10c.json")).unwrap();
        profile("A-10C", &["A-10C"]).save(&active.join("a-10c.json")).unwrap();
        profile("Mine", &["A-10C_2"]).save(&active.join("mine.json")).unwrap();
        let profiles = Profiles::new(&defaults, &active);

        let err = delete_giving(&profiles, "a-10c.json", None).unwrap_err();
        assert!(err.contains("Reset"), "{err}");
        assert!(active.join("a-10c.json").exists());

        delete_giving(&profiles, "a-10c.json", Some("mine.json")).unwrap();
        assert!(!active.join("a-10c.json").exists());
        assert!(profiles.seed().unwrap().is_empty(), "and it stays deleted");
    }

    #[test]
    fn a_shipped_profile_goes_only_to_a_profile_for_the_same_aircraft() {
        // Two defaults on one module, shipped apart on purpose: the F-14BU is
        // not an F-14, and neither takes the other's aircraft.
        let dir = scratch("family");
        let (defaults, active) = (dir.join("defaults"), dir.join("active"));
        std::fs::create_dir_all(&defaults).unwrap();
        std::fs::create_dir_all(&active).unwrap();
        for (file, name, aircraft) in [("f-14.json", "F-14", "F-14B"), ("f-14bu.json", "F-14BU", "F-14BU")] {
            let mut p = profile(name, &[aircraft]);
            p.module = "F-14".into();
            p.save(&defaults.join(file)).unwrap();
            p.save(&active.join(file)).unwrap();
        }
        let profiles = Profiles::new(&defaults, &active);

        let err = delete_giving(&profiles, "f-14bu.json", Some("f-14.json")).unwrap_err();
        assert!(err.contains("F-14BU"), "{err}");
        assert!(delete_giving(&profiles, "f-14bu.json", None).is_err());
        assert!(active.join("f-14bu.json").exists());
        assert_eq!(Profile::load(&active.join("f-14.json")).unwrap().aircraft, vec!["F-14B".to_string()]);
    }

    #[test]
    fn aircraft_go_only_to_a_profile_on_the_same_module() {
        let dir = scratch("give-module");
        profile("A-10C II", &["A-10C_2"]).save(&dir.join("a-10c-ii.json")).unwrap();
        let mut viper = profile("Viper", &["F-16C_50"]);
        viper.module = "F-16C_50".into();
        viper.save(&dir.join("viper.json")).unwrap();
        let profiles = Profiles::new(dir.join("none"), &dir);

        let err = delete_giving(&profiles, "a-10c-ii.json", Some("viper.json")).unwrap_err();
        assert!(err.contains("Viper cannot fly A-10C_2"), "{err}");
        assert!(dir.join("a-10c-ii.json").exists(), "nothing was deleted");
        assert_eq!(Profile::load(&dir.join("viper.json")).unwrap().aircraft, vec!["F-16C_50".to_string()]);
    }
}
