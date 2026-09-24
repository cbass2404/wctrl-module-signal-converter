# TODO

What is still outstanding, in one place, so nothing has to be reconstructed by
reading [STATUS.md](STATUS.md) end to end. This file is the checklist only: why
a thing is shaped the way it is, and what flying it taught, belong in
`STATUS.md`, and anything a user would notice belongs in
[CHANGELOG.md](../CHANGELOG.md) under the version in
[VERSION.md](../VERSION.md), as it lands.

Line links point into `STATUS.md` as it stood on 2026-09-22. If one lands in
the wrong place, search the phrase beside it; line numbers move and the words
do not.

## Next

- [x] ~~**Name the pages, then move the shipped defaults onto them.**~~
      Done 2026-09-23: every shipped profile is version 2, and the six
      aircraft with MCDU content each have one page in slot 1. Checked with a
      dry run and the tests, not yet seen on the panel.
      [STATUS.md](STATUS.md), "Built 2026-09-23: MCDU pages"

- [x] ~~**Build page swapping.**~~ Built 2026-09-23 on
      `feature/mcdu-page-selection-inputs`, with the Settings dialog behind
      the gear. [STATUS.md](STATUS.md), "Built 2026-09-23: page swapping"

- [x] ~~**Swap pages on the panel.**~~ Flown 2026-09-23 in the A-10C: odd
      slots a page, even slots blank, the last disabled. Every combination of
      Ctrl, Shift and Alt, left and right, with the modifier changed in
      Settings mid-flight and run through again. Pages swapped, blanks went
      dark and the disabled key did nothing, each only when intended.
      [CONFIG.md](CONFIG.md), "Swapping"

- [x] ~~**Settings and import with pages.**~~ Proven 2026-09-23: the
      modifier, the three themes (the first look at the light theme) and the
      theme surviving a restart, Import profile... and Manage Converter...
      from the gear, a version 1 profile refused on import, a whole import
      offering its pages, and a partial merge from an F-14BU export bringing
      its page slot into the F-14.

- [ ] **Fly pages on the UFC and the ICP.** Built 2026-09-24, checked with
      the tests and a dry run only. In the F/A-18 (UFC page) and the F-16
      (DED page): the start page looks as before, a page, a blank and a
      disabled slot swap as on the MCDU, and the modifier held alone is the
      only thing that swaps. Then the A-10C CMSC and Mi-24P Radios pages.
      [STATUS.md](STATUS.md), "Built 2026-09-24: pages on the UFC and the ICP"

- [ ] **Decide on the dead field-reset code.** With every screen on pages,
      nothing in a profile's own `readouts` is valid, so Reset this field,
      "+ the field that shipped here", the `shipped` argument of `fieldTable`
      and line merging in `merge.rs` can no longer be reached. Remove, or
      give pages a reset of their own.

- [ ] **Site page for UFC and ICP pages.** `docs/index.html` still says pages
      are the MCDU's; bring it current once flown.

- [ ] **See the pages on the panel.** Fly one aircraft per page file and check
      the MCDU looks as it did before the move: A-10C CDU, AH-64D KU, CH-47F
      CDU, F-14BU CDNU, F-16 Flight, F/A-18 IFEI.

- [ ] **Open the page editor in the window.** Built and type-checked, not yet
      clicked through: the six slots (Disabled, Blank, pages), Edit page and
      New page, Save page, Save as new page, Delete page, and export, import
      and merge with pages. The shipped defaults are version 2 now, so any
      of the six aircraft with a page will do.

- [x] ~~**Text output fields.**~~ Built and flown 2026-09-20, on the panel
      with DCS feeding it: a chain on the A-10C's free rows from the A-10C II
      fuel strings, pieced together with typed text, and labels put on rules.
      A field is a chain of pieces, each characters the user typed or a signal,
      each with its own colour and size. Font selection came with it, and the
      editor now lists every area of every screen in the order it sits on the
      glass. See [CHANGELOG.md](../CHANGELOG.md) and "Content: what fills a
      field" in [CONFIG.md](CONFIG.md).

