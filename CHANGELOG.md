# Changelog

All notable changes to `sprawl-guard` will be documented in this file.

## Unreleased

This release renames configuration keys, ratchet keys, and machine-readable output to match the
documentation's vocabulary. Configurations written for 0.2.0 (schema version 1) still upgrade with
`sprawl-guard config upgrade --write`. Configuration and ratchet files written for 0.3.0-dev.0 keep
their schema versions and need the renames below applied by hand.

### Changed

- Configuration keys:
  - `max_leaf_files_per_directory` is now `max_direct_files_per_directory`, at the root, in
    language groups, and in `[[overrides]]`.
  - `[tests].check_path_tests` is now `[tests].check`.
  - `[traversal].error_policy` is now `[traversal].unreadable_paths`.
  - `included_line_types` is now `count_lines` and lists only `comments` and `blank_lines`. Code
    lines always count, so remove `"code"` from the list.
  - `[files].exclude` is now `[files].extend_exclude`.
  - Language `additional_globs` is now `extend_include`.
- Ratchet keys: `max_loc` is now `max_lines`, and `leaf_files` is now `max_direct_files`.
- The `check --traversal-error-policy` option is now `--unreadable-paths`.
- JSON violation kinds: `file_loc` and `test_file_loc` are now `file_lines` and
  `test_file_lines`. `directory_fanout` and `test_directory_fanout` are now
  `directory_direct_files` and `test_directory_direct_files`. Ratchet kinds follow the same
  pattern.
- SARIF rule IDs follow the same renames in kebab case: for example, `sprawl-guard/file-loc` is now
  `sprawl-guard/file-lines`, and `sprawl-guard/directory-fanout` is now
  `sprawl-guard/directory-direct-files`.
- JSON report fields: `scope` is now `paths` (`"all"` or a list), and `traversal_incidents` is now
  `unreadable_paths`.
- In JSON violations, SARIF properties, and `explain` JSON, `included_line_types` is now
  `count_lines` and no longer lists `"code"`. In `languages` JSON, `automatic_included_line_types`
  is now `automatic_count_lines` and `additional_globs` is now `extend_include`.
- `init` and `config upgrade` now preview by default; pass `--write` to save. `init --dry-run` is
  removed, and `config upgrade --write` cannot be combined with `--check`.
- Options now follow the command that uses them, for example `sprawl-guard check --root /repo`.
  `--root`, `--config`, and `--quiet` are accepted only by commands that read a repository, and
  `languages --root` requires `--detected`. Without `--root`, the current directory is the root.
- The active config file is never checked as a source file, so
  `extend_exclude = ["sprawl-guard.toml"]` is no longer needed. `explain` reports the file as
  `config_excluded`.
- For a path that does not exist, `explain` JSON reports `state: "not_found"` instead of
  `"classified"`, and human output says the path was not found.
- The npm wrapper accepts `--root` anywhere before `--`, mounts the current directory when it is
  omitted, and exits with code `2` when the wrapper itself fails.
- Human output uses the documentation's vocabulary. `check` starts with `Configuration: PATH`
  instead of `Policy: ...`, uses headings such as "Files over the line limit", and reports
  measurements as `src/big.rs: 700 lines (limit 400)`. Paths in every command's human output are
  relative to the root, unquoted, and have control characters escaped.
- `check --quiet` prints nothing when the check passes.
- `explain` prints one labeled line per fact, and `languages` prints a table of names, keys, and
  extensions. Use `--format json` for the full language details.
- SARIF rule descriptions and messages use the same vocabulary.
- Each command's help lists only its own `--format` values and states every default.
- The documentation uses one vocabulary, defined on the "How checks work" page. The guide is now
  four pages: "Get started", "Run in CI", "Work with the ratchet", and "Configure checks".

### Added

- The JSON `summary` reports `ratchet_entries`, and a passing human check prints how many ratchet
  entries remain.

### Removed

- The staged-snapshot preparation hook and its documentation. To check before you commit, run
  `sprawl-guard check` in a pre-commit hook, as described in "Run in CI".

## [0.3.0-dev.0] - 2026-10-22

This development release changes command names and source-selection behavior. Run
`sprawl-guard config upgrade` after upgrading, and replace `baseline` and `ratchet` commands with
`update` and `tighten`.

### Added

- Versioned JSON Schemas and editor directives for both configuration and ratchet TOML files.
- Configuration upgrades that preserve authored comments and formatting, with a read-only CI check.
- Ratchet change previews with before-and-after ceilings; complete TOML remains available.
- Requested scan scope and configuration provenance in inspection output.
- Markdown documentation pages for coding agents and an optional staged-snapshot preparation hook.

### Changed

- Production and test directory fan-out both default to 10 files.
- Common dependency lockfiles are excluded from source discovery by default.
- `update` replaces `baseline` and adds newly discovered ratchet entries without raising existing
  ceilings by default. `tighten` replaces `ratchet` for lowering and removing entries.
- `init` creates configuration without scanning the repository.
- Source scans use only in-root ignore policy and consistent dot-name hidden-file filtering.
- Ratchet operations reject incomplete scans, including malformed or symlinked ignore policy.
- The WASI fallback runs the CLI directly and preserves its exit status.

### Removed

- Configuration include fragments; all `[[overrides]]` entries now live in the root configuration.
- The no-op `--color` option and the silent traversal-depth limit.

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
