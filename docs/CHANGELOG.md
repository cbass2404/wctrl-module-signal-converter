# Changelog

What changed in each release, for the people running it. The release pipeline
puts the matching section at the top of the release notes, so anything written
here reaches users; development detail belongs in
[docs/STATUS.md](docs/STATUS.md) instead.

**When a shipped profile changes**, say so here and name the rows. An update
adds new rows but never rewrites one you have changed, so a fix to a shipped
lamp only reaches you if you reset that lamp, and you cannot decide to unless
this says what moved.

## 1.0.0-alpha.002

### Dividers on the MCDU screen

A divider draws a fixed line across a row of the MCDU. It reads no signal and
never changes, so it is there from the moment the aircraft loads. It is for a
page that does not fill the screen, which would otherwise run off into the dark
with no edge to it.

**Two shipped profiles gained one**, so you will see a line appear the first
time you fly them after updating:

| Profile | Where | Why |
| --- | --- | --- |
| A-10C | Row 4, above the first CDU line | The CDU is ten lines on a screen of fourteen |
| AH-64D | Row 13, above the keyboard unit | The Apache exports only the KU, on the bottom row |

The F-14B (Upgrade) gets none: its CDNU comes within two rows of filling the
screen already.

Both arrive on their own, because an update adds rows it has not seen before.
If you had already put something of your own on those cells, nothing is
touched and no divider is added there.

**Add your own** with **Add a divider**, beside Add a field on any MCDU
section in the editor, and pick the colour it draws in. Dividers are an MCDU
thing: the UFC and the ICP draw from a fixed set of shapes with no line in it.

### Manage Converter

A new button on the profile list, for the few times the converter needs
restarting. Editing a profile is not one of them: a running converter picks up
a saved profile within about a second, and it always has.

What it is for is narrower:

- the converter stopped while DCS kept running, which the DCS hook cannot
  recover from, because it starts one when a mission begins and never learns
  that it has gone
- you plugged a panel in after it started, since panels are found once
- you updated DCS-BIOS and the signal catalogue was rebuilt underneath it

**Restart** stops it, waits for it to clear the panels, and starts a fresh one
exactly the way a mission start does. **Kill** is for a converter that will not
answer at all; it cannot clear the panels, because the lamps latch and a killed
program runs none of its shutdown, so start it again and stop it properly to
clear them.

`dcs-signal stop` does the same from the command line, which is the way to stop
one you did not start yourself.

### Fixed

- **A display field added by an update never arrived.** An update that added a
  field to a shipped profile without also adding a lamp merged the field and
  then discarded it, silently, every time the converter started. That is how
  the new dividers would have reached nobody.
