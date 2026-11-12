---
title: Configuration reference
description: Fields, defaults, and precedence rules for sprawl-guard.toml.
---

Sprawl Guard implicitly discovers only `sprawl-guard.toml` under the repository root. `--config
PATH` selects an explicit file, including alternative filenames such as `.sprawlrc.toml`. Unknown
sections and fields are errors.

Every root configuration starts with an editor schema directive and declares its schema version:

```toml title="sprawl-guard.toml"
#:schema https://kpozin.github.io/sprawl-guard/schemas/v2/sprawl-guard.schema.json
schema_version = 2
```

Run `sprawl-guard init` to create this minimal file, then add the sections below.

Use `sprawl-guard config resolved` to inspect the effective policy. Use
`sprawl-guard config export` to print a complete configuration suitable for committing.

## Editor support

Sprawl Guard publishes separate JSON Schemas for root configuration and ratchet files. `init`,
`config export`, `update --write`, and `tighten --write` include the Taplo directive that selects
the appropriate schema in editors such as the Even Better TOML extension for VS Code.

Run `sprawl-guard schema config` or `sprawl-guard schema ratchet` to print either schema for local
or offline use.

After upgrading Sprawl Guard, run `sprawl-guard config upgrade` to update the authored root config.
The command preserves authored formatting and comments and replaces the file atomically after the
config validates. `sprawl-guard config upgrade --check` is read-only and exits with code `1` when
an upgrade is needed, so CI can require committed configs to stay current. The V1→V2 migration
removes the no-op `[output].color` field while preserving all other authored settings.

## Sections and defaults

### `[limits.max_lines_per_file]`

| Field                 | Type and default        | Meaning                                                   |
| --------------------- | ----------------------- | --------------------------------------------------------- |
| `included_line_types` | array, `["code"]`       | Count `code`, with optional `comments` and `blank_lines`. |
| `production`          | positive integer, `400` | Configured LOC in a production file.                      |
| `test`                | positive integer, `800` | Configured LOC in a path-classified test file.            |

`included_line_types` must contain `code` and may not contain duplicates. Literate languages
automatically add `comments` unless their language-specific setting replaces the global selection.

### `[limits.max_leaf_files_per_directory]`

| Field        | Type and default                        | Meaning                                          |
| ------------ | --------------------------------------- | ------------------------------------------------ |
| `production` | positive integer or `"unlimited"`, `10` | Direct production source files in one directory. |
| `test`       | positive integer or `"unlimited"`, `10` | Direct test source files in one directory.       |

### `[ignore]`

All fields are booleans and default to `true`.

Only ignore files within the selected root affect ignore policy. Sprawl Guard ignores global Git
excludes, `.git/info/exclude`, and ignore files above that root.

Enabled ignore files must not be symlinks, including broken links. Sprawl Guard rejects such policy
instead of reading rules from an external target.

| Field               | Meaning                                                                                        |
| ------------------- | ---------------------------------------------------------------------------------------------- |
| `respect_gitignore` | Respect `.gitignore` files.                                                                    |
| `respect_ignore`    | Respect `.ignore` files.                                                                       |
| `respect_rgignore`  | Respect `.rgignore` files.                                                                     |
| `respect_fdignore`  | Respect `.fdignore` files.                                                                     |
| `skip_hidden`       | Skip dot-prefixed file and directory names; Windows hidden attributes do not affect selection. |

### `[traversal]`

| Field              | Type and default             | Meaning                                                          |
| ------------------ | ---------------------------- | ---------------------------------------------------------------- |
| `follow_symlinks`  | enum, `"none"`               | `none`, `files`, `directories`, or `files-and-directories`.      |
| `same_file_system` | boolean, `true`              | Stay on the root's filesystem.                                   |
| `max_entries`      | positive integer, `1000000`  | Maximum visited filesystem entries.                              |
| `max_file_bytes`   | positive integer, `10485760` | Largest source file read for line counting.                      |
| `error_policy`     | enum, `"fail"`               | `fail` or `warn` for recoverable descendant traversal incidents. |

Native builds enforce `same_file_system`. WASI builds accept but ignore it. The npm fallback
preopens only the selected project root, but it can still traverse mounted filesystems below that
root. `config resolved` reports the configured value, not this platform-specific behavior.

### `[files]`

`exclude` is an array of repository-relative globs. Matching paths are removed before language
classification. Authored patterns extend the built-in list rather than replacing it.

