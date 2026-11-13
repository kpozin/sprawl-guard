---
name: sprawl-guard
description: Keep source files and directories within the size limits that Sprawl Guard checks. Use before declaring work complete in a repository that has a sprawl-guard.toml file, or when a sprawl-guard check fails.
---

# Sprawl Guard

Sprawl Guard checks that source files stay under a line limit and that directories hold no more
than a set number of source files. A ratchet file, `sprawl-guard.ratchet.toml`, records the
violations that a repository already had, and the check fails if any of them grows.

## Run Sprawl Guard

Run commands from the repository root. If `sprawl-guard` is not on `PATH`, run it through the
project's package manager, such as `npm exec -- sprawl-guard` or `pnpm exec sprawl-guard`. If the
repository defines its own task for the check, such as a `just` or `make` recipe, use that instead.

## Before declaring work complete

1. If the repository has a ratchet file, run `sprawl-guard tighten --write`. It lowers or removes
   ratchet entries for violations that shrank, and never raises one, so it needs no review.
2. Run `sprawl-guard check`. It exits with status 0 when the check passes, 1 when it finds
   violations, and 2 when it cannot run, for example because of a configuration error.
3. Keep ratchet file changes with the code changes that caused them.

## Fix a failing check

The check groups failures under these headings:

- **Files over the line limit** or **Test files over the line limit**: split the file into
  cohesive, clearly named modules with distinct responsibilities.
- **Directories with too many direct files** or **Test directories with too many direct files**:
  group related files into clearly named subdirectories.
- **Ratchet failures**, for an entry whose count rose above its ceiling: shrink the file or
  directory back to the ceiling or below. The same path also appears under its limit heading;
  both describe one problem.
- **Ratchet failures**, for a stale entry: the violation shrank, so run
  `sprawl-guard tighten --write`.

Do not satisfy a limit by moving code into miscellaneous "helpers", "utils" or "common" files.

Do not change what the check counts or allows to make it pass. That includes raising limits, adding
overrides or exclusions to `sprawl-guard.toml`, passing `--no-require-ratchet-current`, and running
`sprawl-guard update --write` or `--allow-increases`. If a violation cannot reasonably be fixed,
stop and ask the user. Show them the output of `sprawl-guard update PATH` without `--write`, which
previews the ratchet entry that recording it would add.

## Investigate

- `sprawl-guard explain PATH` shows how Sprawl Guard classifies a path and which limits apply to it.
- `--format json` on `check` and `explain` prints machine-readable output. Check the exit code
  before parsing it.
- The documentation is available as Markdown, starting at
  <https://kpozin.github.io/sprawl-guard/index.md>. The most useful pages are
  [Work with the ratchet](https://kpozin.github.io/sprawl-guard/ratchet.md) and
  [CLI and output](https://kpozin.github.io/sprawl-guard/reference/cli-and-output.md).
