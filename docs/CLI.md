# Command line

The converter is `dcs-signal.exe`. Installed, the DCS hook runs it for you; this
page is for running it by hand, to check or tinker. From a checkout, put
`cargo run --bin dcs-signal --` in front of each command (add `--release` for
flying). `dcs-signal --help` lists every command and flag.

## Running

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

The path defaults shown are a checkout's. Installed, they point at the
installed program and the profiles folder chosen at install.

| Flag | Default | What it does |
| --- | --- | --- |
| `--dry-run` | off | Prints every write and opens no device. Nothing touches the hardware. Worth one pass to confirm the aircraft is detected and the right profile is picked. |
| `--verbose` | off | Puts every action on the console as it happens. See below. The session log holds it either way, so this is only for watching a run live. |
| `--seconds <N>` | runs until Ctrl-C | Stops after N seconds. Useful for a quick check without having to interrupt it. |
| `--exit-when-idle <N>` | off | Clears the panels and exits once there has been no export stream for N seconds and DCS is no longer running, once it has seen the stream at least once. Used by the DCS hook. |
| `--profiles <DIR>` | `data/profiles` | Where profiles are read from, and where a starter profile is written for an aircraft that has none. Seeded from `--defaults` at startup. |
| `--defaults <DIR>` | `data/defaults` | Shipped profiles. Copied into `--profiles` for any name not already there, and never over one that is. |
| `--catalogue <DIR>` | `data/catalogue` | Generated signal catalogue. See above. |
| `--devices <FILE>` | `data/devices.json` | Hardware inventory: which LEDs exist, and what values each accepts. |
| `--log-dir <DIR>` | `Saved Games\DCS\Logs` installed, `data/logs` in a checkout | Where the session log goes. |
| `--no-log` | off | Writes no session log at all. |

From a checkout, the two runs worth knowing. Both are the `run` command, which
is the daemon; everything after the bare `--` is passed to it rather than to
cargo.

```powershell
# A dry pass. Prints every write, opens no device, stops itself after 30 s.
cargo run --bin dcs-signal -- run --dry-run --verbose --seconds 30

# Flying it. Drives the panels for real, and runs until Ctrl-C.
cargo run --release --bin dcs-signal -- run --verbose
```

**Do the dry pass first** whenever a profile or the hardware has changed. It
reads live DCS the same way the real thing does, so it confirms the aircraft is
detected and the right profile picked, with nothing to clear afterwards if the
answer is wrong.

**Fly with `--release`.** A debug build is fine for a dry pass, but the figures
in [PERFORMANCE.md](PERFORMANCE.md) were measured on a release build and that is
what the installer ships. Drop `--verbose` once a run looks right; it costs
nothing to leave on, and says so in the table above.

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

### The session log

Every run writes one, to `Saved Games\DCS\Logs\dcs-signal.log`. That is DCS's
own log folder, beside `dcs.log`: the folder to zip up when reporting a problem,
and the two line up against each other by their timestamps.

Started by the DCS hook, the daemon has no console, so this is the only account
of a flight there is. It holds everything `--verbose` puts on the console, and
more besides: where every file was read from, every WinCtrl device plugged in
whether this build knows it or not, which profiles loaded and which were thrown
out, and the error behind an exit.

**Two sessions are kept.** Each start makes the last log `dcs-signal.log.bak`
and deletes the one before that. Notice something wrong, land, and read it,
rather than flying on and rolling it away.

Each line is the local time, how loud it is, and the text:

```text
2026-09-20 11:47:51.534  INFO   paths    catalogue C:\Users\you\...\data\catalogue
2026-09-20 11:47:51.725  INFO   usb      pid 0xbf05  WINCTRL CarrierAce PTO 2  serial 56E5...
2026-09-20 11:47:53.568  INFO   aircraft A-10C_2  ->  profile A-10C
2026-09-20 11:47:53.881  TRACE      2151 ms  signal  LCP_CONSOLE                  = 0
2026-09-20 11:47:53.881  TRACE      2151 ms  write   CarrierAce_UFC.INST_PNL_Backlight = 255
2026-09-20 11:47:59.759  INFO   status   93 frame(s), 126 word(s) in, 45 lamp write(s), 27 paint(s), longest pass 1 ms
```

Search `ERROR` and `WARN` first: those are a device that would not open, a
profile that was skipped, a signal this DCS-BIOS does not have. `INFO` is what
the daemon did, `TRACE` is the traffic, and the traffic reads exactly as the
`--verbose` timeline above does.