The built-in exclusions use the lockfile-related patterns from
[GitHub's code-review exclusions](https://docs.github.com/en/copilot/reference/review-excluded-files)
as a baseline. They match these patterns at any depth:

```text
**/*.lock              **/bun.lockb            **/go.sum
**/npm-shrinkwrap.json **/package-lock.json    **/Package.resolved
**/pnpm-lock.yaml
```

### `[tests]`

`check_path_tests` is a boolean that defaults to `true`. When false, files matched by a test glob
are omitted. `globs` replaces the default array of test globs:

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

| Field              | Type and default         | Meaning                                                                                    |
| ------------------ | ------------------------ | ------------------------------------------------------------------------------------------ |
| `enabled`          | boolean, `true`          | Enable or disable this language.                                                           |
| `include`          | optional array of globs  | Replace the language's bundled candidate paths.                                            |
| `additional_globs` | array of globs, `[]`     | Add candidate paths without replacing bundled paths.                                       |
| `exclude`          | array of globs, `[]`     | Remove matching candidates for this language.                                              |
| `test_globs`       | array of globs, `[]`     | Additional test-file classifiers. Rust defaults to `["benches/**"]`.                       |
| `exclude_cfg_test` | boolean, `true` for Rust | Exclude top-level Rust `cfg(test)` items from production LOC. Invalid for other languages. |

Every Tokei language is enabled by default. Under `[languages.<id>.limits.max_lines_per_file]`,
`production`, `test`, and `included_line_types` are optional. Numeric fields replace the
corresponding global threshold. `included_line_types` replaces the global or automatic
literate-language selection. Run `sprawl-guard languages` to list all supported IDs and their
bundled paths. IDs are case-insensitive.

### `[language_groups.<name>]`

Custom language groups give related languages a shared directory fan-out count and policy.
`languages` is a non-empty array of distinct supported language IDs. A language may belong to at
most one custom group; disabled languages may remain listed.

```toml title="sprawl-guard.toml"
[language_groups.frontend]
languages = ["javascript", "typescript", "jsx", "tsx"]

[language_groups.frontend.limits.max_leaf_files_per_directory]
production = 12
test = 20
```

Omitted custom-group limit fields inherit their global values. Languages outside custom groups are
assigned automatically: Tokei literate languages use `documentation`, and all others use `source`.
The `source` group uses the global directory limits and cannot be configured directly.

The `documentation` group is unlimited for production and test fan-out by default. Its table cannot
declare `languages`, but a custom group may claim a literate language and replace that language's
automatic assignment. Documentation-group limits can be made finite:

```toml title="sprawl-guard.toml"
[language_groups.documentation.limits.max_leaf_files_per_directory]
production = 20
```

### `[[overrides]]`

| Field                                     | Type                                       | Meaning                                            |
| ----------------------------------------- | ------------------------------------------ | -------------------------------------------------- |
| `paths`                                   | non-empty array of globs                   | Repository-relative paths matched by the override. |
| `reason`                                  | non-empty string                           | Human-readable justification.                      |
| `max_lines_per_file.production`           | optional positive integer                  | Production file limit.                             |
| `max_lines_per_file.test`                 | optional positive integer                  | Test file limit.                                   |
| `max_leaf_files_per_directory.production` | optional positive integer or `"unlimited"` | Production fan-out limit.                          |
| `max_leaf_files_per_directory.test`       | optional positive integer or `"unlimited"` | Test fan-out limit.                                |

Each override must set at least one limit. Matching entries apply in order; later entries replace
only the fields they set.

```toml title="sprawl-guard.toml"
[[overrides]]
paths = ["vendor/**"]
max_lines_per_file = { production = 1200, test = 1600 }
max_leaf_files_per_directory = { production = 20, test = "unlimited" }
reason = "Upstream layout"
```

### `[ratchet]`

| Field             | Type and default                    | Meaning                                     |
| ----------------- | ----------------------------------- | ------------------------------------------- |
| `file`            | path, `"sprawl-guard.ratchet.toml"` | Repository-relative ratchet file.           |
| `require_current` | boolean, `true`                     | Fail checks when ratchet entries are stale. |

This section selects the ratchet file and stale-entry policy; ratchet ceilings are stored in the
separate file. See
[Adopt Sprawl Guard in an existing repository](/sprawl-guard/adopt-existing-repository/) for its
format and creation workflow.

### `[output]`

| Field    | Type and default | Meaning                      |
| -------- | ---------------- | ---------------------------- |
| `format` | enum, `"human"`  | `human`, `json`, or `sarif`. |

## Precedence

Sprawl Guard applies configuration in this order:

1. Built-in defaults provide the base policy. Fields in the root configuration replace the
   corresponding defaults.
2. Arrays in the root configuration replace their default arrays unless their field documentation
   says otherwise. In particular, `[files].exclude` extends the built-in lockfile exclusions. Root
   `[[overrides]]` replace the empty default override list.
3. Language-specific file thresholds and included line types replace their global values. Literate
   languages add comments automatically when no language-specific line-type selection is present.
4. Custom-group directory limits replace their global values. Documentation directory limits start
   from their unlimited defaults.
5. Matching path overrides apply in declaration order. Later entries replace only the fields they
   set.
6. CLI options override the output format, ratchet-current requirement, traversal error policy,
   root, and configuration path for that run.
