# Performance and install size

Measured 2026-09-18 on an i9-12900KF with 64 GB, Windows 11, release builds from
rustc 1.98.1. All six panels were connected: PTO2, Orion Throttle Base II,
CarrierAce UFC + HUD, ViperAce ICP, Orion Combat Rudder Pedals, MCDU Captain.

## Summary

- The daemon uses about 22 MB of private memory and around 0.2% of one core
  in flight. Under a stream far heavier than DCS produces, it stays near 1%.
- The editor uses about 165 MB, almost all of it WebView2. It uses no CPU while
  idle.
- Installed, everything comes to about 23 MB, or about 12 MB without the
  catalogue. An installer would be about 3 MB.

## The daemon

`wctrl run` with the A-10C profile, against a synthetic DCS-BIOS stream. Each
scenario was measured for 30 seconds, after 3 seconds of warmup that absorb
the startup and module-load flood.

| Scenario | Frames/s | CPU avg | CPU peak | Working set | Private |
|---|---|---|---|---|---|
| idle | 0 | 0.16% | 1.6% | 25.5 MB | 21.4 MB |
| typical | 30 | 0.21% | 1.6% | 26.4 MB | 22.2 MB |
| stress | 60 | 1.09% | 3.1% | 26.7 MB | 22.5 MB |

- **idle:** no stream at all, which is the daemon waiting for DCS.
- **typical:** 30 frames a second. Each frame moves 20 integer outputs and
  one text field, and the whole map is re-exported every 300 ms, the same
  cycle DCS-BIOS uses (see `crates/wctrl-bios`).
- **stress:** 60 frames a second, and every output in the module takes a new
  random value in every frame. Every bound lamp and every display field
  changes every frame, which no real cockpit does.

CPU is a percentage of one core. Peaks are the busiest 1-second sample.

These numbers are from a live run, with the panels driven. A dry run gave
nearly the same numbers (0.00%, 0.00% and 1.15% CPU), so the HID writes cost
very little next to the decoding.

**What the numbers show.** CPU follows how much actually changes, not how many
frames arrive. That matches the engine's design: `BiosState::apply` reports
whether a word moved, and only words that moved are resolved against the
profile. Memory is flat across all three scenarios and did not grow during any
run.

**Where the memory goes.** This has not been profiled. The daemon loads every
module in `data/catalogue` at startup (51 files, 11 MB of JSON), and that is
the likeliest owner of most of the 22 MB. If the daemon ever needs to be
smaller, the first thing to try is loading only the active aircraft's module.

**Resolution.** Windows counts process CPU time in steps of about 15.6 ms, so
the idle and typical figures mean "under about 0.2%", not exact values.

## The editor

`wctrl-editor.exe` idle with a profile open, measured across its whole process
tree after 10 seconds.

| Process | Private memory |
|---|---|
| wctrl-editor.exe | 4.1 MB |
| msedgewebview2.exe (6 processes) | 161.2 MB |
| **Total** | **165.3 MB** |

Idle CPU was 0%.

Adding up the working sets gives 344 MB, but that overcounts: the WebView2
processes share their DLL pages, and each process's working set counts them
again. Private memory is the fair figure. WebView2's cost comes with Tauri and
does not depend on what the editor does.

## Install size

| Part | On disk | Compressed |
|---|---|---|
| `wctrl.exe` | 2.1 MB | 0.6 MB |
| `wctrl-editor.exe`, frontend embedded | 9.2 MB | 1.8 MB |
| `data/defaults`, `data/displays`, `data/devices.json` | 0.35 MB | 0.02 MB |
| `data/catalogue`, 51 modules | 11 MB | 0.4 MB |
| **Total** | **about 23 MB** | **about 3 MB** |

The compressed column is `xz -9`, which is close to what the NSIS bundle
(`bundle.targets` in `tauri.conf.json`) gets with LZMA. No installer has been
built yet, so these are estimates.

- **The catalogue** is generated from the DCS-BIOS installed on each machine,
  and a catalogue from another DCS-BIOS release reads the wrong addresses (see
  STATUS.md). An installer should probably build it at install time instead of
  shipping one. That makes the install about 12 MB, but the machine then needs
  Python to build it.
- **WebView2** is not counted. It ships with Windows 11. On Windows 10 machines
  without it, the NSIS bootstrapper downloads about 2 MB, and the runtime then
  takes about 150 MB of its own.

## Reproducing

```powershell
cargo build --release --bin wctrl
python tools/bench_daemon.py                  # dry run, all three scenarios
python tools/bench_daemon.py --live           # drives the panels
python tools/bench_daemon.py --module F-16C_50 --aircraft F-16C_50
```

The tool needs only the Python standard library, and DCS does not need to be
running. It plays the synthetic stream onto 239.255.50.10:5010 from the
generated catalogue, starts the daemon with its output sent to NUL, and samples
it with `GetProcessTimes` and `GetProcessMemoryInfo`.

- **Close other DCS-BIOS clients first.** Anything else reading the multicast
  group, such as `wctrl listen` or the editor's learn mode, sees the synthetic
  stream too.
- **`--live` drives the panels.** Lamps and screens flash random values for
  the length of the run. The tool lets the daemon's `--seconds` expire instead
  of killing it, so the daemon still clears the panels when it exits.

The editor figures were taken by hand. The tool does not measure the editor.
