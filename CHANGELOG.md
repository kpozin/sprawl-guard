# Changelog

All notable changes to `sprawl-guard` will be documented in this file.

## Unreleased

## [0.2.0-dev.0] - 2026-08-03

### Added

- Versioned configuration and ratchet schemas.
- Repository-defined language groups for directory fan-out limits and ratchets.
- Language path customization through include, additional, and exclude globs.
- Configurable inclusion of code, comments, and blank lines in file line limits.

### Changed

- All bundled languages are enabled by default.
- Configuration and setup documentation now covers the complete schema and package installation
  paths.

## [0.1.0] - 2026-07-23

### Added

- Configurable file LOC and directory fan-out limits across supported source languages.
- Baselines and ratchets for adopting strict limits without accepting further growth.
- Native Cargo installation and a project-local npm wrapper with platform-specific runtimes.
- Human-readable, JSON, and SARIF check output for local use and CI.
