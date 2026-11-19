# Changelog

All notable changes to `sprawl-guard` will be documented in this file.

## [Unreleased]

### Added

- `check` lists at most 50 findings in human and GitHub output, so a large first run keeps its
  summary on screen. Set the cap with `--max-findings <N|unlimited>` or `[output].max_findings`.
  Violations take the listed slots before warnings and skipped unreadable paths. Group headings show
  their totals, a group cut short ends with `… and N more`, and a footer gives how many findings
  were shown out of the total. The cap also bounds the files a directory finding names and the
  paths listed when unreadable paths stop the check. GitHub annotations cover only the listed
  violations. The exit status still reflects every violation, and JSON and SARIF output list every
  finding.

### Changed

- Human output headings show their group's total, as in `Files over the line limit (3):`.
- `[traversal].max_file_bytes` is now `[traversal].max_parsed_code_file_bytes`, since it limits only
  the files whose code lines are counted. The old name still loads; `config upgrade --check` reports
  it and `config upgrade --write` renames it.

## [0.6.0-beta.1] - 2026-11-18

### Added

- Sprawl Guard is on PyPI as `sprawl-guard`, so Python projects can install it with uv, pip, or
  pipx and pin it in their lockfile. Each wheel holds only the native binary, for macOS, Windows on
  x64, Linux on x64 and Arm with glibc 2.35 or newer, and Linux on x64 with musl. The release
  workflow publishes the wheels through PyPI trusted publishing, with attestations.

## [0.5.3] - 2026-11-18

### Fixed

- The Linux musl binary, which npm installs on Alpine, no longer slows down when it checks files on
  several threads. Before, it could be many times slower than on one thread.

## [0.5.2] - 2026-11-18

### Fixed

- The prebuilt Linux glibc binaries, in the npm packages and the release archives, now run on glibc
  2.35 and newer, as in Ubuntu 22.04 and Debian 12. Before, the Arm binary needed glibc 2.39 and
  didn't start on older systems, and the x64 binary printed a glibc warning on every run.
- On Node.js 22, the npm package no longer prints Node's "WASI is an experimental feature" warning
  on every run when it falls back to the WebAssembly build.

### Known issues

- The Linux musl binary, which npm installs on Alpine, is many times slower than the glibc binary
  when it checks files on several threads.

## [0.5.2-beta.1] - 2026-11-18

### Fixed

- On Node.js 22, the npm package no longer prints Node's "WASI is an experimental feature" warning
  on every run when it falls back to the WebAssembly build.

## [0.5.2-beta.0] - 2026-11-18

### Fixed

- The prebuilt Linux glibc binaries, in the npm packages and the release archives, now run on glibc
  2.35 and newer, as in Ubuntu 22.04 and Debian 12. Before, the Arm binary needed glibc 2.39 and
  didn't start on older systems, and the x64 binary printed a glibc warning on every run.

### Known issues

- The Linux musl binary, which npm installs on Alpine, is many times slower than the glibc binary
  when it checks files on several threads.

## [0.5.1] - 2026-11-17

### Fixed

- The configuration and ratchet schemas no longer give integers a non-standard `format`, such as
  `uint32`, so strict JSON Schema validators like SchemaStore's accept them. Limits that Sprawl
  Guard reads as 32-bit numbers have a `maximum` instead, so editors flag values that are too large.

## [0.5.0] - 2026-11-17

### Added

- `check --format github` and `[output].format = "github"` print the human report followed by GitHub
  Actions annotations, so violations show up on the workflow run and, when a file's first line is in
  the diff, beside that file in the pull request.
- In the `sprawl-guard-lib` crate, `OutputFormat` has a new `Github` variant, so a `match`
  that lists every variant needs a new arm.

### Fixed

- The npm package README and the agent skill now say to run Sprawl Guard through npm as
  `npm exec -- @kpozin/sprawl-guard`. The previous form, `npm exec -- sprawl-guard`, could run an
  unrelated package named `sprawl-guard` when Sprawl Guard wasn't installed in the project.

## [0.4.1] - 2026-11-16

### Changed

- CLI help: `config import eslint` names its argument `FILE`, and `update --import` names its value
  `LINTER`. The `--unreadable-paths` help and the `[traversal].unreadable_paths` schema
  description refer to the unreadable paths found while scanning.

## [0.4.0] - 2026-11-16

### Added

- `config import eslint` and `config import oxlint` read a project's existing `max-lines` rule
  into the Sprawl Guard configuration. `update --import eslint` and `update --import oxlint` add
  ratchet entries for over-limit files whose `max-lines` violations the linter suppresses. Both
  commands preview their changes until you pass `--write`, and they use the linter that the
  project already has installed.

### Changed

- The npm install is smaller: on Apple Silicon, with the native executable and the WebAssembly
  fallback, it went from 20.3 MiB to under 7.5 MiB. Release executables are smaller too.
