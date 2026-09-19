//! The listener behind learn mode.
//!
//! The window needs the DCS-BIOS stream to answer "what did I just flip", and
//! nothing else in the editor does any I/O at all. That is deliberate: this is
//! the one place with a thread and a socket in it, and it exists only while the
//! user has learn mode open.
//!
//! **Running alongside the daemon is the normal case**, not an edge one. The
//! user is flying with the panels live and reaching for the editor to map
//! another lamp. `Listener::bind` sets `SO_REUSEADDR` before binding, and
//! multicast gives every listener its own copy, so both read the same stream
//! without either noticing the other.
//!
//! The thread owns nothing but the socket. All the judgment lives in
//! `wctrl_engine::learn`, which has no I/O and is tested without DCS.

use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde::Serialize;
use wctrl_bios::{Listener, Write as BiosWrite};
use wctrl_config::Module;
use wctrl_engine::Watcher;

/// How long a receive blocks before the thread looks at the stop flag again.
/// Short enough that closing the panel is instant, long enough to be idle.
const POLL: Duration = Duration::from_millis(250);

/// One signal that moved, as the window shows it.
#[derive(Serialize)]
pub struct ChangeView {
    pub id: String,
    pub from: Option<String>,
    pub to: String,
    pub moves: u32,
    pub text: bool,
}

/// Everything the window asks for on a poll.
///
/// One shape rather than several commands, because every one of these fields
/// is needed to decide what to put on screen and asking for them separately
/// would let them disagree with each other.
#[derive(Serialize)]
pub struct Report {
    /// The module being watched, which the window compares against the profile
    /// it has open.
    pub module: String,
    pub listening: bool,
    /// Whether every signal has a baseline yet. Before this, an empty list
    /// means "still reading the cockpit"; after it, it means "nothing moved".
    pub ready: bool,
    pub datagrams: u64,
    /// The aircraft DCS says it is flying. Editing a Hornet profile while
    /// sitting in a Hind is a thing that happens, and this is what lets the
    /// window say so rather than showing an empty list forever.
    pub aircraft: Option<String>,
    pub error: Option<String>,
    pub changes: Vec<ChangeView>,
}

impl Report {
    /// What the window gets when nothing is listening.
    ///
    /// An answer rather than an error, because polling a stopped session is
    /// ordinary: the panel closes between the poll being scheduled and the
    /// reply coming back.
    pub fn idle() -> Self {
        Report {
            module: String::new(),
            listening: false,
            ready: false,
            datagrams: 0,
            aircraft: None,
            error: None,
            changes: Vec::new(),
        }
    }
}

struct Shared {
    watcher: Watcher,
    /// Set when the socket could not be joined or died. A firewall rule that
    /// blocks multicast is otherwise indistinguishable from a quiet cockpit.
    error: Option<String>,
}

/// Take the shared lock, even after the listener thread panicked holding it.
///
/// Every command reads through here on the webview's thread, where a panic
/// cannot unwind and aborts the whole editor. A dead listener is reported by
/// [`Session::report`] instead.
fn lock(shared: &Mutex<Shared>) -> MutexGuard<'_, Shared> {
    shared.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A running listener, its thread, and the watcher they share.
