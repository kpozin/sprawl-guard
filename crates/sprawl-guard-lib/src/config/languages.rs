use std::collections::BTreeMap;

use schemars::JsonSchema;
use schemars::{Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};
use tokei::LanguageType;

use crate::{GlobPattern, IncludedLineTypes, LanguageId, LocLimit, Result, SprawlError};

/// Per-language configuration.
///
/// The TOML representation is flat for ergonomics, but the resolved model separates fields that
/// apply to every language from settings that are valid only for the language selected by the
/// surrounding `[languages.<id>]` key.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LanguageConfig {
    #[serde(flatten)]
    common: CommonLanguageConfig,
    #[serde(default, flatten)]
    specific: LanguageSpecificConfig,
}

impl LanguageConfig {
    /// Returns shared settings.
    pub fn common(&self) -> &CommonLanguageConfig {
        &self.common
    }

    /// Returns mutable shared settings.
    pub(super) fn common_mut(&mut self) -> &mut CommonLanguageConfig {
        &mut self.common
    }

    /// Returns true when the language should be checked.
    pub fn is_enabled(&self) -> bool {
        self.common().mode.is_enabled()
    }

    /// Returns Rust-specific settings when this is Rust.
    pub fn rust(&self) -> Option<&RustLanguageConfig> {
        match &self.specific {
            LanguageSpecificConfig::Generic(_) => None,
            LanguageSpecificConfig::Rust(rust) => Some(rust),
        }
    }

    /// Replaces this language's Rust-specific settings.
    pub(super) fn set_rust(&mut self, rust: RustLanguageConfig) {
        self.specific = LanguageSpecificConfig::Rust(rust);
    }

    /// Creates generic language config.
    pub(crate) fn generic(common: CommonLanguageConfig) -> Self {
        Self {
            common,
            specific: LanguageSpecificConfig::Generic(GenericLanguageConfig),
        }
    }

    /// Creates Rust language config.
    pub(crate) fn rust_config(common: CommonLanguageConfig, rust: RustLanguageConfig) -> Self {
        Self {
            common,
            specific: LanguageSpecificConfig::Rust(rust),
        }
    }
}

impl Default for LanguageConfig {
    fn default() -> Self {
        Self::generic(CommonLanguageConfig {
            mode: LanguageMode::Enabled,
            include: None,
            additional_globs: vec![],
            exclude: vec![],
            test_globs: vec![],
            limits: LanguageLimits::default(),
        })
    }
}

/// Language-specific config selected by the surrounding language ID.
///
/// This enum is not internally tagged in TOML. The external `LanguageId` map key determines which
/// variant is valid, and `merge_language` rejects Rust-only fields for non-Rust keys.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum LanguageSpecificConfig {
    /// Rust-only settings.
    Rust(RustLanguageConfig),
    /// No language-specific settings.
    Generic(GenericLanguageConfig),
}

impl Default for LanguageSpecificConfig {
    fn default() -> Self {
        Self::Generic(GenericLanguageConfig)
    }
}

/// Empty marker for languages without language-specific settings.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GenericLanguageConfig;

/// Partial per-language configuration.
///
/// Root config files may specify only the fields they want to override. Serde flattens the common
/// fields so users can write `[languages.rust] enabled = true` without nested common/specific
/// tables.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct PartialLanguageConfig {
    /// Optional shared settings.
    #[serde(flatten)]
    common: PartialCommonLanguageConfig,
    /// Optional Rust-only setting.
    #[serde(rename = "exclude_cfg_test")]
    cfg_test_exclusion: Option<RustCfgTestExclusion>,
}

/// User-authored language settings keyed by `tokei` language ID.
///
/// The map needs a custom schema because the enclosing key determines whether Rust-only fields are
/// valid. Deserialization still uses the same partial config structs as runtime loading.
#[derive(Debug, Deserialize)]
#[serde(transparent)]
pub(super) struct PartialLanguagesConfig(BTreeMap<LanguageId, PartialLanguageConfig>);

