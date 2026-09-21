# Changelog

What changed in each release, for the people running it. The release pipeline
puts the matching section at the top of the release notes, so anything written
here reaches users; development detail belongs in
[docs/STATUS.md](docs/STATUS.md) instead.

**When a shipped profile changes**, say so here and name the rows. An update
never rewrites anything you have changed: a lamp row, a display field, or a
setting such as which panel follows which. Anything still exactly as the last
release shipped it is brought up to the new one for you. A fix to something
you have touched only reaches you if you reset it, and you cannot decide to
unless this says what moved.

## 1.0.0-alpha.005

### Fixed

- **Updating now carries lamp rows and profile settings forward, not just
  display fields.** Upgrading to alpha.004 left every panel on "its own setup"
  instead of following the one it shipped following, kept each backlight on its
  own knob instead of matching the PTO2's, and left the MCDU without the font
  the new Hornet page is drawn in. The Hornet profile was then refused at
  start and the cockpit stayed dark until the profile was saved again. Lamp
  rows, `follows`, the font and disabled panels are now updated wherever they
  still match what the previous release shipped, and left alone wherever you
  changed them.

## 1.0.0-alpha.004

### New Features

- **A piece of a display field can be held to a fixed width.** Give it a number
  of cells and it takes exactly that many whatever it reads, so the pieces
  after it stay where they are as the reading changes width. Choose where the
  value sits inside it: centred for a label, or right for a number, which keeps
  the digits pinned and grows the blanks in front of them as it counts down
  from 1000 to 9. It also means you can tell, while you are building a row,
  whether anything will ever run off the edge of the screen.
- **A whole field can be centred in its cells**, alongside left and right.
- **A gap can draw a rule instead of blanks**, so a line of dashes can sit
  between two pieces of the same row rather than needing a field of its own.
  Left to size itself it takes whatever the pieces each side leave, so a
  reading that grows eats into the dashes instead of being cropped. Give it a
  fixed width and it can carry a label, with its own colour, exactly as a
  whole-row rule does.
- **One panel sold under several names can share one setup.** The MCDU comes
  as Captain, Co-Pilot and Observer and the MFD as L, C and R, and each used to
  need every lamp and field set up again. Choose **uses** in a panel's header
  and point it at another unit of the same kind: it gets exactly what that one
  has, and its own setup is kept in case you switch back.
- **A lamp can match a lamp on another panel.** "Match another lamp" now lists
  the dimmers on every panel, not only its own, so an MFD's backlight can
  follow the throttle's and the whole pit dims from one place. Two lamps still
  cannot follow each other, on one panel or across two.

### Changed

- **A number on a display is chosen the way a lamp's test is.** Pick the
  signal, then say how to show it: as sent, or converted to what the dial is
  marked with, with the decimals beside it. A needle (0 to 65535) arrives
  converted and anything narrower arrives as sent, so picking the signal is
  usually the only step. Before, every number was forced through a 0 to 100
  conversion, which turned a selector's 0 to 3 into 0, 33, 67 and 100.
- **A number shown as sent is measured from its maximum**, so a field says
  exactly how many characters it would lose instead of warning that it might
  run past its cells.

### Shipped profiles

- **Hornet:** the UFC scratchpad number is held to its 7 cells and aligned
  right within them. It draws exactly as it did. DCS-BIOS sends 8 characters
  for 7 cells, so one of them was always going, and saying which in the profile
  is what stops the editor warning that one might.
- **Every aircraft: one knob dims the whole pit.** Every backlight on every
  panel now matches the centre MFD's backlight (the MFD C's
  `INST_PNL_Backlight`) instead of reading the cockpit knob for itself. That is
  the same knob as before, so the lamps look the same until you change the MFD
  C's row, and then they all follow it. Rows: `Backlight`,
  `INST_PNL_Backlight`, `HUD_INST_PNL_Backlight`, `Marker_Light`, `SL`, `FLAG`,
  `Backlight_L`, `Backlight_R` and `Logo` on every panel except the MFD C.
- **Every aircraft: the MFD L and R use the MFD C's setup, and the MCDU
  Co-Pilot and Observer use the Captain's.** Their own rows stay in the profile
  and come back if you point **uses** back at the panel itself.
- **F-14, F-14BU and Mi-24P:** the MCDU `Marker_Light` and the PTO2 `SL` and
  `FLAG` were held at full. They now dim with the console lights, and go full
  bright when the console lights are off.
- **F-14 and F-14BU:** the PTO2 is wired. `LEFT`, `NOSE` and `RIGHT` show the
  gear, `HALF` and `FULL` the flaps, `FLAPS` and `HOOK` their warning lights,
  `Landing_gear_lights` the gear handle light, and `Master_Caution` lights for
  the pilot's or the RIO's master caution.
- **F-16:** the PTO2 `LEFT`, `NOSE` and `RIGHT` show the gear.
- **AH-64D:** the PTO2 `LI`, `LO`, `RI` and `RO` show the jettison stations
  selected in your seat, `JETT` lights when any station is selected in either
  seat, and `Master_Caution` follows your seat and lights for a master warning
  as well as a master caution.
- **A-10C:** the throttle's `A/A` lights with the master arm at ARM, and `A/G`
  with the GUN/PAC switch at ARM, where the gun is live. The UFC's `HUD_INST_PNL_Backlight` goes full
  bright when the console lights are off.
- **CH-47F and F-14:** the ICP is no longer switched off, so its backlight
  follows the pit.
- **No aircraft:** every panel is switched off, so nothing lights while you
  spectate.

### Fixed

- The editor refused a label on a rule that the panel would have drawn. It was
  still asking for a blank margin at each end of the line, which went when the
  rule was changed to run corner to corner, so it wanted two cells more than
  the rule actually needs.
- A field one cell wide no longer warns that it is about to lose a character.
  A single cell takes its whole value as one glyph, which is how the Hornet UFC
  draws a two-digit comm channel and a scratchpad mark, so nothing was ever
  being dropped. The check was counting characters and put four warnings on the
  Hornet for a screen drawing exactly what it was built to draw.
- Profiles are saved with Windows line endings again. Every profile was written
  with CRLF and the editor was saving them back with LF, which turned a change
  to one row into a change to every line of the file and left it as one long
  line in anything that still wants the pair. The catalogue is written the same
  way now.
- A rule started in an empty area of a screen can now have text, readings and
  gaps added beside it, and can be changed to another kind of piece. It was
  being made as a whole-row rule, which holds nothing else, so the only way
  round it was to add the text first and move a rule above it. An empty area
  can also start with a gap now.
