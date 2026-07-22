use std::collections::BTreeMap;

use serde::Serialize;

use crate::{LanguageId, Result, SprawlError};

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

    /// Enables exactly the selected languages and disables every other supported language.
    ///
    /// This is used by bootstrap flows that start from built-in defaults but need a narrow language
    /// set. The method lives on `Config` so callers cannot accidentally pair a language ID with the
    /// wrong language-specific settings.
    pub fn enable_only_languages(
        &mut self,
        languages: impl IntoIterator<Item = LanguageId>,
    ) -> Result<()> {
        let languages = languages.into_iter().collect::<Vec<_>>();
        for language in &languages {
            if !self.languages.contains_key(language) {
                return Err(SprawlError::UnknownLanguage {
                    language: language.to_string(),
                });
            }
        }
        for config in self.languages.values_mut() {
            config.common_mut().mode = LanguageMode::Disabled;
        }
        for language in languages {
            self.languages
                .get_mut(&language)
                .expect("language set was validated before mutation")
                .common_mut()
                .mode = LanguageMode::Enabled;
        }
        Ok(())
    }
}
