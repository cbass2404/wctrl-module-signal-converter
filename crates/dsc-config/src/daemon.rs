//! Finding out whether the converter is running, and asking it to stop.
//!
//! Both the daemon and the editor need to agree on this, so the address and the
//! message live here rather than in either of them.

use std::net::UdpSocket;
use std::time::{Duration, Instant};

/// Loopback address the daemon binds to prove it is the only one running.
///
/// It exists because binding is atomic and the operating system releases it
/// when the process dies, crash included, so a second daemon can ask "is one
/// already running" and get a truthful answer with no stale state to clean up.
/// Having bound it, the daemon also reads it, which is what [`request_stop`]
/// talks to.
pub const INSTANCE_LOCK: &str = "127.0.0.1:16539";

/// The one message the lock socket acts on.
///
/// A magic string rather than any datagram at all, because something else on
/// loopback probing ports must not be able to take the panels down in the
/// middle of a flight.
pub const STOP_MESSAGE: &[u8] = b"dcs-signal: stop";

/// How long [`request_stop`] waits for the daemon to finish clearing up.
///
/// It clears every lamp it lit and blanks every screen it drove on the way out,
/// which is USB work, so the wait is generous. Returning early would mean the
/// caller starting a second daemon while the first still held the panels.
pub const STOP_TIMEOUT: Duration = Duration::from_secs(5);

/// Claim the right to drive the panels, or report who already has it.
///
/// Two daemons on one set of panels mostly looks fine, because both write the
/// same values from the same stream. It goes wrong at the end: one exits and
/// clears the lamps while the other is still lighting them.
///
/// This is reachable in ordinary use. If DCS crashes and is restarted inside
/// the idle window, the new DCS gets a fresh Lua state, so the hook's own
/// "already started" flag is gone and it launches a second daemon while the
/// first is still alive.
///
/// A lock file would survive a crash and then need its own liveness check,
/// which is the problem this is meant to solve rather than a solution to it.
pub fn take_instance_lock() -> std::io::Result<UdpSocket> {
    let socket = UdpSocket::bind(INSTANCE_LOCK)?;
    // Read without waiting: the daemon asks once per pass of its main loop and
    // must never block there, because that loop is also the export stream.
    socket.set_nonblocking(true)?;
    Ok(socket)
}

/// Whether something has asked this daemon to stop.
///
/// The socket is bound already and proves single instance already, so listening
/// on it costs nothing and needs no second port. Anything that is not the stop
/// message is drained and ignored, because a datagram left unread would come
/// back on every later pass.
pub fn stop_requested(lock: &UdpSocket) -> bool {
    let mut buf = [0u8; 64];
    let mut asked = false;
    while let Ok((n, _)) = lock.recv_from(&mut buf) {
        if buf[..n] == *STOP_MESSAGE {
            asked = true;
        }
    }
    asked
}

/// Whether a daemon is running, asked by trying to take the lock it holds.
///
/// The socket binds and drops inside this call, so a true answer means "nobody
/// held it a moment ago" rather than "nobody can take it". That is the same
/// race the daemon's own claim has, and it is harmless here: the worst case is
/// offering a restart that then finds nothing to stop.
pub fn is_running() -> bool {
    UdpSocket::bind(INSTANCE_LOCK).is_err()
}

