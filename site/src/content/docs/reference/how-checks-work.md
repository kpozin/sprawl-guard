---
title: How checks work
description: Source files, line counts, direct files, limits, selected paths, and the ratchet file.
---

## Terms

| Term                              | Meaning                                                                                                           |
| --------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| configuration (config file)       | `sprawl-guard.toml`.                                                                                              |
| built-in defaults                 | Settings used when no config file exists or a field is omitted.                                                   |
| source file                       | A file that one enabled language claims.                                                                          |
| test file / production file       | A source file that matches a test glob, or one that does not.                                                     |
| language group                    | Languages whose files share one direct-file count per directory.                                                  |
| literate language                 | A language whose content is prose, such as Markdown. Tokei counts the prose as comments.                          |
| lines                             | The counted lines of a file: code, plus comments or blank lines if configured.                                    |
| direct files                      | Source files immediately inside a directory. Files in its subdirectories do not count.                            |
| limit                             | The maximum set in the configuration.                                                                             |
| violation                         | A file whose lines, or a directory whose direct files, exceed its limit.                                          |
| ratchet file                      | `sprawl-guard.ratchet.toml`, which records existing violations.                                                   |
| ratchet entry                     | One recorded violation in the ratchet file.                                                                       |
| ceiling                           | The maximum recorded in a ratchet entry. Ceilings live in the ratchet file, and limits live in the configuration. |
| stale entry                       | A ratchet entry whose violation shrank or disappeared.                                                            |
| exclude / ignore                  | "Ignore" for `.gitignore`-style files. "Exclude" for Sprawl Guard globs.                                          |
| unreadable path                   | A file or directory that could not be read during the scan.                                                       |
| whole repository / selected paths | What `check`, `update`, and `tighten` cover without or with path arguments.                                       |
| preview / write                   | Showing a change versus saving it to disk.                                                                        |

## Source files and languages

Sprawl Guard uses the language table bundled with Tokei. Run `sprawl-guard languages` to list every
supported language, its key, and its extensions and filenames. Run `sprawl-guard languages
--detected` to see which languages claim files in the repository.

The scan starts at the repository root, or at the selected paths. Sprawl Guard skips ignored files,
hidden paths unless configured otherwise, the active config file, and the ratchet file. It then
applies the global file excludes and each language's paths and excludes. Globs match paths relative
to the repository root. Every bundled language is enabled by default, and a source file must belong
to exactly one enabled language.

## Test files

A source file is a test file when it matches `[tests].globs` or its language's `test_globs`. Test
files use the test line and direct-file limits. When `[tests].check = false`, test files are not
checked.

For a production Rust file, `exclude_cfg_test = true` makes Sprawl Guard skip top-level items marked
`#[cfg(test)]`. The file stays a production file. If Sprawl Guard cannot parse the file, it counts
every line and adds a warning. Rust test files are counted in full.

## Lines, direct files, and violations

Sprawl Guard uses Tokei to count lines. Code always counts. Comments and blank lines count when
`count_lines` includes them. Sprawl Guard also counts comments in literate languages, so that their
prose counts. A language's own `count_lines` overrides this.

Sprawl Guard measures four things:

- lines in each production file;
- lines in each test file;
- direct production files in each directory;
- direct test files in each directory.

`check` never fails on an `"unlimited"` limit. Sprawl Guard still counts the direct files under it
and compares that count with any ratchet entry. A matching path override can replace an unlimited
group limit with a finite one.

Global limits apply first. Language settings can replace file limits, and custom groups can replace
directory limits. Matching path overrides apply last. A measurement above its limit is a
violation.

### Direct files example

Production and test files have separate counts.

Suppose TypeScript and Python are enabled, `src/generated.ts` is excluded, and the repository
contains:

```text
src/
├── app.ts
├── app.test.ts
├── generated.ts
├── index.ts
├── README.md
└── feature/
    ├── api.ts
    └── model.py
```

The counts are:

| Directory     | Direct production files | Direct test files |
| ------------- | ----------------------- | ----------------- |
| `src`         | 2: `app.ts`, `index.ts` | 1: `app.test.ts`  |
| `src/feature` | 2: `api.ts`, `model.py` | 0                 |

`src/generated.ts` does not count because it is excluded. Markdown belongs to the automatic
documentation group, so `src/README.md` does not count against the source group's limit. Custom
groups can likewise separate related languages. The `feature` directory itself does not count
toward `src`, and its files count only toward `src/feature`.

A source file counts even when it is empty or is an index file such as `index.ts`. Files from
different languages in the same group share one count, as `api.ts` and `model.py` do above.

Ignored files, excluded files, files in disabled languages, and files no language claims do not
count. A test file counts only toward its directory's direct test files. When
`[tests].check = false`, it does not count at all.

## Ratchet file

The ratchet file records a ceiling for each existing violation. Each entry stores only `max_lines`
or `max_direct_files`. Reasons for permanent exceptions belong in `[[overrides]]`.

File entries are keyed by path and production or test role. Directory entries are keyed by path,
role, and language group.

`check` compares each measurement with its limit and with any ceiling:

- With no ratchet entry, `check` accepts a measurement at or below its limit.
- With a ratchet entry, `check` accepts a measurement only while it equals the ceiling.
- Above the ceiling, the check fails because the violation grew.
- An entry is stale when its measurement drops below the ceiling, no longer exceeds the limit, or
  disappears. By default, `check` fails on stale entries. Record the improvement with
  `tighten --write`.

For a quick local check, `check --no-require-ratchet-current` allows stale entries. Setting
`[ratchet].require_current = false` makes that the default. Neither allows growth above a ceiling.

`update` adds ratchet entries for new violations and keeps existing ceilings. It raises a ceiling
only with `--allow-increases`. `tighten` only lowers ceilings or removes stale entries.

## Selected paths

Paths passed to `check`, `update`, or `tighten` select what they cover. Paths are relative to the
repository root, and absolute paths must still be inside it. A directory covers itself and
everything under it. A file covers only itself, so `update` on a file never adds an entry for its
directory.

With selected paths, measurements and stale entries elsewhere are not reported. Run `check` with no
paths to cover the whole repository, especially in CI.
