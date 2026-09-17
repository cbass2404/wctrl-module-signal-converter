# Project status

Written 2026-09-16. Enough context to resume cold.

## Resume here

Everything below is background. This is what to actually do next.

**Verify nothing has rotted** (30 seconds, no hardware, no DCS):

```powershell
python tools/build_catalogue.py   # required after a fresh clone - see below
cargo test --workspace            # expect 41 passing
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

2. **Fly it.** Author one profile under `data/profiles`, then, with a mission
   loaded:

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
open with an expand/collapse-all control. Lamp rows render every condition
stacked, not just the first, because a lamp commonly needs more than one.

**Not built yet, in the order it should be done:**

1. **Editing a binding.** Rows are read-only. This is the next piece and it is
   the whole point of the app.
2. **The signal typeahead**, specified in `CONFIG.md`. Three characters
   minimum, matching description, category and identifier, two-line rows.
3. **Add and remove conditions on a binding.** The A-10C half-flaps lamp needs
   the lever at MVR *and* the gauge inside the half window; without this the
   editor cannot express a profile the engine already runs.
4. **The usage hint** behind an info icon, on hover and on focus.
5. **Learn mode**, which needs the editor to read the DCS-BIOS stream.

**Untested:** the window has never been opened. Both halves compile and the
commands are thin wrappers over `wctrl-config`, but nothing has been clicked.

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
crates/wctrl-config   catalogue, device inventory, profiles      (7 tests)
crates/wctrl-engine   aircraft detection, sweep, incremental writes (19 tests)
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
