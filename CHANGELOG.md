# Changelog

## v0.6.1 (2026-06-17)

### Fixed

- `terminal` backgrounds extend to line end

## v0.6.0 (2026-06-17)

### Added

- `--theme` now accepts a bare theme name (e.g. `--theme acid`) resolved against
  the runtime `themes/` dirs, in addition to a path to a `theme.toml`

## v0.5.0 (2026-06-17)

### Added

- `--inline` is now a modifier for the class-based formats (`html`,
  `edn-hiccup`, `json-hiccup`)

### Removed

- Dedicated `html-inline` format

## v0.4.0 (2026-06-17)

### Added

- `html-inline` backend: outputs style attributes instead of classes
- `json-hiccup` backend: JSON formatted Hiccup with classes
- `terminal`: ANSI escape codes

### Changed

- `terminal` is now the default.
