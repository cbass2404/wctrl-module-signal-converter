# Project status

Written 2026-09-16. Enough context to resume cold.

## Resume here

Everything below is background. This is what to actually do next.

**Verify nothing has rotted** (30 seconds, no hardware, no DCS):

```powershell
python tools/build_catalogue.py   # required after a fresh clone - see below
cargo test --workspace            # expect 19 passing
cargo run --bin wctrl -- devices
cargo run --bin wctrl -- catalogue --aircraft F-4E-45MC --find hook
```

`data/catalogue/` is **not in git**. It is generated from the DCS-BIOS installed
on this machine, and a catalogue from a different DCS-BIOS release reads the
wrong addresses silently, because addresses are allocated sequentially as
controls are defined. Rebuild it after cloning, and again whenever DCS-BIOS
updates it is stamped with the version it came from.

**Then, in order:**

1. **Build the engine crate** (`crates/wctrl-engine`). No hardware or DCS needed,
   and it is the last piece before a UI has anything to drive.
   - Detect aircraft change from `_ACFT_NAME`: address `0`, 24-byte string,
     decoded by `BiosState::string`.
   - On change, wait for DCS-BIOS's post-load flood to settle, then do **one
     sweep**: write every LED on every connected device, using `0` for any the
     profile does not bind. Not a reset followed by a sync one pass. Reasoning
     is in `CONFIG.md`.
   - After the sweep, write individual LEDs only as their source values change.
   - Clear every owned LED on shutdown and on mission end. LED state latches in
     the device, so a crash otherwise leaves the panel frozen mid-flight.

2. **Prove the live stream.** The decoder passes synthetic tests but has never
   seen DCS. With a mission loaded:

   ```powershell
   cargo run --bin wctrl -- listen --seconds 20
   cargo run --bin wctrl -- listen --seconds 30 --watch 2af8:1000:12   # F-4E hook lamp
   ```

   Empty output means multicast is blocked on the interface or DCS-BIOS is not
   exporting; the command says so itself.

3. **Measure the Orion II.** Three LEDs, nothing ever driven. Indices come from
   the vendor table, which was right for the PTO2 but is not evidence. Warn Cory
   to watch the throttle first, then `wctrl led --pid 0xbd64 --part 0xbe60
--index 1 --value 1`. Note the part id differs from the USB pid.

4. **Then the Tauri editor.** Shape is specified in `CONFIG.md`.

**Do not** start the UI before the engine the CLI exists precisely so every
layer can be exercised on real hardware before anything is wrapped in Tauri.

**Housekeeping:** the project is not yet a git repo. `target/` is ~330 MB and
needs a `.gitignore`. SimAppPro's `HIDLog` is currently **off**, which is how it
was found.

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
- PTO2 indices 0/1/2 are dimmers (0–255); 4–17 are indicators (**0 or 1 only**
  writing 255 acks and lights nothing).
- Config offset `0x114` persists both PTO2 dimmers to flash. **Never write it.**
- DCS-BIOS allocates addresses sequentially, so a catalogue must be built from
  the _installed_ DCS-BIOS. Catalogues are stamped with its version.

## Where the code is

```text
crates/wctrl-hid      frame building, part discovery, SET_LEDX   (5 tests)
crates/wctrl-bios     export-stream decoder + address space      (5 tests)
crates/wctrl-config   catalogue, device inventory, profiles      (9 tests)
crates/wctrl-cli      `wctrl`  devices/parts/led/blink/sweep/listen/catalogue
data/catalogue        50 modules, generated, version-stamped
data/devices.json     PTO2 verified; Orion II unverified
tools/                catalogue builder, HID probe, WWTHID log parser
```

Rust 1.98 MSVC. `hidapi` uses its `windows-native` backend, so no C toolchain
beyond the MSVC Build Tools already present. End users need nothing installed
Tauri renders through WebView2, which ships with Windows.

## Open threads

1. **The live DCS-BIOS stream is unproven.** The decoder passes synthetic tests
   but has never seen DCS. `cargo run --bin wctrl -- listen --seconds 20` with a
   mission loaded is the check.
2. **Orion II is entirely unmeasured** three LEDs, never driven. About a
   minute of work at the panel.
3. **Does SL (index 2) govern indicator brightness?** Stated from SimAppPro's UI
   and consistent with observation, never isolated.
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
3. **Tauri editor** LED-keyed rows, searchable signal dropdown, value input
   constrained to what the chosen signal can report, and a learn mode that
   watches the stream while the user flips a cockpit switch.

## Method note

Several assumptions here were wrong and expensive: that indicators take 0–255,
that a vendor table could be trusted, that an ack meant an effect. The pattern
was asserting inference as fact and then reading failures as confirmation.

Capture beats inference on this hardware, and SimAppPro will tell us exactly what
it sends set `"HIDLog": true` in `%APPDATA%\SimAppPro\config.json`, restart it,
and read `%APPDATA%\WWTHID\SimAppPro\WWTHID.log` (see `PROTOCOL.md`). Fields that
have not been measured are left absent rather than given a plausible default, and
`verified: true` marks the ones that have.
