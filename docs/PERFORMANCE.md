# Performance and install size

The daemon was measured 2026-09-22, at 1.0.0-alpha.007 in development, on an
i9-12900KF with 64 GB, Windows 11, release builds from rustc 1.98.1. Six
panels were connected: PTO2, Orion Throttle Base II, CarrierAce UFC + HUD,
ViperAce ICP, Orion Combat Rudder Pedals, MCDU Captain. The editor figures are
from 2026-09-18 and the install sizes from the 1.0.0-alpha.006 release.

## Summary

- The daemon uses about 23 MB of private memory and under 0.5% of one core in
  flight. Under a stream far heavier than DCS produces, it stays under 4%.
- The editor uses about 165 MB, almost all of it WebView2. It uses no CPU while
  idle.
- The installer is 3.2 MB. Installed, the program comes to 17 MB, and the
  catalogue it builds on first run adds about 10 MB.

## The daemon

`dcs-signal run --dry-run` with the A-10C profile, against a synthetic
DCS-BIOS stream. Each scenario was measured for 60 seconds, after 3 seconds of
warmup that absorb the startup and module-load flood.

| Scenario | Frames/s | CPU avg | CPU peak | Working set | Private |
|---|---|---|---|---|---|
| idle | 0 | 0.00% | 0.00% | 29.0 MB | 22.6 MB |
| typical | 30 | 0.31% | 1.56% | 29.1 MB | 22.6 MB |
| stress | 60 | 2.2 to 3.7% | 6.25% | 29.2 MB | 22.7 MB |

- **idle:** no stream at all, which is the daemon waiting for DCS.
- **typical:** 30 frames a second. Each frame moves 20 integer outputs and
  one text field, and the whole map is re-exported every 300 ms, the same
  cycle DCS-BIOS uses (see `crates/dsc-bios`).
- **stress:** 60 frames a second, and every output in the module takes a new
  random value in every frame. Every bound lamp and every display field
  changes every frame, which no real cockpit does.

CPU is a percentage of one core. Peaks are the busiest 1-second sample.
Stress is given as the range of three runs, because it varied more than the
other two. A run with other work open on the machine is not worth keeping:
one put typical above stress.

**No panel is written.** The benchmark only runs the daemon as a dry run,
which finds the panels but never opens them. The one live run, on 2026-09-18,
measured within a few hundredths of a percent of a dry run in every scenario,
so the HID writes cost very little next to the decoding. Stress rewrites every
lamp and screen 60 times a second for minutes, and that is not worth doing to
real hardware again for a number that does not move.

**Stress costs more than it did on 2026-09-18**, when it measured 1.09%. The
A-10C default now puts far more on its screens, three MCDU rows of radios and
a countermeasures page on the DED, and each field has more to decide before it
paints: conversions, aliases, bands, formats. Under stress all of it is redone
every frame. Not profiled, and not worth profiling: under 4% of one core for
every screen repainted 60 times a second is still very little, and typical,
which is what flying looks like, barely moved.

**What the numbers show.** CPU follows how much actually changes, not how many
frames arrive. That matches the engine's design: `BiosState::apply` reports
whether a word moved, and only words that moved are resolved against the
profile. Memory is flat across all three scenarios and did not grow during any
run.

**Where the memory goes.** This has not been profiled. The daemon loads every
module in `data/catalogue` at startup (51 files, 11 MB of JSON), and that is
the likeliest owner of most of it. Loading only the active aircraft's module
was considered and turned down: learn mode and the startup profile checks need
every module, and they are worth far more than a few megabytes.

**Resolution.** Windows counts process CPU time in steps of about 15.6 ms, so
the idle and typical figures mean "under about 0.3%", not exact values.

## The editor

The editor idle with a profile open, measured across its whole process tree
after 10 seconds, on 2026-09-18.

| Process | Private memory |
|---|---|
| editor | 4.1 MB |
| msedgewebview2.exe (6 processes) | 161.2 MB |
| **Total** | **165.3 MB** |

Idle CPU was 0%.

Adding up the working sets gives 344 MB, but that overcounts: the WebView2
processes share their DLL pages, and each process's working set counts them
again. Private memory is the fair figure. WebView2's cost comes with Tauri and
does not depend on what the editor does.

## Install size

From the 1.0.0-alpha.006 release, installed per user.

| Part | Size |
|---|---|
| The installer, `DCS-Signal-Converter-1.0.0-alpha.006-setup.exe` | 3.2 MB |
| `DCS Signal Converter.exe`, the editor with its frontend embedded | 11.8 MB |
| `dcs-signal.exe`, the daemon | 2.8 MB |
| `data`: devices, displays, MCDU fonts, defaults and their snapshot | 1.3 MB |
| **Installed, `%LOCALAPPDATA%\DCS Signal Converter`** | **about 17 MB** |
| The catalogue, built on first run in `Saved Games\DCS Signal Converter` | about 10 MB |

- **The catalogue is not shipped.** It is generated from the DCS-BIOS
  installed on each machine, because one from another DCS-BIOS release reads
  the wrong addresses (see STATUS.md). The daemon and the editor both build it
  when it is missing or out of date, so it needs nothing else installed.
- **WebView2** is not counted. It ships with Windows 11. On Windows 10 machines
  without it, the NSIS bootstrapper downloads about 2 MB, and the runtime then
  takes about 150 MB of its own.

## Reproducing

```powershell
cargo build --release --bin dcs-signal
python tools/bench_daemon.py                  # all three scenarios, 30 s each
python tools/bench_daemon.py --seconds 60     # what the table above used
python tools/bench_daemon.py --scenario stress
python tools/bench_daemon.py --module F-16C_50 --aircraft F-16C_50
```

The tool needs only the Python standard library, and DCS does not need to be
running. It plays the synthetic stream onto 239.255.50.10:5010 from the
generated catalogue, starts the daemon as a dry run with its output sent to
NUL, and samples it with `GetProcessTimes` and `GetProcessMemoryInfo`.

- **Plug the panels in.** A dry run never opens them, but it still looks for
  them, and with none connected it has nothing to drive and exits.
- **Close other DCS-BIOS clients first.** Anything else reading the multicast
  group, such as `dcs-signal listen` or the editor's learn mode, sees the
  synthetic stream too.
- **Close other work.** The numbers are small enough that a build or a video
  in the background swamps them.

The editor figures were taken by hand. The tool does not measure the editor.
