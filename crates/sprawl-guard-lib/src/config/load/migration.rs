use std::num::NonZeroU32;
use std::path::Path;

use serde::{Deserialize, Deserializer};
use toml_edit::DocumentMut;

use super::{
    AuthoredConfigPath, ConfigSchemaVersion, LoadOptions, OutputFormat, PartialConfig,
    PartialFilesConfig, PartialIgnoreConfig, PartialLanguageGroupsConfig, PartialLanguagesConfig,
    PartialLimits, PartialOutputConfig, PartialRatchetConfig, PartialTestsConfig,
    PartialTraversalConfig, Result, SourceFileByteLimit, SprawlError, SymlinkTraversal,
    TraversalEntryLimit, TraversalErrorPolicy, deserialize_config, discover_config,
    parse_config_value, read_config_text, resolve_config_root, root_schema_version,
    validate_partial_config,
};

/// Prepared upgrade for one authored root configuration.
pub struct PreparedConfigUpgrade {
    path: AuthoredConfigPath,
    upgrade: ConfigUpgrade,
}

impl PreparedConfigUpgrade {
    /// Separates the authored path from its required upgrade state.
    pub fn into_parts(self) -> (AuthoredConfigPath, ConfigUpgrade) {
        (self.path, self.upgrade)
    }
}

/// Whether an authored root configuration is current or requires replacement.
pub enum ConfigUpgrade {
    /// The authored config already uses the current structural schema and directive.
    Current,
    /// The authored config must be replaced with this validated migrated document.
    Required(MigratedAuthoredConfigDocument),
}

/// Formatting-preserving authored config migrated to the current schema.
pub struct MigratedAuthoredConfigDocument(String);

impl std::fmt::Display for MigratedAuthoredConfigDocument {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Loads and validates an authored config using its declared schema before preparing an upgrade.
///
/// Unlike normal config loading, this entry point accepts supported legacy schema versions and
/// retains authored formatting only for documents that need to be rewritten.
pub fn prepare_config_upgrade(options: &LoadOptions) -> Result<Option<PreparedConfigUpgrade>> {
    let root = resolve_config_root(options)?;
    let Some(path) = discover_config(&root, options.config.as_deref())? else {
        return Ok(None);
    };
    let path = AuthoredConfigPath::new(path)?;
    let contents = read_config_text(path.as_path())?;
    let value = parse_config_value(path.as_path(), &contents)?;
    let version = root_schema_version(&value)?;

    let upgrade = match version {
        ConfigSchemaVersion::V1 => {
            let legacy = deserialize_config::<PartialConfigV1>(path.as_path(), value)?;
            validate_partial_config(legacy.into_v2())?;
            let migrated = migrate_v1_document(path.as_path(), &contents)?;
            validate_migrated_current_config(path.as_path(), &migrated)?;
            ConfigUpgrade::Required(migrated)
        }
        ConfigSchemaVersion::V2 => {
            let partial = deserialize_config::<PartialConfig>(path.as_path(), value)?;
            validate_partial_config(partial)?;
            if has_current_schema_directive(&contents) {
                ConfigUpgrade::Current
            } else {
                let migrated = repair_schema_directive(path.as_path(), &contents)?;
                validate_migrated_current_config(path.as_path(), &migrated)?;
                ConfigUpgrade::Required(migrated)
            }
        }
    };

    Ok(Some(PreparedConfigUpgrade { path, upgrade }))
}

fn validate_migrated_current_config(
    path: &Path,
    migrated: &MigratedAuthoredConfigDocument,
) -> Result<()> {
    let value = parse_config_value(path, &migrated.0)?;
    let version = root_schema_version(&value)?;
    if version != ConfigSchemaVersion::CURRENT {
        return Err(SprawlError::UnsupportedConfigSchemaVersion {
            version: version.into(),
        });
    }
    validate_partial_config(deserialize_config(path, value)?)
}

fn migrate_v1_document(path: &Path, contents: &str) -> Result<MigratedAuthoredConfigDocument> {
    rewrite_authored_config(path, contents, |document| {
        if let Some(output) = document
            .get_mut("output")
            .and_then(toml_edit::Item::as_table_like_mut)
        {
            output.remove("color");
        }
        if let Some(traversal) = document
            .get_mut("traversal")
            .and_then(toml_edit::Item::as_table_like_mut)
        {
            traversal.remove("max_depth");
        }
    })
}

fn repair_schema_directive(path: &Path, contents: &str) -> Result<MigratedAuthoredConfigDocument> {
    rewrite_authored_config(path, contents, |_| {})
}

fn rewrite_authored_config(
    path: &Path,
    contents: &str,
    mutate: impl FnOnce(&mut DocumentMut),
) -> Result<MigratedAuthoredConfigDocument> {
    let line_ending = ConfigLineEnding::from_source(contents);
    let first_line_end = contents
        .find('\n')
        .map_or(contents.len(), |index| index + 1);
    let first_line = contents[..first_line_end].trim_end_matches(['\r', '\n']);
    let source_without_directive = if is_schema_directive(first_line) {
        &contents[first_line_end..]
    } else {
        contents
    };
    let mut document = source_without_directive
        .parse::<DocumentMut>()
        .map_err(|source| SprawlError::ParseEditableToml {
            path: path.to_path_buf(),
            source,
        })?;
    mutate(&mut document);
    document["schema_version"] = toml_edit::value(i64::from(ConfigSchemaVersion::CURRENT));
    document.as_table_mut().decor_mut().set_prefix(format!(
        "{}\n",
        ConfigSchemaVersion::CURRENT.schema_directive()
    ));
    let rendered = document.to_string();
    let rendered = match line_ending {
        ConfigLineEnding::Lf => rendered,
        ConfigLineEnding::CrLf => rendered.replace('\n', "\r\n"),
    };
    Ok(MigratedAuthoredConfigDocument(rendered))
}

fn has_current_schema_directive(contents: &str) -> bool {
    let first_line_end = contents
        .find('\n')
        .map_or(contents.len(), |index| index + 1);
    contents[..first_line_end].trim_end_matches(['\r', '\n'])
        == ConfigSchemaVersion::CURRENT.schema_directive().to_string()
}

fn is_schema_directive(line: &str) -> bool {
    line.strip_prefix("#:schema")
        .is_some_and(|suffix| suffix.is_empty() || suffix.starts_with(char::is_whitespace))
}

#[derive(Clone, Copy)]
enum ConfigLineEnding {
    Lf,
    CrLf,
}

impl ConfigLineEnding {
    fn from_source(source: &str) -> Self {
        source.find('\n').map_or(Self::Lf, |index| {
            if source[..index].ends_with('\r') {
                Self::CrLf
            } else {
                Self::Lf
            }
        })
    }
}

/// Version 1 authored shape retained for V1 → V2 migration.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PartialConfigV1 {
    #[serde(rename = "schema_version")]
    _schema_version: ConfigSchemaVersion,
    limits: Option<PartialLimits>,
    ignore: Option<PartialIgnoreConfig>,
    traversal: Option<PartialTraversalConfigV1>,
    files: Option<PartialFilesConfig>,
    tests: Option<PartialTestsConfig>,
    languages: Option<PartialLanguagesConfig>,
    language_groups: Option<PartialLanguageGroupsConfig>,
    overrides: Option<Vec<super::PathOverride>>,
    ratchet: Option<PartialRatchetConfig>,
    output: Option<PartialOutputConfigV1>,
}

