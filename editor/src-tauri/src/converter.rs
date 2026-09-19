//! Managing the converter daemon from the editor.
//!
//! The daemon is normally started by the DCS hook and needs nothing from here:
//! a saved profile reaches a running one within about a second, so editing is
//! not a reason to touch any of this. What it is for is the cases the hook
//! cannot cover, chiefly a daemon that died while DCS stayed up, since the hook
//! starts one once per DCS session and never learns that it has gone.

use std::path::{Path, PathBuf};

use dsc_config::daemon::{self, STOP_TIMEOUT};

use crate::Reply;

/// What the window shows beside Manage Converter.
#[derive(serde::Serialize)]
pub struct ConverterState {
    pub running: bool,
    /// The process holding the panels, when one does. Shown so a wedged daemon
    /// can be recognised in Task Manager if it comes to that.
    pub pid: Option<u32>,
    /// Whether there is anything here to start. False in a checkout with no
    /// built daemon beside the editor, where Restart can stop but not start.
    pub can_start: bool,
}

/// Where the daemon lives: beside this executable, as the installer puts it.
///
/// `tauri dev` runs the editor out of the build tree, so the daemon it finds is
/// whichever one was last built there. That is the right answer for both: the
/// installed editor manages the installed daemon, and a development one manages
/// the development build.
fn install_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(Path::to_path_buf)
}

fn daemon_exe() -> Option<PathBuf> {
    let path = install_dir()?.join("dcs-signal.exe");
    path.is_file().then_some(path)
}

/// The shim that starts the daemon without a console window.
///
/// The same one the DCS hook uses, so a daemon started here is started exactly
/// the way a mission starts one: `run --exit-when-idle 20`, hidden, with the
/// install folder as its working directory. Running the exe directly would
/// flash a console and, worse, mean two ways of starting one thing.
fn launcher() -> Option<PathBuf> {
    let path = install_dir()?.join("run-hidden.vbs");
    path.is_file().then_some(path)
}

/// Seconds of silence after which a daemon started here clears the panels and
/// exits. Matches `IDLE_CLEAR` in the hook: one number, one behaviour.
const IDLE_CLEAR: &str = "20";

#[tauri::command]
pub fn converter_state() -> Reply<ConverterState> {
    Ok(ConverterState {
        running: daemon::is_running(),
        pid: daemon::owner_pid(),
        can_start: daemon_exe().is_some() && launcher().is_some(),
    })
}

/// Start a daemon, hidden, the way the hook does.
#[cfg(windows)]
fn start() -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let vbs = launcher().ok_or_else(|| {
        "run-hidden.vbs is not beside the editor, so there is nothing here to start.".to_string()
    })?;
    std::process::Command::new("wscript.exe")
        .arg(vbs)
        .arg(IDLE_CLEAR)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("starting the converter: {e}"))?;
    Ok(())
}

#[cfg(not(windows))]
fn start() -> Result<(), String> {
    Err("only implemented on Windows".into())
}

/// Stop the converter if one is running, then start a fresh one.
///
/// A stop that times out is reported and nothing is started. The lock is still
/// held by the old one, so a new daemon would refuse to run and the user would
/// be left with a button that appeared to do nothing; saying so and pointing at
/// Kill is the honest answer.
#[tauri::command]
pub fn converter_restart() -> Reply<String> {
    let stopped = daemon::request_stop(STOP_TIMEOUT).map_err(|e| {
        format!(
            "The converter did not stop when asked: {e}. Nothing was started, and nothing was \
             killed, so the panels are as it left them. Use Kill if it will not answer."
        )
    })?;
    start()?;
    Ok(if stopped {
        "Stopped the converter and started a fresh one.".into()
    } else {
        "No converter was running. Started one.".into()
    })
}

/// End the converter without asking it, for one that will not answer.
///
/// Nothing is started afterwards. The panels keep whatever was last written to
/// them, because a terminated process runs none of its shutdown, and the window
/// says so before this is reached.
#[tauri::command]
pub fn converter_kill() -> Reply<String> {
    match daemon::kill() {
        Ok(true) => Ok("Ended the converter. The panels keep whatever was last sent to them; \
                        start it again and stop it properly to clear them."
            .into()),
        Ok(false) => Ok("No converter is running.".into()),
        Err(e) => Err(format!("Could not end the converter: {e}")),
    }
}