impl PartialLanguagesConfig {
    /// Consumes the authored language entries in deterministic key order.
    pub(super) fn into_entries(self) -> impl Iterator<Item = (LanguageId, PartialLanguageConfig)> {
        self.0.into_iter()
    }
}

impl JsonSchema for PartialLanguagesConfig {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Language settings".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let mut schema =
            <BTreeMap<LanguageId, PartialCommonLanguageConfig>>::json_schema(generator);
        let rust_schema = generator.subschema_for::<PartialLanguageConfig>();
        schema
            .get_mut("properties")
            .and_then(|properties| properties.as_object_mut())
            .expect("language map schema must define completion properties")
            .insert("rust".to_owned(), rust_schema.clone().to_value());
        schema.insert(
            "patternProperties".to_owned(),
            json_schema!({
                "^[Rr][Uu][Ss][Tt]$": rust_schema,
            })
            .to_value(),
        );
        schema
    }
}

impl PartialLanguageConfig {
    /// Returns common partial settings independent of language kind.
    fn common(&self) -> &PartialCommonLanguageConfig {
        &self.common
    }

    /// Materializes Rust-specific partial settings when the flat Rust-only field is present.
    fn rust(&self) -> Option<RustLanguageConfig> {
        self.cfg_test_exclusion
            .map(|cfg_test_exclusion| RustLanguageConfig { cfg_test_exclusion })
    }
}

/// Settings shared by every supported language.
///
/// These fields are valid for every `tokei` language and are merged regardless of which language ID
/// owns the enclosing config entry.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommonLanguageConfig {
    /// Whether this language is checked.
    #[serde(rename = "enabled")]
    pub mode: LanguageMode,
    /// Authored glob patterns that replace this language's bundled paths when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include: Option<Vec<GlobPattern>>,
    /// Authored glob patterns added to the selected bundled or explicit paths.
    pub additional_globs: Vec<GlobPattern>,
    /// Glob patterns excluded for this language.
    pub exclude: Vec<GlobPattern>,
    /// Language-specific glob patterns that classify this language's source files as test code.
    ///
    /// Matching files are checked with test-code limits when global test-code
    /// checking is enabled, and omitted when it is disabled.
    pub test_globs: Vec<GlobPattern>,
    /// Optional per-language limits.
    pub limits: LanguageLimits,
}

/// Rust-only language settings.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RustLanguageConfig {
    /// Whether top-level `cfg(test)` items are excluded from production LOC.
    #[serde(rename = "exclude_cfg_test")]
    pub cfg_test_exclusion: RustCfgTestExclusion,
}

/// Whether a language is enabled.
///
/// TOML still represents this as a boolean, but the resolved config uses an enum so callers cannot
/// accidentally pass an unrelated bool where an enablement state is expected.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(from = "bool", into = "bool")]
pub enum LanguageMode {
    /// Include this language in checks.
    Enabled,
    /// Ignore this language during checks.
    Disabled,
}

impl From<bool> for LanguageMode {
    fn from(value: bool) -> Self {
        Self::from_enabled(value)
    }
}

impl From<LanguageMode> for bool {
    fn from(value: LanguageMode) -> Self {
        value.is_enabled()
    }
}

impl LanguageMode {
    /// Creates the mode represented by a config boolean.
    pub fn from_enabled(value: bool) -> Self {
        if value { Self::Enabled } else { Self::Disabled }
    }

    /// Returns true when this mode enables checking.
    pub fn is_enabled(self) -> bool {
        matches!(self, Self::Enabled)
    }
}

/// Whether Rust `cfg(test)` exclusion is enabled.
///
/// This is Rust-specific policy, not a generic language option. It is converted to and from a flat
/// TOML boolean only at the serialization boundary.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(from = "bool", into = "bool")]
pub enum RustCfgTestExclusion {
    /// Exclude top-level Rust `cfg(test)` items.
    Enabled,
    /// Count Rust `cfg(test)` items as production LOC.
    Disabled,
}

