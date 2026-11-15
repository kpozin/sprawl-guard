use std::fs;
use std::path::{Component, Path, PathBuf};

use schemars::{JsonSchema, Schema};
use serde::Deserialize;

#[cfg(test)]
use crate::LanguageId;
use crate::value::schema::authored_toml_schema_generator;
use crate::{ConfigUpgradeCommand, RelativePath, Result, SprawlError};

use super::defaults::built_in_config;
use super::files::{PartialFilesConfig, merge_global_file_excludes};
use super::ignore::{PartialIgnoreConfig, merge_ignore};
use super::language_groups::{PartialLanguageGroupsConfig, resolve_language_groups};
use super::languages::{PartialLanguagesConfig, merge_language};
use super::limits::{PartialLimits, merge_limits};
use super::output::{PartialOutputConfig, merge_output};
use super::ratchet::{PartialRatchetConfig, merge_ratchet};
use super::tests::{PartialTestsConfig, merge_tests};
use super::traversal::{PartialTraversalConfig, merge_traversal};
use super::{
    AuthoredConfigPath, Config, ConfigSchemaVersion, ConfigSource, PathOverride, ResolvedConfig,
};

const PRIMARY_CONFIG_FILE: &str = "sprawl-guard.toml";

mod effective_toml;
mod migration;

pub use effective_toml::render_effective_config_toml;
pub use migration::{
    ConfigUpgrade, MigratedAuthoredConfigDocument, PreparedConfigUpgrade, prepare_config_upgrade,
};

/// Options used while loading configuration.
#[derive(Clone, Debug)]
pub struct LoadOptions {
    /// Repository root used for config discovery and relative paths.
    pub root: PathBuf,
    /// Optional explicit config path.
    pub config: Option<PathBuf>,
}

/// Loaded resolved configuration and where it came from.
#[derive(Clone, Debug)]
pub struct LoadedConfig {
    /// Resolved configuration.
    pub config: Config,
    source: ConfigSource,
}

impl LoadedConfig {
    /// Binds the resolved configuration to the source from which it was assembled.
    pub fn resolved_config(&self) -> ResolvedConfig<'_> {
        ResolvedConfig::new(&self.config, self.source.clone())
    }
}

/// Loads the resolved current configuration.
pub fn load_resolved_config(options: &LoadOptions) -> Result<LoadedConfig> {
    let root = resolve_config_root(options)?;
    let source_path = discover_config(&root, options.config.as_deref())?;
    let mut config = built_in_config();

    let source = if let Some(path) = source_path {
        let path = AuthoredConfigPath::new(path)?;
        let partial = read_current_root_config(path.as_path())?;
        merge_root_config(&mut config, partial)?;
        config.authored_file = authored_file_under_root(&root, &path)?;
        ConfigSource::Authored { path }
    } else {
        ConfigSource::BuiltIn
    };

    Ok(LoadedConfig { config, source })
}

/// Returns the authored config's repository-relative path, or `None` when it lives outside `root`.
///
/// Discovery reports paths as written, without resolving symlinks, so a path already under the
/// canonical `root` is kept as is. Any other path, such as an absolute path through a symlinked
/// root or one that leaves the root with `..`, has only its parent directory canonicalized, so the
/// file name still matches a symlinked config file's own name.
fn authored_file_under_root(
    root: &Path,
    path: &AuthoredConfigPath,
) -> Result<Option<RelativePath>> {
    let lexical = path.as_path();
    if let Ok(relative) = lexical.strip_prefix(root)
        && !relative
            .components()
            .any(|component| component == Component::ParentDir)
    {
        return RelativePath::new(relative).map(Some);
    }
    let (Some(parent), Some(file_name)) = (lexical.parent(), lexical.file_name()) else {
        return Ok(None);
    };
    let parent = parent
        .canonicalize()
        .map_err(|source| SprawlError::ReadConfig {
            path: lexical.to_path_buf(),
            source,
        })?;
    parent
        .join(file_name)
        .strip_prefix(root)
        .ok()
        .map(RelativePath::new)
        .transpose()
}

