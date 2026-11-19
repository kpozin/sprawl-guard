#![cfg(test)]

use std::fs;
use std::path::{Path, PathBuf};

use indoc::{formatdoc, indoc};

use super::*;
use crate::config::Config;
use crate::test_support::{
    assert_symlink_loop_error, load_config, write_config, write_rust_enabled_config,
};
use crate::{
    CheckStatus, CheckWarningKind, DirectoryDirectFilesViolation, RatchetViolation, RelativePath,
};

fn write_ratchet(root: &Path, contents: &str) {
    fs::write(root.join("sprawl-guard.ratchet.toml"), contents).unwrap();
}

fn write_file(root: &Path, path: &str, contents: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn write_rust_file_limit_config(root: &Path, production: u32) {
    write_config(
        root,
        &formatdoc! {r#"
            [limits.max_lines_per_file]
            production = {production}

            [languages.rust]
            enabled = true
        "#},
    );
}

fn write_rust_source_with_function_count(root: &Path, path: &str, function_count: u32) {
    let contents = (0..function_count)
        .map(|index| format!("fn function_{index}() {{}}\n"))
        .collect::<String>();
    write_file(root, path, &contents);
}

fn write_rust_directory_direct_files_fixture(root: &Path) {
    write_config(
        root,
        indoc! {r#"
            [limits]
            max_direct_files_per_directory = { production = 1 }

            [languages.rust]
            enabled = true
        "#},
    );
    write_file(root, "src/a.rs", "fn a() {}\n");
    write_file(root, "src/b.rs", "fn b() {}\n");
}

fn check_root(root: &Path) -> Result<CheckReport> {
    let config = load_config(root);
    check_with_config(root, &config, vec![], None, None)
}

fn check_selected_paths(root: &Path, paths: Vec<PathBuf>) -> Result<CheckReport> {
    let config = load_config(root);
    check_with_config(root, &config, paths, None, None)
}

fn check_with_traversal_policy(
    root: &Path,
    paths: Vec<PathBuf>,
    unreadable_paths: crate::config::UnreadablePathPolicy,
) -> Result<CheckReport> {
    let config = load_config(root);
    check_with_config(root, &config, paths, None, Some(unreadable_paths))
}

fn check_with_ratchet_current(root: &Path, require_ratchet_current: bool) -> Result<CheckReport> {
    let config = load_config(root);
    check_with_config(root, &config, vec![], Some(require_ratchet_current), None)
}

fn check_with_config(
    root: &Path,
    config: &Config,
    paths: Vec<PathBuf>,
    require_ratchet_current: Option<bool>,
    unreadable_paths: Option<crate::config::UnreadablePathPolicy>,
) -> Result<CheckReport> {
    check(CheckOptions {
        root: root.to_path_buf(),
        policy: crate::config::ResolvedConfig::built_in(config),
        paths,
        require_ratchet_current,
        unreadable_paths,
    })
}

fn count_rust_production_source(root: &Path, source_text: &str) -> CountedSource {
    count_rust_production_source_with_line_types(root, source_text, CountLines::Code)
}

fn count_rust_production_source_with_line_types(
    root: &Path,
    source_text: &str,
    count_lines: CountLines,
) -> CountedSource {
    const SOURCE_PATH: &str = "src/lib.rs";

    write_rust_enabled_config(root);
    write_file(root, SOURCE_PATH, source_text);
    let config = load_config(root);
    let source = ClassifiedSource::new(
        RelativePath::new(Path::new(SOURCE_PATH)).unwrap(),
        crate::LanguageId::new("Rust").unwrap(),
        crate::LanguageGroupId::Source,
        SourceKind::Production,
    );
    count_source(
        &config,
        &TokeiCodeLineCounter,
        &source,
        &root.join(SOURCE_PATH),
        count_lines,
    )
    .unwrap()
}

fn file_violations(report: &CheckReport) -> Vec<&FileLinesViolation> {
    report
        .violations()
        .iter()
        .filter_map(|violation| match violation {
            CheckViolation::FileLines(violation) => Some(violation),
            CheckViolation::DirectoryDirectFiles(_) | CheckViolation::Ratchet(_) => None,
        })
        .collect()
}

fn directory_violations(report: &CheckReport) -> Vec<&DirectoryDirectFilesViolation> {
    report
        .violations()
        .iter()
        .filter_map(|violation| match violation {
            CheckViolation::DirectoryDirectFiles(violation) => Some(violation),
            CheckViolation::FileLines(_) | CheckViolation::Ratchet(_) => None,
        })
        .collect()
}

fn ratchet_violations(report: &CheckReport) -> Vec<&RatchetViolation> {
    report
        .violations()
        .iter()
        .filter_map(|violation| match violation {
            CheckViolation::Ratchet(violation) => Some(violation),
            CheckViolation::FileLines(_) | CheckViolation::DirectoryDirectFiles(_) => None,
        })
        .collect()
}

fn single_failed_file_violation(
    report: &CheckReport,
    kind: FileViolationKind,
) -> &FileLinesViolation {
    let violations = file_violations(report);
    assert_eq!(report.status(), CheckStatus::Failed);
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].kind, kind);
    violations[0]
}

fn single_directory_violation(report: &CheckReport) -> &DirectoryDirectFilesViolation {
    let violations = directory_violations(report);
    assert_eq!(violations.len(), 1);
    violations[0]
}

#[derive(Clone, Copy)]
struct MismatchedLineTypeCounter;

impl CodeLineCounter for MismatchedLineTypeCounter {
    fn count_path(
        &self,
        _path: &Path,
        _language: crate::LanguageId,
        _count_lines: CountLines,
    ) -> Result<LineMeasurement> {
        mismatched_measurement()
    }

    fn count_source(
        &self,
        _source: &str,
        _language: crate::LanguageId,
        _count_lines: CountLines,
    ) -> Result<LineMeasurement> {
        mismatched_measurement()
    }

    fn backend_id(&self) -> &'static str {
        "test"
    }

    fn backend_version(&self) -> &'static str {
        "1"
    }
}

fn mismatched_measurement() -> Result<LineMeasurement> {
    Ok(LineMeasurement::new(
        CountLines::Code,
        crate::LineCounts::new(
            crate::LineCount::new(1),
            crate::LineCount::new(2),
            crate::LineCount::new(0),
        ),
    )
    .unwrap())
}

mod when_an_injected_counter_reports_a_different_aggregate_policy {
    use super::*;

