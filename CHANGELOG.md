# Changelog

What changed in the release being prepared, for the people running it. Only
the current version is kept here: the release pipeline puts this section at
the top of that release's notes, and each earlier release already carries its
own. Anything written here reaches users; development detail belongs in
[docs/STATUS.md](docs/STATUS.md) instead.

**When a shipped profile changes**, say so here and name the rows. An update
never rewrites anything you have changed: a lamp row, a display field, or a
setting such as which panel follows which. Anything still exactly as the last
release shipped it is brought up to the new one for you. A fix to something
you have touched only reaches you if you reset it, and you cannot decide to
unless this says what moved.

## 1.0.0-alpha.010

### Added

- **Undo unsaved changes, on one lamp or one page field.** Puts it back the
  way it was last saved and leaves everything else alone. It shows only while
  there is something to undo, on your own profiles and pages as well as the
  shipped ones. A shipped lamp or field with changes offers both this and
  Reset, which goes back to how it shipped. A field deleted from a page since
  its last save is offered back in its empty area.
- **Reset a field on a shipped page.** Each field of a page that shipped
  can be put back the way it shipped, and a shipped field you deleted is
  offered back in its empty area.
- **A caution on characters the UFC or DED cannot draw.** Text you type into
  a field on either screen, including an alias or a substitution, is checked
  against what those cells can show, such as a letter on the scratchpad's
  digits. The field is flagged and the profile still loads, since the panel
  may draw it anyway.

### Changed

- **Page fields open for editing one at a time.** Each field in the page
  editor is a line saying which signals it reads, with a picture of what it
  draws. The pencil opens it, the tick keeps the change and the cross puts
  it back as it was when opened, the way a lamp's conditions work. A field
  you add opens by itself. While a field is open, its preview stays just
  above Save page as you scroll through its settings.
- **The DED preview is in the DED's own colours.** A field on the ICP's
  screen was previewed white on black. It is now black on green, as the
  glass shows it, and an inverse field green on black.
- **Merging takes whole page slots, not single screen lines.** Merge from...
  and a merged import no longer list a screen's lines to tick one by one.
  A screen's content comes across as the page slots you tick, the way pages
  are shared everywhere else.
- **Shipped profiles: A-10C and A-10C2.** The notes on the PTO2's CTR, LI,
  LO, RO and RI lamps now say what those lamps show, the NMSP EGI, STEER PT,
  TCN, ILS and ANCHR modes, where they still named the fire lamps they
  replaced. What the lamps do is unchanged.

### Fixed

- **Every character on the F-16's DED is now drawn as SimAppPro draws it.**
  27 were our own drawings, and several were wrong: J, K, Y, the slash,
  the apostrophe, the up and down arrows and most punctuation. The DED now
  uses SimAppPro's own font throughout.
