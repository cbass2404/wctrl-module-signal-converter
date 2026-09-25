# Project status

Written 2026-09-16, last updated 2026-09-25. Enough context to resume cold.

## Resume here

Everything below is background. This is what to actually do next, with the
reasoning attached. [TODO.md](TODO.md) is the same outstanding work as a bare
checklist, for when that is all that is wanted.

**Verify nothing has rotted** (30 seconds, no hardware, no DCS):

```powershell
cargo test --workspace            # expect 481 passing
cargo run --bin dcs-signal -- devices
cargo run --bin dcs-signal -- catalogue --aircraft F-4E-45MC --find hook
```

`data/catalogue/` is **not in git**. It is generated from the DCS-BIOS installed
on this machine, and a catalogue from a different DCS-BIOS release reads the
wrong addresses silently, because addresses are allocated sequentially as
controls are defined. Nothing needs doing after a clone: every command that
reads the catalogue builds it first if it is missing or out of date (see below).

**Built 2026-09-25: page fields open one at a time, and two ways back.**
Built and type-checked, not yet clicked through. What users see is in
[CHANGELOG.md](../CHANGELOG.md).

- **A page field is closed until its pencil is clicked**: the signals it
  reads and its preview. Tick keeps, cross restores the copy taken when it
  opened, the lamp condition pattern. Which fields are open lives on the
  page book's `Editing`, so a section redraw keeps them open; Save page
  closes them.
- **Shipped pages reach the editor.** `open_pages` returns `shipped`, the
  module's pages from `pages.defaults`, and a field finds its shipped self
  by cells and seat, as profile fields used to.
- **Saved versions are tied by identity**, not cells: `Editing.saved` maps
  each working field to its field in the last saved baseline, carried across
  a reset or a cancel, so a field moved to other cells still undoes to its
  saved self. A deleted one is offered back in its empty area.
- **Lamps got the same undo**: `Session.saved` holds each binding as last
  saved, and a save redraws every lamp's footer through `afterSave`.

**Built 2026-09-24: the editor keeps things in reach, and a shipped profile
can split.** `80a1123` to `a87bb32`. What users see is in
[CHANGELOG.md](../CHANGELOG.md); the split's rules are "Splitting a shipped
profile" in [CONFIG.md](CONFIG.md).

- **Panels grouped by plug state**: Active Devices, Inactive Devices and
  Devices not found, each alphabetical. The device poll moves a panel between
  groups and keeps unsaved edits.
- **The profile list**: the row opens the profile; Copy to..., Export...,
  Merge from..., Reset and Delete are under the row's menu.
- **`docs/language.html`**, the profile language guide, opens from the **?**
  through the fixed-URL `open_guide` command. Any change to profile logic
  updates its prose, demo and dictionary in the same change.
- **The page editor stays open** after Save page, and Save as new page carries
  on with the copy. Its buttons stick to the foot of the window, and an open
  device's title sticks under the header.
- **The A-10C split.** `a-10c.json` flies `A-10C` only and the new
  `a-10c2.json` flies `A-10C_2`, because the two want different radios on the
  CDU rows. `a10c-cdu`, renamed "A-10C2 CDU", stays with the A-10C II; the
  A-10C has "A-10C CDU", reading VHF AM. That page's id, `i63dn3`, was made in
  the editor, which is how shipped pages get their ids from now on (see
  "Shipped ids" below).
- **PTO2 on both A-10C profiles** shows the NMSP EGI, STEER PT, TCN, ANCHR and
  ILS lamps on CTR, LI, LO, RI and RO, in place of the fire lamps. The rows'
  `note`s say so (2026-09-25).
- **The F-14BU's ICP is no longer disabled.** It came out of
  `disabled_devices` with the move of the UFC and DED fields onto pages
  (`6c05d7a`), so the DED now shows a Blank slot. Kept that way (decided
  2026-09-25).
- **alpha.008's changed rows are written up** in CHANGELOG.md, per profile and
  per page module, from a diff of `data/defaults` and `data/default-pages`
  against their `-previous` snapshots.

**Built and flown 2026-09-24: pages on the UFC and the ICP.** Cory reversed
"text grids only": every screen now takes pages, with six slots each and the
same app-wide modifier. The design is "Pages" in [CONFIG.md](CONFIG.md), the
section formerly "MCDU pages". Flown the same day: pages on the UFC and the
ICP's DED swap from the mapped page keys, with the modifier chosen in
Settings and only with that one. Since seen on the panel: a blank slot takes
these screens dark, a disabled slot's key does nothing, and the A-10C CMSC
and Mi-24P Radios pages show on the glass.

- **The gate is any known display.** `Profile::takes_pages` is
  `displays.get(display).is_some()`, so nothing names a panel or a glass
  type. Errors renamed to match: `LooseScreenField`, `SlotsWithoutScreen`,
  `PageOnUnknownDisplay`.
- **Loose fields refused everywhere, no migration** (Cory, 2026-09-24):
  nobody had updated past the version 2 break yet, so this lands as part of
  it. An unchanged shipped profile updates cleanly (its UFC/DED rows are
  removed as no longer shipped and the slot comes in); an edited one is
  refused with the reason. Checked with a throwaway test against
  `defaults-previous`.
- **Shipped pages added**, names agreed with Cory: A-10C "CMSC"
  (`a10c-cmsc`, DED), F-16C_50 "DED" (`f16-ded`), FA-18C_hornet "UFC"
  (`fa18-ufc`), Mi-24P "Radios" (`mi24p-ufc`, a new page file). Each in slot
  1 and the start slot, slots 2 to 6 disabled.
- **Page keys captured** with `dcs-signal buttons`: ICP COM 1 to A-G are
  buttons 1 to 6, UFC A/P to BCN are 20 to 25. In `devices.json` as
  `buttons` and `page_keys`, named `COM_1`, `COM_2`, `IFF`, `LIST`, `A_A`,
  `A_G` and `A_P`, `IFF`, `TCN`, `ILS`, `D_N`, `BCN`.
- **Editor**: every display gets a page section; `displaySection` and the
  session's shipped readouts are gone. Reset this field and "+ the field
  that shipped here" came back on 2026-09-25, fed from the shipped pages.
- **Test fixtures** in `editor_checks.rs` and `seat_validation.rs` mark their
  fields as a page's, the way the engine tests' `resolved()` does.

**Built 2026-09-24: the PFP-3N, PFP-7 and PFP-4, from WwDevicesDotnet.** Cory
owns none, so nothing is captured: PIDs, parts, lamps and keys are the
library's, taken from Cory's checkout at `C:\Users\coryb\Dev\WwDevicesDotnet`
(`2bf28fa`, the bridge's pin), and everything is `verified: false`. Cory's
rule, 2026-09-24: **only pages are shared** between the MCDU and the PFPs,
because the screens are identical; lamps and keys belong to each model; the
three names of one model follow each other as the MCDU's do. See the PFP
section in [PROTOCOL.md](PROTOCOL.md).

- **A display map no longer carries a part id.** `devices.json` already says
  which part carries each screen (`display` on the part), and live writes
  always took the part id from there, so the map's own `part_id` only fed
  `mcdu-test` and an invariant. Removed from `Display` and the three maps;
  `mcdu-test` now finds the part from `--pid` in `devices.json`, so it
  works on a PFP too. That is what lets four parts share `MCDU`, and so its
  pages, with no compatibility table.
- **`same_hardware` compares keys too.** The PFP-3N, PFP-7 and PFP-4 have the
  same five lamps, so on lamps and screen alone they counted as one panel and
  could follow each other. `the_cdus_share_a_screen_and_nothing_else` pins
  the rule.
- **31px fonts on the PFP for now.** Its glass fits 32px, which the bridge
  uses to line rows up with the keys; that needs a per-part glyph height and
  32px copies of every font, and cannot be checked without a panel.
- **Every default** has the PFP rows (dimmers copied from the MCDU's seat,
  indicators blank), the MCDU's slots on each PFP name, Co-Pilot and
  Observer following their own Captain, and no-aircraft disabling them. Held
  out of the feature commit, per the defaults rule.

**Built and flown 2026-09-23: page swapping and the Settings dialog, on
`feature/mcdu-page-selection-inputs`.** The design is "Swapping" under "MCDU
pages" in [CONFIG.md](CONFIG.md). Flown in the A-10C with pages in the odd
slots, blanks in the even ones and the last disabled: every combination of
Ctrl, Shift and Alt, left and right, swapped only when intended, and
changing the modifier in Settings mid-flight took effect. The three themes
redraw the window as picked, which was the first look at the light theme.
The theme survives a restart, Import profile... and Manage Converter... open
from the gear, import refuses a version 1 profile, and a partial merge from
an F-14BU export brought its page slot into the F-14. Nothing on the branch
is waiting on a check.

- **Inputs belong to their device** (Cory, 2026-09-23). The three MCDU
  entries in `devices.json` list `buttons` (LSK 1L to 6L as 1 to 6, LSK 1R as
  7) and `page_keys` (LSK 1L to 6L). `DeviceInventory::load` refuses a page
  key naming no button, and a name or number listed twice.
- **Slots come from the device.** `SLOTS = 6` is gone, in Rust and in
  `pages.ts`: `DeviceSpec::slot_count` is the number of page keys, or 1, and
  the editor labels each slot with its key's `label`, sent in `DeviceView`.
- **Every slot is resolved up front.** `with_pages` fills
  `Profile::page_runs` (never written) with each slot as `SlotRun::Off`,
  `Blank` or `Page`, and `with_followers` gives a follower the leader's under
  its own name. `Profile::show_slot` swaps a device's page fields (the
  readouts with `page` set) for another slot's; `reset_pages` goes back to
  `start`. The engine's `show_slot` calls it and paints, `Cause::PageSwap`;
  `select_profile` resets, and `set_profiles` keeps the slot each screen
  showed. Tests: `dsc-engine/tests/page_swap.rs`,
  `dsc-config/tests/page_keys.rs`.
- **The daemon reads keys.** `page_keys.rs` in `dsc-cli` starts one thread
  per connected device with page keys, on the collection that declares
  buttons, and sends each key going down with `keyboard::held_now()`. The
  loop maps number to slot, checks `Modifier::alone_in`, and applies the
  engine's batch. The modifier is read from `settings.json` at start and on
  the profile poll.