- [ ] **Watch the panels stay lit through the options menu.** Built
      2026-09-22, not yet seen in DCS. Mid-mission, sit in the options or
      controls menu for more than 20 seconds: every lamp and screen should
      keep the last cockpit and come back without a blank and a rebuild. Then
      quit DCS: the panels should clear and the daemon exit on its own.
      [STATUS.md:25](STATUS.md#L25)

- [ ] **Look at the UFC and DED glass for the preview.** Two things only the
      panel can answer. What colour each glass is, since both previews are
      drawn white and a colour per display is a small change in `paintInk`.
      And whether each UFC segment sits where `art` in
      `data/displays/ufc1.json` draws it, which was read out of the glyph
      table rather than captured; a photograph of a few lit cells settles it.
      [STATUS.md:83](STATUS.md#L83), [STATUS.md:94](STATUS.md#L94)

- [ ] **Fly `follows` on two MCDUs.** Built 2026-09-21 and tested, not yet on
      a panel. Point the Co-Pilot unit at the Captain in the Hornet and check
      that both show the IFEI page and dim together.
      [STATUS.md:98](STATUS.md#L98)

- [x] ~~**Point the defaults' backlights at one lamp across panels.**~~ Done
      2026-09-21: every backlight in every default matches the MFD C's
      `INST_PNL_Backlight`, moved in the editor. The changed rows are named in
      CHANGELOG.md.

- [x] ~~**Fly a cross-panel `same_as`.**~~ Proven on the panels 2026-09-21: a
      dimmer pointed at a dimmer on another device follows it.

- [x] ~~**Decide whether shipped defaults use `follows`.**~~ Decided
      2026-09-21: they do. In every default the MFD L and R use the MFD C and
      the MCDU Co-Pilot and Observer use the Captain; their own rows are kept.

- [x] ~~**Decide whether the IFEI page ships as the Hornet's MCDU default.**~~
      It did, on the MCDU Captain, in 1.0.0-alpha.004.

- [x] ~~**Watch the update reconcile on a real upgrade.**~~ Seen on the
      alpha.003 and later upgrades: untouched rows corrected, rows changed by
      hand left alone.

- [x] ~~**Fly the #30 display work.**~~ Flown 2026-09-22: the F-16 MCDU
      flight page (drums, CMDS aliases, trim bands), the A-10C MCDU radio
      rows and DED countermeasures page, and a per-seat copy of a field.

## Release

- [ ] **The release-notes list of changed default rows.** Still written by
      hand. An update never rewrites a row the user has changed, so a fix
      reaches them only if the notes name the row and they choose to reset it.
      [STATUS.md:681](STATUS.md#L681), the rule at
      [STATUS.md:1119](STATUS.md#L1119) and
      [CHANGELOG.md:9](../CHANGELOG.md#L9)

## Smaller

- [ ] **Move the DED glyph generator into `tools/`.** `gen_ded.py`, with
      `extract.py` and `glyphs.json`, regenerates `data/displays/ded.json`
      from the captures plus the hand-drawn glyphs, and it exists only in an
      old session's scratchpad under `%TEMP%`, which Windows can clear. The
      next item needs it.
- [ ] **Replace the drawn DED glyphs** as captures turn up. The 27 in
      `data/displays/ded.json` that were drawn by hand rather than captured.
      [STATUS.md:817](STATUS.md#L817)
- [ ] **Name a display from a device spec**, so a part can carry one.
      [STATUS.md:1334](STATUS.md#L1334)

## Blocked on hardware

- [ ] **VIRPIL backlights.** Blocked until the gear arrives, expected around
      January 2027. The seam it plugs into is already in: every device names a
      protocol, and `crates/dsc-cli/src/panels/` holds the `Protocol` and
      `Panel` traits with `wctrl` as the only implementation. Adding a brand is
      a module there and a name in `panels::all()`; nothing above it changes.

      Do not start writing a backend before there is a capture. In order:

      1. **Answer the one question that decides feasibility.** USBPcap and
         Wireshark on the VPC Configuration Tool: does moving the LED
         brightness slider produce traffic immediately, or only on save to
         device? Persistent-only means flash writes at signal rate and this
         cannot be built on it.
      2. **Read the ids and the descriptors off the real hardware** rather than
         trusting a remembered vendor id. `declares_output` in `wctrl-hid`
         already says whether an interface declares host-to-device writes.
      3. **Then** the backend, backlights only, fixed colour per lamp so the
         engine keeps sending one byte of brightness.

      Colour as a bindable signal is a separate, much larger feature and is not
      part of this. See "What VIRPIL will need decided" in STATUS.md for why
      each of those is in that order.

## Deferred, not scheduled

- [ ] **Profile inheritance.** Leaning no for v1.
      [STATUS.md:1725](STATUS.md#L1725)
- [ ] **Backlight contention** with SimAppPro's "Sync with DCS": the one lamp
      both applications may drive. Detect and warn.
      [STATUS.md:1726](STATUS.md#L1726)
- [ ] **A perceptual response curve for dimmers.** Linear PWM feels wrong at
      the bottom. [STATUS.md:1728](STATUS.md#L1728)
- [ ] **The 175 ms pass at mission start.** Loading a profile and painting
      every screen is one pass of the main loop, and nothing else runs during
      it. Watch only: worth a look if the panels ever feel behind at mission
      start. [STATUS.md:445](STATUS.md#L445)
