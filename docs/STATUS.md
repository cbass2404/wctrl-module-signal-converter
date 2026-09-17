# Project status

Written 2026-09-16. Enough context to resume cold.

## Resume here

Everything below is background. This is what to actually do next.

**Verify nothing has rotted** (30 seconds, no hardware, no DCS):

```powershell
python tools/build_catalogue.py   # required after a fresh clone - see below
cargo test --workspace            # expect 94 passing
cargo run --bin wctrl -- devices
cargo run --bin wctrl -- catalogue --aircraft F-4E-45MC --find hook
```

`data/catalogue/` is **not in git**. It is generated from the DCS-BIOS installed
on this machine, and a catalogue from a different DCS-BIOS release reads the
wrong addresses silently, because addresses are allocated sequentially as
controls are defined. Rebuild it after cloning, and again whenever DCS-BIOS
updates it is stamped with the version it came from.

**Then, in order:**

1. ~~Build the engine crate.~~ **Done 2026-09-16.** `crates/wctrl-engine` holds
   all the policy and does no I/O, so the whole module-load sequence is tested
   without hardware or DCS. `wctrl run` is the daemon around it.

   **Proven end to end 2026-09-16.** `wctrl run --verbose` drove the PTO2 from a
   live A-10C mission: gear lamps, Master Caution, backlight tracking the console
   dimmer, and the two-condition HALF lamp lighting only at MVR with the gauge in
   its window. The flap lamps looked dead and were not; see the `FLAG` dimmer in
   the verified facts.

2. **Fly it.** Edit a profile in `data/profiles` (the active folder, seeded
   from `data/defaults` on startup), then, with a mission loaded:

   ```powershell
   cargo run --bin wctrl -- run --dry-run    # prints writes, opens no device
   cargo run --bin wctrl -- run              # drives the panels
   ```

   `--dry-run` is the safe first pass: it proves aircraft detection, profile
   selection and the sweep without touching hardware. Ctrl-C clears whatever
   was lit, which matters because the panels latch.

3. ~~Measure the Orion II.~~ **Done 2026-09-16.** Index 0 is a dimmer; indices
   1 and 2 (A/A, A/G) are binary. Backlight dims them but does not gate them.
   Details under Verified facts.

4. **Inventory the three devices.** `wctrl devices` reports an MCDU
   CAPTAIN (`0xbb36`), Orion Combat Rudder Pedals Metal (`0xbef0`) and a
   CarrierAce UFC + HUD (`0xbede`) that `devices.json` knows nothing about. The
   UFC and MCDU in particular are display devices, so they may not use `SET_LEDX`
   at all. Same method: `wctrl parts --pid ...`, then a SimAppPro HID capture.

5. **The Tauri editor.** Scaffolded 2026-09-16, see below. Shape is specified
   in `CONFIG.md`.

The engine came first on purpose: every layer was exercised on real hardware
through the CLI before anything was wrapped in Tauri.

## Where the editor stands

Scaffolded and compiling, 2026-09-16. `editor/` holds a Tauri 2 app: vanilla
TypeScript with Vite in `editor/src`, and `editor/src-tauri` as a workspace
member. Frontend builds to 5.6 kB of JavaScript, no framework.

```powershell
cd editor
npm install
npm run tauri dev
```

**Working:** the profile library (list, create, reset), the module picker fed
from the catalogue index, and one collapsible section per device, collapsed on
open with an expand/collapse-all control whose label follows the sections.

Bindings are fully editable. A condition reads as a sentence until its pencil is
clicked, and an open condition carries keep, cancel and delete: cancel restores
it as it was when editing began, and delete confirms first. All four binding
forms are offered, each only where it can mean something:

* **conditions**, through the signal typeahead, its test and its values
* **any_of**, through "+ Add alternative (or)"
* **always**, on a lamp nothing is assigned to
* **same_as**, only on a dimmer with another dimmer to point at

`Reset this lamp` appears only where a lamp differs from the shipped profile.
Save is explicit, and the unsaved marker compares against a snapshot rather than
setting a flag, so undoing an edit clears it.

Only the open profile's module is loaded, never the whole catalogue.

Saving from the editor takes effect in the running daemon within about a second:
the profile directory is polled, a settled change triggers a reload, and the
engine re-sweeps. Editing a lamp mid-flight and seeing it change on the panel
needs no restart of anything.

