use std::collections::BTreeMap;

use serde::Serialize;

use crate::LanguageId;

use super::{
    ConfigInclude, FilesConfig, IgnoreConfig, LanguageConfig, LanguageMode, Limits, OutputConfig,
    PathOverride, RatchetConfig, TestsConfig, TraversalConfig,
};

/// Fully resolved `sprawl-guard` configuration.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Config include settings.
    pub config: ConfigInclude,
    /// Production source limits.
    pub limits: Limits,
    /// Ignore traversal settings.
    pub ignore: IgnoreConfig,
    /// Filesystem traversal safety settings.
    pub traversal: TraversalConfig,
    /// Global file include and exclude settings.
    pub files: FilesConfig,
    /// Path-classified test settings.
    pub tests: TestsConfig,
    pub(crate) languages: BTreeMap<LanguageId, LanguageConfig>,
    /// Ordered path overrides.
    pub overrides: Vec<PathOverride>,
    /// Ratchet file settings.
    pub ratchet: RatchetConfig,
    /// Output settings.
    pub output: OutputConfig,
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
}
