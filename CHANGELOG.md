# Changelog

What changed in each release, for the people running it. The release pipeline
puts the matching section at the top of the release notes, so anything written
here reaches users; development detail belongs in
[docs/STATUS.md](docs/STATUS.md) instead.

**When a shipped profile changes**, say so here and name the rows. An update
never rewrites a lamp row you have changed, so a fix to a shipped lamp only
reaches you if you reset that lamp, and you cannot decide to unless this says
what moved. Display fields are the exception from alpha.003 on: one still
exactly as it shipped is corrected for you, and one you have touched is left
alone, so naming what moved matters there too.

## 1.0.0-alpha.003

### New Features

- **Screens take text you write, not just readings.** A field is now a chain of
  pieces drawn end to end, each one either characters you type or a signal, and
  each with its own colour and size. So a row can read
  `RALT` in small red, then the radar altimeter, then `M`, instead of a bare
  number with nothing saying what it is. Works on the MCDU, the ICP DED and the
  UFC, and on every aircraft.

  There is more room for this than it sounds. The A-10C's CDU is ten lines on a
  screen of fourteen, so rows 1 to 3 are yours; the AH-64D puts its keyboard
  unit on the bottom line and leaves twelve rows free; the F-14BU leaves six.

  **A gap is a piece that draws nothing and takes whatever the rest of the row
  leaves**, so `FUEL`, a gap, then the reading puts one at each end of the line
  with the blank between them worked out rather than typed. Typing the spaces
  in works until the reading changes width, which is exactly when it matters.
  Two gaps space three pieces evenly.

  The window works out how wide the content can get and says so before you fly
  it. A run of cells is a fixed width and the panel gives no sign when content
  runs past it: the write goes out looking healthy and the end is simply not
  drawn. Where every piece has a known width, and DCS-BIOS declares one for
  text and you give one for a gauge, it says exactly how many characters would
  be lost. It stays a warning: whether your aircraft ever sends a reading that
  wide is yours to judge.

- **Pick the font for a screen on an aircraft that has no CDU of its own.** The
  MCDU held a font for each aircraft DCS-BIOS exports a CDU for and refused
  fields on any other, which left the screen unusable in most of the fleet.
  Choose one of the four and it works. An aircraft that has its own CDU still
  takes its own font and offers no choice, because those glyphs were drawn to
  match what the module sends and another font would draw the wrong symbol
  rather than the same one differently.

  The F-14BU font is the one to reach for when writing your own text: it is the
  only one of the four with lowercase, and it has `!`, `#`, `?` and `@` as
  well. The others are uppercase only.

  The window draws your line in the font the panel will use, from the font's
  own bitmaps, because checking the characters is not enough: these fonts reuse
  slots, and in the A-10C font `%` draws a question mark. A character the font
  has no glyph for is refused before you fly rather than left as a blank cell
  with nothing saying why. Every font draws fewer characters small than large,
  so marking a piece small can take one away.

- **The daemon keeps a log of each session**, in `Saved Games\DCS\Logs\dcs-signal.log`
  beside DCS's own `dcs.log`. Started by the DCS hook it has no console, so
  until now a flight left no account of itself at all; if something goes wrong,
  this is the file to send. It holds where every file was read from, every
  WinCtrl device found, which profiles loaded and which were skipped, the
  signals the profile reads as they move, every lamp and screen written, a
  status line each minute, and the error behind an exit. Each start keeps the
  last session as `dcs-signal.log.bak` and deletes the one before it, so land
  and read it rather than flying on. See [docs/CLI.md](docs/CLI.md).

### Changed

- **Every area of every screen is listed, in the order it sits on the glass.**
  The MCDU shows all fourteen rows, the DED five lines, the UFC its fifteen
  named areas, each either holding a field or offering to. Adding a field used
  to append it to the end of the list however far up the panel it was drawn, so
  the only way to get a screen back into order was to delete every field and
  build it again. Nothing about your profiles changes; this is how they are
  shown. An area already in use can take a second field beside the first, which
  is how one row shows two readings.

- **A rule can carry a label.** A rule ends a page; a labelled rule says what
  the page was, which is what a CDU does with its own. It sits in the middle of
  the line with a blank each side so it does not read as part of it, and it
  takes its own colour, because a label drawn in the line's colour is a label
  nobody sees as one. Left alone it follows the rule's colour. A label with no
  room for its blanks and a dash each side is refused rather than crowded in,
  and its characters are checked against the font like anything else drawn.

