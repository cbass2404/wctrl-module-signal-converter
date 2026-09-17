# wctrl

Middleware that reads DCS-BIOS signals from whatever DCS aircraft is loaded and
lights the matching LEDs on WinCtrl (WinWing) panels, driven by a per-aircraft
profile.

SimAppPro is not required and should be closed while this runs. DCS-BIOS is
required; it is the only signal source.

This README covers running the daemon. Everything else is in `docs/`:
`STATUS.md` to resume work, `CONFIG.md` for the profile format, `PROTOCOL.md`
for the HID protocol.

## Before the first run

The signal catalogue is generated from the DCS-BIOS installed on this machine
and is not in git, so build it once after cloning, and again whenever DCS-BIOS
updates:

```powershell
python tools/build_catalogue.py
```

Skipping this gives `loading data/catalogue ...` on startup.

If `cargo` is not found inside VS Code but works in a standalone PowerShell
window, VS Code is holding a stale environment. Quit it completely, not "Reload
Window", and reopen.

## Running

From the repository root:

```powershell
cargo run --bin wctrl -- run
```

Add `--release` for actual flying. The debug build spends several seconds just
parsing the 11 MB catalogue at startup.

**Start it whenever you like, including mid-mission.** DCS-BIOS re-exports its
map on a cycle rather than sending changes only, so the aircraft name arrives
within about a second of startup, measured at roughly every 300 ms. The daemon
picks the profile, waits for the stream to settle, then sweeps every lamp to
match the cockpit as it stands. Nothing needs re-slotting and the mission does
not need restarting.

**Profiles are picked up while it runs.** Save a profile, from the editor or from
any text editor, and the daemon reloads it within about a second and re-syncs
every lamp to the cockpit as it stands. Nothing needs stopping and DCS keeps
flying.

### Flags

| Flag | Default | What it does |
| --- | --- | --- |
| `--dry-run` | off | Prints every write and opens no device. Nothing touches the hardware. Worth one pass to confirm the aircraft is detected and the right profile is picked. |
| `--verbose` | off | Logs every action as it happens. See below. Works with or without `--dry-run`, so it can be left on while actually flying. |
| `--seconds <N>` | runs until Ctrl-C | Stops after N seconds. Useful for a quick check without having to interrupt it. |
| `--exit-when-idle <N>` | off | Clears the panels and exits after N seconds with no export stream, once it has seen the stream at least once. Used by the DCS hook. |
| `--profiles <DIR>` | `data/profiles` | Where profiles are read from, and where a starter profile is written for an aircraft that has none. Seeded from `--defaults` at startup. |
| `--defaults <DIR>` | `data/defaults` | Shipped profiles. Copied into `--profiles` for any name not already there, and never over one that is. |
| `--catalogue <DIR>` | `data/catalogue` | Generated signal catalogue. See above. |
| `--devices <FILE>` | `data/devices.json` | Hardware inventory: which LEDs exist, and what values each accepts. |

### What you should see

```
profile  A-10C II               15 set,  6 unset  for A-10C_2, A-10C
profile  F/A-18C Hornet         20 set,  0 unset  for FA-18C_hornet
device   PTO2                   pid 0xbf05
device   Orion Throttle Base II pid 0xbd64
Running. Ctrl-C to stop and clear the panels.
aircraft A-10C_2  ->  profile A-10C II
```

Nothing happens until a cockpit is loaded. Once one is, whether you enter it
after startup or were already flying, it names the aircraft and the profile it
chose, then writes every LED once to match the cockpit. After that it writes
only lamps whose signals change.

`n set, n unset` counts configured lamps against ones still to be decided. An
unset lamp is a normal state, not an error; it is simply driven off.

### Seeing what it is doing

`--verbose` puts every action on one timeline, timestamped in milliseconds from
startup:

```text
aircraft A-10C_2  ->  profile A-10C II
  following 6 signal address(es) for this profile
     731 ms  signal  FLAP_POS                     = 20000
    2818 ms  sweep   TAKEOFF_PLANEL_2.SL          = 255
    2818 ms  sweep   TAKEOFF_PLANEL_2.FLAPS       = 1
    9601 ms  signal  FLAP_POS                     = 0
    9601 ms  write   TAKEOFF_PLANEL_2.FLAPS       = 0
   22017 ms  clear   TAKEOFF_PLANEL_2.SL          = 0
```

`signal` is a value the active profile reads, printed when it moves. `sweep` is
the one-pass write after a module load, `write` is an incremental change, and
`clear` is the shutdown. A `signal` line with no `write` under it means the
value moved but no lamp changed state, which is the normal case for a gauge
travelling inside a threshold.

Only signals the loaded profile actually binds are followed. A cockpit pushes
thousands of writes a second, so logging all of them would bury the few that
matter. Use `listen --watch` when you need to see one that nothing is bound to.

### Stopping

**Ctrl-C.** It clears the lamps on the way out, which matters because the panels
latch: there is no watchdog in the device, so whatever was last written stays
lit until something writes again.

If you ever see the daemon exit with `0xc000013a`, the clean shutdown did not
happen and your lamps are still lit. Start it and stop it again to clear them.

It only clears lamps it lit itself. Anything it never wrote is left alone.

### Starting with DCS

