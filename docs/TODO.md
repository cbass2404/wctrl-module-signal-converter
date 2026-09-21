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

- [x] ~~**Text output fields.**~~ Built and flown 2026-09-20, on the panel
      with DCS feeding it: a chain on the A-10C's free rows from the A-10C II
      fuel strings, pieced together with typed text, and labels put on rules.
      A field is a chain of pieces, each characters the user typed or a signal,
      each with its own colour and size. Font selection came with it, and the
      editor now lists every area of every screen in the order it sits on the
      glass. See [CHANGELOG.md](../CHANGELOG.md) and "Content: what fills a
      field" in [CONFIG.md](CONFIG.md).

- [ ] **Fly `follows` on two MCDUs.** Built 2026-09-21 and tested, not yet on
      a panel. Point the Co-Pilot unit at the Captain in the Hornet and check
      that both show the IFEI page and dim together. See "one panel under
      several names" in STATUS.md.

- [x] ~~**Point the defaults' backlights at one lamp across panels.**~~ Done
      2026-09-21: every backlight in every default matches the MFD C's
      `INST_PNL_Backlight`, moved in the editor. The changed rows are named in
      CHANGELOG.md.

- [x] ~~**Fly a cross-panel `same_as`.**~~ Proven on the panels 2026-09-21: a
      dimmer pointed at a dimmer on another device follows it.

- [x] ~~**Decide whether shipped defaults use `follows`.**~~ Decided
      2026-09-21: they do. In every default the MFD L and R use the MFD C and
      the MCDU Co-Pilot and Observer use the Captain; their own rows are kept.

- [ ] **Decide whether the IFEI page ships as the Hornet's MCDU default.**
      Validated on the panel 2026-09-21; the profile edits are uncommitted
      in `data/defaults/fa-18.json`.

- [ ] **Watch the update reconcile on the alpha.003 upgrade.** It runs once
      when the version changes, so it cannot be exercised by flying; it needs a
      real upgrade. This release gives it one: the AH-64D rule on row 13 was
      labelled, and it is still exactly as alpha.002 shipped it, so three
      fields across the three MCDU names are due a correction. Worth watching:
      that an untouched row takes `KEYBOARD UNIT`, that a row changed by hand
      first does not, that a row deleted by hand stays deleted, and that
      `.updated` lands in the profiles folder naming the version. The tests in
      `crates/dsc-config/tests/profile_reconcile.rs` stand in until then.

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
