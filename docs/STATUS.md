# Project status

Written 2026-09-16, last updated 2026-09-18. Enough context to resume cold.

## Resume here

Everything below is background. This is what to actually do next.

**Verify nothing has rotted** (30 seconds, no hardware, no DCS):

```powershell
python tools/build_catalogue.py   # required after a fresh clone - see below
cargo test --workspace            # expect 207 passing
cargo run --bin wctrl -- devices
cargo run --bin wctrl -- catalogue --aircraft F-4E-45MC --find hook
```

`data/catalogue/` is **not in git**. It is generated from the DCS-BIOS installed
on this machine, and a catalogue from a different DCS-BIOS release reads the
wrong addresses silently, because addresses are allocated sequentially as
controls are defined. Rebuild it after cloning, and again whenever DCS-BIOS
updates it is stamped with the version it came from.

**Next session, first:** start the daemon and the editor with the MFD unplugged
while profiles bind it, and watch what each does. Expected from the code: the
daemon lists only enumerated panels and skips the MFD without error; the editor
keeps the MFD rows, since the merge never drops rows for an unplugged panel.
Unknown: whether the editor shows that the panel is absent.

**Then: DCS-BIOS version mismatch.** The shipped defaults were written against
DCS-BIOS `2026.09.18-nightly`. A user on another release, a stable one in
particular, may have signals the defaults name missing, renamed or changed.
Today that is all or nothing: `validate` reports `UnknownSignal` and
`load_profiles` skips the whole profile, so one renamed signal costs every lamp
in that aircraft.

Wanted: a default that names signals the installed DCS-BIOS lacks still loads.
The rows that can't resolve are left inert and every other row works. The user
gets one warning saying which rows are affected and which DCS-BIOS version gives
full functionality.

There are two different mismatches, and only one of them can degrade gracefully:

- **Defaults vs the installed DCS-BIOS.** The catalogue is built on the user's
  machine, so its addresses are correct for their DCS-BIOS. The only risk is a
  signal the defaults use that their DCS-BIOS doesn't have. This is the case to
  degrade gracefully. The defaults need to record the version they were
  written against, and profiles carry no version today.
- **Catalogue vs the installed DCS-BIOS.** This happens when DCS-BIOS updates
  and the catalogue isn't rebuilt. Every address can be silently wrong, and no
  per-row fallback can detect it. The catalogue already carries `bios_version`,
  but nothing reads it. Compare it at startup with the installed version (the
  one `build_catalogue.py` reads from `BIOSConfig.lua`) and refuse to run, or
  rebuild, when they differ.

To decide:

- Defaults only, or user profiles too? Skipping a user's whole profile over one
  signal is just as harsh.
- A signal can still exist but have changed meaning, for example a selector
  that gained a position. The name check won't catch that. Comparing
  `max_value` against the version the defaults were written for would.
- An `any_of` binding with one dead branch: drop the branch or the whole
  binding?
- The editor has to show inert rows as unavailable without removing them from
  the file, so they come back once DCS-BIOS is updated. This follows the same
  rule as the merge, which never drops rows.

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

4. **The ViperAce ICP (`0xbf06`).** Its DED screen protocol is decoded and
   confirmed on hardware 2026-09-18, see "Driving a pixel display" in
   `PROTOCOL.md`: report `0xf0`, a 200x64 1-bit framebuffer, write then commit.
   **Built and flown 2026-09-18.** Every page drew correctly, inverse fields
   included, and a disabled UFC was confirmed left alone in the jet.

   * `wctrl-hid` builds `0xf0` frames and splits them into 64-byte reports.
     Every write in SimAppPro's capture rebuilds byte for byte.
   * The DED is an ordinary display (`data/displays/ded.json`) with
     `"transport": "pixel"`. A pixel is a bit index, `y * 200 + x`, so the
     segment model fits unchanged: 120 cells generated from a `grid`, a font
     drawn as rows of `#`, and the usual screen diff, now per row. Changed rows
     go out as one write per contiguous run, then one commit.
   * A readout takes a `format` signal: `i` in `DED_Ln_FORMAT` draws that cell
     inverse, host side, as SimAppPro does. `exact_case` stops `a` (the arrow)
     being looked up as `A`.
   * 39 of 66 glyphs are captured from SimAppPro's frames; 27 are drawn in the
     same style and listed in the file. `tests/ded_render.rs` reproduces every
     captured frame's lines from the DCS-BIOS text, including an inverse one.
   * `data/defaults/f-16.json` maps `DED_L1..5` to the five lines, and the
     panel backlight follows `PRI_CONSOLES_BRT_KNB`.
   * The DED backlight (`Screen_Backlight`, index 1, capture-verified) is
     marked `lights_display`. A profile binds it like any lamp (reversed
     2026-09-18, so a screen can follow its cockpit brightness knob), but it
     lights only while the profile has fields on the DED and is 0 when nothing
     is drawn there. A binding not yet resolved counts as full. Every default
     holds it at full; it is outside the one-knob backlight rule. The UFC's
     `LCDBacklight` and the MCDU's `Screen_Backlight` work the same way.

   The brief flash on a page change is the panel's own; SimAppPro does it too.

   Both ICP lamps are verified on hardware: `Backlight` (0) followed the
   PRIMARY CONSOLES knob in the jet.

   Left to do: give the editor a way to pick a readout's `format`; replace
   drawn glyphs as captures turn up.

