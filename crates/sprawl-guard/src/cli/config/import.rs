use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sprawl_guard_lib::config::{
    ConfigSchemaVersion, LanguageLimits, LanguageMaxLinesPerFileLimits,
};
use sprawl_guard_lib::{CountLines, LanguageId, LineLimit, RelativePath};

use super::super::init::{create_config, resolve_cli_root, resolve_target_config_path};
use super::super::linter::{self, ImportError, Invocation, Linter};
use super::super::{ConfigImportArgs, ConfigImportSource};
use crate::error::Result;

const LINTER_DEFAULT_MAX_LINES: u32 = 300;

/// Translates a resolved rule to authored language tables and optionally creates a config.
pub(in crate::cli) fn execute_config_import(
    default_root: PathBuf,
    args: ConfigImportArgs,
) -> Result<String> {
    let options = args.project.load_options(default_root);
    let root = resolve_cli_root(options.root)?;
    let report: PrintedConfig = match &args.source {
        ConfigImportSource::Eslint { sample } => {
            let sample = RelativePath::new(sample)?.to_path_buf();
            linter::run(&root, Invocation::EslintConfig(&root.join(sample)))?
        }
        ConfigImportSource::Oxlint => linter::run(&root, Invocation::OxlintConfig)?,
    };
    let policy = LinePolicy::decode(report.rules.max_lines, report_linter(&args.source))?;
    let imported = ImportedConfig::from(policy);
    let toml = super::render_authored_toml(&imported)?;
    if args.write {
        let target = resolve_target_config_path(&root, options.config.as_deref());
        create_config(&target, &toml)?;
    }
    Ok(toml)
}

fn report_linter(source: &ConfigImportSource) -> Linter {
    match source {
        ConfigImportSource::Eslint { .. } => Linter::Eslint,
        ConfigImportSource::Oxlint => Linter::Oxlint,
    }
}

#[derive(Serialize)]
struct ImportedConfig {
    schema_version: ConfigSchemaVersion,
    languages: BTreeMap<LanguageId, ImportedLanguage>,
}

#[derive(Serialize)]
struct ImportedLanguage {
    limits: LanguageLimits,
}

impl From<LinePolicy> for ImportedConfig {
    fn from(policy: LinePolicy) -> Self {
        let languages = linter::js_languages()
            .into_iter()
            .map(|language| {
                let limits = LanguageLimits {
                    max_lines_per_file: LanguageMaxLinesPerFileLimits {
                        count_lines: Some(policy.count_lines),
                        production: Some(policy.limit),
                        test: Some(policy.limit),
                    },
                };
                (language, ImportedLanguage { limits })
            })
            .collect();
        Self {
            schema_version: ConfigSchemaVersion::CURRENT,
            languages,
        }
    }
}

/// An enabled rule expressed in Sprawl Guard's units.
struct LinePolicy {
    limit: LineLimit,
    count_lines: CountLines,
}

impl LinePolicy {
    fn decode(rule: Option<Rule>, linter: Linter) -> std::result::Result<Self, ImportError> {
        let inactive = || ImportError::NoActiveLimit { linter };
        let options = rule
            .ok_or_else(inactive)?
            .enabled_options()?
            .ok_or_else(inactive)?;
        let limit = LineLimit::new(options.max)?;
        let count_lines = match (options.skip_comments, options.skip_blank_lines) {
            (false, false) => CountLines::All,
            (true, false) => CountLines::CodeAndBlankLines,
            (false, true) => CountLines::CodeAndComments,
            (true, true) => CountLines::Code,
        };
        Ok(Self { limit, count_lines })
    }
}

#[derive(Deserialize)]
struct PrintedConfig {
    #[serde(default)]
    rules: PrintedRules,
}

