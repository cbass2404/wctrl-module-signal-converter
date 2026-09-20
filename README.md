# DCS Signal Converter

**Your WinWing panels, lit by the cockpit you are actually sitting in.**

Gear lamps that follow the gear. A Master Caution that comes on when the jet's
does. Panel backlights that dim with the cockpit's console knob. The Hornet UFC
showing the Hornet UFC, the Viper's DED on the ICP, the A-10C's CDU on your
MCDU screen, and more.

Jump into a Hornet and the panels follow the Hornet. Switch to an Apache and they
follow the Apache. Quit DCS and they go dark, so nothing is left lit.

No SimAppPro necessary to be running in the background, and no alt-tabbing to it when you change
aircraft.

- Starts with DCS on its own. Nothing to remember before a flight.
- Every lamp is yours to reassign in a point-and-click editor, per aircraft.
- Runs unelevated. No DCS, DCS-BIOS, SimAppPro or WinWing file is modified.
- DCS-BIOS is required. It is where every signal comes from.

> **Alpha.** Flown daily on the author's panels, but the USB protocol was
> reverse-engineered and the installer is new, so treat it as community software
> rather than a vendor feature. Bug reports welcome.

An independent project. It is not made, endorsed or supported by WinCtrl or
WinWing; their names appear here only to say which hardware it drives.

---

## Contents

**Getting it running**

