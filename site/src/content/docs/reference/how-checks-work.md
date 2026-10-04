---
title: How checks work
description: Source discovery, measurements, limits, scopes, and ratchet semantics.
---

## Discovery and language support

Sprawl Guard uses the language table bundled with Tokei. Run `sprawl-guard languages` to list every
supported language ID, extension, and exact filename. Run `sprawl-guard languages --detected` to
see which languages own visible files in the repository.

Discovery starts at the repository root or the supplied path scopes. Sprawl Guard applies standard
ignore files, the hidden-path policy, and traversal safety limits. It then applies global file
excludes and each language's bundled or configured paths and excludes. Globs match normalized
repository-relative paths. Every bundled language is enabled by default, and a source file must
belong to exactly one enabled language.

## Test classification

A source file is test code when it matches `[tests].globs` or its language's `test_globs`. Test
files use the test line and fan-out limits; when `check_path_tests = false`, they are omitted.

For a production Rust file, `exclude_cfg_test = true` removes top-level items guarded by
`cfg(test)` from every included line type. It does not turn the file into a test file. A parse
failure counts the original source with the effective line types and adds a warning. A
path-classified Rust test is not preprocessed.

## Measurements and violations

Sprawl Guard uses Tokei to count configured logical lines of code (LOC). `code` always counts;
`comments` and `blank_lines` are optional. Literate languages include comments automatically unless
their language configuration replaces that selection. Sprawl Guard evaluates four measurements:

- configured LOC in each production file;
- configured LOC in each path-classified test file;
- direct production source files in each directory;
- direct test source files in each directory.

Directory fan-out is measured even when its effective limit is `"unlimited"`; unlimited disables
the policy violation, not the measurement or its ratchet comparison. A matching path override can
still replace an unlimited group limit with a finite one.

Global limits apply first. Language-specific policy can replace file limits, and custom groups can
replace directory limits. Matching path overrides apply last. A measurement above its effective
limit is a current violation.

### Directory fan-out example

Directory fan-out is the number of included source files whose immediate parent is the directory
being measured. It is not a recursive count. Production and test files have separate totals.

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

The resulting measurements are:

| Directory     | Production fan-out      | Test fan-out     |
| ------------- | ----------------------- | ---------------- |
| `src`         | 2: `app.ts`, `index.ts` | 1: `app.test.ts` |
| `src/feature` | 2: `api.ts`, `model.py` | 0                |

`src/generated.ts` does not count because it is excluded. Markdown belongs to the automatic
documentation group, so `src/README.md` does not consume the source-group fan-out budget. Custom
groups can likewise separate related languages from the source count. The `feature` directory
itself does not count toward `src`; its files count only toward `src/feature`.

An included source file counts even when it is empty or uses a facade name such as `index.ts`.
Files from different enabled languages share the same total, as `api.ts` and `model.py` do above.

Ignored files, global or language-level exclusions, files in disabled languages, and unmatched
files do not count. A path-classified test counts only toward test fan-out;
when `[tests].check_path_tests = false`, it is omitted entirely.

## Ratchet ceilings

The ratchet file records ceilings for current violations: the maximum permitted measurement for a
path that already violates the policy. It stores only the allowed `max_loc` or `leaf_files` value,
not Git history, ownership, reasons, languages, or configured limits.

File ceilings are keyed by path and production/test role. Directory ceilings are keyed by path,
role, and language group.

Checks compare each measurement with both its configured limit and any ceiling:

- Without a ratchet entry, a measurement at or below its configured limit passes.
- A ratcheted measurement passes only while it remains equal to its ceiling.
- Above the ceiling, the ratchet fails because the violation grew.
- An entry becomes stale when its measurement falls below the ceiling, no longer exceeds the
  configured limit, or disappears. Stale entries fail by default so an explicit `tighten --write`
  preserves the improvement.

For an exploratory local check, `check --no-require-ratchet-current` allows stale entries. Setting
`[ratchet].require_current = false` makes that behavior the default. Neither option permits growth
above a ceiling.

`update` adds ceilings for newly discovered violations while preserving existing ceilings. It may
raise existing ceilings only with `--allow-increases`. `tighten` only lowers ceilings or removes
stale entries.

## Scopes

Paths passed to `check`, `update`, or `tighten` form repository-relative scopes; absolute paths
must still resolve under the repository root. A directory scope covers that directory and its
descendants. A file scope covers the file, but does not create a ratchet entry for its containing
directory's fan-out.

Scoped checks intentionally ignore measurements and stale entries outside their scopes. Use an
unscoped `check` to enforce the complete repository, especially in CI.

## Terminology

| Term              | Meaning                                                                            |
| ----------------- | ---------------------------------------------------------------------------------- |
| Ceiling           | Recorded maximum for one existing violation.                                       |
| Current violation | Measurement above its effective configured limit.                                  |
| Directory fan-out | Direct source files in one directory, counted separately for production and tests. |
| Production source | Source file not classified as a path-based test.                                   |
| Ratchet           | Checked-in ceilings that allow existing violations but reject growth.              |
| Stale entry       | Ceiling for a measurement that improved, disappeared, or satisfies policy.         |
| Test source       | Source file matched by a global or language-specific test glob.                    |
| Unscoped check    | `sprawl-guard check` with no path arguments.                                       |
| Update            | Add entries for newly discovered violations without lowering existing ceilings.    |