The daemon can be started by a DCS hook (`tools/hook`). The hook only starts it;
stopping is the daemon's own business, because a hook cannot run when DCS is
killed or crashes and the panels latch. With `--exit-when-idle`, a quiet export
stream clears the panels and a dead `DCS.exe` is what ends the process, so
sitting in the menu between missions is survived rather than treated as a crash.
Only one daemon runs at a time; a second backs off.

**Not built yet:**

1. **Learn mode.** The editor has to read the DCS-BIOS stream for this, which
   it does not do at all yet. `CONFIG.md` calls it worth more than any amount
   of search polish, and it is the reason the typeahead needs no browse mode.
2. **Renaming a profile, and editing its aircraft list.** Both are fixed at
   creation right now.
3. **Validation before save.** `Profile::validate` exists and is not called from
   the editor, so a binding it would reject still saves quietly. The four forms
   are mutually exclusive and the editor keeps them that way by construction,
   but a hand-edited file is only caught when the daemon loads it.

**Confirmed in the window 2026-09-16:** profile list, create with the module
picker, collapsible sections, and the signal search. Three faults found by using
it and fixed: columns not aligning between sections, the hint box being cut off
at the window edge, and dropdown rows losing clicks to a focus race.

**Verified on hardware 2026-09-17**, in a running mission with real panels:

* **Lamps still follow signals** after `apply` began reporting whether a word
  actually moved. This was the regression risk of that change: a wrong answer
  would have left lamps lit by the module-load sweep and then frozen.
* **Hot reload.** A profile saved in the editor reached the running daemon and
  changed the panel without stopping anything.
* **A quiet stream clears the panels**, and the daemon stays up through it while
  DCS is still running.
* **The same aircraft loaded twice** sweeps the second time. This is the one
  that fails silently if the engine does not forget the cockpit on a quiet
  stream, and a different aircraft would have passed either way.
* **`any_of` in the AH-64D**, including the seat swap, which is the half that
  cannot be proven any other way.

The daemon exiting when `DCS.exe` disappears was proven from the command line
rather than through the hook, which is not installed yet.

**Still not exercised:** `always` and `same_as` are used by no profile, so they
pass their tests but have never driven a real lamp.

## The UFC, and the first device with a display

Mapped 2026-09-17. The CarrierAce UFC and the HUD control panel below it are one
USB interface, PID `0xbede`, answering as two parts: `0xbed0` for the UFC and
`0xbe0e` for the HUD.

**The lamps are trivial and are done.** Three dimmers, no indicators at all:
`INST_PNL_Backlight` and `LCDBacklight` on the UFC, `INST_PNL_Backlight` on the
HUD. All three verified on the wire taking the full 0-255 range. They need no
new code; the existing `set_led` path drives them, and `UFC_BRT` and
`INST_PNL_DIMMER` already exist in the Hornet catalogue to feed them.

**The display is the new thing.** 96 bytes of segment bitmap, 36 character
cells, written four bytes at a time with `SET_LCDS` (`0x4c`). There is no text
on the wire. `data/displays/ufc1.json` holds the cell and glyph map, transcribed
from SimAppPro's tables and then **confirmed against hardware**: replaying a
captured mission through it rendered the Hornet A/P page (`ATTH HSEL BALT RALT
CPL`) and COMM page (`GRCV SQCH CPHR AM MENU`) with 0 of 36 cells unmatched.
`docs/PROTOCOL.md` has the frame layout and the four behaviours that are not
obvious.

**Built and wired, 2026-09-17.** Signal to glass works end to end and is
checked against captured hardware traffic rather than against our own reasoning:
`crates/wctrl-engine/tests/display_paint.rs` feeds the engine a Hornet COMM page
as DCS-BIOS frames and asserts the bytes it paints are the ones SimAppPro sent
the real device. All six compared groups match.

* `wctrl-config::display` holds `Display`, `DisplayCatalogue`, `Screen`,
  `CellRange` and `Readout`.
* The engine repaints the whole screen on every batch and diffs whole groups.
  Not an optimisation: a field spans several words, DCS-BIOS delivers them
  across separate writes, and a part-arrived field is a state that was never in
  the cockpit. Painting from scratch means the glass only shows settled text.
* `mission_ended` and shutdown blank the glass, which the latch behaviour makes
  mandatory.
* `Device::set_lcd` sends `SET_LCDS`. It is never acknowledged, so a failed
  write is corrected by the next repaint rather than retried.
