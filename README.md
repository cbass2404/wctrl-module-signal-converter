# DCS Signal Converter

Middleware that reads DCS-BIOS signals from whatever DCS aircraft is loaded and
lights the matching LEDs on WinCtrl (WinWing) panels, driven by a per-aircraft
profile.

SimAppPro is not required. DCS-BIOS is required; it is the only signal source.

An independent project. It is not made, endorsed or supported by WinCtrl or
WinWing; their names appear here only to say which hardware it drives.

This README covers running the daemon. Everything else is in `docs/`:
`STATUS.md` to resume work, `CONFIG.md` for the profile format, `PROTOCOL.md`
for the HID protocol.

## Before the first run

The signal catalogue is generated from the DCS-BIOS installed on this machine
and is not in git. Nothing needs doing: the daemon and the editor build it on
startup, and again whenever the installed DCS-BIOS changes, whether to a new
version or by its files being replaced. The editor's profiles page says which
DCS-BIOS the signals came from and whether it just rebuilt them. To force a
rebuild, or to point at DCS-BIOS outside `Saved Games\DCS\Scripts`:

```powershell
cargo run --bin dcs-signal -- catalogue --rebuild
cargo run --bin dcs-signal -- catalogue --rebuild --bios "D:\DCS-BIOS\doc\json"
```

The folder given with `--bios` is remembered, so it is needed only once.

If `cargo` is not found inside VS Code but works in a standalone PowerShell
window, VS Code is holding a stale environment. Quit it completely, not "Reload
Window", and reopen.

## Running

From the repository root:

```powershell
cargo run --bin dcs-signal -- run
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
profile  A-10C                  15 set,  6 unset  for A-10C_2, A-10C
profile  FA-18                  23 set,  1 unset  for EA-18G, FA-18C_hornet, FA-18E, FA-18F
device   PTO2                   pid 0xbf05
device   Orion Throttle Base II pid 0xbd64
Running. Ctrl-C to stop and clear the panels.
aircraft A-10C_2  ->  profile A-10C
```

Nothing happens until a cockpit is loaded. Once one is, whether you enter it
after startup or were already flying, it names the aircraft and the profile it
chose, then writes every LED once to match the cockpit. After that it writes
only lamps whose signals change.

`n set, n unset` counts configured lamps against ones still to be decided. An
unset lamp is a normal state, not an error; it is simply driven off.

A `caution` line under a profile means it loads but probably does not do what
was meant, such as a PTO2 gate that goes dark with the cockpit lighting off. The
same caution shows in the editor.

### Seeing what it is doing

`--verbose` puts every action on one timeline, timestamped in milliseconds from
startup:

```text
aircraft A-10C_2  ->  profile A-10C
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

A panel with glass adds two more. `paint` is a group of four bytes sent to a
segment display, and `blank` is the same on the way out. The bytes are a bitmap
rather than characters, so read the `signal` line above them for what the field
now says:

```text
   31440 ms  signal  UFC_SCRATCHPAD_NUMBER_DISPLAY = " 264.000"
   31440 ms  paint   CarrierAce_UFC group 1       = b6 c7 63 c6
   31440 ms  paint   CarrierAce_UFC group 2       = f5 f5 f5 00
```

A string is printed quoted, because on a display field the padding is the
layout: a right aligned scratchpad would otherwise read the same as a left
aligned one. It is logged once the whole field has arrived, not once per word,
since DCS-BIOS delivers a string across several writes.

A gauge feeding a display field shows both numbers:

```text
   31502 ms  signal  PLT_RV5_ALT                  = 8738 -> 100
```

The position is what the stream carries and the reading is what reaches the
glass. Neither alone answers "is this right": 8738 cannot be checked against a
cockpit gauge, and 100 cannot be checked against the stream. Both go through
the same conversion the display uses, so they cannot drift apart.

Only signals the loaded profile actually reads are followed, whether a lamp
condition or a display field reads them. A cockpit pushes thousands of writes a
second, so logging all of them would bury the few that matter. Use
`listen --watch` when you need to see one that nothing is bound to.

This is why a busy log can go quiet on switching aircraft, which looks like a
fault and is not one. The line under the aircraft says how much there is to
see:

```text
aircraft F-14BU  ->  profile F-14BU
  following 1 signal address(es) for this profile
