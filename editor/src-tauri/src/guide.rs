//! The profile language guide, on the project's site.
//!
//! The window may not open URLs itself (see the capability file), so the
//! address is fixed here and the window can only ask for this one page.

/// Where the guide is published, beside the landing page.
const GUIDE: &str = "https://cbass2404.github.io/wctrl-module-signal-converter/language.html";

/// Opens the profile language guide in the default browser.
#[tauri::command]
pub fn open_guide() -> Result<(), String> {
    std::process::Command::new("explorer")
        .arg(GUIDE)
        .spawn()
        .map_err(|e| format!("opening {GUIDE}: {e}"))?;
    Ok(())
}
