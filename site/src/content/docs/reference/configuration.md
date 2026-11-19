---
title: Configuration reference
description: Fields, defaults, and precedence rules for sprawl-guard.toml.
---

Sprawl Guard looks only for `sprawl-guard.toml` in the repository root. To use another file, such as
`.sprawlrc.toml`, pass `--config PATH`. Unknown sections and fields are errors.

Every root configuration starts with an editor schema directive and declares its schema version:

```toml title="sprawl-guard.toml"
#:schema https://kpozin.github.io/sprawl-guard/schemas/v2/sprawl-guard.schema.json
schema_version = 2
```

Run `sprawl-guard init --write` to create this minimal file, then add the sections below.

Use `sprawl-guard config resolved` to see the full resolved configuration. Use
`sprawl-guard config export` to print a complete configuration suitable for committing.

## Editor support

Sprawl Guard publishes JSON Schemas for the
[configuration](https://kpozin.github.io/sprawl-guard/schemas/v2/sprawl-guard.schema.json) and the
[ratchet file](https://kpozin.github.io/sprawl-guard/schemas/v1/sprawl-guard.ratchet.schema.json).
`init`, `config export`, `update --write`, and `tighten --write` include the Taplo directive that
selects the appropriate schema in editors such as the Even Better TOML extension for VS Code.

Run `sprawl-guard schema config` or `sprawl-guard schema ratchet` to print either schema for local
or offline use.

## Upgrade the configuration

After upgrading Sprawl Guard, run `sprawl-guard config upgrade` to preview the upgraded root
configuration, and `sprawl-guard config upgrade --write` to save it. `--write` keeps your formatting
and comments, and leaves the file unchanged if the upgraded configuration is invalid.
`sprawl-guard config upgrade --check` saves nothing and exits with code `1` when an upgrade is
needed, so CI can require the committed configuration to stay current.

The schema v1 to v2 upgrade renames these keys and keeps all other settings:

| Schema v1                               | Schema v2                                 |
| --------------------------------------- | ----------------------------------------- |
| `[limits.max_leaf_files_per_directory]` | `[limits.max_direct_files_per_directory]` |
| `included_line_types`                   | `count_lines`, without `"code"`           |
| `[tests].check_path_tests`              | `[tests].check`                           |
| `[traversal].error_policy`              | `[traversal].unreadable_paths`            |
| `[files].exclude`                       | `[files].extend_exclude`                  |
| `[languages.<id>].additional_globs`     | `[languages.<id>].extend_include`         |

It also removes the unused `[output].color` and `[traversal].max_depth` fields. Values are checked
after the upgrade, so error messages name the v2 keys.

## Sections and defaults

### `[limits]`

| Field                | Type and default                               | Meaning                       |
| -------------------- | ---------------------------------------------- | ----------------------------- |
| `max_bytes_per_file` | positive integer or `"unlimited"`, `unlimited` | Largest size of any one file. |

`max_bytes_per_file` applies to every file that `check` visits, not only source files: images,
archives, generated data, and files of disabled or excluded languages count too. Files skipped by
ignore files, by `[files].extend_exclude`, and the configuration and ratchet files are not checked.
Set a default here and use `[[overrides]]` to raise or lower it for particular paths.

### `[limits.max_lines_per_file]`

| Field         | Type and default        | Meaning                                               |
| ------------- | ----------------------- | ----------------------------------------------------- |
| `count_lines` | array, `[]`             | Extra line types to count: `comments`, `blank_lines`. |
| `production`  | positive integer, `400` | Lines in a production file.                           |
| `test`        | positive integer, `800` | Lines in a test file.                                 |

Code lines always count. `count_lines` adds comment or blank lines and must not contain duplicates.
For [literate languages](/sprawl-guard/reference/how-checks-work/#terms), such as Markdown, Sprawl
Guard adds `comments` automatically unless the language sets its own `count_lines`.

### `[limits.max_direct_files_per_directory]`

| Field        | Type and default                        | Meaning                                          |
| ------------ | --------------------------------------- | ------------------------------------------------ |
| `production` | positive integer or `"unlimited"`, `10` | Direct production source files in one directory. |
| `test`       | positive integer or `"unlimited"`, `10` | Direct test source files in one directory.       |

### `[ignore]`

All fields are booleans and default to `true`.

Only ignore files inside the repository root are read. Global Git excludes, `.git/info/exclude`,
and ignore files above the root have no effect.

Enabled ignore files must not be symlinks, including broken links. Sprawl Guard reports an error
instead of reading rules from the link's target.

| Field               | Meaning                                                                                        |
| ------------------- | ---------------------------------------------------------------------------------------------- |
| `respect_gitignore` | Respect `.gitignore` files.                                                                    |
| `respect_ignore`    | Respect `.ignore` files.                                                                       |
| `respect_rgignore`  | Respect `.rgignore` files.                                                                     |
| `respect_fdignore`  | Respect `.fdignore` files.                                                                     |
| `skip_hidden`       | Skip dot-prefixed file and directory names; Windows hidden attributes do not affect selection. |

### `[traversal]`

| Field                        | Type and default             | Meaning                                                     |
| ---------------------------- | ---------------------------- | ----------------------------------------------------------- |
| `follow_symlinks`            | enum, `"none"`               | `none`, `files`, `directories`, or `files-and-directories`. |
| `same_file_system`           | boolean, `true`              | Stay on the root's filesystem.                              |
| `max_entries`                | positive integer, `1000000`  | Maximum visited filesystem entries.                         |
| `max_parsed_code_file_bytes` | positive integer, `10485760` | Largest source file whose code lines Sprawl Guard counts.   |
| `unreadable_paths`           | enum, `"fail"`               | `fail` or `warn` for gaps in what `check` could analyze.    |

Sprawl Guard doesn't count the lines of a source file larger than `max_parsed_code_file_bytes`.
`check` lists the file and keeps checking everything else. Exclude large generated files instead of
raising the limit. This setting protects the line counter and is not a policy. To fail `check` on
large files, set [`[limits].max_bytes_per_file`](#limits).

Paths below the root that can't be read and source files too large to count are gaps in the
analysis. `check` reports every gap along with every violation. With `unreadable_paths = "fail"`,
any gap makes `check` exit `2`. With `"warn"`, gaps are warnings, and violations alone decide the
exit code. `update` and `tighten` refuse to write the ratchet file after an unreadable path under
either setting.

`max_file_bytes` is a deprecated name for `max_parsed_code_file_bytes` and still loads. Run
`sprawl-guard config upgrade --write` to rename it; setting both names is an error. The
configuration schema accepts the old name and marks it as deprecated, so editors that use the schema
show a warning.

### `[files]`

`extend_exclude` is an array of repository-relative globs. Matching paths are removed before any
language claims them. Like every `extend_*` field, it adds to the built-in list instead of
replacing it.

The built-in exclusions are the lockfile-related patterns from
[GitHub's code-review exclusions](https://docs.github.com/en/copilot/reference/review-excluded-files),
matched at any depth:

```text
**/*.lock              **/bun.lockb            **/go.sum
**/npm-shrinkwrap.json **/package-lock.json    **/Package.resolved
**/pnpm-lock.yaml
```

### `[tests]`

`check` is a boolean that defaults to `true`. When false, test files are not checked. `globs` lists
the patterns that mark test files. Setting it replaces these defaults:

```toml title="sprawl-guard.toml"
[tests]
globs = [
  "tests/**",
  "**/tests/**",
  "test/**",
  "**/test/**",
  "spec/**",
  "**/spec/**",
  "__tests__/**",
  "**/__tests__/**",
  "__mocks__/**",
  "**/__mocks__/**",
  "**/*.test.*",
  "**/*.spec.*",
  "**/*_test.*",
  "**/*_spec.*",
]
```

### `[languages.<id>]`

| Field              | Type and default         | Meaning                                                                        |
| ------------------ | ------------------------ | ------------------------------------------------------------------------------ |
| `enabled`          | boolean, `true`          | Enable or disable this language.                                               |
| `include`          | optional array of globs  | Replace the language's bundled paths.                                          |
| `extend_include`   | array of globs, `[]`     | Add paths to the language's bundled paths.                                     |
| `exclude`          | array of globs, `[]`     | Remove matching paths for this language.                                       |
| `test_globs`       | array of globs, `[]`     | More globs that mark test files. Rust defaults to `["benches/**"]`.            |
| `exclude_cfg_test` | boolean, `true` for Rust | Stop counting top-level Rust `cfg(test)` items in production files. Rust only. |

Every Tokei language is enabled by default. Under `[languages.<id>.limits.max_lines_per_file]`,
`production`, `test`, and `count_lines` are optional. Each replaces the corresponding global value,
and `count_lines` also replaces the automatic comments for literate languages. Run
`sprawl-guard languages` to list all supported IDs and their bundled paths. IDs are
case-insensitive.

### `[language_groups.<name>]`

Custom language groups give related languages one shared direct-file count and limit per
directory. `languages` is a non-empty array of distinct supported language IDs. A language may
belong to at most one custom group, and disabled languages may remain listed.

```toml title="sprawl-guard.toml"
[language_groups.frontend]
languages = ["javascript", "typescript", "jsx", "tsx"]

[language_groups.frontend.limits.max_direct_files_per_directory]
production = 12
test = 20
```

Omitted custom-group limit fields inherit their global values. Languages outside custom groups are
assigned automatically: Tokei literate languages use `documentation`, and all others use `source`.
The `source` group uses the global directory limits and cannot be configured directly.

The `documentation` group has unlimited direct files by default. Its table cannot declare
`languages`, but a custom group may list a literate language, which replaces that language's
automatic assignment. Documentation-group limits can be made finite:

```toml title="sprawl-guard.toml"
[language_groups.documentation.limits.max_direct_files_per_directory]
production = 20
```

### `[[overrides]]`

| Field                                       | Type                                       | Meaning                                            |
| ------------------------------------------- | ------------------------------------------ | -------------------------------------------------- |
| `paths`                                     | non-empty array of globs                   | Repository-relative paths matched by the override. |
| `reason`                                    | non-empty string                           | Why the override exists.                           |
| `max_lines_per_file.production`             | optional positive integer                  | Production file limit.                             |
| `max_lines_per_file.test`                   | optional positive integer                  | Test file limit.                                   |
| `max_direct_files_per_directory.production` | optional positive integer or `"unlimited"` | Production direct-file limit.                      |
| `max_direct_files_per_directory.test`       | optional positive integer or `"unlimited"` | Test direct-file limit.                            |
| `max_bytes_per_file`                        | optional positive integer or `"unlimited"` | Byte limit for any matching file.                  |

Each override must set at least one limit. Matching entries apply in order, and later entries
replace only the fields they set.

```toml title="sprawl-guard.toml"
[[overrides]]
paths = ["vendor/**"]
max_lines_per_file = { production = 1200, test = 1600 }
max_direct_files_per_directory = { production = 20, test = "unlimited" }
reason = "Upstream layout"

[[overrides]]
paths = ["assets/**"]
max_bytes_per_file = 500000
reason = "Image budget"
```

### `[ratchet]`

| Field             | Type and default                    | Meaning                                     |
| ----------------- | ----------------------------------- | ------------------------------------------- |
| `file`            | path, `"sprawl-guard.ratchet.toml"` | Repository-relative ratchet file.           |
| `require_current` | boolean, `true`                     | Fail checks when ratchet entries are stale. |

This section names the ratchet file and whether `check` fails on stale entries. The ceilings
themselves live in the ratchet file. See
[Record existing violations](/sprawl-guard/quick-start/#record-existing-violations) to create it.

### `[output]`

| Field          | Type and default                      | Meaning                                                       |
| -------------- | ------------------------------------- | ------------------------------------------------------------- |
| `format`       | enum, `"human"`                       | `human`, `json`, `sarif`, or `github`.                        |
| `max_findings` | positive integer or `"unlimited"`, 50 | Most findings that human and GitHub output list from `check`. |

Only `check` reads `[output]`. `check --format` and `check --max-findings` take precedence. Other
commands default to human output.

`max_findings` counts violations first, then warnings, then skipped unreadable paths, so a failing
check always lists why it failed. It also bounds how many files a directory finding names, and how
many paths the report lists when unreadable paths stop the check. Findings past the cap still count
toward their group's total, a footer gives how many findings were shown out of the total, and the
exit status still reflects every violation. Recorded violations within their ratchet ceilings are
not findings, so they never count. JSON and SARIF output always list every finding.

## Precedence

Sprawl Guard applies configuration in this order:

1. Built-in defaults come first. Fields in the root configuration replace the corresponding
   defaults.
2. Arrays in the root configuration replace their default arrays, except `extend_*` fields, which
   add to them. `[files].extend_exclude` extends the built-in lockfile excludes.
3. Language-specific file limits and `count_lines` replace their global values. For literate
   languages, Sprawl Guard adds comments automatically unless a language-specific `count_lines` is
   set.
4. Custom-group directory limits replace their global values. Documentation directory limits start
   from their unlimited defaults.
5. Matching path overrides apply in declaration order. Later entries replace only the fields they
   set.
6. CLI options override the output format, whether `check` fails on stale entries, unreadable-path
   handling, root, and configuration path for that run.
