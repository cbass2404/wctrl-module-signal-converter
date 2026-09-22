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

## 1.0.0-alpha.006

### Changed

### Fixed

- **Opening the options or controls menu mid-flight no longer blanks the
  panels.** The menu pauses DCS-BIOS, and after 20 seconds of that every
  lamp and screen was cleared and rebuilt on the way back. The panels now
  keep the last cockpit until a new aircraft loads or DCS closes.

### New Features

### Shipped profiles