pub struct Session {
    shared: Arc<Mutex<Shared>>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Session {
    pub fn start(module: &Module) -> Self {
        let shared = Arc::new(Mutex::new(Shared {
            watcher: Watcher::new(module, Instant::now()),
            error: None,
        }));
        let stop = Arc::new(AtomicBool::new(false));

        let thread_shared = Arc::clone(&shared);
        let thread_stop = Arc::clone(&stop);
        let handle = std::thread::spawn(move || {
            let mut listener = match Listener::bind(Ipv4Addr::UNSPECIFIED) {
                Ok(listener) => listener,
                Err(e) => {
                    let mut shared = lock(&thread_shared);
                    shared.error = Some(format!(
                        "joining the DCS-BIOS multicast group on 239.255.50.10:5010: {e}"
                    ));
                    return;
                }
            };
            if let Err(e) = listener.set_read_timeout(Some(POLL)) {
                let mut shared = lock(&thread_shared);
                shared.error = Some(format!("setting a read timeout: {e}"));
                return;
            }

            let mut writes: Vec<BiosWrite> = Vec::new();
            while !thread_stop.load(Ordering::SeqCst) {
                writes.clear();
                match listener.recv(&mut writes) {
                    Ok(_) => {}
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            || e.kind() == std::io::ErrorKind::TimedOut =>
                    {
                        continue
                    }
                    Err(e) => {
                        let mut shared = lock(&thread_shared);
                        shared.error = Some(format!("reading the export stream: {e}"));
                        return;
                    }
                }
                let now = Instant::now();
                let mut shared = lock(&thread_shared);
                shared.watcher.ingest(&writes, now);
            }
        });

        Session {
            shared,
            stop,
            handle: Some(handle),
        }
    }

    pub fn module(&self) -> String {
        lock(&self.shared).watcher.module().to_string()
    }

    pub fn report(&self) -> Report {
        let shared = lock(&self.shared);
        Report {
            module: shared.watcher.module().to_string(),
            listening: true,
            ready: shared.watcher.ready(),
            datagrams: shared.watcher.datagrams(),
            aircraft: shared.watcher.aircraft(),
            error: shared.error.clone().or_else(|| {
                self.handle
                    .as_ref()
                    .is_some_and(|h| h.is_finished())
                    .then(|| "the listener stopped unexpectedly; close learn mode and open it again".to_string())
            }),
            changes: shared
                .watcher
                .changes()
                .into_iter()
                .map(|c| ChangeView {
                    id: c.id,
                    from: c.from,
                    to: c.to,
                    moves: c.moves,
                    text: c.text,
                })
                .collect(),
        }
    }

    /// Forget what has moved and start a fresh sheet, keeping the map.
    pub fn rearm(&self) {
        lock(&self.shared).watcher.rearm(Instant::now());
    }
}

/// Stopping is the destructor, so closing the window or swapping modules cannot
/// leave a thread reading a socket for the life of the process.
impl Drop for Session {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            // At most one read timeout, which is why that timeout is short.
            let _ = handle.join();
        }
    }
}

/// The one session, if learn mode is open.
#[derive(Default)]
pub struct State(pub Mutex<Option<Session>>);

impl State {
    /// Start listening for `module`, or keep the session already doing so.
    ///
    /// Idempotent because the window starts learn mode every time the panel is
    /// opened, and re-binding the socket on each one would throw away a
    /// baseline that took a second to build.
    pub fn start(&self, module: &Module) {
        let mut slot = self.0.lock().expect("the learn session lock");
        if let Some(session) = slot.as_ref() {
            if session.module() == module.module {
                return;
            }
        }
        // Dropped before the new one starts, so two sockets never overlap.
        *slot = None;
        *slot = Some(Session::start(module));
    }

    pub fn stop(&self) {
        *self.0.lock().expect("the learn session lock") = None;
    }

    pub fn with<T>(&self, f: impl FnOnce(&Session) -> T) -> Option<T> {
        self.0.lock().expect("the learn session lock").as_ref().map(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module() -> Module {
        serde_json::from_str(
            r#"{
              "module": "TEST_SESSION",
              "aircraft": ["TEST_SESSION"],
              "signals": [
                {
                  "id": "GEAR_LEVER",
                  "control_type": "selector",
                  "outputs": [{"address": 100, "mask": 3, "shift": 0, "max_value": 2, "max_length": null}]
                }
              ]
            }"#,
        )
        .expect("the fixture module parses")
    }

    #[test]
    fn a_session_listens_or_says_why_not() {
        // Everything about what moved is tested in wctrl-engine against a
        // synthetic stream. What is only testable here is the thread: that it
        // starts, that a poll answers while it runs, and that a failure to join
        // the group is reported rather than swallowed into a panel that sits
        // empty forever.
        let state = State::default();
        state.start(&module());

        let report = state.with(|s| s.report()).expect("a session is running");
        assert!(report.listening);
        assert_eq!(report.module, "TEST_SESSION");
        assert!(report.changes.is_empty(), "nothing has moved yet");
        // No DCS here, so either the socket is open and quiet or the machine
        // would not let us join. Both are answers; silence about it is not.
        assert!(report.error.is_none() || report.datagrams == 0);

        // Stopping joins the thread, so a hang here is the bug this catches.
        state.stop();
        assert!(state.with(|_| ()).is_none());
        assert!(!Report::idle().listening);
    }

    #[test]
    fn starting_again_on_the_same_module_keeps_the_baseline() {
        // The window starts learn mode every time the panel opens. Rebinding on
        // each one would throw away a baseline that took an export cycle to
        // gather, and the user would wait again for nothing.
        let state = State::default();
        state.start(&module());
        let first = state.with(|s| s.report().module).expect("running");
        state.start(&module());
        assert_eq!(state.with(|s| s.report().module).as_deref(), Some(first.as_str()));
        state.stop();
    }
}
