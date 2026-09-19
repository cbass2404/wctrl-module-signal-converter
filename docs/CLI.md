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