- **A rule now runs to the edge of its cells.** It used to leave a blank at
  each end. Beside real CDU lines, which start in the first cell of their run,
  that made the rule the one thing on the screen not lining up with what sat
  above and below it. If you have a rule in a profile you will see it get two
  cells longer; nothing in your file changes, and nothing needs resetting. The
  two blanks around a label stay, since those are what keep it from reading as
  part of the line.

- **"+ another in <row>" is gone.** Two readings on one line is what a chain of
  pieces is for, and a chain can count the cells. That button could not: every
  field it added landed on cells the row already held, so it produced a field
  that said "already taken" and had to be deleted again.

- **Updates now correct display fields you never touched.** Until now a fix to
  a shipped field reached nobody who already had that profile, including people
  who had never opened it, because there was no way to tell a field somebody had
  customised from one sitting exactly as it shipped. An update now compares your
  fields against the defaults as the previous release shipped them:

  - still exactly what we shipped, and we still ship it: you get the new one
  - still exactly what we shipped, and we have retired it: it is taken out
  - changed in any way: left alone, it is yours
  - deleted: it stays deleted, instead of coming back on the next start

  That last one is a fix in its own right. Deleting a shipped field used to be
  undone silently every time the app started.

  This runs once when the version changes, not on every start, so putting a
  field back the way you like it is the last word until a release actually has
  something new to say about it. Lamp rows are untouched by any of it, and a
  profile from two releases back is left entirely alone, since there is nothing
  left to compare it against.

### Shipped profiles

- **AH-64D, the rule on MCDU row 13** (cells 289-310, on all three MCDU names)
  now reads `---- KEYBOARD UNIT ---`, the label in white across the amber rule.
  The Apache exports only its keyboard unit, so everything above that row is
  dark, and the rule that said "the page ends here" now says what the page is.

  You will get this without doing anything if you have not touched that row,
  since an update corrects a display field still exactly as it shipped. If you
  have changed it, the row is yours and is left alone; the reset button on the
  field will put it back the way it shipped if you want it.

### Fixed

- **Closing the window with unsaved changes now asks.** Leaving a profile by
  the back button has always asked. Closing the window, which is how most
  people leave an app, went straight through and took the edits with it.

- **A display field can be put back the way it shipped.** Lamps have had this
  from the start; fields had nothing, so changing one was final and deleting
  one was worse, since there was then nothing on screen to say a field had ever
  been there. Every field the default has a version of now carries a reset
  beside its note, disabled while it already matches, and an area whose shipped
  field was deleted offers it back. Nothing else is touched either way, and a
  profile you made yourself has nothing to go back to and shows no buttons.

- **Deleting a display field asks first, and its button is red.** It was grey,
  took two clicks, and said nothing about what was going: the first click only
  changed the tooltip, since nothing styled the half-pressed state. It is now
  one click and the same dialog a lamp's condition gets, naming the field's
  pieces in the order they are drawn and saying whether the area will offer it
  back. The smaller delete buttons inside a field were asking for the same red
  by a name nothing styled, and every disabled one now looks disabled.

- **Typing into a piece of text lost focus after every character.** Each
  keypress rebuilt the whole field, so the box being typed into was thrown
  away and replaced by an identical one. Only what a keystroke actually
  changes is updated now: what the field will draw, and the line saying how
  wide it comes out.

- **"+ a reading" gave you a text box.** A piece that had not been pointed at a
  signal yet read back as text, so the menu said "text", the text box is what
  you got, and choosing "a reading" in the menu put it straight back. Which of
  the two a piece is no longer depends on whether it has been filled in.

- **A screen that needs a font arrives with one.** The font menu used to open
  on "pick a font...", so a CDU screen on an aircraft without one of its own
  drew nothing until you found the menu and chose, and the preview could only
  say it had nothing to draw with. It now starts on the F-14BU font, which has
  lowercase and ordinary punctuation and so is the one to type into, and the
  empty choice is gone: a screen with no font is blank on the panel and cannot
  be checked, which was never worth offering. Aircraft with a CDU of their own
  are unaffected, as they always took their font from the aircraft. If you have
  already picked a font it is left exactly as it is.