**One line a second per thing.** A gauge moves on every export frame and a
screen repaints nearly as often, so each signal, lamp and screen gets at most
one line a second, carrying the latest value with the rest counted:

```text
    9601 ms  signal  PLT_RV5_ALT                  = 8738 -> 100  (x14)
```

Fourteen changes in that second, and the one shown is where it ended up. A
screen is logged as the rows it says, once a second; a segment or pixel display
is a bitmap, so the log keeps its first sixteen bytes and its length rather than
a screen of hex a second.

**A status line every minute**, whether or not anything happened. An idle
daemon and a wedged one look alike otherwise, and which of the two it was is
usually the whole question. `longest pass` is the slowest trip round the main
loop in that minute, which is the number to watch if the panels feel behind.

At ten megabytes it rolls over, keeping one behind exactly as a restart does,
and repeats the startup lines at the top of the new file so it still says what
was flying.

### Stopping

**Ctrl-C.** It clears the lamps on the way out, which matters because the panels
latch: there is no watchdog in the device, so whatever was last written stays
lit until something writes again.

If you ever see the daemon exit with `0xc000013a`, the clean shutdown did not
happen and your lamps are still lit. Start it and stop it again to clear them.

It only clears lamps it lit itself. Anything it never wrote is left alone.

### Starting with DCS

A hook starts the daemon when a mission begins, so nothing has to be
remembered. The installer places it. To set it up by hand, for a checkout,
copy the two files from `tools/hook`:

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
silent and DCS has closed, which covers a crash, a kill and an ordinary exit
under one rule. A quiet stream while DCS is still running, such as the options
menu or the main menu between missions, leaves the panels as they were.

`--exit-when-idle <SECONDS>` is ignored until the stream has been heard at least
once, so launching before DCS is up is a wait rather than an immediate exit.

**Only one daemon drives the panels.** A second one notices the first and exits
without touching anything. This is reachable in ordinary use: if DCS crashes and
you restart it quickly, the new DCS has no memory of having started a daemon and
the hook launches another, while the first is still running. Two daemons on one
panel look fine until one exits and clears the lamps while the other is still
lighting them.

The new one backing off is deliberate. The daemon already running is synced to
the stream and heals itself, holding the last cockpit through a quiet stream and
sweeping again when a new aircraft loads. Replacing it would gain nothing and would
mean killing a process mid-write. If you do want to replace it, stop it first;
the message says so.

A `--dry-run` writes nothing, so it is allowed alongside a live daemon.

The editor is separate on purpose. The daemon is what you need to fly; the
editor is only needed when you want to change something.

## The signal catalogue

The signal catalogue is generated from the DCS-BIOS installed on this machine
and is not in git. Nothing needs doing: the daemon and the editor build it on
startup, and again whenever the installed DCS-BIOS changes, whether to a new
version or by its files being replaced. The editor's profiles page says which
DCS-BIOS the signals came from and whether it just rebuilt them. To force a
rebuild, or to point at DCS-BIOS outside `Saved Games\DCS\Scripts`:

```powershell
dcs-signal catalogue --rebuild
dcs-signal catalogue --rebuild --bios "D:\DCS-BIOS\doc\json"
```

The folder given with `--bios` is remembered, so it is needed only once.

## Other commands

`dcs-signal --help` lists them. `listen` is the useful one
alongside the daemon: it prints the raw stream and can follow named signals on
one timestamped timeline, which is how the flap thresholds were measured.

```powershell
dcs-signal listen --seconds 60 --watch FLAP_POS --watch FLAPS_SWITCH
```

It can run at the same time as the daemon.

`stop` asks a running converter to clear the panels and exit. It is what the
editor's **Manage Converter** presses, and the way to stop one you did not
start, such as the daemon the DCS hook launched.

```powershell
dcs-signal stop
```

Nothing is killed. The message goes to the loopback socket the daemon already
holds to prove it is the only one running, and the daemon leaves by the same
path as Ctrl-C, clearing every lamp it lit and blanking every screen it drove.
A daemon that will not answer is reported rather than forced, because a
terminated one would leave the panels latched with nothing left to clear them.

`learn` is the editor's learn mode without the editor. It watches everything the
loaded module publishes and prints a table per window, fewest movements first,
so a control can be named without a window open:

```powershell
dcs-signal learn --seconds 5
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
