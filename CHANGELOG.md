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

## 1.0.0-alpha.008

### New Features

- **Pages on the UFC and the ICP.** The UFC and the ICP's DED now take pages
  the way the MCDU does: each has six slots, one marked as the page shown when
  a mission starts, and the same Edit page, New page, Save page and Delete
  page. Hold the page key modifier from Settings and press a key to swap: on
  the UFC, A/P, IFF, TCN, ILS, D/N and BCN are slots 1 to 6, and on the ICP,
  COM 1, COM 2, IFF, LIST, A-A and A-G. Each screen's slots offer only the
  pages made for that screen. Export, Import and Merge from... carry
  their pages and slots as they do the MCDU's.
- **Panels grouped by what is plugged in.** A profile's panels are listed
  under Active Devices (plugged in and driven), Inactive Devices (plugged in,
  but "drive this panel" is off) and Devices not found (supported, not
  plugged in), each in alphabetical order. Every one opens: a panel left
  alone can be read to see how it was set up, and one not plugged in can
  still be set up. Plugging a panel in or pulling it out while the page is
  open moves it to its group within a couple of seconds, keeping your edits.

### Breaking

- **A profile with fields of its own on the UFC or the DED no longer loads.**
  Everything on those screens now comes from a page, as on the MCDU. A
  profile you have not changed is brought over by the update; one you edited
  or made yourself is skipped by the converter and listed with the reason in
  the editor. Reset your app data before installing this release, or move
  those fields onto a page.