impl From<bool> for RustCfgTestExclusion {
    fn from(value: bool) -> Self {
        Self::from_enabled(value)
    }
}

impl From<RustCfgTestExclusion> for bool {
    fn from(value: RustCfgTestExclusion) -> Self {
        value.is_enabled()
    }
}

impl RustCfgTestExclusion {
    /// Creates the setting represented by a config boolean.
    pub fn from_enabled(value: bool) -> Self {
        if value { Self::Enabled } else { Self::Disabled }
    }

    /// Returns true when `cfg(test)` items should be excluded.
    pub fn is_enabled(self) -> bool {
        matches!(self, Self::Enabled)
    }
}

/// Optional per-language limits.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LanguageLimits {
    /// Optional per-language line-count limits.
    #[serde(skip_serializing_if = "LanguageMaxLinesPerFileLimits::is_empty")]
    pub max_lines_per_file: LanguageMaxLinesPerFileLimits,
}

/// Optional line-count limits for one language.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LanguageMaxLinesPerFileLimits {
    /// Optional replacement for inherited included line types.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub included_line_types: Option<IncludedLineTypes>,
    /// Optional production configured-LOC limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub production: Option<LocLimit>,
    /// Optional test configured-LOC limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test: Option<LocLimit>,
}

impl LanguageMaxLinesPerFileLimits {
    fn is_empty(&self) -> bool {
        self.included_line_types.is_none() && self.production.is_none() && self.test.is_none()
    }
}

/// Applies a partial language override to a resolved language config.
///
/// The `id` parameter is the external tag from `[languages.<id>]`. It is required because the
/// flattened TOML shape alone cannot tell whether `exclude_cfg_test` is legal; only the map key can.
pub(super) fn merge_language(
    id: LanguageId,
    config: &mut LanguageConfig,
    partial: PartialLanguageConfig,
) -> Result<()> {
    let common = partial.common();
    if let Some(value) = common.mode {
        config.common_mut().mode = value;
    }
    if let Some(value) = common.include.clone() {
        config.common_mut().include = Some(value);
    }
    if let Some(value) = common.additional_globs.clone() {
        config.common_mut().additional_globs = value;
    }
    if let Some(value) = common.exclude.clone() {
        config.common_mut().exclude = value;
    }
    if let Some(value) = common.test_globs.clone() {
        config.common_mut().test_globs = value;
    }
    if let Some(limits) = common
        .limits
        .clone()
        .and_then(|limits| limits.max_lines_per_file)
    {
        let resolved = &mut config.common_mut().limits.max_lines_per_file;
        if let Some(value) = limits.included_line_types {
            resolved.included_line_types = Some(value);
        }
        if let Some(value) = limits.production {
            resolved.production = Some(value);
        }
        if let Some(value) = limits.test {
            resolved.test = Some(value);
        }
    }
    if let Some(rust) = partial.rust() {
        if id.as_tokei() != LanguageType::Rust {
            return Err(SprawlError::LanguageSpecificSetting {
                language: id.to_string(),
                setting: "exclude_cfg_test",
            });
        }
        config.set_rust(rust);
    }
    Ok(())
}

/// Optional shared language settings from a user-authored config table.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PartialCommonLanguageConfig {
    #[serde(rename = "enabled")]
    mode: Option<LanguageMode>,
    include: Option<Vec<GlobPattern>>,
    additional_globs: Option<Vec<GlobPattern>>,
    exclude: Option<Vec<GlobPattern>>,
    test_globs: Option<Vec<GlobPattern>>,
    limits: Option<PartialLanguageLimits>,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PartialLanguageLimits {
    max_lines_per_file: Option<PartialLanguageMaxLinesPerFileLimits>,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PartialLanguageMaxLinesPerFileLimits {
    included_line_types: Option<IncludedLineTypes>,
    production: Option<LocLimit>,
    test: Option<LocLimit>,
}
