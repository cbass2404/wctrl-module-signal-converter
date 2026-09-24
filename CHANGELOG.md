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

## 1.0.0-alpha.009

### Fixed

- **Pages load again after updating from an earlier release.** Alpha 008
  looked for page files under lowercase names, such as `ah-64d.json`, but a
  library from an earlier release still had them under their old names, such
  as `AH-64D.json`. Windows took the old file for the new one, so it was
  neither loaded nor replaced, and every page slot on those aircraft came up
  empty with a warning that the file "says it holds pages for" its own
  aircraft. The old files are now renamed when the editor or the converter
  starts, with your pages in them as you left them, and the pages alpha 008
  added for those aircraft, such as the A-10C CDU, the F-16 DED and the
  Hornet UFC, are added to them. Where alpha 008 had
  already added a lowercase file beside an old one, as it did for the F-16
  and the Hornet, the old file is kept and the added one is set aside as
  `.json.seeded`.
- **Clear app data on uninstall now clears pages and settings too.** It removed your
  profiles but left your pages and settings behind, so a reinstall picked up
  the old page files again.