```

One address, from a profile with two lamps on one word and no display fields,
will log twice in a sortie. A profile with several display fields logs
constantly. Both are working.

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
| `dcs-signal-hook.lua` | `Saved Games/DCS/Scripts/Hooks/dcs-signal-hook.lua` |
| `run-hidden.vbs` | next to `dcs-signal.exe`, in the install folder |

Then replace `DSC_DIR` in the hook with the folder `dcs-signal.exe` lives in. The
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
| **+ Add alternative (or)** | Another way to light the lamp, independent of the first. Written for multicrew aircraft, where a lamp follows whichever seat you are in. When more than one could light it, the lamp takes the brightest, or the one whose signal moved last: that is for two seats with a knob each and no seat signal, as in the F-14. |
| **Always on** | Lit whenever the aircraft is loaded, reading nothing. On a lamp that dims, this is also how a fixed brightness is set. |
| **Match another lamp** | Follow another dimmer on the same device, so both move together. Only offered between lamps that dim. |

Each condition reads as a sentence until you click its pencil. An open condition
has keep, cancel and delete: cancel puts it back the way it was before you
started, and delete asks first.

A lamp that dims shows its values in the Output column. **when lit** is its
brightness when a threshold condition holds. **at zero** is what it takes when
its binding resolves to 0: the lamp it follows is dark, the scaled source is at
the bottom, or no condition holds. On the PTO2's `SL` and `FLAG` that is the
daylight floor; see "A lamp is configured but dark" below.

Problems the daemon would refuse the profile for are listed in red above the
lamps, and Save waits until they are fixed. Cautions, in yellow, are about a
profile that loads but probably does not do what was meant, and never block
Save.

#### New profiles and copies

**New profile** asks for the module, then which of its aircraft the profile is
for, as DCS reports them. One module often serves several aircraft, and sharing
DCS-BIOS outputs does not mean wanting the same lamps: the A-10C and A-10C II
read one module. It starts blank, or as a copy of a related profile.

**Copy to...** on a profile does the same from the other end: a copy under a new
name, for aircraft you type.

An aircraft belongs to one profile. Taking one that another profile already
claims moves it, and the dialog says so before you confirm. A move that would
leave a profile with no aircraft at all is refused.

#### Learn mode

Beside every signal box is a **Learn** button. Press it, flip the switch in the
cockpit, and the thing you just moved is at the top of the list. Click it to
bind it.

This is the answer to "I do not know what this is called". A module publishes
hundreds of signals, up to 1,440, named by DCS-BIOS rather than by anyone who
flies, so searching only works if you already half know the answer.

What is listed is everything that moved, ordered by how often it moved. A switch
thrown once comes out above an altimeter that has moved ninety times, and the
count is shown so you can tell them apart. Nothing is hidden on the grounds of
being busy: somebody mapping a gauge to a display field needs to find it.

It waits about a second first, while every signal gets a starting value. Until
that is done an empty list means "still reading", which it says, and afterwards
it means "nothing in the cockpit moved", which it also says. **Watch again**
clears the list for the next control without that wait.

**It only listens while the panel is open.** Nothing joins the DCS-BIOS stream
until you press Learn, and **Stop**, closing the panel, going back to the
profile list or closing the window all end it. It never sends anything, to DCS
or to the panels, and it reads its own copy of the multicast, so it changes
nothing about a mission in progress and runs happily alongside the daemon.

If DCS is flying an aircraft this profile does not cover, it says so rather than
sitting there empty: nothing you flip in a Hind will appear in a Hornet
profile.

It reads and writes `data/profiles`, the same folder the daemon reads, and the
daemon reloads a profile about a second after it is saved. Edit a lamp, save, and
watch it change on the panel without leaving the cockpit.

**Reset** replaces a profile with the copy that shipped in `data/defaults`, and
asks first. **Reset this lamp** does the same for one lamp in the open profile,
leaving the rest alone; it is not saved until you save, so leaving without saving
undoes it.

To build a standalone installer instead of running from source:

```powershell
cd editor
npm run tauri build
```

### The MCDU screen

Where an aircraft has a CDU of its own, its shipped profile puts that CDU on the
MCDU's screen, on all three MCDU names:

| Aircraft | What shows |
| --- | --- |
| A-10C, A-10C II | The CDU's ten lines on rows 5 to 14, so its scratchpad lands on the MCDU's |
| CH-47F | The seated crew member's CDU, all 14 lines in its own colours |
| F-14B (Upgrade) | The RIO's CDNU on the bottom eight rows, from either seat |
| AH-64D | The seated crew member's keyboard unit scratchpad, on the bottom row |

The screen holds no font across a power cycle, so the daemon uploads the
aircraft's own font when it first paints, and again when the next aircraft
needs a different one.
Each font draws that CDU's symbols, which is why only these aircraft have
one: a profile that puts fields on the MCDU for any other aircraft is refused.
The screen stays black while nothing is on it.

`dcs-signal mcdu-test` draws a test pattern without DCS, to check a panel on its
own.

### When something looks wrong

**Nothing lights.** Check it printed an `aircraft ...` line. No line means no
stream is arriving at all: DCS is not running, DCS-BIOS is not installed in
`Saved Games/DCS/Scripts`, or a firewall is blocking multicast. `listen` will
confirm which in fifteen seconds.

**A profile is missing from the startup list.** It was skipped, and the reason
is printed next to its filename. One bad profile never stops the others.

**An aircraft has no profile.** It writes a starter one to `--profiles` with
every lamp listed and none assigned, except the PTO2 gates, which start held at
full, then clears the panels. It never overwrites a file that already exists.
An aircraft DCS-BIOS has no module for gets no starter profile, since there is
nothing to bind. `NONE`, which DCS-BIOS reports when you have no aircraft of
your own, gets one named "No aircraft".

**A lamp is configured but dark.** The PTO2 has two gates above its lamps, and
an unbound gate is swept to 0 like any other unbound LED, so the lamp beneath it
acks normally and stays dark. `SL` gates all 14 indicators. `FLAG` governs seven
of them: NOSE, LEFT, RIGHT, FLAPS, HALF, FULL and HOOK. Every shipped profile
binds both. A gate tied to a cockpit dimmer with no floor is the same fault in
daylight: console lighting off reads as zero, so the gate goes to 0. Every
shipped gate that follows a dimmer carries an **at zero** of 255, and the
daemon and the editor both caution about one that does not.

## Other commands

`cargo run --bin dcs-signal -- --help` lists them. `listen` is the useful one
alongside the daemon: it prints the raw stream and can follow named signals on
one timestamped timeline, which is how the flap thresholds were measured.

```powershell
cargo run --bin dcs-signal -- listen --seconds 60 --watch FLAP_POS --watch FLAPS_SWITCH
```

It can run at the same time as the daemon.

`learn` is the editor's learn mode without the editor. It watches everything the
loaded module publishes and prints a table per window, fewest movements first,
so a control can be named without a window open:

```powershell
cargo run --bin dcs-signal -- learn --seconds 5
```

```text
  flying FA-18C_hornet, which is module FA-18C_hornet
  watching 503 signals
  ready. Flip something in the cockpit.

moves  signal                             value
    1  GEAR_LEVER                         0 -> 1   Gear Lever
   14  IFEI_RPM_L                         " 77" -> " 91"   RPM_L
```

Each window starts a fresh sheet, so several controls can be found in one run.
Ctrl-C stops it.

## Resource use

The daemon uses about 22 MB of memory and well under 1% of one core in flight.
The editor uses about 165 MB, almost all of it WebView2. The install is about
23 MB. `docs/PERFORMANCE.md` has the measurements and explains how to rerun them
with `tools/bench_daemon.py`.

## License

MIT, see `LICENSE`. The MCDU screen code and fonts come from other projects
under their own licenses, listed with their full text in
`THIRD_PARTY_NOTICES.md`.
