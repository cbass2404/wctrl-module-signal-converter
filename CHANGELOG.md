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

- **The signal tooltip says how long a text signal is** instead of showing
  "0 to 65535", which suggested a range to convert.

- **What DCS-BIOS says a signal is no longer stops a profile loading.** Its
  metadata is not right for every module. A range or aliases on a signal it
  calls text, or a highlighting signal it calls a number, is now a caution:
  the profile loads and you judge the result on the panel.

### New Features

- **Show a switch position as a word.** A reading can now be drawn "as
  aliases": each value gets the text to draw in its place, so the F-16 CMDS
  mode knob can read `SEMI` instead of `3`. A switch whose positions DCS-BIOS
  names starts with those names filled in. Shorten them to fit your cells. A
  value with no alias draws as the number.

### Shipped profiles
