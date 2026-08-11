use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use globset::{Glob, GlobSetBuilder};
use schemars::generate::SchemaSettings;
use schemars::transform::ReplaceConstValue;
use schemars::{JsonSchema, Schema};
use serde::Deserialize;

use crate::GlobPattern;
#[cfg(test)]
use crate::LanguageId;
use crate::value::path::normalize_under_root;
use crate::{Result, SprawlError};

use super::defaults::built_in_config;
use super::files::{PartialFilesConfig, merge_global_file_excludes};
use super::ignore::{PartialIgnoreConfig, merge_ignore};
use super::include::{IncludeFragment, PartialConfigInclude, merge_config_include};
use super::language_groups::{PartialLanguageGroupsConfig, resolve_language_groups};
use super::languages::{PartialLanguagesConfig, merge_language};
use super::limits::{PartialLimits, merge_limits};
use super::output::{PartialOutputConfig, merge_output};
use super::ratchet::{PartialRatchetConfig, merge_ratchet};
use super::tests::{PartialTestsConfig, merge_tests};
use super::traversal::{PartialTraversalConfig, merge_traversal};
use super::{Config, ConfigSchemaVersion, PathOverride};

const PRIMARY_CONFIG_FILE: &str = "sprawl-guard.toml";
const ALIAS_CONFIG_FILE: &str = ".sprawlrc.toml";
const CONFIG_SCHEMA_ID: &str =
    "https://kpozin.github.io/sprawl-guard/schemas/v1/sprawl-guard.schema.json";
const CONFIG_SCHEMA_META_SCHEMA: &str = "http://json-schema.org/draft-04/schema#";

/// Options used while loading configuration.
#[derive(Clone, Debug)]
pub struct LoadOptions {
    /// Repository root used for config discovery and relative paths.
    pub root: PathBuf,
    /// Optional explicit config path.
    pub config: Option<PathBuf>,
}

/// Loaded resolved configuration and source metadata.
#[derive(Clone, Debug)]
pub struct LoadedConfig {
    /// Resolved configuration.
    pub config: Config,
    /// Root config file path, if one was found.
    pub source: Option<PathBuf>,
    /// Included override fragment paths.
    pub includes: Vec<PathBuf>,
}

/// Loads the resolved configuration.
pub fn load_resolved_config(options: &LoadOptions) -> Result<LoadedConfig> {
    let root = options
        .root
        .canonicalize()
        .map_err(|source| SprawlError::ResolveRoot {
            path: options.root.clone(),
            source,
        })?;
    let source = discover_config(&root, options.config.as_deref())?;
    let mut config = built_in_config();
    let mut includes = Vec::new();

    if let Some(source) = &source {
        let partial = read_root_config(source)?;
        merge_root_config(&mut config, partial)?;
        includes = load_includes(&root, source, &config.config.include)?;
        for include in &includes {
            let fragment = read_include_fragment(include)?;
            config.overrides.extend(fragment.overrides);
        }
    }

    Ok(LoadedConfig {
        config,
        source,
        includes,
    })
}

/// Generates the JSON Schema for user-authored root configuration files.
pub fn authored_config_json_schema() -> Schema {
    let generator = SchemaSettings::draft07()
        .with(|settings| {
            settings.meta_schema = Some(CONFIG_SCHEMA_META_SCHEMA.into());
            settings
                .transforms
                .push(Box::new(ReplaceConstValue::default()));
        })
        .for_deserialize()
        .into_generator();
    let mut schema = generator.into_root_schema_for::<PartialConfig>();
    schema.insert("id".to_owned(), CONFIG_SCHEMA_ID.into());
    schema
}