* The editor has a display section per device with glass, and a "drive this
  panel" checkbox per device.
* `data/defaults/fa-18c-hornet.json` ships all 15 UFC fields.

**Two bugs this uncovered, both fixed:**

1. **Lamp names were not unique within a device.** The vendor calls a lamp
   `INST_PNL_Backlight` on both the UFC part and the HUD part, and a profile
   addresses a lamp by device and name, so the second was unreachable with no
   error anywhere. Renamed, and `every_lamp_name_is_unique_within_its_device`
   now enforces it.
2. **The editor stripped string signals from its own signal list**, because a
   lamp condition compares numbers. Every display field therefore fell through
   to the numeric branch and had a range written into it, which validation then
   rejected, skipping the whole profile. Signals now carry a `text` flag; the
   lamp picker hides them and the display picker offers them.

**Profiles now self-update.** `Profiles::merge_new` runs at daemon startup and
adds rows for hardware a profile predates, from the shipped default first and
then as blank rows. It never touches an existing row, keeps rows for unplugged
panels, leaves an unparseable file alone, and is idempotent. Bindings are sorted
by device display name, then part in declared order, then hardware index.

**Not built yet, and this is the actual work:**

1. ~~**A host-side shadow of the buffer.**~~ Done. `wctrl-config::display`
   has `Display`, `DisplayCatalogue` and `Screen`, checked against captured
   hardware traffic by `crates/wctrl-config/tests/display_render.rs`: rendering
   two real display states reproduces the exact 96 bytes the device was sent.
   Still to do is naming a display from a device spec so a part can carry
   one.
2. **Fly it.** Everything above is proven against captured traffic and in
   tests. No profile with readouts has yet driven the real glass in a live
   mission, which is the one thing left to confirm.
3. **A numeric source has never been flown.** The conversion is tested,
   including faces that start below zero and run backwards, but no gauge has
   been put on the glass. Whether 65535 is linear in the quantity or in needle
   angle is still unmeasured, and matters for how honest the reading is.
4. **The editor cannot yet edit aliases or notes** on a display field; both are
   editable by hand in the JSON.

**Design call, open to revision:** the cell map is a property of the device and
lives in `data/displays`, but which signal feeds which cell is a property of the
aircraft and belongs in the profile. SimAppPro hard-codes the Hornet mapping in
a per-aircraft source file; putting it in the profile is what lets someone point
the glass at another module without a code change.

## Profiles ship from `data/defaults`

Changed 2026-09-16. `data/defaults` holds the profiles we ship, tracked in git.
`data/profiles` is the active folder the daemon reads and the editor writes; it
is gitignored and seeded from `data/defaults` on every start for any name not
already there. Seeding adds and never replaces. Reset is the only overwrite.

`Profiles` in `wctrl-config` owns this, and both the CLI and the editor call it,
so there is one implementation of the rule rather than two.

**Housekeeping:** the repo is initialised and `.gitignore` is written 19 files,
~126 KB, with `target/` and the generated `data/catalogue/` excluded and the
reasons recorded in the file itself. SimAppPro's `HIDLog` is **off** again as of
2026-09-16. Turn it back on in `%APPDATA%\SimAppPro\config.json` only while
capturing a device, since it grows `WWTHID.log` by ~5 MB per session.

## What this is

Middleware that reads DCS-BIOS signals from whatever aircraft is loaded and
lights the corresponding LEDs on WinCtrl (WinWing) panels, driven by a
per-aircraft profile the user authors in a UI.

## Architecture: direct HID

Three approaches were considered. **We chose direct HID.**

|                     | Source       | Sink                              | DCS edits  | Elevation | Status             |
| ------------------- | ------------ | --------------------------------- | ---------- | --------- | ------------------ |
| **A. Direct HID**   | DCS-BIOS UDP | raw HID writes                    | none       | none      | **chosen, proven** |
| B. Masquerade       | DCS-BIOS UDP | SimAppPro, posing as FA-18C       | 1 Lua shim | none      | rejected           |
| C. Generated config | DCS-BIOS UDP | SimAppPro + rewritten bind config | none       | **yes**   | rejected           |

Why A won:

- **No SimAppPro at runtime.** Verified: we drove a lamp with SimAppPro closed,
  and separately with it running, without contention.
- **No elevation.** HID access needs no admin rights. B and C both did, because
  C writes into `Program Files` and B needed a Lua shim.