- [What it drives](#what-it-drives), the panels and aircraft it ships for
- [Setup](#setup), start here
  - [1. Install DCS-BIOS](#1-install-dcs-bios)
  - [2. Install](#2-install)
  - [3. Fly](#3-fly)
  - [4. Make it yours](#4-make-it-yours), the profile editor
- [Updating and uninstalling](#updating-and-uninstalling)
- [Troubleshooting](#troubleshooting), something is not working

**Background**, none of this is needed to use the tool

- [How it works](#how-it-works)
- [Command line](#command-line), running it by hand
- [Running from source](#running-from-source)
- [Further reading](#further-reading), the detailed docs
- [License](#license)

**Supporting it**

- [Support the Project](#-support-the-project), it is free, but coffee helps

---

## What it drives

**Panels.** Any of these that are plugged in. Panels that are not are skipped, and
a profile can bind one you do not own without harm.

- PTO2 (take-off panel 2)
- Orion Throttle Base II
- CarrierAce UFC + HUD
- ViperAce ICP
- CarrierAce MFD (L, C and R)
- Orion Combat Rudder Pedals
- MCDU (Captain, Co-Pilot and Observer)

Which lamps each one has is in [docs/PROTOCOL.md](docs/PROTOCOL.md).

**Aircraft.** A profile ships for each of these. Any other aircraft gets a blank
starter profile the first time you fly it, ready to fill in with the
[editor](#4-make-it-yours).

| Profile     | Covers                                |
| ----------- | ------------------------------------- |
| A-10C       | A-10C, A-10C II                       |
| AH-64D      | AH-64D                                |
| CH-47F      | CH-47F                                |
| F-14        | F-14A, F-14B                          |
| F-14BU      | F-14B (Upgrade)                       |
| F-16        | F-16C, F-16D and variants, F-16I      |
| FA-18       | F/A-18C, and the EA-18G, E and F mods |
| Mi-24P      | Mi-24P                                |
| FC3         | The Flaming Cliffs aircraft           |
| No aircraft | Spectator and free camera             |

The A-10C, AH-64D, CH-47F and F-14B (Upgrade) also put their own CDU on the MCDU
screen. Open a profile in the [editor](#4-make-it-yours) to see exactly what it
drives.

Every profile puts every panel backlight on one cockpit knob, so the whole pit
dims together until you decide otherwise.

---

## Setup

- [ ] [Install DCS-BIOS](#1-install-dcs-bios) into `Saved Games\DCS\Scripts`
- [ ] Close DCS
- [ ] [Run the installer](#2-install) from the latest release
- [ ] [Start DCS and fly](#3-fly)
- [ ] **Optional** [Open the editor](#4-make-it-yours) to change what a lamp does

Four steps, about five minutes. **SimAppPro is not needed for any of them**, and
can stay closed.

### 1. Install DCS-BIOS

DCS Signal Converter reads the cockpit through
[DCS-BIOS](https://github.com/DCS-Skunkworks/dcs-bios). Without it there is nothing
to read and nothing lights. Install it as its own instructions say, into
`Saved Games\DCS\Scripts`. Update your exports.lua to include the line inside your
DCS-BIOS download.

**Stable or nightly both work.** The shipped profiles were written against a
DCS-BIOS nightly. On a stable release everything works except the few lamps and
fields that need a newer DCS-BIOS. The editor names any that are affected, and
they come back on their own when you update DCS-BIOS.

### 2. Install

**Download `DCS-Signal-Converter-<version>-setup.exe` from the
[latest release](../../releases/latest) and run it.** That is the whole install.

- It installs for your Windows user only, so there is no administrator prompt.
- It finds where DCS saves its settings and puts a small hook in
  `Saved Games\DCS\Scripts\Hooks`, so the converter starts with DCS.
- If it cannot be sure of the DCS folder, for example you run a DCS variant, it
  asks you to pick it rather than guess.
- It asks where to keep your profiles. The default,
  `Saved Games\DCS Signal Converter`, is fine for almost everyone.

**Close DCS first.** DCS reads hooks only when it launches, so a DCS left running
keeps the old hook until you restart it. The installer tells you if it finds DCS
open.

### 3. Fly

Start DCS and fly. That is it.

When a mission starts, the hook launches the converter in the background. Within a
second or two of the cockpit loading it picks the profile for your aircraft and
sets every lamp to match the cockpit as it stands, then follows it from there.

- **Start it whenever you like.** Joining mid-mission, or switching slots, needs
  nothing restarted. The panels catch up within a couple of seconds.
- **Leave the mission** and the panels go dark about 20 seconds later. Quit DCS
  and the converter quits too.
- **It clears up after a crash.** If DCS dies, the converter notices the silence,
  clears the panels and exits on its own.

To check it is working, fly one of the [shipped aircraft](#what-it-drives) and put
the gear down, or turn the console lights knob. If nothing moves, see
[Troubleshooting](#troubleshooting).

### 4. Make it yours

Open **DCS Signal Converter** from the Start Menu. This is the profile editor. You
only need it to change something; the converter flies without it.

It lists every profile, and inside each one every lamp on every panel, one section
per panel, whether or not that panel is plugged in.

**Changing a lamp** is usually one step: type three letters of a signal and pick
it, and the test is filled in to suit the signal and the lamp. There are four ways
to drive a lamp, and the editor offers each only where it can mean something:

| Button                     | What it does                                                |
| -------------------------- | ----------------------------------------------------------- |
| **+ Add condition**        | Another test that must _also_ hold.                         |
| **+ Add alternative (or)** | Another way to light the lamp, such as from the other seat. |
| **Always on**              | Lit whenever the aircraft is loaded, or a fixed brightness. |
| **Match another lamp**     | Follow another dimmer on the same panel.                    |

**Do not know what a switch is called?** Press **Learn** beside the signal box,
flip the switch in the cockpit, and what you just moved is at the top of the list.
Click it to bind it. Learn only listens while its panel is open and never sends
anything to DCS or to the panels, so it is safe mid-mission.

**Changes take effect in flight.** Save, and the running converter picks the
profile up within about a second. Edit a lamp, save, and watch it change on the
panel without leaving the cockpit.

A few more things the editor does:

- **New profile** starts one for an aircraft that has none, blank or copied from a
  related one. **Copy to...** copies an existing profile to other aircraft, which
  is how the Hornet profile serves the Super Hornet mod.
- **Rename** a profile with the pencil beside its name. Two profiles cannot
  share a name, since the name is all the list shows.
- **Export...** saves a copy of a profile anywhere you choose, to share it.
  **Import...** brings one in. It is checked first, and refused if it would not
  load here. If it is for an aircraft another profile flies, you are asked
  before the aircraft moves, and asked again before a profile left with no
  aircraft is deleted. Saying no to the delete cancels the import.
- **Delete** removes a profile. If that leaves an aircraft with no profile, you
  choose which profile takes it, so splitting a profile and deleting a half
  gives its aircraft back.
- **Add a divider** on the MCDU screen draws a rule across a row, for an
  aircraft whose page does not fill the glass. The A-10C and AH-64D profiles
  ship with one, and you choose its colour.
- **Manage Converter** is for the rare times the converter needs restarting.
  Saving a profile is not one of them. See below.
- **Reset** puts a profile back to the shipped one. **Reset this lamp** does the
  same for a single lamp, and shows you what it will reset to before it does.
- **Drive this panel** per panel. Untick it and the profile leaves that panel
  alone entirely, so another program can have it.
- **Problems** in red stop a save until they are fixed. **Cautions** in yellow are
  about a profile that works but probably not as meant, and never stop a save.

The full profile model, every condition form and the reasons behind them, is in
[docs/CONFIG.md](docs/CONFIG.md).

---

## Updating and uninstalling

**Updates.** When a newer release is out, the editor shows a bar at the foot of
the window linking to it. Close DCS, download the new installer and run it. It
remembers your folders and asks nothing it asked before.

**Your changes are kept.** An update adds new profiles and rows for new panels on
its own, but never rewrites a lamp you have changed. When a release fixes a
shipped lamp, its release notes say so, and you choose whether to reset that lamp
to pick up the fix.

**Uninstalling.** From **Settings → Apps**, like any other program. It removes the
program and the DCS hook. Your profiles are kept unless you tick **delete app
data**.

---

## Troubleshooting

**Nothing lights at all.**
Check `Saved Games\DCS\Logs\dcs.log` for `DCS-SIGNAL` lines.

- No lines at all: the hook did not load. Confirm
  `Saved Games\DCS\Scripts\Hooks\dcs-signal-hook.lua` exists, and that you
  restarted DCS after installing.
- `daemon not installed at ...`: the hook points at a folder the converter is not
  in. Run the installer again.
- `daemon launched` but still nothing: the cockpit data is not arriving. DCS-BIOS
  is not installed in `Saved Games\DCS\Scripts`, or a firewall is blocking it.
  [`dcs-signal listen`](docs/CLI.md#other-commands) confirms which in fifteen seconds.

**One lamp is configured but stays dark.**
On the PTO2, most indicators sit under two master lamps, `SL` and `FLAG`, that
work like gates. If a gate is off, every lamp under it stays dark however it is
set. Every shipped profile holds both gates on; check yours in the editor, which
also warns about a gate that goes dark with the console lights off.

**A lamp does the wrong thing.**
Open the aircraft's profile in the editor and use **Learn** to find the switch you
meant. If a shipped profile is wrong, please report it.

**The panels went dark while DCS kept running.**
The converter stopped. The DCS hook starts it when a mission begins and does not
notice that it has gone, so it will not come back until DCS is restarted. Open
the editor and press **Manage Converter**, then **Restart**. The same dialog is
where to restart it after plugging a panel in, since panels are found once at
startup. **Kill** in that dialog is only for a converter that will not answer;
it cannot clear the panels, because a killed program runs none of its shutdown.

**The panels stayed lit after DCS closed.**
The panels hold whatever was last sent to them, and the converter did not get to
clear them. Start DCS and a mission, then quit normally, and they clear.

**The editor says some lamps need the DCS-BIOS nightly.**
Your DCS-BIOS is older than the one those lamps were written for. They stay off,
everything else works, and they come back when you update DCS-BIOS. See
[Install DCS-BIOS](#1-install-dcs-bios).

**Something in SimAppPro fights it.**
If SimAppPro is running with "Sync with DCS" on, it can drive the same backlights.
Turn that off, or close SimAppPro.

**Reporting a bug.** Include the version (`dcs-signal --version`, or the bar at the
foot of the editor), the aircraft, and the `DCS-SIGNAL` lines from `dcs.log`.

---

## How it works

Everything below is background. You do not need any of it to use the tool.

The panels are ordinary USB HID devices, and HID needs no administrator rights,
so the converter talks to them directly with the same messages SimAppPro sends.
The cockpit side comes from DCS-BIOS, which is already broadcasting on your
machine, so nothing in DCS is patched.

Its list of cockpit signals is built from the DCS-BIOS you have installed, because
signal addresses change between DCS-BIOS releases, and it is rebuilt on its own
whenever DCS-BIOS changes.

The DCS hook only ever starts the converter. The converter clears the panels
itself when the DCS-BIOS stream goes quiet, and exits when DCS closes, because a
hook cannot run when DCS crashes and the panels keep whatever was last sent to
them until something clears them.

The details, and the reasons for each, are in the [further reading](#further-reading).

---

## Command line

The converter is `dcs-signal.exe`, in the install folder. The hook runs it for
you; running it by hand is for checking and tinkering. `dcs-signal --help` lists
every command, and [docs/CLI.md](docs/CLI.md) explains them and how to read the
output.

---

## Running from source

For development. Needs Rust (MSVC toolchain) and, for the editor only, Node.
[docs/STATUS.md](docs/STATUS.md) has the commands, where development stands and
what comes next.

Copy `.env.example` to `.env` and set `env=dev` first. That points the editor
and the daemon at the tracked `data/defaults`, so what you author is what
ships, and nothing you do while developing reaches the profiles you fly.

---

## Further reading

| Document                                   | What is in it                                                        |
| ------------------------------------------ | -------------------------------------------------------------------- |
| [docs/CLI.md](docs/CLI.md)                 | Running the converter by hand, its flags, and reading its output     |
| [docs/CONFIG.md](docs/CONFIG.md)           | The profile format, every binding form, and how the editor checks it |
| [docs/PROTOCOL.md](docs/PROTOCOL.md)       | The reverse-engineered HID protocol and every panel's lamp map       |
| [docs/PERFORMANCE.md](docs/PERFORMANCE.md) | Memory, CPU and install size, and how they were measured             |
| [CHANGELOG.md](CHANGELOG.md)               | What changed in each release, and which shipped profiles moved       |
| [docs/STATUS.md](docs/STATUS.md)           | Development status, verified hardware facts, and what is next        |
| [docs/TODO.md](docs/TODO.md)               | The outstanding work as a checklist, linked into the docs above      |

---

## License

MIT, see [LICENSE](LICENSE). The MCDU screen code and fonts come from other
projects under their own licenses, listed with their full text in
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

---

## ❤️ Support the Project

This utility is free and open source, with no ads and no trackers. If it got your
panels following the cockpit and you back to flying, you can put something in the
tip jar. Entirely optional, every part of this stays free either way.

[![Buy Me A Coffee](https://img.shields.io/badge/Buy_Me_A_Coffee-C25E00?style=for-the-badge&logo=buymeacoffee&logoColor=white)](https://www.buymeacoffee.com/cbass2404)
