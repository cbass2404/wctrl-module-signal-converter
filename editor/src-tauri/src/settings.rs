//! The PC's own settings, read and written for the Settings dialog.
//!
//! The daemon reads the same file and picks up a change the way it picks up a
//! saved profile, so nothing here needs to reach it.

use dsc_config::paths::Paths;
use dsc_config::settings::Settings;

use crate::{fail, Reply};

/// The settings as the window shows them.
#[derive(serde::Serialize)]
pub struct SettingsView {
    #[serde(flatten)]
    pub settings: Settings,
    /// Why the file could not be read, when it could not. The defaults are
    /// shown instead, and saving writes over the broken file.
    pub problem: Option<String>,
}

#[tauri::command]
pub fn settings_read() -> Reply<SettingsView> {
    let paths = Paths::resolve();
    Ok(match Settings::load(&paths.settings) {
        Ok(settings) => SettingsView { settings, problem: None },
        Err(e) => SettingsView { settings: Settings::default(), problem: Some(e.to_string()) },
    })
}

#[tauri::command]
pub fn settings_save(settings: Settings) -> Reply<()> {
    let paths = Paths::resolve();
    settings.save(&paths.settings).map_err(|e| fail("saving the settings", e))
}
