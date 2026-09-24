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
- **A tidier profile list.** Click a profile's row to open it; the Edit
  button is gone. Copy to..., Export..., Merge from..., Reset and Delete move
  into a menu under the ⋯ button at the end of the row.
- **A guide to the profile language.** The **?** beside New profile, and
  beside Save in a profile, opens a page explaining every test, alternative,
  reading, piece and page slot, with examples to try and a dictionary of
  every term.
- **The PFP-3N, PFP-7 and PFP-4.** WinWing's Boeing CDUs are supported under
  each of their Captain, Co-Pilot and Observer names. Their screen is the
  MCDU's, so every MCDU page shows on them too, swapped with LSK 1L to 6L the
  same way. Their lamps (DSPY, FAIL, MSG, OFST, EXEC) and keys are their own,
  so a PFP follows only another of its own model, never an MCDU. None has
  been tried on a real panel here yet: the support is built from
  WwDevicesDotnet, where other owners have confirmed it works.
- **The page editor stays open after a save.** Save page and Save as new
  page leave you where you were in the page, to keep going; Close shuts it.
  After Save as new page, the editor carries on with the new copy. While a
  page is open, its Save, Delete and Close buttons stay at the foot of the
  window, so you can save without scrolling away from the field you changed.
- **An open panel's name stays in view.** While you scroll through an open
  panel, its title row stays under the header, so you can close it from
  anywhere in it. It moves on when the next panel comes up.
- **Updates can split a shipped profile.** When a release gives an aircraft
  variant a profile of its own, an update moves that aircraft to the new
  profile, as long as you have not changed which aircraft the old profile
  flies. The new profile arrives as shipped. Your edits stay in the old one,
  and Merge from... can carry them across. If you did change the aircraft
  list, it is left alone, and the update log says what the release moved, so
  a Reset brings the split when you want it.

### Breaking

- **A profile with fields of its own on the UFC or the DED no longer loads.**
  Everything on those screens now comes from a page, as on the MCDU. A
  profile you have not changed is brought over by the update; one you edited
  or made yourself is skipped by the converter and listed with the reason in
  the editor. Reset your app data before installing this release, or move
  those fields onto a page.
- **Page files are named in lowercase, like profiles.** The pages for
  `FA-18C_hornet` are now in `fa-18c-hornet.json`, not `FA-18C_hornet.json`.
  A page file under its old name no longer loads, and every slot on its
  module is left empty. Reset your app data before installing this release,
  or rename the files in the `pages` folder.
