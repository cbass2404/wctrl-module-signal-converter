//! Settings that belong to the PC rather than to any profile.
//!
//! Which keyboard modifier is free depends on how DCS is bound on this
//! machine, not on the aircraft, and the window's theme is one person's
//! choice. Neither belongs in a profile, where an export would carry it to
//! someone else's PC. Both live in one small file beside the profiles.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// The file, in the folder the profiles and pages folders sit in.
pub const FILE: &str = "settings.json";

/// A keyboard key held with a panel's key to mean something else.
///
/// Left and right are one key: nobody should have to remember which Ctrl.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Modifier {
    #[default]
    Ctrl,
    Shift,
    Alt,
}

impl Modifier {
    pub const ALL: [Modifier; 3] = [Modifier::Ctrl, Modifier::Shift, Modifier::Alt];

    pub fn name(self) -> &'static str {
        match self {
            Modifier::Ctrl => "Ctrl",
            Modifier::Shift => "Shift",
            Modifier::Alt => "Alt",
        }
    }

    /// Whether a press counts, given which of the three were held with it.
    ///
    /// Only this modifier, alone. DCS binds Ctrl+1 and Ctrl+Shift+1 as two
    /// different buttons, so a second modifier means the press is meant for
    /// something else. On a keyboard with AltGr, Windows reports right Alt as
    /// Ctrl and Alt together, so it never counts as Alt, as in DCS.
    pub fn alone_in(self, held: &[Modifier]) -> bool {
        held.contains(&self) && held.iter().all(|m| *m == self)
    }
}

/// How the editor's window is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    /// Whatever Windows is set to.
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// Held with a page key to swap an MCDU's page.
    #[serde(default)]
    pub page_modifier: Modifier,
    #[serde(default)]
    pub theme: Theme,
}

impl Settings {
    /// Read the file. A missing file is every default, since nothing has
    /// been chosen yet; one that will not parse is an error, for the caller
    /// to name and then carry on with the defaults.
    pub fn load(path: &Path) -> Result<Settings> {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).map_err(|e| Error::Json(e, path.display().to_string())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// Write the file in one step, CRLF, as a profile is written, so a
    /// daemon polling for it never reads half.
    pub fn save(&self, path: &Path) -> Result<()> {
        let text = serde_json::to_string_pretty(self)
            .map_err(|e| Error::Json(e, path.display().to_string()))?
            .replace('\n', "\r\n");
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let temp = path.with_extension("json.saving");
        let written = std::fs::write(&temp, text).and_then(|()| std::fs::rename(&temp, path));
        if written.is_err() {
            let _ = std::fs::remove_file(&temp);
        }
        written?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_modifier_counts_only_alone() {
        use Modifier::*;
        assert!(Ctrl.alone_in(&[Ctrl]));
        assert!(!Ctrl.alone_in(&[]));
        assert!(!Ctrl.alone_in(&[Ctrl, Shift]), "Ctrl+Shift is another DCS button");
        assert!(!Alt.alone_in(&[Ctrl, Alt]), "AltGr reads as Ctrl and Alt, and is not Alt");
        assert!(!Shift.alone_in(&[Ctrl]));
    }

    #[test]
    fn a_missing_file_is_every_default_and_a_partial_one_fills_in() {
        let dir = std::env::temp_dir().join(format!("dsc-settings-{}", std::process::id()));
        let path = dir.join(FILE);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(Settings::load(&path).unwrap(), Settings::default());

        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, r#"{ "theme": "dark" }"#).unwrap();
        let read = Settings::load(&path).unwrap();
        assert_eq!(read.theme, Theme::Dark);
        assert_eq!(read.page_modifier, Modifier::Ctrl);

        let chosen = Settings { page_modifier: Modifier::Alt, theme: Theme::Light };
        chosen.save(&path).unwrap();
        assert_eq!(Settings::load(&path).unwrap(), chosen);

        std::fs::write(&path, "{ not json").unwrap();
        assert!(Settings::load(&path).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
