use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroUsize;
use std::path::Path;

use indoc::formatdoc;
use serde_json::{Value, json};
use sprawl_guard_lib::classification::{
    ClassifiedSource, bundled_extensions, bundled_filenames, discover_sources,
};
use sprawl_guard_lib::config::{Config, LanguageConfig, RustCfgTestExclusion, built_in_config};
use sprawl_guard_lib::{GlobPattern, LanguageId};

use crate::error::{CliError, Result};

const SOURCE_COUNT_INCREMENT: usize = 1;

pub(super) fn supported_languages_report() -> LanguagesReport {
    let config = built_in_config();
    LanguagesReport {
        mode: LanguageReportMode::Supported,
        languages: language_entries(&config, LanguageSelection::All, BTreeMap::new()),
    }
}

pub(super) fn detected_languages_report(root: &Path) -> Result<LanguagesReport> {
    let config = config_for_init_style_detection();
    let sources = discover_sources(root, &config)?;
    let detected_counts = source_counts_by_language(&sources);
    Ok(LanguagesReport {
        mode: LanguageReportMode::Detected,
        languages: language_entries(&config, LanguageSelection::DetectedOnly, detected_counts),
    })
}

fn config_for_init_style_detection() -> Config {
    built_in_config()
}

fn source_counts_by_language(sources: &[ClassifiedSource]) -> BTreeMap<LanguageId, NonZeroUsize> {
    let mut counts = BTreeMap::new();
    for source in sources {
        counts
            .entry(source.language())
            .and_modify(|count: &mut NonZeroUsize| {
                *count = NonZeroUsize::new(
                    count
                        .get()
                        .checked_add(SOURCE_COUNT_INCREMENT)
                        .expect("source count must not overflow usize"),
                )
                .expect("incremented source count must remain non-zero");
            })
            .or_insert(NonZeroUsize::MIN);
    }
    counts
}

fn language_entries(
    config: &Config,
    selection: LanguageSelection,
    detected_counts: BTreeMap<LanguageId, NonZeroUsize>,
) -> Vec<LanguageEntry> {
    let detected_languages = detected_counts.keys().copied().collect::<BTreeSet<_>>();
    config
        .languages()
        .iter()
        .filter_map(|(id, config)| {
            if selection.includes(*id, &detected_languages) {
                Some(LanguageEntry::new(
                    *id,
                    config,
                    detected_counts.get(id).copied(),
                ))
            } else {
                None
            }
        })
        .collect()
}

pub(super) fn human_report(report: &LanguagesReport) -> String {
    let mut output = format!("{} languages:\n", report.mode.heading());
    for language in &report.languages {
        let count = language
            .source_files
            .map(|count| format!("    source files: {count}\n"))
            .unwrap_or_default();
        output.push_str(&formatdoc! {"
              {key}
                canonical: {canonical}
                accepted aliases: {aliases}
                enabled: {enabled}
            {count}    include: {include}
                bundled extensions: {bundled_extensions}
                bundled filenames: {bundled_filenames}
                additional globs: {additional_globs}
                exclude: {exclude}
                test globs: {test_globs}{rust}
        ",
        key = language.config_key,
        canonical = language.id,
        aliases = language.human_aliases(),
        enabled = language.enabled,
        include = language
            .include
            .as_ref()
            .map(|include| display_list(include))
            .unwrap_or_else(|| "(bundled)".to_owned()),
        bundled_extensions = display_list(&language.bundled_extensions),
        bundled_filenames = display_list(&language.bundled_filenames),
        additional_globs = display_list(&language.additional_globs),
        exclude = display_list(&language.exclude),
        test_globs = display_list(&language.test_globs),
        rust = language
            .rust
            .as_ref()
            .map(RustLanguageEntry::human_suffix)
            .unwrap_or_default(),
        });
    }
    output
}

pub(super) fn json_report(report: &LanguagesReport) -> Result<String> {
    serde_json::to_string_pretty(&json_report_value(report))
        .map_err(|source| CliError::RenderJson { source })
}

pub(super) fn json_report_value(report: &LanguagesReport) -> Value {
    let languages = report
        .languages
        .iter()
        .map(LanguageEntry::json)
        .collect::<Vec<_>>();
    json!({
        "mode": report.mode.as_json_str(),
        "languages": languages,
    })
}