- **Settings.** `dsc_config::settings` holds `page_modifier` and `theme` in
  `settings.json` beside the profiles (`Paths::settings`; in development
  `data/settings.json`, gitignored). The editor's Profiles header is now New
  profile and a gear; the gear's dialog (`editor/src/settings.ts`, built as
  `dialog.picker confirm` like Manage Converter) holds the theme, the
  modifier, and Import and Manage Converter, which it hands on to once it
  closes. The theme sets `data-theme` on `<html>`, read before the first
  screen; `styles.css` has the forced-dark block beside the OS one.

**Built 2026-09-23: MCDU pages, on `feature/mcdu-page-profiles`.** The design
is "MCDU pages" in [CONFIG.md](CONFIG.md); this is where the build stands.

- **The shipped defaults are on pages** (names agreed with Cory,
  2026-09-23). Every default is version 2. One page per module in
  `data/default-pages`, each in slot 1 and the start slot on all three MCDU
  names, slots 2 to 6 disabled: A-10C "CDU" (the radio rows included),
  AH-64D "KU", CH-47F "CDU", F-14 "CDNU" (used by `f-14bu.json` only; the
  F-14's own profile has no slots, since the CDNU needs the nightly),
  F-16C_50 "Flight" and FA-18C_hornet "IFEI". Profiles with no MCDU content
  have no `screens`.
- **Shipped ids**: the first pages were given readable ids by hand
  (`a10c-cdu`, `ah64d-ku`, `ch47f-cdu`, `f14-cdnu`, `f16-flight`,
  `fa18-ifei`), and those stay, since renaming a shipped id reads as one page
  deleted and another added. New shipped pages keep the id the editor gives
  them, six letters and digits with no hyphen (Cory, 2026-09-25: hand-naming
  every page does not scale).
- **The pages came from the Captain's rows.** The followers' own MCDU rows
  were dropped rather than kept as pages: two of them were stale copies (the
  A-10C's without the radio rows, the AH-64D's with the old KEYBOARD UNIT
  rule), and the rest matched the Captain. A page holds both seats' fields
  itself, as the AH-64D and CH-47F do, so no seat needed a page of its own.
- **Checked with a dry run and the tests**, and since seen on the panel:
  every default loads and validates against the pages with no caution, and
  each page file looks on the MCDU as it did before the move.
- **Where it lives.** `dsc-config`: `pages.rs` (library, slots, resolving the
  start page, seeding and update), `bundle.rs` (export and import), slots in
  `merge.rs`. The editor: `editor/src/pages.ts` for the screen section,
  `editor/src-tauri/src/pages.rs` for the commands. `fieldTable` in
  `readout.ts` is the field editor both screens and pages use.
- **Resolved at load, not in the engine.** The daemon validates a profile
  against the library, then `with_pages` turns each screen's start page into
  ordinary fields marked with the page's id, before `runnable` and
  `with_followers`. The engine never learned about pages.
- **A slot is disabled, blank or a page** (Cory, 2026-09-23): null,
  `{"page": null}`, or `{"page": id}`, so the future key watcher can tell
  "ignore the key" from "take the screen dark".
- **A page saves on its own** (Cory, 2026-09-23): the section shows only the
  six slots until Edit page or New page opens the field editor, and Save page
  writes the library. The profile's Save writes the slots.
- **Clicked through in the window**: the slots, Edit page and New page,
  Save page, Save as new page, Delete page, and export, import and merge
  with pages.
- **Test fixtures** holding MCDU fields are version 2 and marked as a
  resolved start page (`resolved()` in the engine tests), since the fields a
  page puts on the MCDU are ordinary fields once resolved.

**Built 2026-09-22: the panels stay lit through a quiet stream while DCS runs.**
Opening the options or controls menu pauses DCS-BIOS, and after 20 seconds of
that the daemon used to clear every lamp and screen and rebuild them on the way
back. A quiet stream alone now clears nothing; the panels keep the last cockpit
until a new aircraft loads, and clear only once `DCS.exe` has gone. Since
watched in DCS: the panels stay lit through the options menu and clear when
DCS quits.

**Flown 2026-09-22, on the panels with DCS feeding them:** the F-16's MCDU
flight page (fuel from the totalizer drums through round down and wrap, the
CMDS mode as coloured aliases, trim as magnitude with a direction band), the
A-10C's MCDU radio rows and the countermeasures page on the DED with its
inverse blocks, and a per-seat copy of a display field.

**Validated on the panel 2026-09-21: the Hornet's IFEI rebuilt on the MCDU.**
The Hornet has no CDU of its own, so the MCDU is free glass, and the IFEI is
the densest thing in that cockpit worth copying: two engine columns of five
readings each around a column of labels, fuel remaining and bingo, and the
clock. Built entirely in the editor, with DCS feeding it, and photographed in
[editor/demos/mcdu-custom-ifei.jpg](../editor/demos/mcdu-custom-ifei.jpg) (the
site page shows a video of it running instead). What it proved:

- **Numbers shown as sent and numbers converted, side by side.** RPM,
  temperature, fuel flow and oil arrive as the digits the IFEI draws. The
  nozzle positions are needles, 0 to 65535, and read 76 because they were
  converted to 0 to 100, which is how the cockpit gauge is marked. This is the
  change that made numbers on a screen work the way a lamp's test does: pick
  the signal, then say how to show it. Before it, every number was forced
  through 0 to 100, which would have scaled every digit reading on this page.
- **Fixed-width pieces hold columns.** Each engine value is boxed and aligned
  right, so the left and right columns stay put as readings change width, and
  the labels between them stay centred on their row.
- **A chain can mix readings and typed text on one line.** The clock is three
  readings with two typed colons, and the right-hand column labels (MD, QT, UP,
  DN, ZN, ET) are typed text boxed to three cells so they line up.
- **Small and large text, and colour per piece**, on one row: white labels in
  the small font, amber readings, green column labels.

It shipped as the Hornet's MCDU Captain page in 1.0.0-alpha.004.

**Built 2026-09-22: the UFC and the DED are previewed too.** The editor drew a field before it was flown only on the MCDU,
where a cell is a character of an uploaded font. The other two screens are
where a preview is worth more, because a value there lights a set of segments
or pixels that need not resemble the characters it was keyed by: a bare digit
and a spaced one are different glyphs, a comm channel is two characters on one
cell, and the DED spells its arrow with a lowercase `a`. The lit slots are
asked of the backend per cell, so the same lookup the daemon paints with
answers the preview, and only the drawing happens in the window.

What the drawing needed, and what it costs:

- **A slot has no shape anywhere in the maps.** A buffer of bit indices says
  which slots a glyph lights and nothing about where they are. The DED is
  spared this, its slots being pixels of a cell the grid generated, but the
  UFC's are segments, so `art` in `data/displays/ufc1.json` gives each slot a
  stroke. Which slot is which segment was read out of the glyph table itself
  rather than captured, and the derivation is written down in both the file
  and "Where each segment sits" in [PROTOCOL.md](PROTOCOL.md). Checked against
  the glass 2026-09-25: every segment sits where the preview draws it.
- **One layout, two painters.** The measuring the daemon does was already
  copied in the window for the MCDU; it is now `layoutCells`, and the font and
  the slots are two ways of painting what it produces. It picked up the
  daemon's one cell rule on the way: a single cell field takes the whole line
  as one glyph, which is what a comm channel needs and what a text grid takes
  the first character of.
- **Nothing is invented.** A cell the glyph table has nothing for is marked
  the way a missing font glyph already was, and the line under the preview
  names the values that would leave a cell dark. That is the check these two
  screens never had: `alphabet` only ever answered for a font.
- **Each glass in its own colours**, from `glass` on the display map, a
  `ground` and an `ink` (2026-09-25, from looking at the panels). The UFC has
  none and stays white on black: it lights its segments green, one colour,
  and white stands in well enough. The DED is black on green, and an inverse
  block green on black, which `paintInk` gets for nothing: an inverse cell's
  lit set is the box with the glyph knocked out.

**Built 2026-09-21, not yet on a panel: one panel under several names shares a
setup.** Rebuilding the IFEI showed the cost of the MCDU being three devices: it
was built on the Captain, and a Co-Pilot or Observer unit would need it all
again, as the three MFDs need every backlight row three times. `follows` on the
profile points one device at another of the same hardware, and the follower is
driven with a copy of that device's rows. See "One panel under several names"
in [CONFIG.md](CONFIG.md). The decisions:

- **Resolved once, when the engine takes a profile** (`Profile::with_followers`),
  so painting, sweeping and resolving never learn a second kind of device.
- **"Same hardware" is worked out, not listed.** `DeviceSpec::same_hardware`
  compares lamps by index, name and max and the displays per part, ignoring
  part ids and PIDs, which differ between the MFDs. A variant added to
  `devices.json` qualifies with nothing else said, and the editor gets the list
  from the backend (`DeviceView::variants`) rather than keeping its own idea.
- **One step deep**, so there is one place to edit. The editor does not offer a
  follower as a target and locks the chooser on a device something follows.
- **The follower's own rows are kept and ignored**, as a disabled device's are,
  and `inert()` says so.
- **Per profile, not global.** A profile that wants its panels set up apart
  keeps them apart; one global setting would take that away.

Not yet done: flying it on two MCDUs. Every shipped default uses it, by
decision 2026-09-21: the MFD L and R use the MFD C and the MCDU Co-Pilot and
Observer use the Captain. The AH-64D and CH-47F lose nothing by it, because the
seat on each field picks the source and all three MCDU names carried the same
fields.

**Built and proven on the panels 2026-09-21: a lamp can match one on
another panel.** Every default puts its backlights on one knob, and until now that
meant writing the same condition into every panel's backlight row, so moving
the knob was a change made once per panel. `same_as_device` beside `same_as`
names the device the target lamp is on; left out, it is the lamp's own device,
so every existing profile reads the same. See "Matching another lamp" in
[CONFIG.md](CONFIG.md). The decisions:

- **A field beside `same_as`, not a new shape for it.** A string that became
  an object, or a `device/lamp` path, would have changed every row that already
  mirrors something, and lamp names can hold a slash (`A/A`).
- **The chain rule is unchanged, only wider.** The target must read signals of
  its own, now looked for on whichever device it names, so a loop across two
  panels is refused at both ends with no cycle detection added.
- **A device that follows another is read through it** (`mirror_target`), since
  its own rows are not in use. That also catches a lamp pointed at itself
  through a follower, as a chain.
- **The editor lists every panel's dimmers**, its own first, and none from a
  panel that follows another. The list is asked for on each draw rather than
  once, and matching is not offered on a lamp something else already follows,
  so the window cannot build the loop the check would refuse.

Every shipped default moved onto it 2026-09-21. On 2026-09-22 the one row
they all follow became a fixed 175 rather than a cockpit knob, and the test
that held every backlight to one source was deleted with every other test that
read the shipped defaults; see "Profiles ship from `data/defaults`".

**Built and flown 2026-09-20: the editor's display fields, put right.** Six
things, found by using the window rather than by reading it, and one of them
was a feature that had never worked.

- **"+ a reading" handed you a text box.** A piece said which of the two it was
  by whether `source` held anything, so a piece nobody had pointed at a signal
  yet was text by definition, and choosing "a reading" in the menu put it back
  to text on the next redraw. The key being present is what says which it is;
  whether it has been filled in is the profile check's question, and it already
  asks it. Nothing writes an empty `source` onto text, so the two cannot be
  confused: the backend drops the key when it is empty, and every piece the
  window builds goes through `newSpan`.
- **"+ another in <row>" is gone rather than fixed.** It predates chains. It
  guessed at free cells inside a region and handed out ones the row already
  held, so it only ever produced a field that said "already taken". Two
  readings on one line is what a chain is for, and a chain can count. The one
  thing lost with it is adding a second field to a row for a different seat,
  which the cells box still reaches and which nothing shipped uses.
- **Closing the window never asked.** The back button always did, so the way
  to lose an evening was to close the window, which is how most people leave an
  app. Tauri holds the window while a JS listener is registered and closes it
  once the handler returns without objecting, so this is a listener and one
  added permission rather than anything in Rust.
- **A field could not be put back.** Lamps have had a reset from the start.
  Fields had none, and deleting one was the worse half: there was then nothing
  on the screen to say a field had ever been there, and the way back was
  resetting the whole profile. The shipped copy was already loaded for the lamp
  resets, so this is the same data read a second way. Telling a changed field
  from an untouched one needs the same normalising the update reconcile needs
  and for the same reason, since a field touched in the window is a chain until
  the backend writes it flat again; `fieldShape` is that, and it mirrors each
  key's `skip_serializing_if` rather than dropping everything falsy, because
  seat 0 is a real answer.
- **A rule can carry a label**, which is the feature. The colour being its own
  is the whole of it: a label drawn in the line's colour reads as part of the
  line. Unset it follows the rule, so a label added to a green rule does not
  arrive white, and the editor offers "same as the rule" as a choice rather
  than leaving that as the only behaviour. A label with no room is refused
  rather than crowded in, since the blanks each side and a dash each side are
  what make it a labelled rule instead of a broken one, and `divider_rule`
  leaves an unfittable label off, which would otherwise be silent.

- **Typing in a text piece lost focus after every character**, and this one
  was worth chasing rather than patching. Every edit called the chain's
  `redraw`, which empties the chain and builds it again, so the box being typed
  into was thrown away and replaced by an identical one holding the right text.
  A keystroke changes what the field draws and how wide it comes out; it does
  not change how many pieces there are or what kind each is, which is all the
  chain's shape depends on. So `refresh` updates those two things and `redraw`
  is kept for what really does restructure: adding, removing, moving, changing
  a piece's kind, picking a different signal, and toggling small, which changes
  the alphabet the typed text is checked against.

  Leaving the pieces alive between keystrokes exposed what the rebuilding was
  covering up. `spanEditor` derived its own `spans` from `contentOf`, and on a
  field still in the flat shape that returns a fresh array of fresh objects
  every call, so the piece it was handed was not the piece in the chain. Every
  handler had to write a whole replacement into `spans[index]` rather than
  change what it had, and two handlers on one piece each built their
  replacement from the same stale copy, so the first one's work was lost. It
  was only ever right because the redraw rebuilt the closures immediately after.
  It is handed the array now and edits what is in it, which is what made the
  rest of this possible.

The label is the first thing a divider draws that is not fixed, so it goes
through the font check like any other characters. It rides in the flat shape
beside the rule's colour and is dropped from anything that is not a divider,
for the reason the colour is: kept, it would be a setting the window never
shows and nothing ever draws.

**Flown the same day, on the panel with DCS feeding it**: a chain built on the
A-10C's free rows from the A-10C II fuel strings, pieced together with typed
text, labels put on rules, and every one of the fixes above walked through. It
works.

**Flying it settled one thing that reading it had not.** The rule was inset by
a blank cell at each end, which was reasoned about here and never looked at.
On the glass beside real CDU lines, which start in the first cell of their run,
that inset made the rule the one thing on the screen not lining up with what
sat above and below it. It fills its run now, corner to corner. The two blanks
around a label stay, because those do a job: they are what keep the label from
reading as part of the line.

That took the last reason for a minimum width with it. There is no run too
narrow for a rule any more, since one cell is one dash and a `CellRange` is
never shorter than that, so `MIN_DIVIDER_CELLS` and the refusal that used it
are gone rather than left as a check that can no longer fire. A label's own
minimum, its width plus four, is the only one left.

**Two tests were pinning profile content rather than behaviour**, and one of
them failed the moment a rule was labelled while flying. A label differs per
module and changes whenever somebody decides a page is better named, so the
shipped-default tests check that a row is ruled and how far the rule runs, not
what it says. Anything about a label's own drawing sets one up itself.

**Built 2026-09-20, not yet on a panel: an update corrects a field nobody
changed.** Never rewriting anything kept user work safe and quietly froze every
shipped field: a correction reached nobody who already had that profile,
including somebody who had never opened it. Nothing recorded what a field said
when it shipped, so a field sitting exactly as delivered and one somebody had
spent an evening on were the same thing to look at.

`data/defaults-previous` is that record, the defaults as the last release
shipped them, and a field is ours to correct only while it still matches it
exactly. The five cases and the reasoning are in "Correcting a display field an
update changed" in [CONFIG.md](CONFIG.md). What is worth keeping here is why
each decision went the way it did, because three of them were the second
answer:

- **One snapshot, not a history of them.** Keeping every version removes the
  ratchet where somebody who skips a release is frozen for good, and buys it by
  overwriting anybody who deliberately went back to an older shipped layout.
  A frozen field still works; an overwritten one is lost work. Cory's call, and
  the right one.
- **Once per version, not once per start.** This is the decision the design did
  not survive without. `merge_new` runs at both the daemon and the editor
  starting, so with one snapshot and no gate, somebody who preferred the old
  field and put it back would match the snapshot again and have it taken away
  at the next launch, and every launch after that. They could never keep it.
  A marker file naming the version that last reconciled the folder is the whole
  fix. It also makes deleting a field stick immediately rather than at the next
  release.
- **The cells are part of what identifies a field.** So a field that moved rows
  reads as one retired and one arriving, and is drawn once at its new row. The
  alternative was the duplicate that the additive merge would have produced,
  which is what prompted looking at this at all.
- **Removal is real, and it is new.** This is the first code in the project
  that takes content out of a file the user owns, running unattended at
  startup. Everything around it exists to keep that honest: it only ever
  removes a field byte for byte identical to one we shipped, only on a version
  change, and never in a checkout. The failure direction is safe in every case
  - a false non-match declines to act, and a false match can only happen when
  the content is already identical to ours - and
  `crates/dsc-config/tests/profile_reconcile.rs` pins all fifteen branches,
  hardest on the ones that delete.
- **The snapshot follows `--defaults`.** `Profiles::new` derives it as the
  sibling folder named `-previous`, rather than each layout naming it, because
  the daemon lets `--defaults` point anywhere and a snapshot read from the
  install while the defaults came from somewhere else would compare two
  unrelated things.

**The release step is the part that can rot**, since a human has to refresh the
snapshot at the right moment. `tools/snapshot.py --check` runs in `release.cmd`
before the tag, confirming the snapshot still holds the previous release, and
the refresh runs after the push and is left unstaged: until the pipeline is
green there is nothing worth committing. Drift is silent at runtime, since no
field matches and nothing is corrected, which is why it is caught there.

One thing found while building rather than planned: `data/defaults-previous`
was not in `tauri.conf.json`, so an installed build would have shipped with no
snapshot and corrected nothing, silently.

**alpha.003 does have work to do**, which was not true when this was written.
The defaults had not moved since alpha.002, so the first upgrade was going to
be a no-op and the tests were the only proof of anything. Labelling the
Apache's rule was Cory's answer to that: a deliberate change to a shipped
default, made so the first upgrade has a real correction to carry rather than
shipping the mechanism untried. It is one field on three MCDU names, still
exactly as alpha.002 shipped it, so anybody who has not touched row 13 of the
AH-64D gets `KEYBOARD UNIT` on it and anybody who has keeps what they have.
That upgrade was the thing to watch, and Cory has since seen the reconcile do
its work on real upgrades. Which also means the free MCDU rows, A-10C 1 to 3,
AH-64D twelve, F-14BU six, are now shippable: putting labels there reaches
people who already have those profiles instead of only new installs.

**Built and flown 2026-09-20: field content is a chain.** A field
holds pieces drawn end to end, each one either characters the user typed, a
signal, or a gap that draws nothing and takes whatever the rest of the row
leaves. `RALT` small and red, the radar altimeter, then `M`, on one row. The
MCDU, the DED and the UFC all take it, and so does every aircraft: the A-10C's
CDU is ten lines on a screen of fourteen, so rows 1 to 3 are the user's, the
AH-64D leaves twelve rows free and the F-14BU six. The CH-47F is the only one
with nothing spare.

Five decisions are worth keeping, because each was the second answer rather
than the first:

- **The type is `Span`, not `Part`.** `Part` is already the device sub-unit all
  through this codebase and the collision made every mention ambiguous. The
  JSON key is `content`, for the same reason. `Reading` collided with the learn
  watcher's own `Reading` and is named in full at its one use in the engine.
- **A field of one piece is written flat**, with its `source` and styling beside
  the cells, and only a chain of two or more becomes a `content` array.
  `Readout` converts through a `ReadoutRepr` in both directions, so no code
  path can build a field that serializes the other way. This is not tidiness:
  an update never rewrites a row the user has changed, so a field that came
  back from a save as an array where a `source` used to be would turn every row
  into a row the user owns and freeze it against every later fix.
  `crates/dsc-config/tests/profile_round_trip.rs` holds it shut, comparing
  parsed JSON rather than text because `replace` and `aliases` are hash maps
  and have always come back in arbitrary order. That predates this work and is
  noise in a diff rather than a change; a `BTreeMap` would settle it if the
  churn of one pass over the defaults is ever worth it.
- **Overflow is truncation, not refusal.** Checked rather than assumed, and the
  assumption was wrong: `lay_out` pads or crops to exactly the run width and
  the write goes out looking healthy, so a reading loses its end with nothing
  on the panel saying so. The editor therefore works the width out ahead of
  time, exactly where it can (`max_length` for text, the `reads` range for a
  gauge) and says how many characters would be lost. A gauge with no range is
  the one unbounded case and is called out as such. It stays a caution, on the
  existing advisory channel rather than a new one: whether the aircraft ever
  sends a reading that wide is the user's to judge.
- **A gap is measured, not typed.** It carries no text and no source and is
  laid out after everything else, so `FUEL`, gap, reading puts one at each end
  of the row and they stay put when the reading changes width. Two or more
  split what is left evenly, the remainder to the earlier ones. It asks for no
  room of its own, so it never causes an overflow warning, and `align` stops
  meaning anything beside one because the content already fills the run.
- **The aircraft's font always wins.** `font_with` takes `native_fonts` first
  and the profile's `font` only where there is none. A module that draws a CDU
  has glyphs drawn to match what it sends, so an override would draw the wrong
  symbol rather than the same one differently.

**The four fonts are not interchangeable**, which the editor has to show rather
than describe. Dumped from the files: A-10C 66 glyphs large and 64 small,
AH-64D 71 and 63, CH-47F 64 and 63, F-14BU 98 and 65. Only the F-14BU font has
lowercase, `!`, `#`, `?` or `@`, which makes it the one to pick for free text.
Small is a strict subset in every one of them, so marking a piece small can
take away a character that was fine large. And the slots lie: in the A-10C font
`%` draws a question mark, already recorded in `CONFIG.md` and the reason the
editor draws the line from the font's own `BitArray` bitmaps rather than
showing the typed string. Checking the characters alone would have agreed with
the user and disagreed with the glass.

**The editor lists every area of every screen**, in the order it sits on the
glass, the way it lists every lamp of a device. Adding a field used to push it
onto the end of the profile's list however far up the panel it was drawn, so
the only way to get a screen back into order was to delete every field and
build it again. A field now belongs to the region holding its first cell, and
fields inside a region sort by first cell, which covers a field narrower than
its row, one spanning two, and two sharing one without a special case. An area
in use can take a second field beside the first, which lands on the widest free
run inside it.

310 tests passed when it landed, 33 of them new. Flown the same day; see the
2026-09-20 entry above.

**Built and flown 2026-09-20: the session log.** The daemon writes what it
is doing to `Saved Games\DCS\Logs\dcs-signal.log`, beside DCS's own log. It
exists because the hook starts the daemon hidden, so nothing it printed reached
anybody: a user reporting a panel going dark mid-flight had nothing to send.
One file per start, the last kept as `.bak`, ten megabytes before it rolls.

The traffic is no longer gated on `--verbose`; that flag now only decides
whether the console gets it too, which is why the hook does not pass it and
should not. Everything named gets at most one line a second, the rest counted
as `(x14)`, or a single gauge would fill the file in under an hour. A status
line goes out every minute whether or not anything happened, because an idle
daemon and a wedged one are otherwise indistinguishable in a log.

Read the first three lines before anything else. They carry the version, the
full command line and which layout the files came from, and the first thing
they caught was the log's absence being a stale binary rather than a fault in
it. See [Development mode](#development-mode-and-three-faults-it-uncovered).

**Flown 2026-09-20, Mi-24P, one nine minute session.** 274 lines, 29 KB, no
`ERROR` and no `WARN`. It caught every phase without being asked twice:

```text
12:32:16  Running. Ctrl-C to stop and clear the panels.
12:33:16  status   0 frame(s), 0 word(s) in, 0 lamp write(s), 0 paint(s)
12:36:50  stream   first frame after 274444 ms
12:36:50  aircraft Mi-24P  ->  profile Mi-24P
12:37:16  status   518 frame(s), 26350 word(s) in, 41 lamp write(s), 26 paint(s), longest pass 175 ms
12:39:16  status   1111 frame(s), 80577 word(s) in, 0 lamp write(s), 473 paint(s), longest pass 3 ms
12:39:31  stream quiet for 20s. Panels cleared.
12:40:47  stream   frames again after the quiet spell
12:41:07  DCS is no longer running. Exiting.
12:41:07  stopped  cleanly
```

The `stream quiet for 20s. Panels cleared` line and the one after it are from
before 2026-09-22: a quiet stream no longer clears the panels while DCS is
running, so neither is logged any more.

Four idle minutes before the mission each left their status line, which is the
point of writing one on a minute where nothing happened: the gap between
starting and flying is visibly waiting rather than wedged. The profile was
picked 1 ms after the first frame arrived.

**The coalescing earned its place.** The Mi-24P radar altimeter, `PLT_RV5_ALT`,
moves on nearly every export frame; 45 lines in the file carry a suppressed
count and the largest is `(x20)`:

```text
12:38:02.820  TRACE   346624 ms  signal  PLT_RV5_ALT  = 502 -> 6  (x14)
```

At 80,000 words a minute the flying part of that session wrote about 5.6 KB a
minute, so the ten megabyte cap is roughly thirty hours of continuous flight.
Unthrottled, that one gauge alone would have reached it in well under an hour.

**`longest pass 175 ms` at mission start** is the number to watch. Loading a
profile and painting 26 screens happens in one pass of the main loop, and
nothing else, including the stop check, happens during it. Every later minute
sat at 1 to 3 ms. Worth a look if panels ever feel behind at mission start, not
worth doing anything about yet.

Also proven against a synthetic export stream (rollover keeping its header),
and against the real panels started hidden through `run-hidden.vbs`, where it
logged every device it opened and stopped 0.15 s after being asked.

The editor's Restart was watched end to end on 2026-09-20 and the log holds the
whole handover, which is what this file is for:

```text
12:32:02.037  INFO   run      ... run --exit-when-idle 3600   <- the old daemon
12:32:15.720  INFO   Asked to stop. Clearing the panels.
12:32:15.728  INFO   stopped  cleanly                         <- 8 ms
12:32:15.854  INFO   run      ... run --exit-when-idle 20     <- the new one
```

The last line is in a new file, and the first three are in `.bak`, so a restart
reads as two sessions rather than one, exactly as a new flight does. The stop
took 8 ms against a five second timeout; the timeouts reported before this were
a daemon built before it could read the lock.

Nothing outstanding on it.

**Built 2026-09-19, seen on the glass, reshaped 2026-09-20: MCDU dividers.**
The rule was inset by a blank at each end until it was flown beside real CDU
lines; it fills its run now, and can carry a label. See the 2026-09-20 entry
above. A field with `divider`
draws a fixed rule instead of reading a signal, and the A-10C and AH-64D
defaults now carry one. Drawn in a live A-10C mission the same day, which
changed it twice: spaced dashes read as a dotted line and became an unbroken
run, and the colour became something the editor picks. Both changes were drawn
on the glass 2026-09-20. See "Dividers" in `CONFIG.md`, and the entry under the
MCDU below.

**Built 2026-09-19, proven 2026-09-20: Manage Converter.** The editor can stop
and start the daemon, from a dialog on the profiles page. Pressed in the window
and run against a live daemon, which cleared the panels on the way out. Restart
was watched end to end with the session log on both sides of it; the handover
is quoted above. Only the netstat parser has tests, because the rest of it is
process control and sockets. If Restart ever reports that the converter did not
stop when asked, read [Development mode](#development-mode-and-three-faults-it-uncovered)
before believing it is the daemon. See below.

**Verified 2026-09-19: an unplugged panel is harmless.** With the MFDs unplugged
while profiles bound them, the daemon and the editor both ran and everything
else worked as normal.

**Done 2026-09-19: DCS-BIOS version mismatch, all 5 steps.** Flown 2026-09-20,
and the per-mission version check stayed quiet. The
shipped defaults were written against DCS-BIOS `2026.09.18-nightly`. A user on
another release, a stable one in particular, may have signals the defaults
name missing, renamed or changed. Before this work that was all or nothing: `validate` reports
`UnknownSignal` and `load_profiles` skips the whole profile, so one renamed
signal costs every lamp in that aircraft.

Decided 2026-09-19:

- **One rule for every profile**, shipped or the user's. Exceptions get hard to
  maintain.
- **A bad condition** is one whose signal is not in the catalogue, or whose
  value is outside the signal's range. Either way the source is not what the
  profile was written for, and could fire the lamp when nobody meant it to.
- **A bad condition kills its AND chain.** In an `any_of`, each branch is its
  own chain: a branch with a bad condition is dropped and the valid
  alternatives still work. A lamp with no chain left is skipped and stays at
  its swept value, the same as an unset row.
- **Flag it, don't block it.** The editor puts a hazard mark on the condition
  block itself, with the reason; Save stays available and the row stays in the
  file, so it comes back once the source is fixed. The daemon logs one warning
  per profile.
- **Only nightly-only signals are worth a word.** The defaults target the
  nightly; most users run stable. Profiles carry no version. Instead each
  release ships `data/nightly-only.json`: the signals the defaults read that the
  latest stable lacks or reports with a different range, and nothing older.
  A flagged condition on that list says it needs the nightly, and the profile
  page shows one line naming the lamps only when the user's DCS-BIOS is
  actually missing some of them.

The plan:

1. ~~**The catalogue follows the installed DCS-BIOS.**~~ **Done 2026-09-19.**
   The Python builder is ported to Rust (`dsc-config::catalogue_build`) and
   deleted; the output matched it byte for byte on all 50 modules, line
   endings aside. The daemon, the editor and every CLI command that reads the
   catalogue call `ensure` first, which rebuilds only when the version in
   `BIOSConfig.lua` or the stamp of the `doc/json` files differs from the
   catalogue's (see step 4 for why the stamp), so whichever app starts second
   finds the work done. A lock file stops two builds at once, and a build goes
   into `catalogue.building` and is renamed into place, so nothing reads half a
   catalogue. `dcs-signal catalogue --rebuild` forces one; `--bios` points at an
   install outside Saved Games and is remembered in `index.json`.

   The index now records where `CommonData` puts `VERSION` (address 1126, 24
   characters), so at each mission start the daemon asks the DCS-BIOS that DCS
   actually loaded which release it is. `Export.lua` loads DCS-BIOS fresh each
   mission (hooks load once, at game launch), so the release on disk at mission
   start is what runs. If the catalogue is behind the installed release, the
   daemon rebuilds and carries on. If DCS runs a release that is not installed,
   which is an update made mid-mission, it clears the panels and exits, and the
   next mission starts it clean. **Seen against live DCS 2026-09-20:** no
   version message at all.
2. ~~**The nightly-only list.**~~ **Done 2026-09-19.**
   `dsc-config::nightly_only` compares what the defaults read in the local
   nightly catalogue against a stable one; `dcs-signal nightly-only --stable <json>`
   writes `data/nightly-only.json`; `python tools/nightly_only.py` fetches the
   latest stable from GitHub and runs it. **This is a release step** and belongs
   in the release process when it is written. Against stable v0.11.7 it lists 8
   signals, all the F-14's `RIO_CDNU_LINE1` to `8`, which is the F-14BU CDNU.
3. ~~**Bad conditions in the daemon.**~~ **Done 2026-09-19.** `UnknownSignal`
   is gone from `problems()`: `Profile::flags` reports each condition or field
   reading a signal the catalogue lacks or a value above its range, and
   `Profile::runnable` is the copy the daemon runs (a flagged condition clears
   its lamp's chain, a flagged `any_of` branch is dropped and the others kept,
   a flagged field is left out). One grouped warning per profile, naming the
   nightly where `data/nightly-only.json` knows. Checked against a catalogue
   built from stable 0.11.7: the F-14BU default, skipped outright before, now
   loads with only the 24 RIO CDNU fields blank, reported in two lines. The
   defaults test now fails on any flag against the local nightly, so a typo in
   a default is still caught.
4. ~~**Bad conditions in the editor.**~~ **Done and verified 2026-09-19** in
   the window against stable 0.11.7: the profiles page reported the rebuild,
   and the F-14BU profile showed the CDNU fields flagged with the nightly
   notice above them. The editor's check now returns each flag, placed by index,
   with a sentence that follows what the row already says: why, where the
   nightly-only list knows more ("Needs the DCS-BIOS nightly; stable 0.11.7
   does not have it."), then what it costs ("The lamp stays off.", "This
   alternative is left out; the others still work.", "The field stays
   blank."). The window resolves each flag to the condition or field object it
   names (`editor/src/flags.ts`) and shows it under that row with a hazard
   mark, in the caution colour. A one-line notice sits at the top of the page
   only when a flagged row is on the nightly-only list. Save was already
   allowed, since flags are not problems.

   Found while trying that (2026-09-19): the version alone cannot say the
   catalogue matches. `tauri dev` restarted the editor while stable 0.11.7 was
   being copied over the nightly, and it built from the new `BIOSConfig.lua`
   beside the old nightly `doc/json`: a catalogue labelled 0.11.7 holding the
   nightly's signals, which no later start would rebuild. So `index.json` now
   also carries a `stamp` of every `doc/json` file's name, size and modified
   time, taken before the build reads anything, and a catalogue is current
   only when both match. And because no user will ever see a console, the
   profiles page now says what the startup check found: up to date, built,
   rebuilt and why, or DCS-BIOS not found and where it looked.
5. ~~**Tests and docs.**~~ **Done 2026-09-19.** Tests: `flags.rs` (the chain,
   branch and field rules), `catalogue_build.rs` (rebuild on a new version or
   changed files, the lock, the swap), the editor's flag wording and the
   fields the window places it by, and a CLI test that a profile reading a
   missing signal loads with that row off. Docs: "Rows this DCS-BIOS cannot
   back" in `CONFIG.md`, and the catalogue section of the README.

**Done 2026-09-19: renamed to DCS Signal Converter.** Nothing user-facing
may read as WinCtrl software: the product and window are DCS Signal
Converter, the daemon is `dcs-signal.exe`, and the crates are `dsc-*`. Only
`wctrl-hid` keeps its name, because it is the vendor's protocol, and so do
vendor strings such as the `WINCTRL ...` product names. WinCtrl and WinWing
appear only to say which hardware this drives; the README says it is not
affiliated. The repository and folder keep their names.

**Done 2026-09-20: the installer, the release process and a CI pipeline**, so
the whole flow (first-run catalogue build, rebuild on a DCS-BIOS update, the
nightly-only list) has been tested as a user meets it. Written as the plan
below; what is left of it is the release-notes list of changed default rows.
When that work started there was no release process: `VERSION.md` held
`1.0.0-alpha.001` and there were no workflows.
The release pipeline runs `tools/nightly_only.py`. Worth knowing for the
installer: the editor finds `data` beside the executable once installed, and
it writes the catalogue and profiles there, which Program Files does not allow.
Leaning: read-only data stays with the program, and what is written (profiles,
catalogue, its lock) goes to `Saved Games\DCS Signal Converter`, found through the Saved
Games known folder rather than assumed under `%USERPROFILE%`, since users move
it. Chosen over Documents, which OneDrive often syncs, and that fights the
catalogue's lock and rename. Decide there whether a dev run uses it too.

Decided 2026-09-19:

* Dev runs keep using the checkout's `data/`.
* Install per user (`%LOCALAPPDATA%\DCS Signal Converter`, Tauri's per-user
  default, so no admin prompt; confirmed 2026-09-19 over Program Files), in the stock
  NSIS look: no custom pages or theming. Where a question has to be asked it
  is a plain message box and the Windows folder picker.
* The installer places the DCS hook and removes it on uninstall, but only
  after confirming where DCS saves. Saved Games comes from the known-folder
  API. DCS saves to `Saved Games\DCS` unless its install folder (registry
  `HKCU\Software\Eagle Dynamics\DCS World`, `Path`) holds
  `dcs_variant.txt`, which makes it `DCS.<variant>`. Only plain `DCS` with a
  `Config` folder is taken silently; anything else asks for the DCS folder and
  for where profiles go, rather than assume either.
* `VERSION.md` is the one source of the version, printed as written
  (`1.0.0-alpha.001`) by `dcs-signal --version`, the daemon log, the tag and
  the release. Cargo, Tauri and npm reject leading zeros, so
  `tools/version.py` stamps them with `1.0.0-alpha.1`; `--check` fails when
  they drift. Add/Remove Programs shows that form, since Tauri's upgrade check
  compares it as semver.

**Installer, built 2026-09-19, installed and tested 2026-09-20:**

* `dsc-config::paths` is the one resolver for the daemon, the CLI and the
  editor (the editor's `paths.rs` is gone; CLI path flags now default from
  it). Order: `DSC_DATA`; `data\devices.json` beside the exe (installed);
  a `data` found climbing from the cwd or the exe (checkout). Installed, the
  shipped files come from `data` beside the exe and profiles and the
  catalogue go to `HKCU\Software\DCS Signal Converter` `DataDir`, else
  `Saved Games\DCS Signal Converter`. `default_bios_json` follows `DcsDir`,
  else `Saved Games\DCS`.
* `cd editor; npx tauri build` builds `dcs-signal.exe` first and bundles it,
  `run-hidden.vbs`, the hook template (`hook\`), `data\` (devices, displays,
  mcdu fonts, defaults, nightly-only list) and the licences.
* `editor/src-tauri/installer-hooks.nsh`: waits for `dcs-signal.exe` to exit
  (Retry/Cancel; a silent install kills it), finds or asks for the two
  folders and records them, writes the hook with `DSC_DIR` filled in. An
  update reuses the recorded folders and never asks again. Uninstall removes
  the hook and the catalogue; profiles and the recorded folders go only when
  "delete app data" is ticked.

Before copying, an install clears `data\` and `hook\` and the named top-level
files, so installing over an old version leaves nothing it dropped; a
top-level file a release stops shipping keeps its name in that list.
An install finding `DCS.exe` running says to restart DCS, which loads hooks
only at launch. The app icon is `editor/src-tauri/icons/icon.svg`, the
afterburner app's tile and palette with a lamp in place of the flame.

**Tested 2026-09-20** on the pipeline's build rather than a local one, since
that is how releases are compiled: fresh install, fly, update over it, and
uninstall with and without "delete app data"; the folder prompts from a second
Windows user who has never run DCS (Windows 11 Home has no Sandbox). The hook
template is written as ANSI, so an install path outside the system code page
would not reach Lua intact.

What the pipeline had to do, all of it built below. **CI** on push and PR
(Windows runner): `cargo test --workspace`,
`tools/version.py --check`, the editor build. **Release** on a `v*` tag
matching `VERSION.md`: tests, `nightly_only.py`, the installer renamed to
carry `VERSION.md` as written, a draft GitHub release. The release notes
list every change to an existing default row: a row the user has changed is
theirs, and no update rewrites it (decided 2026-09-19), so the notes are how
they learn a fix exists and choose whether to reset that lamp or the profile.
New profiles and new hardware rows still arrive on their own (`seed` and
`merge_new`, run by both the daemon and the editor). The per-lamp reset
asks first and shows the lamp's current and shipped setups side by side. No `cargo fmt` check.

Found 2026-09-19: DCS-BIOS publishes nightlies only as one rolling `latest`
pre-release whose single zip (`DCS-BIOS_nightly_2026-09-18.zip`) is replaced
by the next, so a pinned nightly cannot be fetched again later. CI and the
release need their own copy of the nightly the defaults were written against,
for example attached to a release in this repository, before the
shipped-defaults test and `nightly_only.py` can run there.

**CI and release workflows written 2026-09-19, and run since.** Decided: the
pipeline runs the tests for every PR and every release, since a local run can
be skipped, and a release builds only from a commit on `main`, so every
installer traces to its source.

* The pinned nightly is named in `tools/dcs-bios-pin.json` (version, release
  tag, asset, SHA-256). `tools/fetch_bios.py` downloads it from this repo's
  `dcs-bios-2026.09.18-nightly` release, checks the hash and the version in
  `BIOSConfig.lua`, and with `--build-catalogue` builds `data/catalogue` from
  it. `--zip` checks a local copy instead.
* `.github/workflows/ci.yml`, on PRs and pushes to `main`, Windows runner:
  `version.py --check`, the frontend build, the pinned catalogue,
  `cargo test --workspace --locked`, the installer build.
  `DSC_REQUIRE_CATALOGUE` makes the two tests that skip without a catalogue
  fail instead.
* `.github/workflows/release.yml`, on a `v*` tag: the tag must equal
  `v` + `VERSION.md` and its commit must be on `main`; then all of CI (called,
  not copied); `nightly_only.py` must leave `data/nightly-only.json`
  unchanged, so what ships is what is committed; the build gets
  `DSC_COMMIT`, which `dcs-signal --version` and the daemon log print
  (`local build` otherwise); the installer is renamed
  `DCS-Signal-Converter-<VERSION.md>-setup.exe` with a `.sha256`, given a
  build-provenance attestation, and put on a draft release.
* Found on the way: a Tauri build copies `data\devices.json` into
  `target\debug` and `target\release`, after which a `cargo run` of
  `dcs-signal` resolves as installed and reads and writes Saved Games, not the
  checkout. The release scripts set `DSC_DATA`; a dev `cargo run` does not.

**Update banner, 2026-09-19.** On each start the editor asks GitHub for this
repo's releases (`editor/src-tauri/src/update.rs`). If the newest published
`v*` release is not the running `VERSION.md`, a bar at the foot of every page
links to it. Pre-releases count only while the running version is one.
Offline or any other failure shows nothing. The backend builds and opens the
URL itself (`explorer`), so the window still opens no URLs. `reqwest` with
Windows' own TLS. Seen in the window 2026-09-20, against a published release.

To release: `tools\release.cmd`, adapted from the afterburner project. It
runs only on `main` in step with origin, pushes nothing but the tag, and
checks `version.py --check` and `nightly-only.json` first so the pipeline
does not fail on them after the tag exists. It runs no tests; CI does.

That version check reads `HEAD`, not the working tree (`version.py --check
--ref HEAD`), and takes the tag from `--print --ref HEAD` as well, so what
the tag names is what its commit holds. A stamp that was run but never
committed used to pass here and fail in the build, which meant deleting the
tag locally and on origin. `--check` covers every version in the repo,
Cargo.lock included, since `cargo --locked` fails on a lock that disagrees
with its manifest; the workspace crates come from `Cargo.toml`, so a new one
is covered without editing `version.py`. Last, `release.cmd` warns when
`CHANGELOG.md` has no section for this version: the pipeline allows that and
ships only provenance, which is almost never what was meant.

Done 2026-09-20: the pinned nightly's zip is attached to the
`dcs-bios-2026.09.18-nightly` release, and on GitHub `main` requires CI and
`v*` tags are protected.

Still to do: the release-notes list of changed default rows.

**Then, in order:**

1. ~~Build the engine crate.~~ **Done 2026-09-16.** `crates/dsc-engine` holds
   all the policy and does no I/O, so the whole module-load sequence is tested
   without hardware or DCS. `dcs-signal run` is the daemon around it.

   **Proven end to end 2026-09-16.** `dcs-signal run --verbose` drove the PTO2 from a
   live A-10C mission: gear lamps, Master Caution, backlight tracking the console
   dimmer, and the two-condition HALF lamp lighting only at MVR with the gauge in
   its window. The flap lamps looked dead and were not; see the `FLAG` dimmer in
   the verified facts.

2. ~~**Fly it.**~~ **Flown 2026-09-16.** Edit a profile (in a checkout with
   `env=dev`, that is `data/defaults`; see "Development mode" below), then,
   with a mission loaded:

   ```powershell
   cargo run --bin dcs-signal -- run --dry-run    # prints writes, opens no device
   cargo run --bin dcs-signal -- run              # drives the panels
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
     same style and listed in the file. `tools/gen_ded.py` regenerates the
     font from the two fixtures plus the drawn glyphs. `tests/ded_render.rs` reproduces every
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

   Left to do: replace drawn glyphs as captures turn up. The editor got a
   `format` picker 2026-09-19, offered only on glass that draws inverse.

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
   the L name: `dcs-signal led` wrote 0, 255, 20 and 137 through `col01`, each
   acked, and the backlight went dark, full and dim as sent. Flown from a
   profile in a mission 2026-09-20.

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
   every default, and stay that way: what they show is the user's call
   (Cory, 2026-09-22). `Screen_Backlight` 1 follows the screen rule, as the
   ICP's does. Captures run through `tools/tail_wwthid.py` now, because the log
   wraps within a minute.

   **The MCDU screen is a third kind of display, `text`.** Decided
   2026-09-18 that users run one application, so this app drives the screen
   too. SimAppPro never drives it from DCS, so the protocol is ported from
   WwDevicesDotnet (BSD-3) with its font upload, and the A-10C font comes from
   WCtrlDcsBiosBridge (MIT); notices in `THIRD_PARTY_NOTICES.md`, details in
   `PROTOCOL.md` under "Driving a text grid". **Drawn on our panel** with
   `dcs-signal mcdu-test`. In the engine a text grid is 336 cells of character,
   colour and size (`data/displays/mcdu.json`), sent whole on any change, and
   a readout takes `colour`, `small` and `replace` (one-for-one character
   swaps for DCS-BIOS's stand-ins). The font is the aircraft's, never the
   user's: `native_fonts` maps runtime aircraft name to font, and a profile
   putting fields on the MCDU for an aircraft without one is refused. `dcs-signal
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
     column in (2026-09-18). Flown 2026-09-20.

   Font selection for aircraft without a CDU came with text output fields,
   2026-09-20.

   **Dividers, 2026-09-19.** Superseded in part 2026-09-20: the rule now
   fills its run corner to corner, can carry a label, and has no minimum width;
   see the entries at the top of this file. As first built, a readout with `divider` draws a rule across its
   cells and reads nothing: a blank at each end and an unbroken run of dashes
   between them. Spaced dashes, ` - - - - `, were what it drew first, and
   seeing it on the glass settled it: it read as a dotted line rather than a
   rule. The colour is chosen in the editor, which is the one place the window
   offers a colour at all. It exists because two of the four aircraft leave most of the
   screen dark, and a page with nothing under it runs off into the black. The
   A-10C's CDU is ten lines of fourteen, so its rule sits on row 4; the
   Apache exports only its keyboard unit on row 14, so its rule sits on row 13,
   inset to the same 22 cells. The F-14BU has none: its CDNU comes within two
   rows of filling the glass, and Cory's call was that a rule there would be
   noise. Text grids only, because a segment display draws from a glyph table
   and none of them holds a rule; `validate` refuses one elsewhere, refuses a
   divider that also names a signal, and refuses a run too narrow to hold a
   dash between two margins. The dash and the blank are checked against the
   aircraft's font like `replace`, and every shipped MCDU font has both.

   Both defaults gaining a row means `merge_new` adds it to profiles that
   predate it, unless the user has claimed those cells, which is the rule
   readouts already followed.

   This is deliberately not the text customisation ticket. A rule is fixed and
   needs no input, so it could ship on its own; anything the user types is that
   ticket and waits for it.

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
* **One naming rule.** `file_stem` in `dsc-config` names every generated file,
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

**Added 2026-09-19:**

* **Notes are editable**, on lamps and fields alike, through the one control in
  `editor/src/note.ts`. The 143 notes in the shipped defaults were write only
  before this. A field with nothing written on it shows a button rather than an
  empty box.
* **A field can pick the signal marking its inverse characters**, filled in
  from the source's `_FORMAT` twin and following it when the source changes.
  Offered only on glass that draws inverse, which `draws_inverse` on
  `DisplayView` decides; a test names each shipped display.
* **Reset this lamp** moved to the bottom right of its cell, out of the stack of
  buttons that add things.
* **Drawing a row never rewrites it**, so opening a profile leaves it clean and
  the unsaved marker stays honest.
* **The header sticks to the top** on both pages. Its divider is a shadow,
  because a border at a fractional scroll offset was dropped.
* **Two profiles cannot share a name**, renaming included. The rename box says
  so while typing, `save_profile` refuses it, and `write_new` checks the name as
  well as the file name; `Profiles::name_taken` holds the rule, ignoring case
  and surrounding space.
* **Export confirms in a banner** that clears after ten seconds, above the
  update bar rather than over it.
* **Add a divider**, beside Add a field and only on a text grid. The row has no
  signal picker, because there is nothing to pick: it shows the rule the panel
  will draw, and `divider_rule` in the backend works that out, so the preview
  cannot drift from what is sent. A new one takes the colour the display's
  other fields agree on, since a white rule across a green page reads as a
  fault.
* **A colour menu on a divider**, and nowhere else: a field's colour is the
  aircraft's business. `Colour::ALL` names the eleven in `dsc-config` and
  `DisplayView` hands them over, so the window cannot offer one the panel has
  no index for. The preview draws on the glass's own black rather than the
  page's background, or a white rule would be invisible in light mode.

Bindings are fully editable. A condition reads as a sentence until its pencil is
clicked, and an open condition carries keep, cancel and delete: cancel restores
it as it was when editing began, and delete confirms first. All four binding
forms are offered, each only where it can mean something:

* **conditions**, through the signal typeahead, its test and its values
* **any_of**, through "+ Add alternative (or)", with a choice between the
  brightest alternative and the one whose signal moved last (`pick`)
* **always**, on a lamp nothing is assigned to
* **same_as**, only on a dimmer with another dimmer to point at, on this
  panel or another (added 2026-09-21)

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
killed or crashes and the panels latch. With `--exit-when-idle`, the daemon
clears the panels and exits once the stream is quiet and `DCS.exe` has gone.
Since 2026-09-22 a quiet stream alone clears nothing, so the options menu,
which pauses DCS-BIOS, no longer blanks the pit.
Only one daemon runs at a time; a second backs off.

**Learn mode**, added 2026-09-17, is the one place in the editor that does I/O.
Press **Learn** beside any signal box, flip the control in the cockpit, and what
moved is listed with the most switch-like first. `dcs-signal learn` is the same thing
without a window.

The judgment lives in `dsc-engine`'s `learn` module, which has no I/O and is
tested against a synthetic stream: ranking by movement count, reading each
signal through its own mask so a shared word does not name its neighbours,
counting a multi-word string as one movement, and treating a first sighting as a
baseline rather than a report. The editor backend adds a thread and a socket and
nothing else. It listens only while the panel is open, which is deliberate, and
`CONFIG.md` says why.

**Profile management:**

1. **No editor for a profile's aircraft list, by design.** The list is set
   when a profile is created and afterwards changes only through an operation
   that has somewhere to put every aircraft it moves: New profile, Import...
   and Delete each settle the claim as part of what they already do, and none
   of them can strand an aircraft. A free-standing list editor would have to
   ask where an aircraft goes with nothing in hand to answer it. Renaming was
   built 2026-09-19 (the pencil beside the name; the file name never changes),
   along with Delete on every profile and seeding that checks claims by
   aircraft; see "One aircraft, one profile" in `CONFIG.md`. Aircraft move
   between profiles only within a family, which the shipped defaults define
   (`Profiles::families`).
2. ~~**Import and export a profile.**~~ **Built 2026-09-19.** Export... on
   each row, Import... beside New profile; see "Sharing a profile" in
   `CONFIG.md`. Beyond the plan below, decided with Cory: taking an aircraft
   another profile flies is confirmed, a profile left with none is deleted only
   once confirmed, and declining that cancels the import. `claims::write_new_deleting` rolls every file back on a failure.
   Export sits on the row rather than the edit page, so it never has to ask
   about unsaved edits. Tests no longer leave `dsc-*` folders in `%TEMP%`,
   and `Profile::save` removes its `.json.saving` file when a write fails.
   The plan as written before: So users can share a setup. Export
   saves the open profile through a save dialog run by the backend (the window
   opens nothing itself). Import parses it, checks the module is in this
   catalogue, runs `Profile::problems`, always writes a new file name, and
   settles claimed aircraft the way Copy to... does (move them, or import
   without them), within the same family rules as Delete. "Copy to..." on a profile row
   takes a name and an aircraft list and carries everything else over, module
   included. The module is not offered, because a copy whose
   signal ids resolve against a different catalogue is not a copy, it is a
   profile full of signals that do not exist. This is the FA-18E case made into
   a feature: the Super Hornet community mod reads the Hornet's DCS-BIOS
   definitions, so the Hornet profile drives it with only those two fields
   changed.
3. ~~**Validation before save.**~~ Done: the editor runs `Profile::problems`
   after every edit and withholds Save until there are none. See `CONFIG.md`.

**Confirmed in the window 2026-09-16:** profile list, create with the module
picker, collapsible sections, and the signal search. Three faults found by using
it and fixed: columns not aligning between sections, the hint box being cut off
at the window edge, and dropdown rows losing clicks to a focus race.

**Confirmed in the window 2026-09-19**, every flow used rather than read:
rename, Delete on any profile, and seeding that checks claims; Import... and
Export..., including the confirmations and the rollback; the sticky header, the
export banner, and the refusal of a name another profile holds; notes on lamps
and fields, the inverse-highlight `format` chooser, and Reset in its new
place.

**Verified on hardware 2026-09-17**, in a running mission with real panels:

* **Lamps still follow signals** after `apply` began reporting whether a word
  actually moved. This was the regression risk of that change: a wrong answer
  would have left lamps lit by the module-load sweep and then frozen.
* **Hot reload.** A profile saved in the editor reached the running daemon and
  changed the panel without stopping anything.
* **A quiet stream clears the panels**, and the daemon stays up through it while
  DCS is still running. Changed 2026-09-22: it no longer clears them while DCS
  runs.
* **The same aircraft loaded twice** sweeps the second time. This is the one
  that fails silently if the engine does not forget the cockpit on a quiet
  stream, and a different aircraft would have passed either way.
* **`any_of` in the AH-64D**, including the seat swap, which is the half that
  cannot be proven any other way.

**Proven through the hook 2026-09-20**, from an installed copy: launching DCS
started the daemon, it drove the panels through the mission, and it exited on
its own when DCS closed. Before that it had been proven only from the command
line.

**Confirmed on hardware 2026-09-20:** `always` and `same_as`, both watched
driving a real lamp. Shipped profiles use `always` for the PTO2 gates in the
F-14, Mi-24P, FC3 and No aircraft profiles and `same_as` for both gates in the
AH-64D.

## Release notes

`CHANGELOG.md` is the user-facing record, and the release pipeline puts the
section matching the version at the top of the release notes, above the
provenance it already wrote. A version with no section still releases; the awk
simply finds nothing. It holds the current version only: once a release ships,
its notes on GitHub are the record, and the next bump replaces the section
rather than adding one above it.

**A release that changes a shipped profile has to say which rows**, because an
update never rewrites a row the user has changed: a fix reaches them only if
they reset that lamp, and they cannot choose to unless the notes name it. The
alpha.002 release notes do that for the two MCDU dividers.

## Development mode, and three faults it uncovered

**`.env` beside `data`, 2026-09-19.** `env=dev` makes `Paths::resolve` return
`Layout::Dev`: everything in the checkout's `data`, with the tracked defaults
standing in as the active profiles. So a profile authored in the editor is a
diff rather than something to copy across by hand, and nothing done while
developing reaches the profiles Cory actually flies. `.env` is untracked and
`.env.example` is the copy that ships; `env_is_dev` is tested, and anything but
`dev` means production, because that is the answer that leaves real profiles
alone. CI and the release pipeline write `env=prod` before they build, so the
layout they test is the one they ship rather than one that is right only
because the file is missing.

**Why it was needed.** A Tauri build copies `data` beside the executable, so
`target/debug/data/devices.json` and `target/release/data/devices.json` both
exist. `resolve` tested "data beside the exe" before it looked for a checkout,
so **every development run was classified as installed**: it read a stale
`target/*/data/defaults` copied at build time and wrote profiles and the
catalogue into `Saved Games\DCS Signal Converter`. Found 2026-09-19 by asking
where a divider added in the editor had gone. Dev is now tested before
installed; installed is still tested before a plain checkout, so an installed
copy started from inside a checkout still uses its own files.

**The same copy hid the checkout again, 2026-09-20.** Testing dev before
installed was not enough, because the climb that *finds* the checkout stopped
at the first `data` folder it met, and started from `target/debug` that is the
copied one. No `.env` sits beside it, so `env=dev` was never read and the run
fell through to installed a second time: it drove the profiles and catalogue in
`Saved Games` and wrote its log there too. Found by asking why `data/logs`
stayed empty. The dev checkout now has its own climb, `climb_dev`, which
carries on past a `data` folder found beside the executable; both halves are
tested, including that an install is still not mistaken for a checkout. The
daemon says which layout it chose on its third line, so this is now visible
rather than inferred:

```text
INFO   run      files found as Dev
INFO   paths    profiles  C:\...\wctrl-module-signal-converter\data\defaults
```

**Building the daemon is a step of its own.** `npm run tauri dev` builds the
editor package and reloads the UI from Vite, but never the daemon;
`beforeBuildCommand` builds it only in release, for packaging. The editor's
Start and Restart run whatever `target/debug/dcs-signal.exe` happens to be, so
`cargo build -p dsc-cli` is on you, and it fails while a daemon is running
because the process holds its own executable open:

```text
error: failed to remove file `target\debug\dcs-signal.exe`
Caused by: Access is denied. (os error 5)
```

Stop the daemon first, or build into `target-alt`. A build skipped this way
leaves an old daemon that looks current and reads as a product fault: on
2026-09-20 the editor's Restart timed out for hours against a daemon built
forty-five minutes before the commit that taught it to read the lock, so it
bound the port and ignored every stop sent to it, while Kill worked because it
does not ask. Twice that day the exe also came back as that same 2026-09-19
build after a build that should have left it alone, hard-linked to a
`deps/dcs_signal.exe` of that date; why cargo re-linked a stale artifact was
never established. `cargo clean -p dsc-cli` and a rebuild cleared it, and
`cargo build --workspace` and `cargo build -p dsc-editor` both left the fresh
binary alone afterwards. One check before blaming the daemon:

```sh
ls -l target/debug/dcs-signal.exe        # should be today
target/debug/dcs-signal.exe run --help   # should list --log-dir
```

**`merge_new` dropped display fields, silently.** It decided whether to write by
counting added *bindings* only, so a default that gained a field and no lamp
merged it into the loaded profile and then hit `continue`. Nothing was written
and nothing was said, on every start. The MCDU dividers are exactly that case.
Readouts are now counted too and named in the note, with two tests: one that a
new field reaches the file, one that a field the user has already put on those
cells is never displaced.

## Managing the daemon from the editor

Built 2026-09-19. `dsc-config::daemon` owns what the daemon and the editor both
need to agree on: the lock address, the stop message, and the four operations
around them.

**The lock became a control channel.** The daemon already bound
`127.0.0.1:16539` to prove it was the only one running, and nothing was ever
sent to it. It now reads it once per pass of its main loop, and a datagram
carrying `dcs-signal: stop` breaks the loop. That means leaving by the same path
as Ctrl-C, so every lamp it lit is cleared and every screen it drove is blanked.
No process id is needed, no privileges, and nothing to clean up: the operating
system frees the port when the process dies however it dies.

**Why not kill it.** Terminating a process runs none of its shutdown, and the
lamps latch, so a killed daemon leaves the panels exactly as lit as they were
with nothing left running to clear them. That is the `0xc000013a` case in the
README's troubleshooting, and it would have been the normal path rather than an
accident.

**Kill is still there**, because a wedged daemon answers nothing and the
alternative is Task Manager and a guess about which `dcs-signal.exe` is the
right one. It ends only the process holding the lock, found through `netstat`,
so a `dcs-signal listen` or a second copy being worked on alongside is left
alone. `pid_holding` is the tested part; a TCP row for the same port is refused,
because TCP carries a state column and its fourth field is a word rather than a
pid.

**The shapes decided with Cory, 2026-09-19:**

- **One neutral button, "Manage Converter"**, rather than a bare Restart. The
  reasons to press it are narrow, and a button that says Restart gets pressed
  after every save out of superstition, dropping the panels each time.
- **The dialog lists the reasons before the buttons**, and says outright that
  saving a profile is not one of them, because hot reload already covers that.
- **Three ways out**: Cancel, Kill, Restart. Kill is pushed to the far left of
  the row, away from Restart, and is disabled when nothing is running.
- **Restart does not fall through to a start when the stop times out.** The old
  one still holds the lock, so a new daemon would refuse to run and the button
  would look like it did nothing. It reports and points at Kill.
- **Started the way the hook starts it**, through `run-hidden.vbs` with
  `--exit-when-idle 20`, so a daemon started from the editor behaves exactly
  like one a mission started, and there is one launch path rather than two.
- **Not "the engine"**, though it was proposed: the audience flies aircraft, and
  `Engine` in `dsc-engine` is already the thing that resolves signals to lamp
  values, which is not this.

`dcs-signal stop` is the same thing from the command line, and is how the stop
path can be proven without the window.

**Proven 2026-09-20:** pressed in the window, and a stop run against a live
daemon, which cleared the panels on the way out.

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
`crates/dsc-engine/tests/display_paint.rs` feeds the engine a Hornet COMM page
as DCS-BIOS frames and asserts the bytes it paints are the ones SimAppPro sent
the real device. All six compared groups match.

* `dsc-config::display` holds `Display`, `DisplayCatalogue`, `Screen`,
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

1. ~~**A host-side shadow of the buffer.**~~ Done. `dsc-config::display`
   has `Display`, `DisplayCatalogue` and `Screen`, checked against captured
   hardware traffic by `crates/dsc-config/tests/display_render.rs`: rendering
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

   `note` on a field stayed hand-edited until 2026-09-19; see "Added
   2026-09-19" above.

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
anywhere else. `tests/shipped_defaults.rs` enforced this until 2026-09-22, when
every test reading the shipped defaults was deleted: the defaults change as
each aircraft is reworked, and tests use frozen fixtures instead. Check it by
hand when a device is added.

**Every backlight in a default takes one source**, by decision the same day,
so the whole pit dims together until a user splits it. `devices.json` marks
panel backlights with `backlight: true` (not the PTO2's gates, nor its
`Landing_gear_lights`, which dims the gear handle's own light). The source is the MFD C's
`INST_PNL_Backlight` and every other backlight is `same_as` it. Since
2026-09-22 that row is `always` at 175 in every default, the Mi-24P included,
rather than following a cockpit knob. A new panel's backlight goes on it too.
`data/profiles` is the active folder the daemon reads and the editor writes; it
is gitignored and seeded from `data/defaults` on every start for any name not
already there. Seeding adds and never replaces. Reset is the only overwrite.

`Profiles` in `dsc-config` owns this, and both the CLI and the editor call it,
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

## Making room for other brands

Written 2026-09-22. Nothing here changes what a user sees; it is the seam a
second brand of hardware plugs into, put in while there was only one brand to
get wrong.

**Every device names its protocol.** `DeviceSpec.protocol` in
`data/devices.json`, defaulting to `wctrl` when absent, so a device file from
an older release or written by hand still loads. Declared per device rather
than read off the USB vendor id, because a vendor can ship more than one
protocol and a protocol can outlive the ids it started on.

**Two traits, in `crates/dsc-cli/src/panels/`.** `Protocol` finds and opens a
brand's devices; `Panel` drives one that is open. `panels::all()` builds the
ones this release can drive, and the run loop picks one per device by name. A
device asking for a protocol this build lacks is skipped with a warning and the
other panels still run, because that is what an inventory from a newer release
looks like, not a broken one.

`Panel` is three methods: `set_lamp`, `write_display`, `flush`. It ended up
smaller than planned. The commit pass used to be `commit(part_id)` and lived in
`apply()`, tracking which parts of which devices were owed a commit. Once the
backend sees whole `LcdWrite`s it can track that itself, so it became `flush()`
and a protocol with no commit model implements it as a no-op instead of being
asked about parts it does not have.

**Why this was cheap.** The engine was already free of I/O, so the boundary was
discovered rather than invented: `dsc-engine`, `dsc-config`, `dsc-bios` and the
whole editor never touched HID and none of them changed. The work was eight
call sites in one file. It also pulled real state out of the app:
`prepare_text_grid`, the map of which font each grid is holding, the 40ms pause
after a screen, and the `handles.remove()`/reinsert contortion that existed only
so the handle and the font map could be borrowed at once. `main.rs` came out 187
lines shorter.

**Why now and not when the hardware lands.** Profiles are the user's files and
`data/devices.json` ships to them, so a schema change costs real money once
people have both. A `#[serde(default)]` field added before 1.0 goes out costs
nothing. The refactor itself is behaviour-preserving, which means it could be
proven on the panels already here rather than shipped on faith.

**What was deliberately left alone.** A lamp is still addressed as
`(device, part_id, index)` and its value is still a `u8`, which are WinWing's
answers, not universal ones. Generalising them now would be designing against
imagination; the point of waiting is to have a real second device to design
against. The traits also live inside `dsc-cli` rather than in a crate of their
own, so reshaping them when the first one is wrong costs nothing.

**The diagnostics are outside the seam on purpose.** `parts`, `led`, `blink`,
`sweep`, `probe-brightness` and `mcdu-test` take a raw part id and index and
poke one protocol deliberately. There is no honest generic version of that, so
they call `wctrl_hid` directly and a second brand gets its own commands rather
than a shared vocabulary that fits neither. `devices` is the exception: it is
discovery, so it asks every protocol and a newly plugged brand shows up there
before anything can drive it.

### What VIRPIL will need decided

Gear expected around January 2027; see the blocked item in
[TODO.md](TODO.md). The transport is the easy part. These are not:

1. **Volatile or persistent?** VIRPIL's LED settings are configured in the VPC
   Configuration Tool and saved to the device. If the only path to the LEDs is
   writing that persistent config, this project cannot use it: that would mean
   flash writes at signal rate, wearing the device and clobbering whatever the
   user saved in VPC. It is exactly what `wctrl-hid`'s forbidden list refuses
   and for the same reason. **The first thing a capture has to answer is
   whether moving the brightness slider in VPC produces traffic immediately or
   only on save to device.** Everything else waits on that.

2. **RGB breaks the `u8`.** VIRPIL backlights are colour, and a lamp here is one
   byte from `Led.max` through the binding's on/off values to the editor's
   number inputs. The cheap answer, and the whole of "run the backlights through
   the program", is a fixed colour per lamp in `data/devices.json` with the
   backend scaling it by the brightness the engine already sends: no engine,
   schema or editor change. Colour as a bindable signal, so a lamp turns red on
   a caution, is a much larger separate feature and should stay one. The fixed
   field is forward compatible with it either way.

3. **Capture is harder than it was here.** SimAppPro wrote `WWTHID.log` and
   handed us the frames, which is what `docs/PROTOCOL.md` is built on. VPC
   almost certainly does not, so this means USBPcap and Wireshark: raw URBs,
   with no vendor pretty-printing and no help separating a command channel from
   routine HID polling. Two things already here transfer: `declares_output` in
   `wctrl-hid` walks a report descriptor for an Output item, which says whether
   host-to-device writes are even declared before any sniffing, and `devices`
   with the vendor filter lifted reads the real ids off the hardware rather
   than trusting a number from memory.

Findings observed on the wire are ours. Anything taken from decompiled VPC
internals is not, and does not go in; see `THIRD_PARTY_NOTICES.md` for how the
WwDevicesDotnet port is attributed.

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
  `crates/dsc-engine/tests/flap_capture.rs` replays the real capture, lever
  values included.
- **A daemon started mid-mission syncs to the cockpit on its own.** DCS-BIOS
  re-exports on a cycle rather than sending deltas only: word 0 of `_ACFT_NAME`
  arrived 67 times in 20 seconds, about every 300 ms. Documented the other way
  round until 2026-09-16, which produced a README rule telling users to start
  before entering the cockpit. There is no ordering requirement.
- **`dcs-signal listen` takes repeated `--watch` by signal name.** Watching a gauge
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
crates/wctrl-hid      frames, part discovery, SET_LEDX, 0xf0
crates/dsc-bios       export-stream decoder + address space
crates/dsc-config     catalogue, inventory, profiles, displays
crates/dsc-engine     aircraft detection, sweep, writes, learn
crates/dsc-cli        the dcs-signal binary
crates/dsc-cli/src/panels  Protocol/Panel traits, one module per brand
editor/src-tauri      editor backend, learn listener, claims
data/defaults         shipped profiles, tracked in git
data/profiles         active profiles, gitignored, seeded from data/defaults
editor/               Tauri 2 editor: vanilla TS + Vite, src-tauri in the workspace
crates/dsc-cli        `dcs-signal`  devices/parts/led/blink/sweep/listen/learn/run
data/catalogue        51 modules, generated, version-stamped
data/devices.json     every connected panel verified; each names its protocol
tools/                HID probe, WWTHID log parser and tail, release,
                      version and snapshot scripts, the pinned DCS-BIOS
                      fetch, daemon benchmark (docs/PERFORMANCE.md), DED
                      font generator
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
3. ~~**Tauri editor** scaffold.~~ Done 2026-09-16. Renaming was built and
   used 2026-09-19, and there is no editor for an aircraft list by design; see
   "Where the editor stands". Nothing on that list is outstanding.
4. ~~**Seeding by aircraft, not file name.**~~ Done 2026-09-19. Seeding copied
   any default whose file name was missing, so renaming a shipped profile left
   existing installs with two profiles claiming one aircraft. A default whose
   aircraft are all claimed is now skipped.
5. ~~**Text output fields.**~~ **Built and flown 2026-09-20.** A field is
   a chain of pieces, each characters the user typed, a signal, or a gap, and
   each with its own colour and size. Font selection came with it, for aircraft
   without a native CDU only. The editor lists every area of every screen in
   the order it sits on the glass, which also fixed fields landing out of order
   as they were added. See the entry at the top of this file for the decisions
   and what they cost, and "Content: what fills a field" in `CONFIG.md` for the
   model.

## Method note

Several assumptions here were wrong and expensive: that indicators take 0-255,
that a vendor table could be trusted, that an ack meant an effect. The pattern
was asserting inference as fact and then reading failures as confirmation.

Capture beats inference on this hardware, and SimAppPro will tell us exactly what
it sends set `"HIDLog": true` in `%APPDATA%\SimAppPro\config.json`, restart it,
and read `%APPDATA%\WWTHID\SimAppPro\WWTHID.log` (see `PROTOCOL.md`). Fields that
have not been measured are left absent rather than given a plausible default, and
`verified: true` marks the ones that have.
