# Changelog

All notable changes to Barometer are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [0.2.0] - 2026-10-07

### Added

- Maximized or fullscreen, the station goes to two columns: now and the
  week on the left, the next 24 hours on the right with the chart as tall
  as the window. Gauges and the week's heatmap grow with it.
- The window reopens at the size, place and state it was closed in.

### Fixed

- "PRESSURE" no longer wraps onto two lines at 125–150% display scaling:
  label columns are measured in font cells, not pixels.
- The first window fits the screen, and a window shorter than the station
  scrolls instead of cutting off the bottom.
- The scheme picker in Settings opens (its menu was drawn under the
  drawer; fixed in ferrite-design).

## [0.1.1] - 2026-10-07

### Fixed

- Windows: the Settings button in the title bar does something when clicked. The
  whole title bar was a window-drag area, so Windows took the click as the
  start of a drag (ferrite-design's `title_bar`).
- The date rolls over at local midnight, not UTC midnight.

## [0.1.0] - 2026-10-07

The first release.

### Added

- One panel: the reading in block digits, conditions, feels-like, high and
  low, wind, pressure, and sunrise and sunset.
- ASCII gauges for humidity, cloud, the chance of rain, wind and daylight.
- The next 24 hours as a line chart, with rain as a dither texture under it.
- The week as a 7×24 temperature heatmap, with a line per day.
- Weather from Open-Meteo: no key, no account. Type a town or `lat,lon` in
  Settings, which open by themselves on first launch.
- A demo week before a place is set, or with `--demo`.
- The Phosphor scheme by default, nine others, and the shared look from
  Lodestone (`FERRITE_*`) when launched from it.
- The pixel-art logo as the Windows executable, window and taskbar icon.

[0.1.1]: https://github.com/rwetz/ferrite-barometer/releases/tag/v0.1.1
[0.1.0]: https://github.com/rwetz/ferrite-barometer/releases/tag/v0.1.0
