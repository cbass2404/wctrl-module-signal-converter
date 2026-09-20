# Changelog

What changed in each release, for the people running it. The release pipeline
puts the matching section at the top of the release notes, so anything written
here reaches users; development detail belongs in
[docs/STATUS.md](docs/STATUS.md) instead.

**When a shipped profile changes**, say so here and name the rows. An update
adds new rows but never rewrites one you have changed, so a fix to a shipped
lamp only reaches you if you reset that lamp, and you cannot decide to unless
this says what moved.

## 1.0.0-alpha.003

### New Features

- **The daemon keeps a log of each session**, in `Saved Games\DCS\Logs\dcs-signal.log`
  beside DCS's own `dcs.log`. Started by the DCS hook it has no console, so
  until now a flight left no account of itself at all; if something goes wrong,
  this is the file to send. It holds where every file was read from, every
  WinCtrl device found, which profiles loaded and which were skipped, the
  signals the profile reads as they move, every lamp and screen written, a
  status line each minute, and the error behind an exit. Each start keeps the
  last session as `dcs-signal.log.bak` and deletes the one before it, so land
  and read it rather than flying on. See [docs/CLI.md](docs/CLI.md).

### Fixed