- The agent skill is shorter and explains how to fix a violation by reorganizing: split a file
  along its responsibilities, and group a directory's files by the feature they serve.

### Fixed

- The WebAssembly build that the npm package runs on platforms without a native build now behaves
  like a native build:
  - `config import` and `update --import` run ESLint and Oxlint.
  - Paths may be absolute or relative to the working directory, as in `--root ..`. On Windows they
    may name any drive or network share.
  - `traversal.same_file_system` keeps the walk on the root's filesystem.
  - `traversal.follow_symlinks = "directories"` follows directory links and reports link loops.
    It previously failed with "same-file is not supported on this platform".
- `update --import eslint` imports suppressions for files reached through a followed symbolic link
  that points outside the project. It failed with "is not under root".

### Removed

- In the `sprawl-guard-lib` crate, `TomlSchemaUrl::ConfigV1` is removed.
  `ConfigSchemaVersion::schema_url()` and `schema_directive()` are replaced by
  `ConfigSchemaVersion::CURRENT_SCHEMA_URL` and `ConfigSchemaVersion::current_schema_directive()`.

## [0.3.2] - 2026-11-15

Version 0.3.1 was tagged but never published. This release contains the changes planned for it.

### Added

- `explain DIR` explains a directory. It lists each kind and language group of source files
  directly inside it, with their count against the direct-file limit, any ratchet ceiling, and the
  overrides that matched. JSON output reports `kind: "directory"`. Paths under the directory that
  cannot be read appear as a warning that the counts may be incomplete. A hidden or ignored
  directory is reported as ignored.

### Changed

- A configuration file with an older schema version now fails with a message that names
  `sprawl-guard config upgrade --write`, adding `--config PATH` when the file is not
  `sprawl-guard.toml` in the current directory.
- `config resolved` prints effective values after a comment that says the output is for reading.
  `count_lines` includes `code`, `[files].exclude` lists the built-in and added excludes together,
  and a language with an explicit `include` lists its added globs there. `config export` still
  prints loadable configuration.
- `explain` describes a path that is not a regular file or directory as "not a file or directory
  that checks read".
- Human output for directory violations and ratchet previews labels the language group, as in
  `(limit 10, group: source)`.

## [0.3.0] - 2026-11-14

This release renames configuration keys, ratchet keys, and machine-readable output to match the
documentation's vocabulary. Configurations written for 0.2.0 (schema version 1) still upgrade with
`sprawl-guard config upgrade --write`, and upgrading from 0.2.0 also brings the changes listed under
0.3.0-dev.0. Configuration and ratchet files written for 0.3.0-dev.0 keep their schema versions and
need the renames below applied by hand.

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
- `check` reports a file or directory that grew above its ratchet ceiling once, as a limit
  violation that names the ceiling: `src/big.rs: 700 lines (limit 400, ceiling 650)`. In JSON and
  SARIF, its `ratchet` field holds the ceiling. The heading "Ratchet failures" is now
  "Stale ratchet entries", followed by a hint to run `sprawl-guard tighten --write`.
- `explain` prints one labeled line per fact, and `languages` prints a table of names, keys, and
  extensions. Use `--format json` for the full language details.
- SARIF rule descriptions and messages use the same vocabulary.
- Each command's help lists only its own `--format` values and states every default.
- Language detection uses Tokei 15. It recognizes Blade, C3, Djot, and Godot text scene files, so
  `check` now includes them.

### Added

- The JSON `summary` reports `ratchet_entries`, and a passing human check prints how many ratchet
  entries remain.
- An agent skill, `skills/sprawl-guard/SKILL.md`, tells coding agents how to run the check and fix
  failures. The npm package includes it.
- Release archives, crates, and npm packages come with signed provenance attestations.
  `SECURITY.md` explains how to verify them.
- The repository is a Bazel module, so a Bazel workspace can build the CLI and library from
  source. "Use with Bazel" explains how to add it.

### Fixed

- The npm package's `schema/sprawl-guard.schema.json` is the current (v2) configuration schema.
  It was the v1 schema.
- The WASI fallback writes its complete output when stdout is a pipe that fills up.

### Removed

- The `*_increased` JSON ratchet kinds and the `sprawl-guard/ratchet-increase` SARIF rule. A
  violation that grew above its ceiling is reported only by its limit violation.
- The staged-snapshot preparation hook. To check before you commit, run
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
- An optional staged-snapshot preparation hook.

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

## [0.1.0] - 2026-07-23

### Added

- Configurable file LOC and directory fan-out limits across supported source languages.
- Baselines and ratchets for adopting strict limits without accepting further growth.
- Native Cargo installation and a project-local npm wrapper with platform-specific runtimes.
- Human-readable, JSON, and SARIF check output for local use and CI.
