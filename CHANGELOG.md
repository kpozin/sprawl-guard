# Changelog

All notable changes to `sprawl-guard` will be documented in this file.

## Unreleased

### Added

- Versioned JSON Schemas and editor directives for both configuration and ratchet TOML files.

### Changed

- Production and test directory fan-out both default to 10 files.
- Common dependency lockfiles are excluded from source discovery by default.
- `update` replaces `baseline` and adds newly discovered ratchet entries without raising existing
  ceilings by default. `tighten` replaces `ratchet` for lowering and removing entries.

### Removed

- Configuration include fragments; all `[[overrides]]` entries now live in the root configuration.

## [0.2.0] - 2026-08-11

### Added

- Versioned JSON Schema output for configuration files, distributed through the CLI,
  documentation site, and npm wrapper.

### Changed

- Successful checks now summarize the files, directories, and languages scanned.
- Resolved configuration output can be limited to detected and explicitly configured languages.
- Baseline and ratchet help text now distinguishes creating ceilings from lowering them, and
  ratchet files name LOC ceilings `max_loc`.

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
