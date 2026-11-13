use sprawl_guard_lib::classification::{PathClassification, SourceExplanation, SourceKind};
use sprawl_guard_lib::{MatchedOverride, RelativePath, RustCfgTestStatus};

use super::check_output::plural;
use super::display_relative;

/// Renders the human `explain` output for one classified path.
pub(super) fn explain_message(classification: &PathClassification) -> String {
    let shown = |path: &RelativePath| display_relative(path.as_str());
    match classification {
        PathClassification::Ignored { path } => {
            format!("{}: ignored by standard ignore handling", shown(path))
        }
        PathClassification::GloballyExcluded { path } => {
            format!("{}: excluded by the global file excludes", shown(path))
        }
        PathClassification::RatchetExcluded { path } => {
            format!("{}: excluded because it is the ratchet file", shown(path))
        }
        PathClassification::ConfigExcluded { path } => {
            format!(
                "{}: excluded because it is the active configuration file",
                shown(path)
            )
        }
        PathClassification::NonFile { path } => {
            format!("{}: not a regular file", shown(path))
        }
        PathClassification::NotSource { path } => {
            format!(
                "{}: not a source file for any enabled language",
                shown(path)
            )
        }
        PathClassification::AmbiguousLanguage { path, languages } => {
            let languages = languages
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}: ambiguous language match ({languages})", shown(path))
        }
        PathClassification::LanguageExcluded { path, language } => {
            format!(
                "{}: excluded by the {language} language settings",
                shown(path)
            )
        }
        PathClassification::LanguagesExcluded { path, languages } => {
            let languages = languages
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "{}: excluded by the settings of every matching language ({languages})",
                shown(path)
            )
        }
        PathClassification::TestExcluded { path, language } => {
            format!(
                "{}: excluded because test files are not checked ({language})",
                shown(path)
            )
        }
        PathClassification::Source(source) => source_message(source),
    }
}

/// Renders a source path as a heading followed by one indented, labeled line per fact.
fn source_message(explanation: &SourceExplanation) -> String {
    let source = explanation.source();
    let kind = match source.kind {
        SourceKind::Production => "production source",
        SourceKind::Test => "test source",
    };
    let mut lines = vec![
        display_relative(source.path.as_str()),
        format!("  kind: {kind}"),
        format!("  language: {}", source.language()),
        format!("  group: {}", source.group()),
    ];
    match explanation {
        SourceExplanation::NotFound { .. } => {
            lines.push("  state: not found, so classified by its path only".to_owned());
        }
        SourceExplanation::Measured {
            resolved_limits,
            measurement,
            rust_cfg_test,
            ..
        } => {
            let counts = measurement.line_counts();
            lines.push(format!(
                "  lines: {} (limit {}, counting {})",
                measurement.actual().get(),
                resolved_limits.file.value,
                resolved_limits.file.count_lines
            ));
            lines.push(format!(
                "  line counts: {} code, {} comments, {} blank",
                counts.code, counts.comments, counts.blank_lines
            ));
            lines.push(format!(
                "  direct files limit for its directory: {}",
                resolved_limits.directory.value
            ));
            push_overrides(
                &mut lines,
                "line limit overrides",
                &resolved_limits.file.matched_overrides,
            );
            push_overrides(
                &mut lines,
                "direct files overrides",
                &resolved_limits.directory.matched_overrides,
            );
            if let Some(rust_cfg_test) = rust_cfg_test {
                let excluded = rust_cfg_test.excluded_items.get();
                lines.push(format!(
                    "  Rust cfg(test): {}, {excluded} top-level {} not counted",
                    rust_cfg_test_status_message(rust_cfg_test.status),
                    plural(excluded, "item", "items")
                ));
            }
        }
    }
    lines.join("\n")
}

/// Adds a labeled line listing matched overrides, when there are any.
fn push_overrides(lines: &mut Vec<String>, label: &str, overrides: &[MatchedOverride]) {
    if !overrides.is_empty() {
        lines.push(format!(
            "  {label}: {}",
            matching_overrides_message(overrides)
        ));
    }
}

