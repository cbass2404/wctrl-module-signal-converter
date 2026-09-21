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

### Changed

### Shipped profiles

### Fixed

- The editor refused a label on a rule that the panel would have drawn. It was
  still asking for a blank margin at each end of the line, which went when the
  rule was changed to run corner to corner, so it wanted two cells more than
  the rule actually needs.