5. **Inventory the remaining devices.** The CarrierAce UFC + HUD (`0xbede`) is
   done, see below. So is the **CarrierAce MFD**, 2026-09-18: one dimmer,
   `INST_PNL_Backlight` at index 0 on part `0xbe0d`, captured across 0-255.
   SimAppPro renames an MFD L, C or R so several can be told apart, and each
   name is its own PID (C `0xbee0`, L `0xbee1`, R `0xbee2`), so
   `devices.json` carries three entries that differ only in PID and name, and
   every shipped profile binds all three. "1 Split 3" mode keeps the PID but
   enumerates three collections with only the first writable, which is why
   `Device::open` now chooses by report descriptor. Details in `PROTOCOL.md`.
   **Verified on hardware 2026-09-18** in the hardest case, split mode under
   the L name: `wctrl led` wrote 0, 255, 20 and 137 through `col01`, each
   acked, and the backlight went dark, full and dim as sent. Not yet flown
   from a profile in a mission.

   The **Orion Combat Rudder Pedals** (`0xbef0`) went in alongside, same day:
   `Backlight_L` 0, `Backlight_R` 1 and `Logo` 3, all dimmers from a capture.
   The logo is missing from the vendor table, and index 2, which nothing
   documents, turned out to write both pedal lights at once, last write wins.
   It is left out of the inventory so nothing ever writes it. Every default
   binds `Backlight_L` to the throttle's knob with the other two `same_as` it.
   Details in `PROTOCOL.md`.

   The **MCDU** (part `0xbb32`) went in the same evening, lamps from
   SimAppPro captures. Like the MFD it has three names, each its own PID:
   CAPTAIN `0xbb36`, CO-PILOT `0xbb3e`, OBSERVER `0xbb3a`, so it is three
   inventory entries. `Backlight` 0 joins every default's one knob.
   `Marker_Light` 2 is the gate for the nine indicators at 8 to 16, held like
   the PTO2's FLAG with a daylight floor of 255. The indicators are unbound in
   every default. `Screen_Backlight` 1 follows the screen rule, as the
   ICP's does. Captures run through `tools/tail_wwthid.py` now, because the log
   wraps within a minute.

   **The MCDU screen is a third kind of display, `text`.** Decided
   2026-09-18 that users run one application, so wctrl drives the screen
   too. SimAppPro never drives it from DCS, so the protocol is ported from
   WwDevicesDotnet (BSD-3) with its font upload, and the A-10C font comes from
   WCtrlDcsBiosBridge (MIT); notices in `THIRD_PARTY_NOTICES.md`, details in
   `PROTOCOL.md` under "Driving a text grid". **Drawn on our panel** with
   `wctrl mcdu-test`. In the engine a text grid is 336 cells of character,
   colour and size (`data/displays/mcdu.json`), sent whole on any change, and
   a readout takes `colour`, `small` and `replace` (one-for-one character
   swaps for DCS-BIOS's stand-ins). The font is the aircraft's, never the
   user's: `native_fonts` maps runtime aircraft name to font, and a profile
   putting fields on the MCDU for an aircraft without one is refused. `wctrl
   run` uploads the font on first paint and when it changes. Field strings
   are now read one byte per character (Latin-1), because DCS-BIOS sends CDU
   symbols as single bytes above ASCII.

   Aircraft with their own CDU font, each on all three MCDU names:
   * A-10C: `CDU_LINE0..9` on rows 5 to 14 in green, so the CDU's line
     select lines 3, 5, 7 and 9 sit beside MCDU rows 7 to 13 and the
     scratchpad on row 14. Flown.
   * CH-47F: 14 lines per seat with per-character colours. Flown.
   * F-14BU: the RIO's CDNU on rows 7 to 14, one column in, from either seat.
     Flown.
   * AH-64D: only the seated crew member's KU scratchpad, on row 14 one
     column in (2026-09-18). **Not yet flown.**

   Font selection for aircraft without a CDU comes with the field
   customisation ticket.

6. **The Tauri editor.** Scaffolded 2026-09-16, see below. Shape is specified
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

**Working:** the profile library (list, create, copy, reset), the new profile
flow (module, then which of its aircraft, blank or copied), and one collapsible
section per device, collapsed on open with an expand/collapse-all control whose
label follows the sections.

**Added 2026-09-18:**

* **The daylight floor is editable.** A dimmer's `off` is the **at zero** field
  in the Output column, beside **when lit** (`on`). It was reachable only by
  editing the file, which is how the A-10C shipped with `SL` tied to the
  backlight and no floor, blanking every indicator in daylight.
* **Cautions.** `devices.json` marks `SL` and `FLAG` with `governs`, and
  `Profile::cautions` reports a gate that resolves to 0 with every signal at 0.
  Shown in the editor without blocking Save, and logged by the daemon.
* **Generated profiles start with the gates held at full** and `off: 255`, so a
  new profile's indicators light and switching a gate to a dimmer keeps the
  floor. Every shipped default now meets the same rule.
* **One aircraft, one profile.** New profile and Copy to... move a claimed
  aircraft to the new profile and refuse a move that would empty another.
  `editor/src-tauri/src/claims.rs` holds the rule.
* **One naming rule.** `file_stem` in `wctrl-config` names every generated file,
  from the daemon, New profile and Copy to... alike. `NONE`, which DCS-BIOS
  reports when the player has no aircraft of their own, gets "No aircraft".
* **Confirmations are drawn in the window.** `window.confirm` showed nothing in
  the webview and answered yes, so Reset replaced a profile unasked.
* **Delete, for a profile the user made.** Offered where Reset is not, confirmed
  the same way, and refused by `Profiles::delete` for anything shipped, which
  would only be seeded back. It is how the leftover half of a split aircraft
  list is removed.
* **Long aircraft lists are cut at whole names** with a count, "+81 more" for
  FC3, and the full list on hover.
* **A panel the profile does not drive stays closed.** Its header click is
  cancelled, so it cannot flash open. Its lamps and fields are kept for when it
  is turned back on.
* **A disabled panel is never written.** The sweep and the paint honoured
  `disabled_devices` but the per-change path did not, so a bound lamp on a
  disabled panel moved with its signal. Fixed and confirmed in the jet with the
  UFC disabled under the F-16; `tests/disabled_device.rs` pins it.

Bindings are fully editable. A condition reads as a sentence until its pencil is
clicked, and an open condition carries keep, cancel and delete: cancel restores
it as it was when editing began, and delete confirms first. All four binding
forms are offered, each only where it can mean something:

* **conditions**, through the signal typeahead, its test and its values
* **any_of**, through "+ Add alternative (or)", with a choice between the
  brightest alternative and the one whose signal moved last (`pick`)
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

**Learn mode**, added 2026-09-17, is the one place in the editor that does I/O.
Press **Learn** beside any signal box, flip the control in the cockpit, and what
moved is listed with the most switch-like first. `wctrl learn` is the same thing
without a window.

The judgment lives in `wctrl-engine`'s `learn` module, which has no I/O and is
tested against a synthetic stream: ranking by movement count, reading each
signal through its own mask so a shared word does not name its neighbours,
counting a multi-word string as one movement, and treating a first sighting as a
baseline rather than a report. The editor backend adds a thread and a socket and
nothing else. It listens only while the panel is open, which is deliberate, and
`CONFIG.md` says why.

**Not built yet:**

1. **Renaming a profile, and editing its aircraft list in place.** Both are
   fixed at creation. An aircraft can be moved to another profile through New
   profile or Copy to..., which is the workaround. "Copy to..." on a profile row
   takes a name and an aircraft list and carries everything else over, module
   included. The module is not offered, because a copy whose
   signal ids resolve against a different catalogue is not a copy, it is a
   profile full of signals that do not exist. This is the FA-18E case made into
   a feature: the Super Hornet community mod reads the Hornet's DCS-BIOS
   definitions, so the Hornet profile drives it with only those two fields
   changed.
2. ~~**Validation before save.**~~ Done: the editor runs `Profile::problems`
   after every edit and withholds Save until there are none. See `CONFIG.md`.

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

**Still not confirmed on hardware:** `always` and `same_as`. Shipped profiles now
use both, `always` for the PTO2 gates in the F-14, Mi-24P, FC3 and No aircraft
profiles and `same_as` for both gates in the AH-64D, but neither has been
watched driving a real lamp.

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
* `data/defaults/fa-18.json` ships all 15 UFC fields.

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
3. **`--verbose` followed only lamp conditions**, so a display field printed its
   `paint` lines with nothing above them saying what had moved. `Trace` now
   follows readout sources too, and reads a string back out of the assembled
   state under every address it occupies, so it logs once when the field is
   whole rather than once per word.

**Profiles now self-update.** `Profiles::merge_new` runs at daemon startup and
adds rows for hardware a profile predates, from the shipped default first and
then as blank rows. It never touches an existing row, keeps rows for unplugged
panels, leaves an unparseable file alone, and is idempotent. Bindings are sorted
by device display name, then part in declared order, then hardware index.

**How it got here, and what is left.** Everything on this list is done except the last item, and each entry keeps what flying it taught, because that is the part that does not survive in the code.

1. ~~**A host-side shadow of the buffer.**~~ Done. `wctrl-config::display`
   has `Display`, `DisplayCatalogue` and `Screen`, checked against captured
   hardware traffic by `crates/wctrl-config/tests/display_render.rs`: rendering
   two real display states reproduces the exact 96 bytes the device was sent.
   Still to do is naming a display from a device spec so a part can carry
   one.
2. ~~**Fly it.**~~ **Flown 2026-09-17.** A profile with readouts drove the real
   glass from a live Hornet mission. String fields work end to end.

   Flying it found one fault, now fixed. A comm preset worked to 9 and then
   went blank. Cells 34 and 35 are two digits on one cell, and the vendor
   spells the tens as a prefix character rather than a digit: `` `2 `` for 12,
   `~0` for 20. DCS-BIOS sends `"12"`, which was not in the glyph table, and
   `paint` leaves an undrawable value blank rather than failing the batch. A
   display can now carry `spellings`, consulted only after the plain lookup
   fails, and `data/displays/ufc1.json` maps 10 to 20. Confirmed on the panel.

   Worth keeping, because it is the shape of the next one of these: the capture
   never went past preset 6, so there was no evidence for it anywhere. What
   found it was the vendor table being self-consistent in a way that only makes
   sense one way, `` `X `` being exactly `' X'` plus slots 6 and 7 for all ten
   digits, and holding exactly the 21 values a Hornet comm preset can take.

   The second half of the same fault showed up on the Hind. Cells 34 and 35
   hold **two characters**, and modules do not agree on how to pad a
   one-digit channel: the Hornet sends `" 2"`, the Hind sends `"1"` from a
   one-character field and `"1 "` from a two-character one. A cell now carries
   `width`, and a wide one trims and right aligns its value before the lookup.
   Before the lookup rather than as a rescue after it, because `'1'` is a real
   entry in the shared table: it is the ordinary glyph for cells 0 to 33, and
   its slots land on this cell's units digit, so taking it would draw a
   legible wrong answer instead of nothing. The slots divide with no overlap,
   units 1,2,3,4,9,11,13 and tens 0,5,6,7,15, which is what made that
   readable.

   SimAppPro's own fallback for an unknown pair is to OR `glyphs[v[0]]` with
   `glyphs[' ' + v[1]]` (`LCDControl.js`, `sendData`). **Do not copy it.** It
   is a generic rule for ordinary cells and it is wrong on these two: fusing
   `'1'` with `' 2'` lights slots 1,2,4,9,11,13 where `` `2 `` lights
   1,2,4,6,7,11,13. A test pins the difference.

   Lookup settled as **two tries: the form the cell prefers, then the bare
   trimmed value.** A wide cell prefers its full width. An ordinary cell
   prefers the spaced form of a single character, because a digit has two
   forms there and the only one ever captured is spaced, cell 0 reading
   `' 3'`. The bare fallback then rescues everything with no spaced form,
   which is every letter and mark: DCS-BIOS pads a string out to its
   `max_length` while DCS's own indication does not, so a scratchpad letter
   arrives as `" G"` and a guard channel as `" g"`. A digit never reaches the
   fallback. Seven-segment cells have no spaced digits at all and are served by
   it throughout.

   **Letters are drawn as capitals.** Uppercase is tried first and the value
   as sent second. This panel was built for the Hornet, DCS-BIOS reports the
   Hornet UFC in capitals throughout, and every letter in both captured pages
   is a capital, so the small forms were never exercised. They are real and
   distinct, `'g'` is four slots against eight for `'G'`, but the set is
   incomplete with no r, u, w, y or z, which is not what a font meant to be
   used looks like. The case this serves is another module naming a guard
   channel `"g"`, which should reach the glass looking like the rest of the
   panel. Falling back to the value as sent is what keeps it safe: `digit7`
   has a `'p'` and a `'w'` and no capitals at all, so uppercasing alone would
   have taken both off the glass.

   The generalisation that looked obvious, fit every cell to its width, is
   wrong, and the golden fixture is what caught it: it failed on `comm_page`
   cell 0 because trimming `' 3'` to `'3'` picks the other form. That fixture
   has now paid for itself twice.

   **Confirmed on hardware 2026-09-17**, on two modules the glass was not
   designed for, which is the whole point of putting the mapping in the profile
   rather than in code:

   * **Super Hornet.** The Hornet profile copied to `FA-18E`, keeping
     `"module": "FA-18C_hornet"` and changing only the aircraft list. The UFC
     is known not to work with the Super Hornet community mod under SimAppPro,
     and it works here. Nothing was written for it: `module` names the
     catalogue the signal ids resolve against and `aircraft` names the runtime
     aircraft served, and keeping those two separate is the entire reason a
     copied file was enough.
   * **Hind.** Cells 34 and 35 pointed at `PLT_R828_CHAN_S` and
     `PLT_R863_CHAN_S` drew two-digit presets correctly, from the editor.

   Both now ship in `data/defaults`, along with the A-10C, Apache, F-14,
   Mi-24P, FC3 and No aircraft profiles. `data/profiles` is the user's own
   directory and is not tracked.
3. ~~**A numeric source has never been flown.**~~ **Flown 2026-09-17.** The
   Hind radar altimeter, `PLT_RV5_ALT`, on cells 30 to 33 with
   `"reads": [0, 750]`, read consistently with the gauge in the cockpit.

   That is one gauge on one module, so it is evidence and not a proof, but it
   is the evidence that was missing. DCS-BIOS describes that signal only as
   `analog_gauge`, `"gauge position"`, 0 to 65535, and says nothing about what
   the face is marked with, which is why the range is the user's to supply.
   The open question it answers in part is whether 65535 is linear in the
   quantity or in needle angle. On a face that is linear in both, as this one
   is, the two cannot be told apart; a gauge with a compressed or non-linear
   scale would still read wrong, and there is no way to correct that without
   per-gauge data we have decided not to carry.
4. ~~**The editor cannot yet edit aliases or notes**~~ **Built 2026-09-17,**
   along with two things flying it made obvious:

   * **Fields are placed by name, not by cell run.** A display map now carries
     `regions`, taken from SimAppPro's own cell map for the Hornet, which
     accounts for all 36 cells with no gaps or overlaps. The editor offers
     "Option 5 label" where it used to ask for `30-33`, which is not something
     anyone deciding what to put on a panel can be expected to know. The run is
     still what gets stored, and a free-text box remains for part of a region
     or a display with no regions mapped.
   * **Substitutions are editable.** The `--` to `_` case was hand-written
     JSON before, and it is the one that bites: a value the glyph table cannot
     draw leaves its cell dark with nothing saying why.
   * **A field can belong to one crew station.** `seat` on a readout, matched
     against `SEAT_POSITION`. Two fields may share cells when their seats
     differ, and only then, which is how one window shows the pilot one thing
     and the gunner another. Offered only on the 5 of 50 modules that publish
     a seat, and rejected by `validate` elsewhere rather than accepting a field
     that could never paint. Until the seat is known the field stays dark,
     because the wrong station's reading looks correct.

   Still hand-edited: `note` on a field.

**Design call, open to revision:** the cell map is a property of the device and
lives in `data/displays`, but which signal feeds which cell is a property of the
aircraft and belongs in the profile. SimAppPro hard-codes the Hornet mapping in
a per-aircraft source file; putting it in the profile is what lets someone point
the glass at another module without a code change.

## Profiles ship from `data/defaults`

Changed 2026-09-16. `data/defaults` holds the profiles we ship, tracked in git.

**A new device lands in every default at once**, 2026-09-18. The ICP had gone
into `devices.json` and into only the F-16's default, and the startup merge
hid it by adding blank rows, so nothing failed and the lamp simply never lit
anywhere else. `tests/shipped_defaults.rs` now fails when any default lacks a
row for a profile lamp, and when the startup merge would add or reorder
anything, so a shipped file is already what a user's copy becomes.

**Every backlight in a default follows one knob**, by decision the same day,
so the whole pit dims together until a user splits it. `devices.json` marks
panel backlights with `backlight: true` (not the PTO2's gates, nor its
unidentified `Landing_gear_lights`), and the same test file fails when a
default's backlights resolve to different bindings, following `same_as`. The
Hornet and Super Hornet moved their UFC, MFDs and ICP from `INST_PNL_DIMMER`
to `CONSOLES_DIMMER` for it. **The Mi-24P is exempt, by name and with its
reason, until the Hind's backlight knob is found with learn mode**; remove
the exemption then. A new panel's backlight goes on that knob too.
`data/profiles` is the active folder the daemon reads and the editor writes; it
is gitignored and seeded from `data/defaults` on every start for any name not
already there. Seeding adds and never replaces. Reset is the only overwrite.

`Profiles` in `wctrl-config` owns this, and both the CLI and the editor call it,
so there is one implementation of the rule rather than two.

**Housekeeping:** `.gitignore` excludes `target/`, the generated
`data/catalogue/` and the user's `data/profiles/`, with the reasons recorded in
the file itself. SimAppPro's `HIDLog` is **off** again as of
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
- **Console lights off means daylight, not lamps off.** A gate that scales with
  the cockpit console dimmer takes `off: 255`, so it goes full bright when the
  console reads zero. `FLAG` needs it for its seven lamps and `SL`, harder, for
  all fourteen. `scale` of a zero source resolves to zero and a binding
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
  written with every lamp listed and none assigned, except the PTO2 gates, which
  start held at full.
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
crates/wctrl-hid      frames, part discovery, SET_LEDX, 0xf0     (10 tests)
crates/wctrl-bios     export-stream decoder + address space      (10 tests)
crates/wctrl-config   catalogue, inventory, profiles, displays    (83 tests)
crates/wctrl-engine   aircraft detection, sweep, writes, learn    (48 tests)
crates/wctrl-cli      the wctrl binary                            (5 tests)
editor/src-tauri      editor backend, learn listener, claims      (7 tests)
data/defaults         shipped profiles, tracked in git
data/profiles         active profiles, gitignored, seeded from data/defaults
editor/               Tauri 2 editor: vanilla TS + Vite, src-tauri in the workspace
crates/wctrl-cli      `wctrl`  devices/parts/led/blink/sweep/listen/learn/run
data/catalogue        50 modules, generated, version-stamped
data/devices.json     every connected panel verified, MCDU screen still unmapped
tools/                catalogue builder, HID probe, WWTHID log parser,
                      daemon benchmark (results in docs/PERFORMANCE.md)
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
7. ~~**The MCDU Captain** (`0xbb36`) screen is unmapped.~~ Resolved
   2026-09-18: it is a text grid over report `0xf2`, ported from
   WwDevicesDotnet. See "Driving a text grid" in `PROTOCOL.md`.

## Next steps

1. ~~**Engine crate.**~~ Done 2026-09-16.
2. ~~**Prove the stream** against live DCS.~~ Done 2026-09-16.
3. ~~**Tauri editor** scaffold.~~ Done 2026-09-16. Remaining work is listed
   under "Where the editor stands": renaming a profile and editing its aircraft
   list in place.
4. **Seeding by aircraft, not file name.** Seeding copies any default whose file
   name is missing, so renaming a shipped profile leaves existing installs with
   two profiles claiming one aircraft. Skip a default whose aircraft are
   already claimed.

## Method note

Several assumptions here were wrong and expensive: that indicators take 0-255,
that a vendor table could be trusted, that an ack meant an effect. The pattern
was asserting inference as fact and then reading failures as confirmation.

Capture beats inference on this hardware, and SimAppPro will tell us exactly what
it sends set `"HIDLog": true` in `%APPDATA%\SimAppPro\config.json`, restart it,
and read `%APPDATA%\WWTHID\SimAppPro\WWTHID.log` (see `PROTOCOL.md`). Fields that
have not been measured are left absent rather than given a plausible default, and
`verified: true` marks the ones that have.
