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

## 1.0.0-alpha.006

### Changed

- **A labelled rule no longer has to be given a fixed width.** A rule with a
  line to itself is the whole line in every frame, so its label was never
  going anywhere, and the profile was refused for a width it did not need.
  The width is now only asked for where a reading beside the rule can squeeze
  it, and there it is a caution on the field rather than a refusal: the rule
  draws either way, and the label is dropped only in the frames where the
  reading really does take the room.

- **Cautions about a display field now show on that field** rather than in
  the list at the top of the profile: text too wide for its cells, and
  settings DCS-BIOS says mean nothing. The top of the profile keeps what is
  about the whole profile, what will stop it loading, and signals your
  DCS-BIOS version lacks.

### Fixed

- **Opening the options or controls menu mid-flight no longer blanks the
  panels.** The menu pauses DCS-BIOS, and after 20 seconds of that every
  lamp and screen was cleared and rebuilt on the way back. The panels now
  keep the last cockpit until a new aircraft loads or DCS closes.

- **Importing a profile over one with the same name works.** Taking every
  aircraft from the old profile deletes it, so its name is free, but the
  import still refused the name as taken.

- **A number shown as sent no longer stops a profile loading.** The editor
  offered "as sent" for a switch or a count, and the profile was then refused
  for having no range.

- **A converted reading at zero draws 0, not -0.** A face that starts below
  zero, such as a g meter, drew `-0.0` just under zero.

- **The signal tooltip says how long a text signal is** instead of showing
  "0 to 65535", which suggested a range to convert.

- **What DCS-BIOS says a signal is no longer stops a profile loading.** Its
  metadata is not right for every module. A range or aliases on a signal it
  calls text, or a highlighting signal it calls a number, is now a caution:
  the profile loads and you judge the result on the panel.

### New Features

- **Give each seat its own version of a display field.** On an aircraft with
  more than one crew station, a field set to one seat now offers a copy for
  each seat that has none on those cells yet, so the pilot and the gunner can
  see different things in the same window. The copy starts out the same as
  the field it came from, ready to point at the other seat's signals.

- **See what the UFC and the DED will draw.** The editor drew a field before
  you flew it only on the MCDU. The UFC and the ICP's DED are the screens
  where that is worth most: they draw from a table of their own, where a value
  lights a set of segments or pixels that need not look much like the
  characters it was keyed by, and a two character comm channel is one glyph on
  one cell. Both are now drawn the way the panel will draw them, segment by
  segment and pixel by pixel, inverse fields included. A cell this glass has
  nothing for is marked rather than left looking like a space, and the line
  under the preview names what would be dark.

- **Show a switch position as a word.** A reading can now be drawn "as
  aliases": each value gets the text to draw in its place, so the F-16 CMDS
  mode knob can read `SEMI` instead of `3`. A switch whose positions DCS-BIOS
  names starts with those names filled in. Shorten them to fit your cells. A
  value with no alias draws as the number.

- **Name a band of a dial, not just one value.** An alias now claims one
  reading, a list of them, or a range: `-1.5..-0.1` draws `ND` anywhere below
  centre. The range is in what the dial is marked with, not the 0 to 65535
  DCS-BIOS sends, so you write the numbers you can read off the gauge and they
  keep meaning the same thing if you retune the range. A needle sitting
  between two bands lands in one of them. A reading no band claims still draws
  as the number, so a face can be part named and part read.

- **An alias can have its own colour.** A band is often a warning about where
  the needle is, and a warning in the same colour as the row around it is one
  nobody catches. Aliases without a colour are drawn in the piece's colour as
  before.

- **An alias can draw inverse**, on screens that draw inverse at all, such as
  the DED. It is how a band stands out on glass with no colours, and an alias
  of a single space ticked inverse draws a solid block.

- **Draw a reading without its sign.** A face that runs each way from zero is
  read as a magnitude and a direction: the F-16's trim indicators are marked
  in units nose up and units nose down, so `-1.0 ND` says the same thing
  twice. Tick "without its sign" and the number is the magnitude, with a band
  beside it naming the direction. Offered only on a range that goes below
  zero, since it does nothing to any other.

- **Aliases and a converted range work together.** Naming values used to mean
  giving up the conversion, and the two were a choice of one. The menu now
  picks "as sent" or "converted to", and aliases sit on top of whichever it
  is. Nothing you have already set up changes.

- **Drums, counters and dials that go all the way round.** A converted
  reading can now round down instead of to the nearest, and start again from
  0 at a value you choose. One odometer drum digit is 0 to 10, rounded down,
  wrapping at 10: it shows each digit once the drum has clicked over to it,
  and 0 after 9. A compass is 0 to 360 wrapping at 360, so it reads 0 at the
  top instead of 360. A signal that makes several turns is its whole travel
  wrapping at one turn: twelve turns of 0 to 999 is 0 to 12000 wrapping at
  1000.

### Shipped profiles
