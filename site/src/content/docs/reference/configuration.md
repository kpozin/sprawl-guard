---
title: Configuration reference
description: Fields, defaults, and precedence rules for sprawl-guard.toml.
---

Sprawl Guard discovers `sprawl-guard.toml` or `.sprawlrc.toml` under the repository root. If both
exist, discovery fails. `--config PATH` selects an explicit file. Unknown sections and fields are
errors.

Use `sprawl-guard config resolved` to inspect the effective policy. Use
`sprawl-guard config export` to print a complete configuration suitable for committing.

## Sections and defaults

### `[config]`

| Field     | Type and default     | Meaning                                                        |
| --------- | -------------------- | -------------------------------------------------------------- |
| `include` | array of globs, `[]` | Override-fragment files relative to the root config directory. |

Included files may contain only `[[overrides]]`. Sprawl Guard loads them in normalized
lexicographic path order and appends their entries after the root file's overrides. It does not
follow symlinks while loading these files.

### `[limits.max_lines_per_file]`

| Field                 | Type and default        | Meaning                                                   |
| --------------------- | ----------------------- | --------------------------------------------------------- |
| `included_line_types` | array, `["code"]`       | Count `code`, with optional `comments` and `blank_lines`. |
| `production`          | positive integer, `400` | Configured LOC in a production file.                      |
| `test`                | positive integer, `800` | Configured LOC in a path-classified test file.            |

`included_line_types` must contain `code` and may not contain duplicates. Literate languages
automatically add `comments` unless their language-specific setting replaces the global selection.

### `[limits.max_leaf_files_per_directory]`

| Field        | Type and default                                 | Meaning                                          |
| ------------ | ------------------------------------------------ | ------------------------------------------------ |
| `production` | positive integer or `"unlimited"`, `8`           | Direct production source files in one directory. |
| `test`       | positive integer or `"unlimited"`, `"unlimited"` | Direct test source files in one directory.       |

### `[ignore]`

All fields are booleans and default to `true`.

| Field               | Meaning                                             |
| ------------------- | --------------------------------------------------- |
| `respect_gitignore` | Respect `.gitignore` files.                         |
| `respect_ignore`    | Respect `.ignore` files.                            |
| `respect_rgignore`  | Respect `.rgignore` files.                          |
| `respect_fdignore`  | Respect `.fdignore` files.                          |
| `skip_hidden`       | Skip hidden files and directories during traversal. |

### `[traversal]`

| Field              | Type and default             | Meaning                                                          |
| ------------------ | ---------------------------- | ---------------------------------------------------------------- |
| `follow_symlinks`  | enum, `"none"`               | `none`, `files`, `directories`, or `files-and-directories`.      |
| `same_file_system` | boolean, `true`              | Stay on the root's filesystem.                                   |
| `max_depth`        | positive integer, `128`      | Maximum depth below the root.                                    |
| `max_entries`      | positive integer, `1000000`  | Maximum visited filesystem entries.                              |
| `max_file_bytes`   | positive integer, `10485760` | Largest source file read for line counting.                      |
| `error_policy`     | enum, `"fail"`               | `fail` or `warn` for recoverable descendant traversal incidents. |

Native builds enforce `same_file_system`. WASI builds accept but ignore it. The npm fallback
preopens only the selected project root, but it can still traverse mounted filesystems below that
root. `config resolved` reports the configured value, not this platform-specific behavior.

### `[files]`

`exclude` is an array of repository-relative globs and defaults to `[]`. Matching paths are removed
before language classification.

### `[tests]`

`check_path_tests` is a boolean that defaults to `true`. When false, files matched by a test glob
are omitted. `globs` replaces the default array of test globs:

```toml title="sprawl-guard.toml"
[tests]
globs = [
  "tests/**", "**/tests/**", "test/**", "**/test/**",
  "spec/**", "**/spec/**", "__tests__/**", "**/__tests__/**",
  "__mocks__/**", "**/__mocks__/**", "**/*.test.*", "**/*.spec.*",
  "**/*_test.*", "**/*_spec.*",
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

### `[output]`

| Field    | Type and default | Meaning                                                                     |
| -------- | ---------------- | --------------------------------------------------------------------------- |
| `format` | enum, `"human"`  | `human`, `json`, or `sarif`.                                                |
| `color`  | enum, `"auto"`   | Accepted values are `auto`, `always`, and `never`; currently has no effect. |

## Precedence

Sprawl Guard applies configuration in this order:

1. Built-in defaults provide the base policy. Fields in the root configuration replace the
   corresponding defaults.
2. Arrays in the root configuration replace their default arrays. Root `[[overrides]]` replace the
   empty default override list.
3. Included fragments append overrides in lexicographic file order.
4. Language-specific file thresholds and included line types replace their global values. Literate
   languages add comments automatically when no language-specific line-type selection is present.
5. Matching path overrides apply in declaration order. Later entries replace only the fields they
   set.
6. CLI options override the output format, ratchet-current requirement, traversal error policy,
   root, and configuration path for that run.