- **No DCS file edits.** DCS-BIOS is already installed; we only listen.
- **No F/A-18 translation layer.** That whole idea was an artifact of SimAppPro's
  `dcs_event_bind_config.js` only covering the Hornet fully. Writing HID directly,
  there is no intermediate namespace just LED indices.
- C would have been silently clobbered by SimAppPro updates, which we watched
  happen to an unrelated file mid-session.

**Hard dependency: DCS-BIOS.** It is the only signal source. Without it there is
nothing to read, and there is no fallback.

## Verified facts

Protocol and hardware detail is in `PROTOCOL.md`; the config model is in
`CONFIG.md`. The headlines:

- 14-byte HID vendor frame, part-addressed, every command acked.
- `SET_LEDX` is `0x49`. LED state latches **no host watchdog**, so the daemon
  writes only on change and must clear LEDs on exit.
- PTO2 indices 0/1/2/**3** are dimmers (0-255); 4-17 are indicators (**0 or 1
  only** writing 255 acks and lights nothing).
- **The PTO2 has three brightness groups, not one, and the vendor's table is
  missing one of them.** `SL` (2) hard-gates every indicator. `FLAG` (3),
  absent from `DeviceConfig.js` entirely, governs NOSE, LEFT, RIGHT, FLAPS,
  HALF, FULL and HOOK. `Backlight` (0) governs neither. Measured 2026-09-16
  after the A-10C flap lamps appeared dead: the engine was right, the writes
  acked, and `FLAG` sat near 0 where SimAppPro had left it. **A dimmer nothing
  writes is invisible state**, so the sweep must own every dimmer on the device.
- **Console lights off means daylight, not lamps off.** `FLAG` scales with the
  cockpit console dimmer and takes `off: 255`, so it goes full bright when the
  console reads zero. `scale` of a zero source resolves to zero and a binding
  that resolves to zero takes its `off` value, so the floor needs no new field.
- Config offset `0x114` persists both PTO2 dimmers to flash. **Never write it.**
- **A DCS-BIOS description's position order does not give the value order.**
  `FLAPS_SWITCH` is described "Flaps Setting DN - MVR - UP", which reads as
  0 = DN. Measured, it is **0 = UP, 1 = MVR, 2 = DN**, the exact reverse. Read
  positions off the stream, never off the description.
- **A signal's resting value is not necessarily zero.** A-10C `FLAP_POS` with
  the flaps fully up undershoots to 34, rebounds to **462**, and settles near
  138, though a second capture settled at 0. The rebound is *higher* than the
  first sample of real travel (283), so "retracted" and "just moving" overlap
  and no threshold separates them cleanly.
- **Analog gauges ring, and settle differently by direction.** `FLAP_POS` at MVR
  settles at 22726 arriving from retracted and 23411 arriving from DN, ringing
  out to 22415 and 23476. A threshold read off a single approach works in one
  direction and silently fails in the other.
  `crates/wctrl-engine/tests/flap_capture.rs` replays the real capture, lever
  values included.
- **A daemon started mid-mission syncs to the cockpit on its own.** DCS-BIOS
  re-exports on a cycle rather than sending deltas only: word 0 of `_ACFT_NAME`
  arrived 67 times in 20 seconds, about every 300 ms. Documented the other way
  round until 2026-09-16, which produced a README rule telling users to start
  before entering the cockpit. There is no ordering requirement.
- **`wctrl listen` takes repeated `--watch` by signal name.** Watching a gauge
  alone cannot say which detent it was travelling towards; watching the lever
  beside it, on one timestamped timeline, is what caught both errors above.
- **The DCS-BIOS listener sets `SO_REUSEADDR`.** Without it only one process on
  the machine can read the export stream, so `run` and `listen` could not be
  used together and neither could coexist with any other DCS-BIOS client.
- **A binding is a list of conditions, all of which must hold.** The lamp takes
  the dimmest value any condition asks for, which is boolean AND for on/off
  tests and leaves a scaled value intact for continuous ones. An empty list is a
  placeholder: it loads, validates, sweeps its lamp off, and never reaches the
  incremental path. Nothing about an unconfigured lamp is an error.
- **Nothing aborts the daemon.** A profile that fails to parse, names a module
  with no catalogue entry, or references an unknown signal is reported and
  skipped; the other profiles still run. An aircraft with no profile gets a stub
  written with every lamp listed and none assigned.
- Orion II part `0xbe60`: index 0 Backlight is a dimmer (0255); indices 1 (A/A)
  and 2 (A/G) are **binary** SimAppPro only ever sends 0 or 1. Backlight sets
  how bright they burn but does **not** gate them: at Backlight 0 they are still
  lit, just very dim. Unlike the PTO2, this device lights an indicator sent 255
  instead of ignoring it, so brightness looks identical at 1, 30 and 255. Write 1.
- SimAppPro does not clamp its Backlight field: 1255 went out as 231, the low
  byte of 0x4E7. A vendor defect, not a device range.
- **The two panels' governing dimmers differ, and the engine must not
  generalise.** PTO2 `SL` is a *master gate*: at 0, indices 417 stay dark no
  matter what they are sent. Orion II `Backlight` only *dims*: at 0, A/A and A/G
  are still lit, just faint. Both measured, not inferred.
- **Per-LED values latch beneath the governor.** With SL at 0, `CAUTION` was set
  to 1 and stayed dark; raising SL to 255 with no further write to `CAUTION`
  brought it up at full brightness. The governor is applied downstream of the
  latched value, so changing it never requires re-sweeping the LEDs it governs.
- DCS-BIOS allocates addresses sequentially, so a catalogue must be built from
  the _installed_ DCS-BIOS. Catalogues are stamped with its version.

## Where the code is

```text
crates/wctrl-hid      frame building, part discovery, SET_LEDX   (5 tests)
crates/wctrl-bios     export-stream decoder + address space      (5 tests)
crates/wctrl-config   catalogue, inventory, profiles, binding forms (19 tests)
crates/wctrl-engine   aircraft detection, sweep, incremental writes (24 tests)
editor/src-tauri      profile editor backend                     (1 test)
data/defaults         shipped profiles, tracked in git
data/profiles         active profiles, gitignored, seeded from data/defaults
editor/               Tauri 2 editor: vanilla TS + Vite, src-tauri in the workspace
crates/wctrl-cli      `wctrl`  devices/parts/led/blink/sweep/listen/catalogue/run
data/catalogue        50 modules, generated, version-stamped
data/devices.json     PTO2 and Orion II both verified
tools/                catalogue builder, HID probe, WWTHID log parser
```

Rust 1.98 MSVC. `hidapi` uses its `windows-native` backend, so no C toolchain
beyond the MSVC Build Tools already present. End users need nothing installed
Tauri renders through WebView2, which ships with Windows.

## Open threads

1. ~~The live DCS-BIOS stream is unproven.~~ Proven 2026-09-16 against running
   DCS: 402 datagrams, 9687 writes, 887 distinct addresses in 20 seconds.
2. ~~Orion II is entirely unmeasured.~~ Resolved 2026-09-16.
3. ~~Does SL govern PTO2 indicator brightness?~~ Resolved 2026-09-16: it is a
   hard gate, and per-LED values latch beneath it. See Verified facts.
4. **Profile inheritance** deferred, leaning no for v1.
5. **Backlight contention** the one lamp SimAppPro may also drive, if a user
   runs both with "Sync with DCS" on. Detect and warn.
6. **Perceptual response curve** for dimmers; linear PWM feels wrong at the
   bottom. Deferred.
7. **The MCDU Captain** (`0xbb36`) enumerates with 64-byte reports both ways,
   unlike every other panel here at 14, so its screen is presumably not driven
   by `SET_LCDS` in this form. Unmapped.

## Next steps

1. **Engine crate** aircraft-change detection from `_ACFT_NAME` (address 0,
   24-byte string), the single-sweep sync, then incremental writes. Needs no
   hardware.
2. **Prove the stream** against live DCS.
3. ~~**Tauri editor** scaffold.~~ Done 2026-09-16. Remaining work is listed
   under "Where the editor stands": binding editor, typeahead, conditions,
   hint box, learn mode.

## Method note

Several assumptions here were wrong and expensive: that indicators take 0-255,
that a vendor table could be trusted, that an ack meant an effect. The pattern
was asserting inference as fact and then reading failures as confirmation.

Capture beats inference on this hardware, and SimAppPro will tell us exactly what
it sends set `"HIDLog": true` in `%APPDATA%\SimAppPro\config.json`, restart it,
and read `%APPDATA%\WWTHID\SimAppPro\WWTHID.log` (see `PROTOCOL.md`). Fields that
have not been measured are left absent rather than given a plausible default, and
`verified: true` marks the ones that have.
