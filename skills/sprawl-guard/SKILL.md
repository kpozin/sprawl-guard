---
name: sprawl-guard
description: Keep source files and directories within the size limits that Sprawl Guard checks. Use before declaring work complete in a repository that has a sprawl-guard.toml file, or when a sprawl-guard check fails.
---

# Sprawl Guard

Sprawl Guard limits the lines in each source file and the source files directly inside each
directory. `sprawl-guard.ratchet.toml` records accepted violations, and the check fails if one
grows.

Run commands from the repository root, through the repository's own task for the check if it has
one. If `sprawl-guard` is not on `PATH`, run it through the package manager, as in
`npm exec -- sprawl-guard`.

## Before declaring work complete

1. If the repository has a ratchet file, run `sprawl-guard tighten --write`. It only lowers or
   removes entries.
2. Run `sprawl-guard check`. Fix the violations your changes caused, run it again, and tell the user
   about any others.
3. Commit ratchet changes with the code changes that caused them.

## Fix a violation by reorganizing

Give each file and directory one clear purpose. Read the code first, and decide what its parts are
for.

- **File over its limit:** list its responsibilities, such as a data type with its parsing, one
  subcommand, or the tests for one behavior. Move each into its own module named for it, keep public
  interfaces stable, and update imports.
- **Directory with too many files:** group the files by the feature or concept they serve, and move
  each group into a subdirectory named for it. Follow the repository's existing layout.
- **Violation with a ceiling,** such as `450 lines (limit 400, ceiling 430)`: it grew since it was
  recorded. Reorganize it within the limit if you can, or at least to the ceiling, then run
  `sprawl-guard tighten --write`.

These don't count as reorganizing:

- Moving code into catch-alls such as `utils`, `helpers`, `common`, or `misc`.
- Cutting a file at an arbitrary line, or grouping files that share nothing.
- Joining lines, shortening names, or deleting needed code.
- Raising limits, adding overrides or exclusions, passing `--no-require-ratchet-current`, or running
  `sprawl-guard update --write`.

If no reorganization makes sense, stop and ask the user. Explain why, and show the output of
`sprawl-guard update PATH --allow-increases`, which previews the new or raised ceiling without
writing it.

## Investigate

`sprawl-guard explain PATH` shows how a file or directory is classified and which limits apply. The
documentation is at <https://kpozin.github.io/sprawl-guard/index.md>. It also covers setup tasks
outside this skill, such as importing an ESLint or Oxlint `max-lines` rule.
