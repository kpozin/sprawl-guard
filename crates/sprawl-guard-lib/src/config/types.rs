use std::collections::BTreeMap;

use serde::Serialize;

use crate::LanguageId;

use super::{
    ConfigInclude, FilesConfig, IgnoreConfig, LanguageConfig, LanguageMode, Limits,
    MaxLinesPerFileLimits, OutputConfig, PathOverride, RatchetConfig, TestsConfig, TraversalConfig,
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

    /// Resolves one language's file LOC policy against global defaults.
    pub fn effective_language_file_limits(&self, language_id: LanguageId) -> MaxLinesPerFileLimits {
        let global = &self.limits.max_lines_per_file;
        let language = self
            .languages
            .get(&language_id)
            .expect("resolved config must contain every bundled language");
        let language_limits = &language.common().limits.max_lines_per_file;
        MaxLinesPerFileLimits {
            included_line_types: language_limits.included_line_types.unwrap_or_else(|| {
                language_id.automatic_included_line_types(global.included_line_types)
            }),
            production: language_limits.production.unwrap_or(global.production),
            test: language_limits.test.unwrap_or(global.test),
        }
    }

    /// Returns an inspection copy with inherited per-language LOC policy made explicit.
    pub fn materialized_for_inspection(&self) -> Self {
        let mut materialized = self.clone();
        for (language_id, language) in &mut materialized.languages {
            let effective = self.effective_language_file_limits(*language_id);
            let limits = &mut language.common_mut().limits.max_lines_per_file;
            limits.included_line_types = Some(effective.included_line_types);
            limits.production = Some(effective.production);
            limits.test = Some(effective.test);
        }
        materialized
    }
}
