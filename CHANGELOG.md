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

## 1.0.0-alpha.007

### New Features

- **Import only part of a profile.** Import... can now merge a profile into
  one you already have on the same module instead of bringing it in whole.
  Tick the panels whose lights you want and the screen lines you want; you are
  shown exactly what will be added, replaced and removed, and nothing changes
  until you confirm.
- **Merge from another profile.** Profiles that read the same module, such as
  the F-14 and F-14BU, have a Merge from... button that takes lights and
  screen lines from one into the other the same way, so a change made in one
  no longer has to be made again by hand.
- **Select all** heads the aircraft list in New profile and Import whenever
  there is more than one aircraft to choose.