impl PartialConfigV1 {
    fn into_v2(self) -> PartialConfig {
        let Self {
            _schema_version: _,
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
        } = self;
        PartialConfig {
            schema_version: ConfigSchemaVersion::V2,
            limits,
            ignore,
            traversal: traversal.map(PartialTraversalConfigV1::into_v2),
            files,
            tests,
            languages,
            language_groups,
            overrides,
            ratchet,
            output: output.map(PartialOutputConfigV1::into_v2),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PartialTraversalConfigV1 {
    follow_symlinks: Option<SymlinkTraversal>,
    same_file_system: Option<bool>,
    #[serde(rename = "max_depth")]
    _max_depth: Option<LegacyTraversalDepthV1>,
    max_entries: Option<TraversalEntryLimit>,
    max_file_bytes: Option<SourceFileByteLimit>,
    error_policy: Option<TraversalErrorPolicy>,
}

impl PartialTraversalConfigV1 {
    fn into_v2(self) -> PartialTraversalConfig {
        let Self {
            follow_symlinks,
            same_file_system,
            _max_depth: _,
            max_entries,
            max_file_bytes,
            error_policy,
        } = self;
        PartialTraversalConfig {
            follow_symlinks,
            same_file_system,
            max_entries,
            max_file_bytes,
            error_policy,
        }
    }
}

#[derive(Debug)]
struct LegacyTraversalDepthV1;

impl<'de> Deserialize<'de> for LegacyTraversalDepthV1 {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u32::deserialize(deserializer)?;
        NonZeroU32::new(value).map(|_| Self).ok_or_else(|| {
            serde::de::Error::custom("traversal max_depth must be greater than zero")
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PartialOutputConfigV1 {
    format: Option<OutputFormat>,
    #[serde(rename = "color")]
    _color: Option<LegacyColorModeV1>,
}

impl PartialOutputConfigV1 {
    fn into_v2(self) -> PartialOutputConfig {
        let Self { format, _color: _ } = self;
        PartialOutputConfig { format }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum LegacyColorModeV1 {
    Auto,
    Always,
    Never,
}