fn display_list(values: &[String]) -> String {
    if values.is_empty() {
        return "(none)".to_owned();
    }
    values.join(", ")
}

fn config_key(language: LanguageId) -> String {
    language.to_string().to_ascii_lowercase()
}

fn glob_strings(globs: &[GlobPattern]) -> Vec<String> {
    globs.iter().map(ToString::to_string).collect()
}

pub(super) struct LanguagesReport {
    mode: LanguageReportMode,
    languages: Vec<LanguageEntry>,
}

enum LanguageSelection {
    All,
    DetectedOnly,
}

impl LanguageSelection {
    fn includes(&self, id: LanguageId, detected_languages: &BTreeSet<LanguageId>) -> bool {
        match self {
            Self::All => true,
            Self::DetectedOnly => detected_languages.contains(&id),
        }
    }
}

enum LanguageReportMode {
    Supported,
    Detected,
}

impl LanguageReportMode {
    fn heading(&self) -> &'static str {
        match self {
            Self::Supported => "Supported",
            Self::Detected => "Detected",
        }
    }

    fn as_json_str(&self) -> &'static str {
        match self {
            Self::Supported => "supported",
            Self::Detected => "detected",
        }
    }
}

struct LanguageEntry {
    id: LanguageId,
    config_key: String,
    accepted_aliases: Vec<String>,
    case_insensitive: bool,
    enabled: bool,
    source_files: Option<NonZeroUsize>,
    include: Option<Vec<String>>,
    bundled_extensions: Vec<String>,
    bundled_filenames: Vec<String>,
    additional_globs: Vec<String>,
    exclude: Vec<String>,
    test_globs: Vec<String>,
    rust: Option<RustLanguageEntry>,
}

impl LanguageEntry {
    fn new(id: LanguageId, config: &LanguageConfig, source_files: Option<NonZeroUsize>) -> Self {
        let common = config.common();
        let config_key = config_key(id);
        Self {
            id,
            config_key: config_key.clone(),
            accepted_aliases: vec![config_key],
            case_insensitive: true,
            enabled: config.is_enabled(),
            source_files,
            include: common.include.as_deref().map(glob_strings),
            bundled_extensions: bundled_extensions(id)
                .into_iter()
                .map(str::to_owned)
                .collect(),
            bundled_filenames: bundled_filenames(id)
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            additional_globs: glob_strings(&common.additional_globs),
            exclude: glob_strings(&common.exclude),
            test_globs: glob_strings(&common.test_globs),
            rust: config.rust().map(|rust| RustLanguageEntry {
                exclude_cfg_test: rust.cfg_test_exclusion,
            }),
        }
    }

    fn json(&self) -> Value {
        let mut value = json!({
            "id": self.id,
            "config_key": &self.config_key,
            "accepted_aliases": &self.accepted_aliases,
            "case_insensitive": self.case_insensitive,
            "enabled": self.enabled,
            "include": &self.include,
            "bundled_extensions": &self.bundled_extensions,
            "bundled_filenames": &self.bundled_filenames,
            "additional_globs": &self.additional_globs,
            "exclude": &self.exclude,
            "test_globs": &self.test_globs,
        });
        if let Some(source_files) = self.source_files {
            value["source_files"] = json!(source_files.get());
        }
        if let Some(rust) = &self.rust {
            value["rust"] = rust.json();
        }
        value
    }

    fn human_aliases(&self) -> String {
        let aliases = display_list(&self.accepted_aliases);
        if self.case_insensitive {
            format!("{aliases} (case-insensitive)")
        } else {
            aliases
        }
    }
}

struct RustLanguageEntry {
    exclude_cfg_test: RustCfgTestExclusion,
}

impl RustLanguageEntry {
    fn human_suffix(&self) -> String {
        format!(
            "\n    rust: exclude_cfg_test = {}",
            self.exclude_cfg_test.is_enabled()
        )
    }

    fn json(&self) -> Value {
        json!({
            "exclude_cfg_test": self.exclude_cfg_test.is_enabled(),
        })
    }
}