    #[test]
    fn it_recomputes_the_aggregate_from_the_resolved_line_types_and_breakdown() {
        let tempdir = tempfile::tempdir().unwrap();
        write_config(
            tempdir.path(),
            indoc! {r#"
                [limits.max_lines_per_file]
                count_lines = ["comments"]
                production = 1
            "#},
        );
        write_file(tempdir.path(), "src/lib.rs", "fn source() {}\n");
        let config = load_config(tempdir.path());

        let report = check_with_counter(
            CheckOptions {
                root: tempdir.path().to_path_buf(),
                policy: crate::config::ResolvedConfig::built_in(&config),
                paths: vec![],
                require_ratchet_current: None,
                unreadable_paths: None,
            },
            &MismatchedLineTypeCounter,
        )
        .unwrap();

        let violation = single_failed_file_violation(&report, FileViolationKind::FileLines);
        assert_eq!(
            violation.measurement.count_lines(),
            CountLines::CodeAndComments
        );
        assert_eq!(violation.measurement.actual().get(), 3);
    }
}

mod when_source_exceeds_the_production_file_limit {
    use super::*;

    #[test]
    fn it_reports_a_file_lines_violation() {
        let tempdir = tempfile::tempdir().unwrap();
        write_config(
            tempdir.path(),
            indoc! {r#"
                [limits.max_lines_per_file]
                production = 2

                [languages.rust]
                enabled = true

                [traversal]
                unreadable_paths = "warn"
            "#},
        );
        write_rust_source_with_function_count(tempdir.path(), "src/lib.rs", 3);

        let report = check_root(tempdir.path()).unwrap();

        let violations = file_violations(&report);
        assert_eq!(report.status(), CheckStatus::Failed);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].kind, FileViolationKind::FileLines);
        assert_eq!(violations[0].measurement.actual().get(), 3);
    }
}

mod when_source_exceeds_the_byte_size_limit {
    use super::*;