#[derive(Default, Deserialize)]
struct PrintedRules {
    #[serde(rename = "max-lines", alias = "eslint/max-lines")]
    max_lines: Option<Rule>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Rule {
    Configured(Severity, PrintedOptions),
    Single((Severity,)),
    Bare(Severity),
}

impl Rule {
    fn enabled_options(self) -> std::result::Result<Option<LineOptions>, ImportError> {
        let (severity, options) = match self {
            Self::Configured(severity, options) => (severity, options.into()),
            Self::Single((severity,)) | Self::Bare(severity) => (severity, LineOptions::default()),
        };
        match severity {
            Severity::Number(0) | Severity::Name(SeverityName::Off) => Ok(None),
            Severity::Number(1 | 2) | Severity::Name(SeverityName::Enabled) => Ok(Some(options)),
            Severity::Number(value) => Err(ImportError::InvalidSeverity { value }),
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Severity {
    Number(u8),
    Name(SeverityName),
}

#[derive(Deserialize)]
enum SeverityName {
    #[serde(rename = "off", alias = "allow")]
    Off,
    #[serde(rename = "warn", alias = "error", alias = "deny")]
    Enabled,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RuleOptions {
    Limit(u32),
    Detailed(LineOptions),
}

/// Oxlint's printed config wraps rule options in an additional array.
#[derive(Deserialize)]
#[serde(untagged)]
enum PrintedOptions {
    Direct(RuleOptions),
    Wrapped([RuleOptions; 1]),
    Empty([(); 0]),
}

impl From<PrintedOptions> for LineOptions {
    fn from(options: PrintedOptions) -> Self {
        match options {
            PrintedOptions::Direct(RuleOptions::Limit(max))
            | PrintedOptions::Wrapped([RuleOptions::Limit(max)]) => Self {
                max,
                ..Self::default()
            },
            PrintedOptions::Direct(RuleOptions::Detailed(options))
            | PrintedOptions::Wrapped([RuleOptions::Detailed(options)]) => options,
            PrintedOptions::Empty([]) => Self::default(),
        }
    }
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
struct LineOptions {
    max: u32,
    skip_blank_lines: bool,
    skip_comments: bool,
}

impl Default for LineOptions {
    fn default() -> Self {
        Self {
            max: LINTER_DEFAULT_MAX_LINES,
            skip_blank_lines: false,
            skip_comments: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sprawl_guard_lib::config::{LoadOptions, built_in_config, load_resolved_config};

    mod when_importing_a_printed_rule {
        use super::*;

        #[test]
        fn it_loads_as_language_limits_without_changing_rust() {
            let examples = [
                (
                    Linter::Eslint,
                    r#"{"rules":{"max-lines":["error",{"max":17,"skipComments":true}]}}"#,
                    17,
                    CountLines::CodeAndBlankLines,
                ),
                (
                    Linter::Eslint,
                    r#"{"rules":{"max-lines":[2,18]}}"#,
                    18,
                    CountLines::All,
                ),
                (
                    Linter::Oxlint,
                    r#"{"rules":{"max-lines":["deny",[{"max":19,"skipBlankLines":true}]]}}"#,
                    19,
                    CountLines::CodeAndComments,
                ),
                (
                    Linter::Oxlint,
                    r#"{"rules":{"eslint/max-lines":["warn",[]]}}"#,
                    LINTER_DEFAULT_MAX_LINES,
                    CountLines::All,
                ),
            ];
            for (linter, json, expected_max, expected_count) in examples {
                let root = tempfile::tempdir().unwrap();
                let printed: PrintedConfig = serde_json::from_str(json).unwrap();
                let config = ImportedConfig::from(
                    LinePolicy::decode(printed.rules.max_lines, linter).unwrap(),
                );
                let toml = super::super::super::render_authored_toml(&config).unwrap();
                std::fs::write(root.path().join("sprawl-guard.toml"), toml).unwrap();
                let actual = load_resolved_config(&LoadOptions {
                    root: root.path().to_path_buf(),
                    config: None,
                })
                .unwrap()
                .config;
                for language in linter::js_languages() {
                    let limits = actual.effective_language_file_limits(language);
                    assert_eq!(limits.count_lines, expected_count);
                    assert_eq!(limits.production, LineLimit::new(expected_max).unwrap());
                    assert_eq!(limits.test, limits.production);
                }
                let rust = LanguageId::new("rust").unwrap();
                assert_eq!(
                    actual.effective_language_file_limits(rust),
                    built_in_config().effective_language_file_limits(rust)
                );
            }
        }

        #[test]
        fn it_rejects_absent_and_disabled_rules() {
            for json in [r#"{"rules":{}}"#, r#"{"rules":{"max-lines":[0]}}"#] {
                let printed: PrintedConfig = serde_json::from_str(json).unwrap();
                assert!(matches!(
                    LinePolicy::decode(printed.rules.max_lines, Linter::Eslint),
                    Err(ImportError::NoActiveLimit { .. })
                ));
            }
        }

        #[test]
        fn it_rejects_an_unknown_numeric_severity() {
            let printed: PrintedConfig =
                serde_json::from_str(r#"{"rules":{"max-lines":[3,18]}}"#).unwrap();
            assert!(matches!(
                LinePolicy::decode(printed.rules.max_lines, Linter::Eslint),
                Err(ImportError::InvalidSeverity { value: 3 })
            ));
        }
    }
}
