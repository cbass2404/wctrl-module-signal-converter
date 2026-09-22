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

### Fixed

- **Opening the options or controls menu mid-flight no longer blanks the
  panels.** The menu pauses DCS-BIOS, and after 20 seconds of that every
  lamp and screen was cleared and rebuilt on the way back. The panels now
  keep the last cockpit until a new aircraft loads or DCS closes.

- **Importing a profile over one with the same name works.** Taking every
  aircraft from the old profile deletes it, so its name is free, but the
  import still refused the name as taken.

### New Features

### Shipped profiles
