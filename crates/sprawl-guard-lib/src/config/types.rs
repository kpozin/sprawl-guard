use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::{
    CustomLanguageGroupId, LanguageGroupId, LanguageId, RelativePath, Result, SprawlError,
};

use super::{
    ConfigSchemaVersion, FilesConfig, IgnoreConfig, LanguageConfig, LanguageGroupsConfig,
    LanguageMode, Limits, MaxLinesPerFileLimits, OutputConfig, PathOverride, RatchetConfig,
    TestsConfig, TraversalConfig,
};

/// Fully resolved `sprawl-guard` configuration.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Root configuration schema version.
    pub schema_version: ConfigSchemaVersion,
    /// Production source limits.
    pub limits: Limits,
    /// Ignore traversal settings.
    pub ignore: IgnoreConfig,
    /// Filesystem traversal safety settings.
    pub traversal: TraversalConfig,
    /// Global file include and exclude settings.
    pub files: FilesConfig,
    /// Test file settings.
    pub tests: TestsConfig,
    pub(crate) languages: BTreeMap<LanguageId, LanguageConfig>,
    /// Directory direct-file count groups and resolved language assignments.
    pub(crate) language_groups: LanguageGroupsConfig,
    /// Ordered path overrides.
    pub overrides: Vec<PathOverride>,
    /// Ratchet file settings.
    pub ratchet: RatchetConfig,
    /// Output settings.
    pub output: OutputConfig,
    /// Repository-relative path of the authored config file, when it lives under the root.
    ///
    /// Source discovery skips this file so a TOML config is never checked as source.
    #[serde(skip)]
    pub(crate) authored_file: Option<RelativePath>,
}

/// Source from which a check's resolved configuration was assembled.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConfigSource {
    /// built-in configuration was used because no authored config was selected.
    BuiltIn,
    /// An authored root config was merged over built-in configuration.
    Authored {
        /// Selected authored config path.
        path: AuthoredConfigPath,
    },
}

/// Absolute path of a selected authored config.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthoredConfigPath(PathBuf);

impl AuthoredConfigPath {
    pub(crate) fn new(path: PathBuf) -> Result<Self> {
        debug_assert!(path.is_absolute(), "selected config path must be absolute");
        if path.to_str().is_none() {
            return Err(SprawlError::NonUtf8ConfigPath { path });
        }
        Ok(Self(path))
    }

    /// Returns the selected config path.
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    /// Returns the selected config path as an owned path.
    pub fn into_path_buf(self) -> PathBuf {
        self.0
    }
}

impl Serialize for AuthoredConfigPath {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(
            self.0
                .to_str()
                .expect("authored config paths are validated as UTF-8"),
        )
    }
}

/// A resolved configuration and where it came from.
#[derive(Clone, Debug)]
pub struct ResolvedConfig<'config> {
    config: &'config Config,
    source: ConfigSource,
}

impl<'config> ResolvedConfig<'config> {
    /// Couples a resolved configuration with its selected source.
    pub(crate) fn new(config: &'config Config, source: ConfigSource) -> Self {
        Self { config, source }
    }

    /// Creates a configuration backed only by the built-in configuration.
    pub fn built_in(config: &'config Config) -> Self {
        Self::new(config, ConfigSource::BuiltIn)
    }

    /// Returns the resolved configuration.
    pub fn config(&self) -> &'config Config {
        self.config
    }

    /// Returns the source from which the configuration was assembled.
    pub fn source(&self) -> &ConfigSource {
        &self.source
    }
}

impl Config {
    /// Returns per-language configuration keyed by supported language ID.
    ///
    /// The map is read-only to public callers because each value's
    /// language-specific settings are valid only for its key. Config loading
    /// and defaults construction maintain that invariant.
    pub fn languages(&self) -> &BTreeMap<LanguageId, LanguageConfig> {
        &self.languages
    }

    /// Returns resolved language-group configuration without exposing mutable assignment state.
    pub fn language_groups(&self) -> &LanguageGroupsConfig {
        &self.language_groups
    }

    /// Disables the selected supported languages.
    ///
    /// Every `LanguageId` is backed by the bundled registry, and resolved configs contain every
    /// bundled ID, so this mutation cannot receive an unsupported key.
    pub fn disable_languages(&mut self, languages: impl IntoIterator<Item = LanguageId>) {
        for language in languages {
            self.languages
                .get_mut(&language)
                .expect("resolved config must contain every bundled language")
                .common_mut()
                .mode = LanguageMode::Disabled;
        }
    }

    /// Resolves one language's file line-limit configuration against global defaults.
    pub fn effective_language_file_limits(&self, language_id: LanguageId) -> MaxLinesPerFileLimits {
        let global = &self.limits.max_lines_per_file;
        let language = self
            .languages
            .get(&language_id)
            .expect("resolved config must contain every bundled language");
        let language_limits = &language.common().limits.max_lines_per_file;
        MaxLinesPerFileLimits {
            count_lines: language_limits
                .count_lines
                .unwrap_or_else(|| language_id.automatic_count_lines(global.count_lines)),
            production: language_limits.production.unwrap_or(global.production),
            test: language_limits.test.unwrap_or(global.test),
        }
    }

    /// Resolves one language group's directory direct-file configuration against global defaults.
    pub(crate) fn effective_language_group_directory_limits(
        &self,
        group: &LanguageGroupId,
    ) -> &super::MaxDirectFilesPerDirectoryLimits {
        match group {
            LanguageGroupId::Source => &self.limits.max_direct_files_per_directory,
            LanguageGroupId::Documentation => {
                &self
                    .language_groups
                    .documentation
                    .limits
                    .max_direct_files_per_directory
            }
            LanguageGroupId::Custom(group) => {
                &self
                    .custom_language_group(group)
                    .limits
                    .max_direct_files_per_directory
            }
        }
    }

    fn custom_language_group(
        &self,
        group: &CustomLanguageGroupId,
    ) -> &super::CustomLanguageGroupConfig {
        self.language_groups
            .custom
            .get(group)
            .expect("classified custom group must exist in resolved config")
    }

    /// Returns an inspection copy with inherited per-language line-limit configuration made explicit.
    pub fn materialized_for_inspection(&self) -> Self {
        let mut materialized = self.clone();
        for (language_id, language) in &mut materialized.languages {
            let effective = self.effective_language_file_limits(*language_id);
            let limits = &mut language.common_mut().limits.max_lines_per_file;
            limits.count_lines = Some(effective.count_lines);
            limits.production = Some(effective.production);
            limits.test = Some(effective.test);
        }
        materialized
    }
}