/// Finds the root config file, honoring an explicit path before conventional names.
///
/// Ambiguous implicit config names are rejected so users do not accidentally run with a surprising
/// mix of `.sprawlrc.toml` and `sprawl-guard.toml`.
fn discover_config(root: &Path, explicit: Option<&Path>) -> Result<Option<PathBuf>> {
    if let Some(explicit) = explicit {
        let path = if explicit.is_absolute() {
            explicit.to_path_buf()
        } else {
            root.join(explicit)
        };
        if !path.exists() {
            return Err(SprawlError::MissingConfig { path });
        }
        return Ok(Some(path));
    }

    let primary = root.join(PRIMARY_CONFIG_FILE);
    let alias = root.join(ALIAS_CONFIG_FILE);
    match (primary.exists(), alias.exists()) {
        (true, true) => Err(SprawlError::AmbiguousConfig { primary, alias }),
        (true, false) => Ok(Some(primary)),
        (false, true) => Ok(Some(alias)),
        (false, false) => Ok(None),
    }
}

fn read_root_config(path: &Path) -> Result<PartialConfig> {
    let contents = fs::read_to_string(path).map_err(|source| SprawlError::ReadConfig {
        path: path.to_path_buf(),
        source,
    })?;
    let document =
        toml::from_str::<toml::Value>(&contents).map_err(|source| SprawlError::ParseToml {
            path: path.to_path_buf(),
            source,
        })?;
    validate_root_schema_version(&document)?;
    document
        .try_into()
        .map_err(|source| SprawlError::ParseToml {
            path: path.to_path_buf(),
            source,
        })
}

fn validate_root_schema_version(document: &toml::Value) -> Result<()> {
    let Some(version) = document.get("schema_version") else {
        return Err(SprawlError::MissingConfigSchemaVersion);
    };
    if let Some(version) = version.as_integer() {
        ConfigSchemaVersion::try_from(version)?;
    }
    Ok(())
}

/// Reads an included config fragment after enforcing the fragment-only schema.
///
/// Include files are intentionally restricted to overrides. Global settings must stay in the root
/// config so resolved config precedence remains obvious.
fn read_include_fragment(path: &Path) -> Result<IncludeFragment> {
    let contents = fs::read_to_string(path).map_err(|source| SprawlError::ReadConfig {
        path: path.to_path_buf(),
        source,
    })?;
    let value =
        toml::from_str::<toml::Value>(&contents).map_err(|source| SprawlError::ParseToml {
            path: path.to_path_buf(),
            source,
        })?;
    reject_non_override_fragment_tables(path, &value)?;
    toml::from_str(&contents).map_err(|source| SprawlError::ParseToml {
        path: path.to_path_buf(),
        source,
    })
}

/// Rejects include fragments that attempt to define root-only config tables.
fn reject_non_override_fragment_tables(path: &Path, value: &toml::Value) -> Result<()> {
    let Some(table) = value.as_table() else {
        return Err(SprawlError::IncludeFragmentNotTable {
            path: path.to_path_buf(),
        });
    };
    for key in table.keys() {
        if key != "overrides" {
            return Err(SprawlError::IncludeFragmentUnsupportedTable {
                path: path.to_path_buf(),
                table: key.clone(),
            });
        }
    }
    Ok(())
}

/// Resolves include globs relative to the root config directory.
///
/// Returned paths are sorted by normalized repository-relative path to make merge order
/// deterministic across platforms and filesystems.
fn load_includes(root: &Path, source: &Path, patterns: &[GlobPattern]) -> Result<Vec<PathBuf>> {
    let source_dir = source.parent().unwrap_or(root);
    let candidates = files_under(source_dir)?;
    let mut matched = BTreeSet::new();

    for pattern in patterns {
        let glob = Glob::new(pattern.as_str()).map_err(|source| SprawlError::InvalidGlob {
            pattern: pattern.to_string(),
            source,
        })?;
        let mut builder = GlobSetBuilder::new();
        builder.add(glob);
        let set = builder.build().map_err(|source| SprawlError::InvalidGlob {
            pattern: pattern.to_string(),
            source,
        })?;

        for candidate in &candidates {
            let relative = normalize_under_root(source_dir, candidate)?;
            if set.is_match(relative.as_str()) {
                matched.insert(relative);
            }
        }
    }

    Ok(matched
        .into_iter()
        .map(|relative| source_dir.join(relative.to_path_buf()))
        .collect())
}

