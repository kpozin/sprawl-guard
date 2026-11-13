use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroUsize;
use std::path::Path;

use serde_json::{Value, json};
use sprawl_guard_lib::classification::{
    ClassifiedSource, bundled_extensions, bundled_filenames, discover_sources,
};
use sprawl_guard_lib::config::{Config, LanguageConfig, RustCfgTestExclusion, built_in_config};
use sprawl_guard_lib::{CountLines, GlobPattern, LanguageGroupId, LanguageId};

use crate::error::{CliError, Result};

const SOURCE_COUNT_INCREMENT: usize = 1;

/// Space between columns of the human language table.
const COLUMN_GAP: &str = "  ";

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
        .filter_map(|(id, language_config)| {
            if selection.includes(*id, &detected_languages) {
                Some(LanguageEntry::new(
                    *id,
                    language_config,
                    config.effective_language_file_limits(*id).count_lines,
                    detected_counts.get(id).copied(),
                ))
            } else {
                None
            }
        })
        .collect()
}

/// Renders one row per language: its name, configuration key, detected file count when
/// detecting, and the extensions and exact filenames it claims.
pub(super) fn human_report(report: &LanguagesReport) -> String {
    let mut rows = vec![report.mode.header_row()];
    rows.extend(report.languages.iter().map(|language| {
        let mut row = vec![language.id.to_string(), language.config_key.clone()];
        if let Some(count) = language.source_files {
            row.push(count.to_string());
        }
        row.push(language.matched_names());
        row
    }));
    let widths = (0..rows[0].len())
        .map(|column| rows.iter().map(|row| row[column].len()).max().unwrap_or(0))
        .collect::<Vec<_>>();
    let mut output = format!("{} languages:\n\n", report.mode.heading());
    for row in rows {
        let line = row
            .iter()
            .zip(&widths)
            .map(|(cell, width)| format!("{cell:<width$}"))
            .collect::<Vec<_>>()
            .join(COLUMN_GAP);
        output.push_str(line.trim_end());
        output.push('\n');
    }
    output.push_str("\nUse --format json for globs, groups, and counted line types.\n");
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

    /// Column titles for the human table; only detection reports file counts.
    fn header_row(&self) -> Vec<String> {
        let titles: &[&str] = match self {
            Self::Supported => &["Language", "Key", "Extensions and filenames"],
            Self::Detected => &["Language", "Key", "Files", "Extensions and filenames"],
        };
        titles.iter().map(|title| (*title).to_owned()).collect()
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
    literate: bool,
    group: LanguageGroupId,
    count_lines: CountLines,
    source_files: Option<NonZeroUsize>,
    include: Option<Vec<String>>,
    bundled_extensions: Vec<String>,
    bundled_filenames: Vec<String>,
    extend_include: Vec<String>,
    exclude: Vec<String>,
    test_globs: Vec<String>,
    rust: Option<RustLanguageEntry>,
}

impl LanguageEntry {
    fn new(
        id: LanguageId,
        config: &LanguageConfig,
        count_lines: CountLines,
        source_files: Option<NonZeroUsize>,
    ) -> Self {
        let common = config.common();
        let config_key = config_key(id);
        let literate = id.is_literate();
        let group = id.automatic_group();
        Self {
            id,
            config_key: config_key.clone(),
            accepted_aliases: vec![config_key],
            case_insensitive: true,
            enabled: config.is_enabled(),
            literate,
            group,
            count_lines,
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
            extend_include: glob_strings(&common.extend_include),
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
            "literate": self.literate,
            "automatic_group": self.group,
            "automatic_count_lines": self.count_lines,
            "include": &self.include,
            "bundled_extensions": &self.bundled_extensions,
            "bundled_filenames": &self.bundled_filenames,
            "extend_include": &self.extend_include,
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

    /// Bundled extensions as `.ext`, followed by exact filenames.
    fn matched_names(&self) -> String {
        let names = self
            .bundled_extensions
            .iter()
            .map(|extension| format!(".{extension}"))
            .chain(self.bundled_filenames.iter().cloned())
            .collect::<Vec<_>>();
        display_list(&names)
    }
}

struct RustLanguageEntry {
    exclude_cfg_test: RustCfgTestExclusion,
}

impl RustLanguageEntry {
    fn json(&self) -> Value {
        json!({
            "exclude_cfg_test": self.exclude_cfg_test.is_enabled(),
        })
    }
}