/// Ask a running daemon to stop, and wait until it has.
///
/// Returns whether one was running to begin with. A daemon that ignores this,
/// because it is wedged rather than merely busy, leaves the lock held and the
/// timeout expires: reported as an error rather than pressed on with, since
/// starting a second daemon over a live one is the thing the lock exists to
/// prevent.
///
/// Nothing is killed. The daemon clears the panels on its own way out, and a
/// terminated process would not: the lamps latch, so whatever was lit would
/// stay lit with nothing left running to clear it.
pub fn request_stop(timeout: Duration) -> std::io::Result<bool> {
    if !is_running() {
        return Ok(false);
    }
    // A socket has to be bound to send. An ephemeral port, because binding the
    // lock address is the test rather than the sending.
    let socket = UdpSocket::bind("127.0.0.1:0")?;
    socket.send_to(STOP_MESSAGE, INSTANCE_LOCK)?;

    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if !is_running() {
            return Ok(true);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::TimedOut,
        "the converter did not stop when asked",
    ))
}
/// The process id holding the lock, if one does.
///
/// Asked of the operating system rather than of a file the daemon wrote,
/// because the case this exists for is a daemon that has stopped answering, and
/// a pid file written before it wedged is exactly the stale state the lock was
/// chosen to avoid. Shelling out follows what the daemon already does to ask
/// whether DCS is alive.
#[cfg(windows)]
pub fn owner_pid() -> Option<u32> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let out = std::process::Command::new("netstat")
        .args(["-ano", "-p", "UDP"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    pid_holding(&String::from_utf8_lossy(&out.stdout), INSTANCE_LOCK)
}

#[cfg(not(windows))]
pub fn owner_pid() -> Option<u32> {
    None
}

/// Pick the owning process id out of `netstat -ano -p UDP` output.
///
/// Split out from the command so it can be tested with no daemon to find. A UDP
/// row carries no state column, which is what makes the pid the fourth field
/// rather than the fifth.
pub fn pid_holding(stdout: &str, address: &str) -> Option<u32> {
    for line in stdout.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if let [proto, local, _foreign, pid] = fields[..] {
            if proto.eq_ignore_ascii_case("udp") && local == address {
                return pid.parse().ok();
            }
        }
    }
    None
}

/// End the converter without asking it.
///
/// The escape hatch for a daemon that will not answer [`request_stop`]. Returns
/// whether there was one to end.
///
/// **The panels are not cleared.** They latch, and a terminated process runs
/// none of its shutdown, so whatever was lit stays lit until something writes
/// it again. Starting the converter and stopping it properly is what clears
/// them. Anything offering this has to say so.
///
/// Only the process actually holding the lock is ended, so a `dcs-signal
/// listen`, or a second copy being worked on alongside, is left alone.
#[cfg(windows)]
pub fn kill() -> std::io::Result<bool> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    if !is_running() {
        return Ok(false);
    }
    let pid = owner_pid().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "something holds the lock but no process could be found holding it",
        )
    })?;
    let out = std::process::Command::new("taskkill")
        .args(["/F", "/PID", &pid.to_string()])
        .creation_flags(CREATE_NO_WINDOW)
        .output()?;
    if !out.status.success() {
        return Err(std::io::Error::other(format!(
            "taskkill refused to end process {pid}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(true)
}

#[cfg(not(windows))]
pub fn kill() -> std::io::Result<bool> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "only implemented on Windows",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
  Proto  Local Address          Foreign Address        State           PID
  UDP    0.0.0.0:500            *:*                                    4440
  UDP    127.0.0.1:16539        *:*                                    21068
  UDP    0.0.0.0:5010           *:*                                    21068
";

    #[test]
    fn the_lock_holders_pid_is_read_from_netstat() {
        assert_eq!(pid_holding(SAMPLE, INSTANCE_LOCK), Some(21068));
    }

    #[test]
    fn a_port_nobody_holds_has_no_pid() {
        assert_eq!(pid_holding(SAMPLE, "127.0.0.1:16540"), None);
        assert_eq!(pid_holding("", INSTANCE_LOCK), None);
    }

    #[test]
    fn a_tcp_row_for_the_same_port_is_not_the_lock() {
        // TCP rows carry a state column, so the fourth field is a word rather
        // than a pid. Matching one would name the wrong process to end.
        let tcp = "  TCP    127.0.0.1:16539        0.0.0.0:0              LISTENING       999\n";
        assert_eq!(pid_holding(tcp, INSTANCE_LOCK), None);
    }
}