/// Lists regular files below a directory in deterministic order.
fn files_under(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_files(root, &mut files)?;
    files.sort();
    Ok(files)
}

/// Recursively collects regular files while refusing to descend through symlinks.
///
/// Config includes are a trusted input surface; skipping symlink traversal keeps include expansion
/// bounded to the intended directory tree.
fn collect_files(root: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(root).map_err(|source| SprawlError::ReadDirectory {
        path: root.to_path_buf(),
        source,
    })? {
        let entry = entry.map_err(|source| SprawlError::ReadDirectoryEntry {
            path: root.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|source| SprawlError::ReadFileType {
                path: path.clone(),
                source,
            })?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_files(&path, files)?;
        } else if file_type.is_file() {
            files.push(path);
        }
    }
    Ok(())
}

/// Applies root config fields over built-in defaults in precedence order.
///
/// This merge is deliberately not generic: each section has different replacement or append
/// semantics, and spelling those out here keeps config precedence auditable.
fn merge_root_config(config: &mut Config, partial: PartialConfig) -> Result<()> {
    // Keep this destructuring exhaustive so new root config fields fail compilation until their
    // precedence and merge semantics are handled here.
    let PartialConfig {
        schema_version,
        config: config_include,
        limits,
        ignore,
        traversal,
        files,
        tests,
        languages,
        language_groups,
        overrides,
        ratchet,
        output,
    } = partial;

    config.schema_version = schema_version;
    if let Some(value) = config_include {
        merge_config_include(&mut config.config, value);
    }
    if let Some(value) = limits {
        merge_limits(&mut config.limits, value);
    }
    if let Some(value) = ignore {
        merge_ignore(&mut config.ignore, value);
    }
    if let Some(value) = traversal {
        merge_traversal(&mut config.traversal, value);
    }
    if let Some(value) = files {
        merge_global_file_excludes(&mut config.files, value);
    }
    if let Some(value) = tests {
        merge_tests(&mut config.tests, value);
    }
    if let Some(languages) = languages {
        for (language, value) in languages.into_entries() {
            let Some(existing) = config.languages.get_mut(&language) else {
                return Err(SprawlError::UnknownLanguage {
                    language: language.to_string(),
                });
            };
            merge_language(language, existing, value)?;
        }
        for (language, value) in &config.languages {
            if value.is_enabled()
                && value.common().include.as_ref().is_some_and(Vec::is_empty)
                && value.common().additional_globs.is_empty()
            {
                return Err(SprawlError::LanguageWithoutIncludedPaths {
                    language: *language,
                });
            }
        }
    }
    config.language_groups = resolve_language_groups(
        language_groups,
        config.languages.keys().copied(),
        &config.limits.max_leaf_files_per_directory,
    )?;
    if let Some(value) = overrides {
        config.overrides = value;
    }
    if let Some(value) = ratchet {
        merge_ratchet(&mut config.ratchet, value);
    }
    if let Some(value) = output {
        merge_output(&mut config.output, value);
    }
    Ok(())
}

/// User-authored root configuration before defaults and includes are resolved.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(title = "Sprawl Guard configuration")]
struct PartialConfig {
    schema_version: ConfigSchemaVersion,
    config: Option<PartialConfigInclude>,
    limits: Option<PartialLimits>,
    ignore: Option<PartialIgnoreConfig>,
    traversal: Option<PartialTraversalConfig>,
    files: Option<PartialFilesConfig>,
    tests: Option<PartialTestsConfig>,
    languages: Option<PartialLanguagesConfig>,
    language_groups: Option<PartialLanguageGroupsConfig>,
    overrides: Option<Vec<PathOverride>>,
    ratchet: Option<PartialRatchetConfig>,
    output: Option<PartialOutputConfig>,
}

#[cfg(test)]
#[path = "load_tests.rs"]
mod tests;