A hook can start the daemon when a mission begins, so nothing has to be
remembered. Copy the two files from `tools/hook`:

| From | To |
| --- | --- |
| `wctrl-hook.lua` | `Saved Games/DCS/Scripts/Hooks/wctrl-hook.lua` |
| `run-hidden.vbs` | next to `wctrl.exe`, in the install folder |

Then replace `WCTRL_DIR` in the hook with the folder `wctrl.exe` lives in. The
VBS shim exists so no console window flashes on every mission start.

**The hook only starts the daemon; it never stops it.** A hook cannot run when
DCS is killed or crashes, and the panels latch, so a shutdown message would be
missing in exactly the case that needs it most. The daemon is launched with
`--exit-when-idle` instead and leaves on its own once the export stream falls
silent, which covers a crash, a kill and an ordinary mission end under one rule.
Starting the next mission brings it back.

`--exit-when-idle <SECONDS>` is ignored until the stream has been heard at least
once, so launching before DCS is up is a wait rather than an immediate exit.

**Only one daemon drives the panels.** A second one notices the first and exits
without touching anything. This is reachable in ordinary use: if DCS crashes and
you restart it quickly, the new DCS has no memory of having started a daemon and
the hook launches another, while the first is still running. Two daemons on one
panel look fine until one exits and clears the lamps while the other is still
lighting them.

The new one backing off is deliberate. The daemon already running is synced to
the stream and heals itself, clearing the panels when the stream goes quiet and
sweeping again when a mission loads. Replacing it would gain nothing and would
mean killing a process mid-write. If you do want to replace it, stop it first;
the message says so.

A `--dry-run` writes nothing, so it is allowed alongside a live daemon.

The editor is separate on purpose. The daemon is what you need to fly; the
editor is only needed when you want to change something.

### The profile editor

A separate window for assigning lamps, so profiles do not have to be edited as
JSON.

**Node is a build dependency, not a runtime one.** Tauri compiles the interface
to static files and embeds them in the executable, so the installed editor needs
nothing beyond WebView2, which is part of Windows. Node is needed only to run it
from source, as below.

```powershell
cd editor
npm install
npm run tauri dev
```

`npm install` is needed once. The first `tauri dev` compiles the window's
dependencies and takes a few minutes; later runs start in seconds.

It lists every lamp on every device in `data/devices.json`, whether or not the
panel is plugged in, one collapsible section per device. Picking a signal is
usually the only step: type three characters to search it by name, and the test
is filled in to suit the signal and the lamp.

There are four ways to drive a lamp, and the editor offers each one only where
it can mean something:

| | |
| --- | --- |
| **+ Add condition** | Another test that must *also* hold. The A-10C half-flaps lamp needs two. |
| **+ Add alternative (or)** | Another way to light the lamp, independent of the first. Written for multicrew aircraft, where a lamp follows whichever seat you are in. |
| **Always on** | Lit whenever the aircraft is loaded, reading nothing. On a lamp that dims, this is also how a fixed brightness is set. |
| **Match another lamp** | Follow another dimmer on the same device, so both move together. Only offered between lamps that dim. |

Each condition reads as a sentence until you click its pencil. An open condition
has keep, cancel and delete: cancel puts it back the way it was before you
started, and delete asks first.

It reads and writes `data/profiles`, the same folder the daemon reads, and the
daemon reloads a profile about a second after it is saved. Edit a lamp, save, and
watch it change on the panel without leaving the cockpit.

**Reset** replaces a profile with the copy that shipped in `data/defaults`.
**Reset this lamp** does the same for one lamp, leaving the rest of your profile
alone. Those are the only two things in the editor that discard your work, and
both ask first.

To build a standalone installer instead of running from source:

```powershell
cd editor
npm run tauri build
```

### When something looks wrong

**Nothing lights.** Check it printed an `aircraft ...` line. No line means no
stream is arriving at all: DCS is not running, DCS-BIOS is not installed in
`Saved Games/DCS/Scripts`, or a firewall is blocking multicast. `listen` will
confirm which in fifteen seconds.

**A profile is missing from the startup list.** It was skipped, and the reason
is printed next to its filename. One bad profile never stops the others.

**An aircraft has no profile.** It writes a starter one to `--profiles` with
every lamp listed and none assigned, then clears the panels. It never
overwrites a file that already exists.

**A lamp is configured but dark.** The PTO2 has two gates above its lamps, and
an unbound gate is swept to 0 like any other unbound LED, so the lamp beneath it
acks normally and stays dark. `SL` gates all 14 indicators. `FLAG` governs seven
of them: NOSE, LEFT, RIGHT, FLAPS, HALF, FULL and HOOK. Both shipped profiles
bind both. A lamp that is lit but too dim to see in daylight is the same fault
wearing a different hat, which is why `FLAG` goes to full bright when the
cockpit console dimmer reads zero.

## Other commands

`cargo run --bin wctrl -- --help` lists them. `listen` is the useful one
alongside the daemon: it prints the raw stream and can follow named signals on
one timestamped timeline, which is how the flap thresholds were measured.

```powershell
cargo run --bin wctrl -- listen --seconds 60 --watch FLAP_POS --watch FLAPS_SWITCH
```

It can run at the same time as the daemon.
