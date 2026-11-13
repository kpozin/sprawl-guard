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

Sprawl Guard publishes separate JSON Schemas for root configuration and ratchet files. `init`,
`config export`, `update --write`, and `tighten --write` include the Taplo directive that selects
the appropriate schema in editors such as the Even Better TOML extension for VS Code.

Run `sprawl-guard schema config` or `sprawl-guard schema ratchet` to print either schema for local
or offline use.

## Upgrade the configuration

After upgrading Sprawl Guard, run `sprawl-guard config upgrade` to preview the upgraded root
configuration, and `sprawl-guard config upgrade --write` to save it. `--write` keeps your formatting
and comments, and replaces the file atomically only after the upgraded configuration passes
validation. `sprawl-guard config upgrade --check` saves nothing and exits with code `1` when an
upgrade is needed, so CI can require the committed configuration to stay current.

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

| Field              | Type and default             | Meaning                                                     |
| ------------------ | ---------------------------- | ----------------------------------------------------------- |
| `follow_symlinks`  | enum, `"none"`               | `none`, `files`, `directories`, or `files-and-directories`. |
| `same_file_system` | boolean, `true`              | Stay on the root's filesystem.                              |
| `max_entries`      | positive integer, `1000000`  | Maximum visited filesystem entries.                         |
| `max_file_bytes`   | positive integer, `10485760` | Largest source file Sprawl Guard reads.                     |
| `unreadable_paths` | enum, `"fail"`               | `fail` or `warn` for unreadable paths below the root.       |

A source file larger than `max_file_bytes` stops the command with exit code `2`. Exclude large
generated files instead of raising the limit.

The WebAssembly build that the npm package runs on platforms without a native build ignores
`same_file_system`, so it also reads filesystems mounted below the root.

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

Each override must set at least one limit. Matching entries apply in order, and later entries
replace only the fields they set.

```toml title="sprawl-guard.toml"
[[overrides]]
paths = ["vendor/**"]
max_lines_per_file = { production = 1200, test = 1600 }
max_direct_files_per_directory = { production = 20, test = "unlimited" }
reason = "Upstream layout"
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

| Field    | Type and default | Meaning                      |
| -------- | ---------------- | ---------------------------- |
| `format` | enum, `"human"`  | `human`, `json`, or `sarif`. |

Only `check` reads `[output].format`, and `check --format` takes precedence. Other commands default
to human output.

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
