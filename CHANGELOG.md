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
  Tick the lights you want, a lamp at a time or a whole panel at once, and the
  screen lines you want; you are shown exactly what will be added, replaced and
  removed, and nothing changes until you confirm.
- **Merge from another profile.** Profiles that read the same module, such as
  the F-14 and F-14BU, have a Merge from... button that takes lights and
  screen lines from one into the other the same way, so a change made in one
  no longer has to be made again by hand.
- **Select all** heads the aircraft list in New profile and Import whenever
  there is more than one aircraft to choose. Import starts with every aircraft
  ticked; taking one from another profile is still asked first.
- **MCDU pages.** What the MCDU shows is now built as named pages, kept in a
  page library for each aircraft module, rather than inside a profile. Each
  MCDU in a profile has six slots, one per left line select key (LSK 1L to
  6L), and one slot is marked as the page shown when a mission starts. A slot
  shows a page, shows a blank screen, or is disabled. The editor lists the six
  slots, and opens the field editor only when you choose Edit page or New
  page; Save page writes the page to the library, where every profile on that
  module showing it picks up the change. Save as new page copies it, and
  Delete page empties every slot showing it, after listing them.
- **The shipped MCDU screens are pages now**, each in slot 1 and shown at
  mission start, so they look as before: CDU on the A-10C and CH-47F, KU on
  the AH-64D, CDNU on the F-14BU, Flight on the F-16 and IFEI on the F/A-18.
- **Pages travel with a profile.** Export writes the profile with every page
  its slots show, and any other pages on the module you tick. Import lists
  the pages it brings, each ticked on its own and renamed there if a page here
  already has the name; a page already here unchanged is left alone. Merge
  from... takes MCDU slots instead of screen lines: slot n replaces slot n,
  and brings its page with it.

- **Swap MCDU pages from the panel.** Hold Ctrl and press a left line
  select key to put that slot's page on the screen: LSK 1L for slot 1 up to
  LSK 6L for slot 6. A blank slot takes the screen dark, a disabled slot's key
  does nothing, and every mission still starts on the start page. Each MCDU
  swaps on its own, including one that follows another. The key held can be
  Ctrl, Shift or Alt, and counts only when held alone, so Ctrl+Shift with a
  line select key stays yours for DCS. DCS still sees every press, so keep
  the combination you choose unbound there.
- **Settings.** The gear on the Profiles page opens Settings: the window's
  theme (follow Windows, light or dark), the key held to swap MCDU pages, and
  Import profile... and Manage Converter..., which moved there from the top
  of the page.

### Breaking

- **Profiles made before MCDU pages no longer load.** The profile format is
  now version 2, and a version 1 profile is skipped by the converter and
  listed with the reason in the editor. There is no conversion: reset your
  app data before installing this release.