fn matching_overrides_message(overrides: &[MatchedOverride]) -> String {
    overrides
        .iter()
        .map(|override_entry| {
            format!(
                "#{} paths=[{}]",
                override_entry.ordinal.get(),
                override_entry.paths
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn rust_cfg_test_status_message(status: RustCfgTestStatus) -> &'static str {
    match status {
        RustCfgTestStatus::Disabled => "disabled",
        RustCfgTestStatus::PathClassifiedTest => "not applied to test files",
        RustCfgTestStatus::PrefilterSkipped => "skipped because the file never mentions cfg(test)",
        RustCfgTestStatus::Parsed => "parsed",
        RustCfgTestStatus::ParseFailed => "could not parse, so all code was counted",
    }
}

#[cfg(test)]
mod tests {
    use sprawl_guard_lib::classification::ClassifiedSource;
    use sprawl_guard_lib::{
        CountLines, DirectFileLimit, ExcludedItemCount, LanguageId, LineLimit, RelativePath,
        ResolvedDirectFileLimit, ResolvedFileLinesLimit, RustCfgTestInfo, SourceResolvedLimits,
    };

    use indoc::indoc;

    use super::*;

    fn path(value: &str) -> RelativePath {
        RelativePath::new(value).unwrap()
    }

    fn source(path: &str, language: &str, kind: SourceKind) -> SourceExplanation {
        let language = LanguageId::new(language).unwrap();
        SourceExplanation::NotFound {
            source: ClassifiedSource::new(
                RelativePath::new(path).unwrap(),
                language,
                language.automatic_group(),
                kind,
            ),
        }
    }

    mod explain_message_tests {
        use super::*;

        mod when_the_path_is_not_a_regular_file {
            use super::*;

            #[test]
            fn it_reports_the_filesystem_decision() {
                let message = explain_message(&PathClassification::NonFile {
                    path: path("src/lib.rs"),
                });

                assert_eq!(message, "src/lib.rs: not a regular file");
            }
        }

        mod when_the_file_does_not_match_an_enabled_language {
            use super::*;

            #[test]
            fn it_reports_the_language_decision() {
                let message = explain_message(&PathClassification::NotSource {
                    path: path("README.md"),
                });

                assert_eq!(
                    message,
                    "README.md: not a source file for any enabled language"
                );
            }
        }

        mod when_the_source_path_does_not_exist {
            use super::*;

            #[test]
            fn it_says_the_path_was_classified_by_path_only() {
                let message = explain_message(&PathClassification::Source(source(
                    "src/planned.rs",
                    "rust",
                    SourceKind::Production,
                )));

                assert_eq!(
                    message,
                    "src/planned.rs\n  kind: production source\n  language: Rust\n  group: source\n  state: not found, so classified by its path only"
                );
            }
        }

        mod when_path_classified_tests_are_disabled {
            use super::*;

            #[test]
            fn it_reports_test_code_without_calling_it_test_source() {
                let message = explain_message(&PathClassification::TestExcluded {
                    path: path("tests/integration.rs"),
                    language: LanguageId::new("rust").unwrap(),
                });

                assert_eq!(
                    message,
                    "tests/integration.rs: excluded because test files are not checked (Rust)"
                );
            }
        }

        #[cfg(unix)]
        mod when_the_path_contains_terminal_controls {
            use super::*;

            #[test]
            fn it_escapes_them() {
                let message = explain_message(&PathClassification::NotSource {
                    path: path("src/\u{1b}[2J.txt"),
                });

                assert_eq!(
                    message,
                    r"src/\u{1b}[2J.txt: not a source file for any enabled language"
                );
            }
        }

        mod when_all_matching_languages_exclude_the_file {
            use super::*;

            #[test]
            fn it_reports_all_matching_languages() {
                let message = explain_message(&PathClassification::LanguagesExcluded {
                    path: path("src/shared"),
                    languages: vec![
                        LanguageId::new("rust").unwrap(),
                        LanguageId::new("go").unwrap(),
                    ],
                });

                assert_eq!(
                    message,
                    "src/shared: excluded by the settings of every matching language (Rust, Go)"
                );
            }
        }

        mod when_a_rust_source_has_cfg_test_preprocessing {
            use super::*;

            #[test]
            fn it_reports_the_status_and_excluded_item_count() {
                let source = source("src/lib.rs", "rust", SourceKind::Production)
                    .source()
                    .clone();
                let explanation = SourceExplanation::Measured {
                    source,
                    resolved_limits: SourceResolvedLimits {
                        file: ResolvedFileLinesLimit {
                            count_lines: CountLines::Code,
                            value: LineLimit::new(400).unwrap(),
                            matched_overrides: vec![],
                        },
                        directory: ResolvedDirectFileLimit {
                            value: DirectFileLimit::Unlimited,
                            matched_overrides: vec![],
                        },
                    },
                    measurement: sprawl_guard_lib::code_line_counter::LineMeasurement::new(
                        CountLines::Code,
                        sprawl_guard_lib::LineCounts::new(
                            sprawl_guard_lib::LineCount::new(1),
                            sprawl_guard_lib::LineCount::new(0),
                            sprawl_guard_lib::LineCount::new(0),
                        ),
                    )
                    .unwrap(),
                    rust_cfg_test: Some(RustCfgTestInfo {
                        status: RustCfgTestStatus::Parsed,
                        excluded_items: ExcludedItemCount::new(2),
                    }),
                };

                let message = explain_message(&PathClassification::Source(explanation));

                assert_eq!(
                    message,
                    indoc! {"
                        src/lib.rs
                          kind: production source
                          language: Rust
                          group: source
                          lines: 1 (limit 400, counting code)
                          line counts: 1 code, 0 comments, 0 blank
                          direct files limit for its directory: unlimited
                          Rust cfg(test): parsed, 2 top-level items not counted"}
                );
            }
        }

        mod when_a_documentation_source_is_classified {
            use super::*;

            #[test]
            fn it_reports_the_automatic_group() {
                let explanation = source("docs/guide.md", "markdown", SourceKind::Production);

                let message = explain_message(&PathClassification::Source(explanation));

                assert_eq!(
                    message,
                    "docs/guide.md\n  kind: production source\n  language: Markdown\n  group: documentation\n  state: not found, so classified by its path only"
                );
            }
        }
    }
}
