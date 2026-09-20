# TODO

What is still outstanding, in one place, so nothing has to be reconstructed by
reading [STATUS.md](STATUS.md) end to end. This file is the checklist only: why
a thing is shaped the way it is, and what flying it taught, belong in
`STATUS.md`, and anything a user would notice belongs in
[CHANGELOG.md](../CHANGELOG.md) under the version in
[VERSION.md](../VERSION.md), as it lands.

Line links point into `STATUS.md` as it stood on 2026-09-20. If one lands in
the wrong place, search the phrase beside it; line numbers move and the words
do not.

## Next

- [ ] **Text output fields.** One way to author screen content across the UFC,
      the ICP DED and the MCDU, each input constrained by its cell's
      parameters (width, allowed characters, rows) read from the display
      catalogue rather than derived again.
      [STATUS.md:1142](STATUS.md#L1142)
  - [ ] Font selection, for aircraft without a native CDU only: one that has
        a CDU takes its font from the aircraft (`native_fonts`) and offers no
        choice. [STATUS.md:423](STATUS.md#L423)

## Release

- [ ] **The release-notes list of changed default rows.** Still written by
      hand. An update never rewrites a row the user has changed, so a fix
      reaches them only if the notes name the row and they choose to reset it.
      [STATUS.md:298](STATUS.md#L298), the rule at
      [STATUS.md:228](STATUS.md#L228) and
      [CHANGELOG.md:8](../CHANGELOG.md#L8)

## Smaller

- [ ] **Replace the drawn DED glyphs** as captures turn up. The ones in
      `data/displays/ded.json` that were drawn by hand rather than captured.
      [STATUS.md:362](STATUS.md#L362)
- [ ] **Name a display from a device spec**, so a part can carry one.
      [STATUS.md:825](STATUS.md#L825)

## Deferred, not scheduled

- [ ] **Profile inheritance.** Leaning no for v1.
      [STATUS.md:1122](STATUS.md#L1122)
- [ ] **Backlight contention** with SimAppPro's "Sync with DCS": the one lamp
      both applications may drive. Detect and warn.
      [STATUS.md:1123](STATUS.md#L1123)
- [ ] **A perceptual response curve for dimmers.** Linear PWM feels wrong at
      the bottom. [STATUS.md:1125](STATUS.md#L1125)