fn resolve_config_root(options: &LoadOptions) -> Result<PathBuf> {
    options
        .root
        .canonicalize()
        .map_err(|source| SprawlError::ResolveRoot {
            path: options.root.clone(),
            source,
        })
}

/// Generates the JSON Schema for user-authored root configuration files.
pub fn authored_config_json_schema() -> Schema {
    let mut schema = authored_toml_schema_generator().into_root_schema_for::<PartialConfig>();
    schema.insert(
        "id".to_owned(),
        ConfigSchemaVersion::CURRENT_SCHEMA_URL.as_str().into(),
    );
    schema
}

/// Finds the root config file, honoring an explicit path before the conventional name.
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
    Ok(primary.exists().then_some(primary))
}

fn read_current_root_config(path: &Path) -> Result<PartialConfig> {
    let contents = read_config_text(path)?;
    let value = parse_config_value(path, &contents)?;
    let version = root_schema_version(&value)?;
    // Every version this build recognizes, other than the current one, is older and upgradable.
    if version != ConfigSchemaVersion::CURRENT {
        let current_dir = std::env::current_dir()
            .and_then(|directory| directory.canonicalize())
            .ok();
        return Err(outdated_config_error(path, version, current_dir.as_deref()));
    }
    deserialize_config(path, value)
}

/// Describes an outdated config file the way the user would name it from `current_dir`.
///
/// The upgrade command finds `sprawl-guard.toml` in the current directory by itself, so it names
/// the file with `--config` only when the file is anywhere else.
fn outdated_config_error(
    path: &Path,
    version: ConfigSchemaVersion,
    current_dir: Option<&Path>,
) -> SprawlError {
    let path = current_dir
        .and_then(|directory| path.strip_prefix(directory).ok())
        .unwrap_or(path)
        .to_path_buf();
    let upgrade = if path == Path::new(PRIMARY_CONFIG_FILE) {
        ConfigUpgradeCommand::InCurrentDirectory
    } else {
        ConfigUpgradeCommand::WithConfig(path.clone())
    };
    SprawlError::OutdatedConfigSchemaVersion {
        path,
        upgrade,
        version,
    }
}

fn read_config_text(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|source| SprawlError::ReadConfig {
        path: path.to_path_buf(),
        source,
    })
}

fn parse_config_value(path: &Path, contents: &str) -> Result<toml::Value> {
    toml::from_str(contents).map_err(|source| SprawlError::ParseToml {
        path: path.to_path_buf(),
        source,
    })
}

fn deserialize_config<T: for<'de> Deserialize<'de>>(path: &Path, value: toml::Value) -> Result<T> {
    value.try_into().map_err(|source| SprawlError::ParseToml {
        path: path.to_path_buf(),
        source,
    })
}

fn root_schema_version(document: &toml::Value) -> Result<ConfigSchemaVersion> {
    let Some(version) = document.get("schema_version") else {
        return Err(SprawlError::MissingConfigSchemaVersion);
    };
    match version.as_integer() {
        Some(version) => ConfigSchemaVersion::try_from(version),
        None => Err(SprawlError::InvalidConfigSchemaVersionType),
    }
}

fn validate_partial_config(partial: PartialConfig) -> Result<()> {
    let mut config = built_in_config();
    merge_root_config(&mut config, partial)
}

/// Applies root config fields over built-in defaults in precedence order.
///
/// This merge is deliberately not generic: each section has different replacement or append
/// semantics, and spelling those out here keeps config precedence auditable.
fn merge_root_config(config: &mut Config, partial: PartialConfig) -> Result<()> {
    let PartialConfig {
        schema_version,
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
                && value.common().extend_include.is_empty()
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
        &config.limits.max_direct_files_per_directory,
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

/// User-authored root configuration before defaults are resolved.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(title = "Sprawl Guard configuration")]
struct PartialConfig {
    schema_version: ConfigSchemaVersion,
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