    #[test]
    fn it_reports_a_file_size_error_before_counting() {
        let tempdir = tempfile::tempdir().unwrap();
        write_config(
            tempdir.path(),
            indoc! {r#"
                [traversal]
                max_parsed_code_file_bytes = 8

                [languages.rust]
                enabled = true
            "#},
        );
        write_file(tempdir.path(), "src/lib.rs", "fn too_large() {}\n");

        let error = check_root(tempdir.path()).unwrap_err();

        assert!(matches!(
            error,
            crate::SprawlError::ParsedCodeFileTooLarge { limit, .. } if limit.into_inner() == 8
        ));
    }

    #[test]
    fn it_allows_a_source_equal_to_the_byte_size_limit() {
        let tempdir = tempfile::tempdir().unwrap();
        write_config(
            tempdir.path(),
            indoc! {r#"
                [traversal]
                max_parsed_code_file_bytes = 10

                [languages.rust]
                enabled = true
            "#},
        );
        write_file(tempdir.path(), "src/lib.rs", "fn a() {}\n");

        let report = check_root(tempdir.path()).unwrap();

        assert_eq!(report.status(), CheckStatus::Passed);
        assert!(report.violations().is_empty());
    }
}

#[cfg(unix)]
mod when_recoverable_traversal_errors_occur {
    use std::os::unix::fs::symlink;

    use crate::classification::UnreadablePathSeverity;

    use super::*;

    #[test]
    fn it_collects_unreadable_paths_before_failing_in_fail_mode() {
        let tempdir = tempfile::tempdir().unwrap();
        write_config(
            tempdir.path(),
            indoc! {r#"
                [traversal]
                follow_symlinks = "directories"

                [languages.rust]
                enabled = true
            "#},
        );
        write_file(tempdir.path(), "real/lib.rs", "fn a() {}\n");
        symlink("..", tempdir.path().join("real/cycle")).unwrap();

        let error = check_root(tempdir.path()).unwrap_err();

        assert_symlink_loop_error(error);
    }

    #[test]
    fn it_marks_warn_mode_reports_partial_without_failing_the_check() {
        let tempdir = tempfile::tempdir().unwrap();
        write_config(
            tempdir.path(),
            indoc! {r#"
                [traversal]
                follow_symlinks = "directories"
                unreadable_paths = "warn"

                [languages.rust]
                enabled = true
            "#},
        );
        write_file(tempdir.path(), "real/lib.rs", "fn a() {}\n");
        symlink("..", tempdir.path().join("real/cycle")).unwrap();

        let report = check_root(tempdir.path()).unwrap();

        assert_eq!(report.status(), CheckStatus::Passed);
        assert!(!report.analysis_complete());
        assert_eq!(report.unreadable_paths().len(), 1);
        assert_eq!(
            report.unreadable_paths()[0].severity,
            UnreadablePathSeverity::Warning
        );
    }
}

mod when_test_code_exceeds_the_test_file_limit {
    use super::*;

    #[test]
    fn it_reports_a_test_file_lines_violation() {
        let tempdir = tempfile::tempdir().unwrap();
        write_config(
            tempdir.path(),
            indoc! {r#"
                [limits.max_lines_per_file]
                test = 2

                [languages.rust]
                enabled = true
            "#},
        );
        write_file(
            tempdir.path(),
            "tests/integration.rs",
            indoc! {r#"
                #[test]
                fn a() {}
                fn b() {}
            "#},
        );

        let report = check_root(tempdir.path()).unwrap();

        let violation = single_failed_file_violation(&report, FileViolationKind::TestFileLines);
        assert_eq!(violation.measurement.actual().get(), 3);
    }
}

mod when_rust_production_source_contains_cfg_test_items {
    use super::*;

    #[test]
    fn it_excludes_top_level_cfg_test_items_from_the_production_file_count() {
        let tempdir = tempfile::tempdir().unwrap();
        write_config(
            tempdir.path(),
            indoc! {r#"
                [limits.max_lines_per_file]
                production = 1

                [languages.rust]
                enabled = true
            "#},
        );
        write_file(
            tempdir.path(),
            "src/lib.rs",
            indoc! {r#"
                fn production_one() {}
                fn production_two() {}

                #[cfg(test)]
                mod tests {
                    fn fixture_one() {}
                    fn fixture_two() {}
                }
            "#},
        );

        let report = check_root(tempdir.path()).unwrap();

        let violation = single_failed_file_violation(&report, FileViolationKind::FileLines);
        assert_eq!(violation.measurement.actual().get(), 2);
    }

    #[test]
    fn it_counts_cfg_test_items_when_rust_exclusion_is_disabled() {
        let tempdir = tempfile::tempdir().unwrap();
        write_config(
            tempdir.path(),
            indoc! {r#"
                [limits.max_lines_per_file]
                production = 1

                [languages.rust]
                enabled = true
                exclude_cfg_test = false
            "#},
        );
        write_file(
            tempdir.path(),
            "src/lib.rs",
            indoc! {r#"
                #[cfg(test)]
                fn fixture() {}
            "#},
        );

        let report = check_root(tempdir.path()).unwrap();

        let violation = single_failed_file_violation(&report, FileViolationKind::FileLines);
        assert!(violation.measurement.actual().get() > 1);
    }

    #[test]
    fn it_excludes_file_wide_cfg_test_sources_from_the_production_file_count() {
        let tempdir = tempfile::tempdir().unwrap();
        let counted = count_rust_production_source(
            tempdir.path(),
            indoc! {r#"
                #![cfg(test)]

                fn fixture_one() {}
                fn fixture_two() {}

                // trailing test-only comment

            "#},
        );

        assert_eq!(
            counted.measurement.line_counts(),
            crate::LineCounts::new(
                crate::LineCount::new(0),
                crate::LineCount::new(0),
                crate::LineCount::new(0),
            ),
        );
    }

    #[test]
    fn it_excludes_cfg_test_items_from_every_logical_line_category() {
        let tempdir = tempfile::tempdir().unwrap();
        let counted = count_rust_production_source_with_line_types(
            tempdir.path(),
            indoc! {r#"
                fn production() {}

                #[cfg(test)]
                mod tests {
                    // fixture comment

                    fn fixture() {}
                }
            "#},
            CountLines::All,
        );

        assert_eq!(
            counted.measurement.line_counts(),
            crate::LineCounts::new(
                crate::LineCount::new(1),
                crate::LineCount::new(0),
                crate::LineCount::new(1),
            ),
        );
        assert_eq!(counted.measurement.actual().get(), 2);
    }
}

mod when_rust_test_source_contains_cfg_test_items {
    use super::*;

    #[test]
    fn it_counts_them_against_the_path_classified_test_file_limit() {
        let tempdir = tempfile::tempdir().unwrap();
        write_config(
            tempdir.path(),
            indoc! {r#"
                [limits.max_lines_per_file]
                test = 1

                [languages.rust]
                enabled = true
            "#},
        );
        write_file(
            tempdir.path(),
            "tests/integration.rs",
            indoc! {r#"
                #[cfg(test)]
                fn fixture() {}
            "#},
        );

        let report = check_root(tempdir.path()).unwrap();

        let violation = single_failed_file_violation(&report, FileViolationKind::TestFileLines);
        assert!(violation.measurement.actual().get() > 1);
    }
}

mod when_rust_cfg_test_preprocessing_fails {
    use super::*;

    #[test]
    fn it_warns_and_counts_the_original_source() {
        let tempdir = tempfile::tempdir().unwrap();
        write_config(
            tempdir.path(),
            indoc! {r#"
                [limits.max_lines_per_file]
                count_lines = ["comments"]
                production = 1

                [languages.rust]
                enabled = true
            "#},
        );
        write_file(
            tempdir.path(),
            "src/lib.rs",
            indoc! {r#"
                #[cfg(test)]
                fn broken( {
            "#},
        );

        let report = check_root(tempdir.path()).unwrap();

        assert_eq!(report.warnings().len(), 1);
        assert_eq!(
            report.warnings()[0].kind,
            CheckWarningKind::RustCfgTestParseFailed
        );
        assert_eq!(report.warnings()[0].path.as_str(), "src/lib.rs");
        assert!(!report.warnings()[0].detail.is_empty());
        assert_eq!(report.status(), CheckStatus::Failed);
        let violation = single_failed_file_violation(&report, FileViolationKind::FileLines);
        assert_eq!(violation.measurement.actual().get(), 2);
    }

    #[test]
    fn it_preserves_every_logical_line_category_from_the_original_source() {
        let tempdir = tempfile::tempdir().unwrap();
        let counted = count_rust_production_source(
            tempdir.path(),
            indoc! {r#"
                #[cfg(test)]
                fn broken( {
                // comment

            "#},
        );

        assert_eq!(
            counted.measurement.line_counts(),
            crate::LineCounts::new(
                crate::LineCount::new(2),
                crate::LineCount::new(1),
                crate::LineCount::new(1),
            ),
        );
        assert_eq!(counted.warnings.len(), 1);
        assert_eq!(
            counted.warnings[0].kind,
            CheckWarningKind::RustCfgTestParseFailed,
        );
    }
}

mod when_checking_directory_direct_files {
    use super::*;

    mod when_languages_share_a_custom_group {
        use super::*;

        #[test]
        fn it_counts_the_custom_group_separately_from_source() {
            let tempdir = tempfile::tempdir().unwrap();
            write_config(
                tempdir.path(),
                indoc! {r#"
                    [limits.max_direct_files_per_directory]
                    production = 1

                    [language_groups.frontend]
                    languages = ["javascript", "typescript", "jsx", "tsx"]

                    [language_groups.frontend.limits.max_direct_files_per_directory]
                    production = 3
                "#},
            );
            write_file(tempdir.path(), "src/app.js", "export const app = 1;\n");
            write_file(
                tempdir.path(),
                "src/component.jsx",
                "export const C = () => null;\n",
            );
            write_file(tempdir.path(), "src/lib.ts", "export const lib = 1;\n");
            write_file(
                tempdir.path(),
                "src/view.tsx",
                "export const View = () => null;\n",
            );
            write_file(tempdir.path(), "src/main.rs", "fn main() {}\n");

            let report = check_root(tempdir.path()).unwrap();

            let violation = single_directory_violation(&report);
            assert_eq!(
                violation.group,
                crate::LanguageGroupId::custom("frontend").unwrap()
            );
            assert_eq!(violation.actual.get(), 4);
            assert_eq!(violation.limit.get(), 3);
        }
    }

    mod when_a_custom_group_has_role_specific_limits {
        use super::*;

        #[test]
        fn it_resolves_production_and_test_members_independently() {
            let tempdir = tempfile::tempdir().unwrap();
            write_config(
                tempdir.path(),
                indoc! {r#"
                    [language_groups.frontend]
                    languages = ["typescript"]

                    [language_groups.frontend.limits.max_direct_files_per_directory]
                    production = 2
                    test = 1
                "#},
            );
            write_file(tempdir.path(), "src/a.ts", "export const a = 1;\n");
            write_file(tempdir.path(), "src/b.ts", "export const b = 1;\n");
            write_file(tempdir.path(), "src/a.test.ts", "export const a = 1;\n");
            write_file(tempdir.path(), "src/b.test.ts", "export const b = 1;\n");

            let report = check_root(tempdir.path()).unwrap();

            let violation = single_directory_violation(&report);
            assert_eq!(
                violation.kind,
                DirectoryViolationKind::TestDirectoryDirectFiles
            );
            assert_eq!(
                violation.group,
                crate::LanguageGroupId::custom("frontend").unwrap()
            );
            assert_eq!(violation.actual.get(), 2);
            assert_eq!(violation.limit.get(), 1);
        }
    }

    mod when_a_path_override_matches_a_custom_group {
        use super::*;

        #[test]
        fn it_replaces_the_group_limit() {
            let tempdir = tempfile::tempdir().unwrap();
            write_config(
                tempdir.path(),
                indoc! {r#"
                    [limits.max_direct_files_per_directory]
                    production = 8

                    [language_groups.frontend]
                    languages = ["typescript"]

                    [language_groups.frontend.limits.max_direct_files_per_directory]
                    production = 1

                    [[overrides]]
                    paths = ["src/**"]
                    max_direct_files_per_directory = { production = 2 }
                    reason = "frontend modules are colocated"
                "#},
            );
            write_file(tempdir.path(), "src/a.ts", "export const a = 1;\n");
            write_file(tempdir.path(), "src/b.ts", "export const b = 1;\n");
            write_file(tempdir.path(), "src/c.ts", "export const c = 1;\n");

            let report = check_root(tempdir.path()).unwrap();

            let violation = single_directory_violation(&report);
            assert_eq!(
                violation.group,
                crate::LanguageGroupId::custom("frontend").unwrap()
            );
            assert_eq!(violation.actual.get(), 3);
            assert_eq!(violation.limit.get(), 2);
        }
    }

    mod when_the_documentation_group_uses_its_default_limit {
        use super::*;

        #[test]
        fn it_does_not_enforce_the_global_production_limit() {
            let tempdir = tempfile::tempdir().unwrap();
            write_config(
                tempdir.path(),
                indoc! {r#"
                    [limits]
                    max_direct_files_per_directory = { production = 1 }

                    [languages.markdown]
                    enabled = true
                "#},
            );
            write_file(tempdir.path(), "docs/a.md", "# A\n");
            write_file(tempdir.path(), "docs/b.md", "# B\n");

            let report = check_root(tempdir.path()).unwrap();

            assert_eq!(report.status(), CheckStatus::Passed);
            assert!(directory_violations(&report).is_empty());
        }
    }

    mod when_a_path_override_matches_the_documentation_group {
        use super::*;

        #[test]
        fn it_replaces_the_unlimited_default_with_a_finite_limit() {
            let tempdir = tempfile::tempdir().unwrap();
            write_config(
                tempdir.path(),
                indoc! {r#"
                    [limits]
                    max_direct_files_per_directory = { production = 8 }

                    [[overrides]]
                    paths = ["docs/**"]
                    max_direct_files_per_directory = { production = 1 }
                    reason = "keep documentation directories focused"

                    [languages.markdown]
                    enabled = true
                "#},
            );
            write_file(tempdir.path(), "docs/a.md", "# A\n");
            write_file(tempdir.path(), "docs/b.md", "# B\n");

            let report = check_root(tempdir.path()).unwrap();

            let violation = single_directory_violation(&report);
            assert_eq!(violation.group, crate::LanguageGroupId::Documentation);
            assert_eq!(violation.actual.get(), 2);
            assert_eq!(violation.limit.get(), 1);
        }
    }

    #[test]
    fn it_counts_tiny_production_files_as_production_leaves() {
        let tempdir = tempfile::tempdir().unwrap();
        write_config(
            tempdir.path(),
            indoc! {r#"
                [limits]
                max_direct_files_per_directory = { production = 1 }

                [languages.rust]
                enabled = true
            "#},
        );
        write_file(tempdir.path(), "src/tiny.rs", "");
        write_file(tempdir.path(), "src/full.rs", "fn full() {}\n");

        let report = check_root(tempdir.path()).unwrap();

        let violations = directory_violations(&report);
        assert_eq!(report.status(), CheckStatus::Failed);
        assert!(file_violations(&report).is_empty());
        assert_eq!(violations.len(), 1);
        assert_eq!(
            violations[0].kind,
            DirectoryViolationKind::DirectoryDirectFiles
        );
        assert_eq!(violations[0].actual.get(), 2);
        assert_eq!(
            violations[0]
                .files
                .iter()
                .map(RelativePath::as_str)
                .collect::<Vec<_>>(),
            vec!["src/full.rs", "src/tiny.rs"]
        );
    }

    #[test]
    fn it_counts_facade_named_files_as_production_leaves() {
        let tempdir = tempfile::tempdir().unwrap();
        write_config(
            tempdir.path(),
            indoc! {r#"
                [limits]
                max_direct_files_per_directory = { production = 1 }

                [languages.rust]
                enabled = true

                [languages.typescript]
                enabled = true
            "#},
        );
        write_file(tempdir.path(), "src/mod.rs", "mod child;\n");
        write_file(tempdir.path(), "src/index.ts", "export const value = 1;\n");

        let report = check_root(tempdir.path()).unwrap();

        let violation = single_directory_violation(&report);
        assert_eq!(violation.path.as_str(), "src");
        assert_eq!(violation.actual.get(), 2);
    }

    #[test]
    fn it_counts_mixed_language_files_in_the_same_directory() {
        let tempdir = tempfile::tempdir().unwrap();
        write_config(
            tempdir.path(),
            indoc! {r#"
                [limits]
                max_direct_files_per_directory = { production = 1 }

                [languages.rust]
                enabled = true

                [languages.go]
                enabled = true
            "#},
        );
        write_file(tempdir.path(), "src/lib.rs", "fn lib() {}\n");
        write_file(tempdir.path(), "src/main.go", "package main\n");

        let report = check_root(tempdir.path()).unwrap();

        let violations = directory_violations(&report);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].actual.get(), 2);
    }

    #[test]
    fn it_counts_path_classified_test_files_as_test_leaves() {
        let tempdir = tempfile::tempdir().unwrap();
        write_config(
            tempdir.path(),
            indoc! {r#"
                [limits]
                max_direct_files_per_directory = { production = 1, test = 1 }

                [languages.rust]
                enabled = true
            "#},
        );
        write_file(tempdir.path(), "tests/a.rs", "fn a() {}\n");
        write_file(tempdir.path(), "tests/b.rs", "fn b() {}\n");

        let report = check_root(tempdir.path()).unwrap();

        let violations = directory_violations(&report);
        assert!(
            violations
                .iter()
                .all(|violation| violation.kind == DirectoryViolationKind::TestDirectoryDirectFiles)
        );
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].path.as_str(), "tests");
    }

    #[test]
    fn it_applies_path_overrides_to_production_directory_limits() {
        let tempdir = tempfile::tempdir().unwrap();
        write_config(
            tempdir.path(),
            indoc! {r#"
                [limits]
                max_direct_files_per_directory = { production = 1 }

                [[overrides]]
                paths = ["src/**"]
                max_direct_files_per_directory = { production = 2 }
                reason = "generated fan-out is tracked separately"

                [languages.rust]
                enabled = true
            "#},
        );
        write_file(tempdir.path(), "src/a.rs", "fn a() {}\n");
        write_file(tempdir.path(), "src/b.rs", "fn b() {}\n");

        let report = check_root(tempdir.path()).unwrap();

        assert!(directory_violations(&report).is_empty());
    }

    #[test]
    fn it_does_not_apply_non_recursive_child_globs_to_the_parent_directory() {
        let tempdir = tempfile::tempdir().unwrap();
        write_config(
            tempdir.path(),
            indoc! {r#"
                [limits]
                max_direct_files_per_directory = { production = 1 }

                [[overrides]]
                paths = ["src/*"]
                max_direct_files_per_directory = { production = 2 }
                reason = "single child exception"

                [languages.rust]
                enabled = true
            "#},
        );
        write_file(tempdir.path(), "src/a.rs", "fn a() {}\n");
        write_file(tempdir.path(), "src/b.rs", "fn b() {}\n");

        let report = check_root(tempdir.path()).unwrap();

        let violations = directory_violations(&report);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].path.as_str(), "src");
    }

    #[test]
    fn it_applies_path_overrides_to_test_directory_limits() {
        let tempdir = tempfile::tempdir().unwrap();
        write_config(
            tempdir.path(),
            indoc! {r#"
                [limits]
                max_direct_files_per_directory = { test = 1 }

                [[overrides]]
                paths = ["tests/**"]
                max_direct_files_per_directory = { test = 2 }
                reason = "integration fixture fan-out"

                [languages.rust]
                enabled = true
            "#},
        );
        write_file(tempdir.path(), "tests/a.rs", "fn a() {}\n");
        write_file(tempdir.path(), "tests/b.rs", "fn b() {}\n");

        let report = check_root(tempdir.path()).unwrap();

        assert!(directory_violations(&report).is_empty());
    }

    #[test]
    fn it_reports_directories_and_files_in_deterministic_order() {
        let tempdir = tempfile::tempdir().unwrap();
        write_config(
            tempdir.path(),
            indoc! {r#"
                [limits]
                max_direct_files_per_directory = { production = 1, test = 1 }

                [languages.rust]
                enabled = true
            "#},
        );
        write_file(tempdir.path(), "z/a.rs", "fn a() {}\n");
        write_file(tempdir.path(), "z/b.rs", "fn b() {}\n");
        write_file(tempdir.path(), "a/a.rs", "fn a() {}\n");
        write_file(tempdir.path(), "a/b.rs", "fn b() {}\n");
        write_file(tempdir.path(), "tests/z.rs", "fn z() {}\n");
        write_file(tempdir.path(), "tests/a.rs", "fn a() {}\n");

        let report = check_root(tempdir.path()).unwrap();

        let paths = report
            .violations()
            .iter()
            .filter_map(|violation| match violation {
                CheckViolation::DirectoryDirectFiles(violation) => Some(violation),
                CheckViolation::FileLines(_) | CheckViolation::Ratchet(_) => None,
            })
            .map(|violation| violation.path.as_str())
            .collect::<Vec<_>>();
        assert_eq!(paths, vec!["a", "z", "tests"]);
        let violations = directory_violations(&report);
        assert_eq!(
            violations[0]
                .files
                .iter()
                .map(RelativePath::as_str)
                .collect::<Vec<_>>(),
            vec!["a/a.rs", "a/b.rs"]
        );
    }
}

mod when_resolving_limits {
    use super::*;

    #[test]
    fn it_counts_globally_included_comments() {
        let tempdir = tempfile::tempdir().unwrap();
        write_config(
            tempdir.path(),
            indoc! {r#"
                [limits.max_lines_per_file]
                count_lines = ["comments"]
                production = 1
            "#},
        );
        write_file(tempdir.path(), "src/lib.rs", "// comment\nfn main() {}\n");

        let report = check_root(tempdir.path()).unwrap();

        let violations = file_violations(&report);
        assert_eq!(violations.len(), 1);
        let violation = violations[0];
        assert_eq!(violation.measurement.actual().get(), 2);
    }

    #[test]
    fn it_counts_literate_prose_by_default() {
        let tempdir = tempfile::tempdir().unwrap();
        write_config(
            tempdir.path(),
            indoc! {r#"
                [limits.max_lines_per_file]
                production = 3
            "#},
        );
        write_file(
            tempdir.path(),
            "README.md",
            "Prose\n\n```rust\nfn main() {}\n```\n",
        );

        let report = check_root(tempdir.path()).unwrap();

        let violations = file_violations(&report);
        assert_eq!(violations.len(), 1);
        let violation = violations[0];
        assert_eq!(violation.measurement.actual().get(), 4);
    }

    #[test]
    fn it_allows_a_language_to_replace_the_literate_default() {
        let tempdir = tempfile::tempdir().unwrap();
        write_config(
            tempdir.path(),
            indoc! {r#"
                [limits.max_lines_per_file]
                production = 1

                [languages.markdown.limits.max_lines_per_file]
                count_lines = []
            "#},
        );
        write_file(
            tempdir.path(),
            "README.md",
            "Prose\n\n```rust\nfn main() {}\n```\n",
        );

        let report = check_root(tempdir.path()).unwrap();

        assert_eq!(report.status(), CheckStatus::Passed);
        assert!(file_violations(&report).is_empty());
    }

    #[test]
    fn it_applies_language_test_limits() {
        let tempdir = tempfile::tempdir().unwrap();
        write_config(
            tempdir.path(),
            indoc! {r#"
                [limits.max_lines_per_file]
                test = 1

                [languages.rust.limits.max_lines_per_file]
                test = 2
            "#},
        );
        write_file(tempdir.path(), "tests/lib.rs", "fn a() {}\nfn b() {}\n");

        let report = check_root(tempdir.path()).unwrap();

        assert_eq!(report.status(), CheckStatus::Passed);
        assert!(file_violations(&report).is_empty());
    }

    #[test]
    fn it_applies_global_language_and_path_precedence_to_production_file_limits() {
        let tempdir = tempfile::tempdir().unwrap();
        write_config(
            tempdir.path(),
            indoc! {r#"
                [limits]
                max_lines_per_file = { production = 1 }

                [languages.rust]
                enabled = true

                [languages.rust.limits]
                max_lines_per_file = { production = 2 }

                [[overrides]]
                paths = ["src/**"]
                max_lines_per_file = { production = 3 }
                reason = "generated bindings are tracked separately"
            "#},
        );
        write_file(
            tempdir.path(),
            "src/lib.rs",
            indoc! {r#"
                fn a() {}
                fn b() {}
                fn c() {}
            "#},
        );

        let report = check_root(tempdir.path()).unwrap();

        assert_eq!(report.status(), CheckStatus::Passed);
        assert!(file_violations(&report).is_empty());
    }

    #[test]
    fn it_applies_later_matching_file_overrides_per_setting() {
        let tempdir = tempfile::tempdir().unwrap();
        write_config(
            tempdir.path(),
            indoc! {r#"
                [limits]
                max_lines_per_file = { production = 1 }

                [languages.rust]
                enabled = true

                [[overrides]]
                paths = ["src/**"]
                max_lines_per_file = { production = 2 }
                reason = "package-wide exception"

                [[overrides]]
                paths = ["src/legacy/**"]
                max_lines_per_file = { production = 3 }
                reason = "legacy exception"
            "#},
        );
        write_rust_source_with_function_count(tempdir.path(), "src/legacy/lib.rs", 3);

        let report = check_root(tempdir.path()).unwrap();

        assert_eq!(report.status(), CheckStatus::Passed);
        assert!(file_violations(&report).is_empty());
    }

    #[test]
    fn it_preserves_earlier_override_settings_when_later_overrides_are_partial() {
        let tempdir = tempfile::tempdir().unwrap();
        write_config(
            tempdir.path(),
            indoc! {r#"
                [limits]
                max_direct_files_per_directory = { production = 1 }

                [languages.rust]
                enabled = true

                [[overrides]]
                paths = ["src/**"]
                max_direct_files_per_directory = { production = 2 }
                reason = "module fan-out exception"

                [[overrides]]
                paths = ["src/**"]
                max_lines_per_file = { production = 10 }
                reason = "file LOC exception"
            "#},
        );
        write_file(tempdir.path(), "src/a.rs", "fn a() {}\n");
        write_file(tempdir.path(), "src/b.rs", "fn b() {}\n");

        let report = check_root(tempdir.path()).unwrap();

        assert_eq!(report.status(), CheckStatus::Passed);
        assert!(directory_violations(&report).is_empty());
    }

    #[test]
    fn it_does_not_apply_language_file_limits_to_directory_direct_files() {
        let tempdir = tempfile::tempdir().unwrap();
        write_config(
            tempdir.path(),
            indoc! {r#"
                [limits]
                max_direct_files_per_directory = { production = 1 }

                [languages.rust]
                enabled = true

                [languages.rust.limits]
                max_lines_per_file = { production = 100 }
            "#},
        );
        write_file(tempdir.path(), "src/a.rs", "fn a() {}\n");
        write_file(tempdir.path(), "src/b.rs", "fn b() {}\n");

        let report = check_root(tempdir.path()).unwrap();

        let violation = single_directory_violation(&report);
        assert_eq!(violation.kind, DirectoryViolationKind::DirectoryDirectFiles);
        assert_eq!(violation.limit.get(), 1);
    }
}

mod when_sources_are_within_file_limits {
    use super::*;

    #[test]
    fn it_passes_without_violations() {
        let tempdir = tempfile::tempdir().unwrap();
        write_rust_enabled_config(tempdir.path());
        write_file(tempdir.path(), "src/lib.rs", "fn a() {}\n");

        let report = check_root(tempdir.path()).unwrap();

        assert_eq!(report.status(), CheckStatus::Passed);
        assert!(report.violations().is_empty());
    }
}

mod when_check_is_path_scoped {
    use super::*;

    #[test]
    fn it_reports_only_violations_under_the_scope() {
        let tempdir = tempfile::tempdir().unwrap();
        write_rust_file_limit_config(tempdir.path(), 1);
        write_rust_source_with_function_count(tempdir.path(), "checked/lib.rs", 2);
        write_rust_source_with_function_count(tempdir.path(), "skipped/lib.rs", 2);

        let report = check_selected_paths(tempdir.path(), vec![PathBuf::from("checked")]).unwrap();

        let violations = file_violations(&report);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].path.as_str(), "checked/lib.rs");
    }

    #[test]
    fn it_rejects_missing_scopes() {
        let tempdir = tempfile::tempdir().unwrap();
        write_rust_enabled_config(tempdir.path());
        write_file(tempdir.path(), "src/lib.rs", "fn a() {}\n");

        let error = check_with_traversal_policy(
            tempdir.path(),
            vec![PathBuf::from("missing")],
            crate::config::UnreadablePathPolicy::Warn,
        )
        .unwrap_err();

        assert!(error.to_string().contains("selected path does not exist"));
    }

    #[cfg(unix)]
    #[test]
    fn it_prunes_unreadable_directories_outside_the_scope() {
        use std::os::unix::fs::PermissionsExt;

        let tempdir = tempfile::tempdir().unwrap();
        write_rust_file_limit_config(tempdir.path(), 1);
        write_rust_source_with_function_count(tempdir.path(), "checked/lib.rs", 2);
        write_rust_source_with_function_count(tempdir.path(), "skipped/lib.rs", 2);
        let config = load_config(tempdir.path());
        let skipped = tempdir.path().join("skipped");
        let original_permissions = fs::metadata(&skipped).unwrap().permissions();
        fs::set_permissions(&skipped, fs::Permissions::from_mode(0o000)).unwrap();

        let report = check_with_config(
            tempdir.path(),
            &config,
            vec![PathBuf::from("checked")],
            None,
            None,
        );

        fs::set_permissions(&skipped, original_permissions).unwrap();
        let report = report.unwrap();

        let violations = file_violations(&report);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].path.as_str(), "checked/lib.rs");
    }

    #[test]
    fn it_ignores_ambiguous_language_matches_outside_the_scope() {
        let tempdir = tempfile::tempdir().unwrap();
        write_config(
            tempdir.path(),
            indoc! {r#"
                [limits.max_lines_per_file]
                production = 1

                [languages.rust]
                enabled = true
                include = ["checked/**", "ambiguous"]

                [languages.go]
                enabled = true
                include = ["ambiguous"]
            "#},
        );
        write_rust_source_with_function_count(tempdir.path(), "checked/lib.rs", 2);
        write_file(tempdir.path(), "ambiguous", "fn a() {}\n");

        let report = check_selected_paths(tempdir.path(), vec![PathBuf::from("checked")]).unwrap();

        let violations = file_violations(&report);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].path.as_str(), "checked/lib.rs");
    }
}

mod when_checking_with_a_ratchet_file {
    use super::*;

    fn write_file_ratchet(root: &Path, path: &str, lines: u32) {
        write_ratchet(
            root,
            &formatdoc! {r#"
                schema_version = 1

                [[files]]
                path = "{path}"
                role = "production"
                max_lines = {lines}
            "#},
        );
    }

    fn write_rust_directory_ratchet_fixture(root: &Path, max_direct_files: u32) {
        write_config(
            root,
            indoc! {r#"
                [limits]
                max_direct_files_per_directory = { production = 1 }

                [languages.rust]
                enabled = true
            "#},
        );
        write_ratchet(
            root,
            &formatdoc! {r#"
                schema_version = 1

                [[directories]]
                path = "src"
                role = "production"
                group = "source"
                max_direct_files = {max_direct_files}
            "#},
        );
        write_file(root, "src/a.rs", "fn a() {}\n");
        write_file(root, "src/b.rs", "fn b() {}\n");
    }

    #[test]
    fn it_allows_a_file_equal_to_its_ratchet_ceiling() {
        let tempdir = tempfile::tempdir().unwrap();
        write_rust_file_limit_config(tempdir.path(), 1);
        write_file_ratchet(tempdir.path(), "src/lib.rs", 2);
        write_rust_source_with_function_count(tempdir.path(), "src/lib.rs", 2);

        let report = check_root(tempdir.path()).unwrap();

        assert_eq!(report.status(), CheckStatus::Passed);
        assert!(report.violations().is_empty());
    }

    #[test]
    fn it_fails_when_a_file_increases_above_its_ratchet_ceiling() {
        let tempdir = tempfile::tempdir().unwrap();
        write_rust_file_limit_config(tempdir.path(), 1);
        write_file_ratchet(tempdir.path(), "src/lib.rs", 2);
        write_rust_source_with_function_count(tempdir.path(), "src/lib.rs", 3);

        let report = check_root(tempdir.path()).unwrap();

        let file_violations = file_violations(&report);
        assert_eq!(report.status(), CheckStatus::Failed);
        assert_eq!(file_violations.len(), 1);
        assert_eq!(file_violations[0].ratchet.unwrap().get(), 2);
        assert!(ratchet_violations(&report).is_empty());
    }

    #[test]
    fn it_fails_when_require_current_finds_a_lower_file_value() {
        let tempdir = tempfile::tempdir().unwrap();
        write_rust_file_limit_config(tempdir.path(), 1);
        write_file_ratchet(tempdir.path(), "src/lib.rs", 3);
        write_rust_source_with_function_count(tempdir.path(), "src/lib.rs", 2);

        let report = check_root(tempdir.path()).unwrap();

        let ratchet_violations = ratchet_violations(&report);
        assert_eq!(report.status(), CheckStatus::Failed);
        assert_eq!(ratchet_violations.len(), 1);
        assert_eq!(
            ratchet_violations[0].kind(),
            RatchetViolationKind::FileLinesStale
        );
        assert_eq!(ratchet_violations[0].actual().unwrap().get(), 2);
    }

    #[test]
    fn it_allows_stale_values_when_require_current_is_disabled() {
        let tempdir = tempfile::tempdir().unwrap();
        write_rust_file_limit_config(tempdir.path(), 1);
        write_file_ratchet(tempdir.path(), "src/lib.rs", 3);
        write_rust_source_with_function_count(tempdir.path(), "src/lib.rs", 2);

        let report = check_with_ratchet_current(tempdir.path(), false).unwrap();

        assert_eq!(report.status(), CheckStatus::Passed);
        assert!(report.violations().is_empty());
    }

    #[test]
    fn it_treats_ratchets_below_a_non_violating_actual_as_stale() {
        let tempdir = tempfile::tempdir().unwrap();
        write_rust_file_limit_config(tempdir.path(), 10);
        write_file_ratchet(tempdir.path(), "src/lib.rs", 2);
        write_rust_source_with_function_count(tempdir.path(), "src/lib.rs", 3);

        let report = check_root(tempdir.path()).unwrap();

        let ratchet_violations = ratchet_violations(&report);
        assert_eq!(file_violations(&report).len(), 0);
        assert_eq!(ratchet_violations.len(), 1);
        assert_eq!(
            ratchet_violations[0].kind(),
            RatchetViolationKind::FileLinesStale
        );
    }

    #[test]
    fn it_allows_ratchets_below_a_non_violating_actual_when_current_is_not_required() {
        let tempdir = tempfile::tempdir().unwrap();
        write_rust_file_limit_config(tempdir.path(), 10);
        write_file_ratchet(tempdir.path(), "src/lib.rs", 2);
        write_rust_source_with_function_count(tempdir.path(), "src/lib.rs", 3);

        let report = check_with_ratchet_current(tempdir.path(), false).unwrap();

        assert_eq!(report.status(), CheckStatus::Passed);
        assert!(report.violations().is_empty());
    }

    #[test]
    fn it_fails_when_a_ratcheted_path_is_deleted() {
        let tempdir = tempfile::tempdir().unwrap();
        write_rust_enabled_config(tempdir.path());
        write_file_ratchet(tempdir.path(), "src/deleted.rs", 3);

        let report = check_root(tempdir.path()).unwrap();

        let ratchet_violations = ratchet_violations(&report);
        assert_eq!(report.status(), CheckStatus::Failed);
        assert_eq!(ratchet_violations.len(), 1);
        assert_eq!(ratchet_violations[0].path().as_str(), "src/deleted.rs");
        assert!(ratchet_violations[0].actual().is_none());
    }

    #[test]
    fn it_ignores_out_of_scope_ratchets_for_staleness() {
        let tempdir = tempfile::tempdir().unwrap();
        write_rust_file_limit_config(tempdir.path(), 1);
        write_ratchet(
            tempdir.path(),
            indoc! {r#"
                schema_version = 1

                [[files]]
                path = "checked/lib.rs"
                role = "production"
                max_lines = 2

                [[files]]
                path = "skipped/deleted.rs"
                role = "production"
                max_lines = 2
            "#},
        );
        write_rust_source_with_function_count(tempdir.path(), "checked/lib.rs", 2);

        let report = check_selected_paths(tempdir.path(), vec![PathBuf::from("checked")]).unwrap();

        assert_eq!(report.status(), CheckStatus::Passed);
        assert!(report.violations().is_empty());
    }

    #[test]
    fn it_applies_directory_ratchet_ceilings() {
        let tempdir = tempfile::tempdir().unwrap();
        write_rust_directory_ratchet_fixture(tempdir.path(), 2);

        let report = check_root(tempdir.path()).unwrap();

        assert_eq!(report.status(), CheckStatus::Passed);
        assert!(report.violations().is_empty());
    }

    mod when_a_directory_ratchet_belongs_to_another_group {
        use super::*;

        #[test]
        fn it_does_not_cover_the_source_measurement_at_the_same_path() {
            let tempdir = tempfile::tempdir().unwrap();
            write_rust_directory_direct_files_fixture(tempdir.path());
            write_ratchet(
                tempdir.path(),
                indoc! {r#"
                    schema_version = 1

                    [[directories]]
                    path = "src"
                    role = "production"
                    group = "documentation"
                    max_direct_files = 2
                "#},
            );

            let report = check_root(tempdir.path()).unwrap();

            assert_eq!(directory_violations(&report).len(), 1);
            let ratchets = ratchet_violations(&report);
            assert_eq!(ratchets.len(), 1);
            assert!(ratchets[0].actual().is_none());
            assert_eq!(
                serde_json::to_value(ratchets[0]).unwrap()["group"],
                serde_json::json!("documentation")
            );
        }
    }

    mod when_a_directory_measurement_increases_above_its_ratchet {
        use super::*;

        #[test]
        fn it_reports_one_directory_violation_with_the_ceiling() {
            let tempdir = tempfile::tempdir().unwrap();
            write_rust_directory_ratchet_fixture(tempdir.path(), 1);

            let report = check_root(tempdir.path()).unwrap();

            let directories = directory_violations(&report);
            let [violation] = directories.as_slice() else {
                panic!("expected one directory violation");
            };
            assert_eq!(violation.group, crate::LanguageGroupId::Source);
            assert_eq!(violation.actual.get(), 2);
            assert_eq!(violation.ratchet.unwrap().get(), 1);
            assert!(ratchet_violations(&report).is_empty());
        }
    }

    mod when_an_unlimited_directory_measurement_makes_its_ratchet_stale {
        use super::*;

        #[test]
        fn it_carries_the_documentation_group_through_the_diagnostic() {
            let tempdir = tempfile::tempdir().unwrap();
            write_config(
                tempdir.path(),
                indoc! {r#"
                    [languages.markdown]
                    enabled = true
                "#},
            );
            write_ratchet(
                tempdir.path(),
                indoc! {r#"
                    schema_version = 1

                    [[directories]]
                    path = "docs"
                    role = "production"
                    group = "documentation"
                    max_direct_files = 3
                "#},
            );
            write_file(tempdir.path(), "docs/a.md", "# A\n");
            write_file(tempdir.path(), "docs/b.md", "# B\n");

            let report = check_root(tempdir.path()).unwrap();

            let ratchets = ratchet_violations(&report);
            let [ratchet] = ratchets.as_slice() else {
                panic!("expected one ratchet violation");
            };
            let RatchetViolation::Measured(crate::RatchetStaleMeasuredViolation::Directory(
                violation,
            )) = ratchet
            else {
                panic!("expected a stale measured directory ratchet");
            };
            assert_eq!(violation.group, crate::LanguageGroupId::Documentation);
            assert_eq!(violation.actual.get(), 2);
            assert_eq!(violation.limit, crate::RatchetLimit::Unlimited);
            assert_eq!(
                serde_json::to_value(ratchet).unwrap()["limit"],
                serde_json::json!("unlimited")
            );
        }
    }
}

mod when_path_classified_tests_are_disabled {
    use super::*;

    #[test]
    fn it_omits_current_measurements_and_treats_existing_ratchets_as_stale() {
        let tempdir = tempfile::tempdir().unwrap();
        write_config(
            tempdir.path(),
            indoc! {r#"
                [tests]
                check = false

                [limits]
                max_lines_per_file = { test = 1 }
                max_direct_files_per_directory = { test = 1 }
            "#},
        );
        write_ratchet(
            tempdir.path(),
            indoc! {r#"
                schema_version = 1

                [[files]]
                path = "tests/integration.rs"
                role = "test"
                max_lines = 2

                [[directories]]
                path = "tests"
                role = "test"
                group = "source"
                max_direct_files = 1
            "#},
        );
        write_file(
            tempdir.path(),
            "tests/integration.rs",
            "fn first() {}\nfn second() {}\n",
        );

        let report = check_root(tempdir.path()).unwrap();

        assert!(file_violations(&report).is_empty());
        assert!(directory_violations(&report).is_empty());
        let ratchets = ratchet_violations(&report);
        assert_eq!(ratchets.len(), 2);
        assert!(
            ratchets
                .iter()
                .all(|violation| violation.actual().is_none())
        );
    }
}

mod when_multiple_files_violate {
    use super::*;

    #[test]
    fn it_reports_violations_in_deterministic_order() {
        let tempdir = tempfile::tempdir().unwrap();
        write_config(
            tempdir.path(),
            indoc! {r#"
                [limits.max_lines_per_file]
                production = 1
                test = 1

                [languages.rust]
                enabled = true
            "#},
        );
        write_file(
            tempdir.path(),
            "z/lib.rs",
            indoc! {r#"
                fn a() {}
                fn b() {}
            "#},
        );
        write_file(
            tempdir.path(),
            "a/lib.rs",
            indoc! {r#"
                fn a() {}
                fn b() {}
            "#},
        );
        write_file(
            tempdir.path(),
            "tests/z.rs",
            indoc! {r#"
                fn a() {}
                fn b() {}
            "#},
        );

        let report = check_root(tempdir.path()).unwrap();

        let paths = file_violations(&report)
            .iter()
            .map(|violation| violation.path.as_str())
            .collect::<Vec<_>>();
        assert_eq!(paths, vec!["a/lib.rs", "z/lib.rs", "tests/z.rs"]);
    }
}
