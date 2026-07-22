use sprawl_guard_lib::classification::{PathClassification, SourceExplanation, SourceKind};
use sprawl_guard_lib::{MatchedOverride, RustCfgTestStatus, SourceResolvedLimits};

pub(super) fn explain_message(classification: &PathClassification) -> String {
    match classification {
        PathClassification::Ignored { path } => {
            format!("{path}: ignored by standard ignore handling")
        }
        PathClassification::GloballyExcluded { path } => {
            format!("{path}: excluded by global file config")
        }
        PathClassification::NonFile { path } => {
            format!("{path}: not a regular file")
        }
        PathClassification::NotSource { path } => {
            format!("{path}: not a source file for any enabled language")
        }
        PathClassification::AmbiguousLanguage { path, languages } => {
            let languages = languages
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{path}: ambiguous language match ({languages})")
        }
        PathClassification::LanguageExcluded { path, language } => {
            format!("{path}: excluded by {language} language config")
        }
        PathClassification::LanguagesExcluded { path, languages } => {
            let languages = languages
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{path}: excluded by all matching language configs ({languages})")
        }
        PathClassification::TestExcluded { path, language } => {
            format!("{path}: excluded because path is classified as test code ({language})")
        }
        PathClassification::Source(source) => source_message(source),
    }
}

fn source_message(explanation: &SourceExplanation) -> String {
    let kind = match explanation.source.kind {
        SourceKind::Production => "production source",
        SourceKind::Test => "test code",
    };
    let mut message = format!(
        "{}: {kind} ({})",
        explanation.source.path, explanation.source.language
    );
    if let Some(limits) = &explanation.resolved_limits {
        message.push_str(&resolved_limits_message(limits));
    }
    if let Some(rust_cfg_test) = &explanation.rust_cfg_test {
        message.push_str(&format!(
            "; rust cfg(test): {}; excluded top-level items: {}",
            rust_cfg_test_status_message(rust_cfg_test.status),
            rust_cfg_test.excluded_items.get()
        ));
    }
    message
}

fn resolved_limits_message(limits: &SourceResolvedLimits) -> String {
    let mut message = format!("; file LOC limit: {}", limits.file.value);
    if let Some(directory) = &limits.directory {
        message.push_str(&format!("; directory leaf-file limit: {}", directory.value));
    }
    let file_overrides = matching_overrides_message(&limits.file.matched_overrides);
    if !file_overrides.is_empty() {
        message.push_str("; matched file overrides: ");
        message.push_str(&file_overrides);
    }
    if let Some(directory) = &limits.directory {
        let directory_overrides = matching_overrides_message(&directory.matched_overrides);
        if !directory_overrides.is_empty() {
            message.push_str("; matched directory overrides: ");
            message.push_str(&directory_overrides);
        }
    }
    message
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
        RustCfgTestStatus::PathClassifiedTest => "not applied to path-classified test code",
        RustCfgTestStatus::PrefilterSkipped => "skipped by prefilter",
        RustCfgTestStatus::Parsed => "parsed",
        RustCfgTestStatus::ParseFailed => "parse failed; counted without exclusion",
    }
}

#[cfg(test)]
mod tests {
    use sprawl_guard_lib::classification::ClassifiedSource;
    use sprawl_guard_lib::{ExcludedItemCount, LanguageId, RelativePath, RustCfgTestInfo};

    use super::*;

    fn path(value: &str) -> RelativePath {
        RelativePath::new(value).unwrap()
    }

    fn source(path: &str, kind: SourceKind) -> SourceExplanation {
        SourceExplanation {
            source: ClassifiedSource {
                path: RelativePath::new(path).unwrap(),
                language: LanguageId::new("rust").unwrap(),
                kind,
            },
            resolved_limits: None,
            rust_cfg_test: None,
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
                    "tests/integration.rs: excluded because path is classified as test code (Rust)"
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
                    "src/shared: excluded by all matching language configs (Rust, Go)"
                );
            }
        }

        mod when_a_rust_source_has_cfg_test_preprocessing {
            use super::*;

            #[test]
            fn it_reports_the_status_and_excluded_item_count() {
                let mut explanation = source("src/lib.rs", SourceKind::Production);
                explanation.rust_cfg_test = Some(RustCfgTestInfo {
                    status: RustCfgTestStatus::Parsed,
                    excluded_items: ExcludedItemCount::new(2),
                });

                let message = explain_message(&PathClassification::Source(explanation));

                assert_eq!(
                    message,
                    "src/lib.rs: production source (Rust); rust cfg(test): parsed; excluded top-level items: 2"
                );
            }
        }
    }
}
