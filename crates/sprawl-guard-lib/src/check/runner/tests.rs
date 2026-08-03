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
    CheckStatus, CheckWarningKind, DirectoryFanoutViolation, RatchetViolation, RelativePath,
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

fn write_rust_directory_fanout_fixture(root: &Path) {
    write_config(
        root,
        indoc! {r#"
            [limits]
            max_leaf_files_per_directory = { production = 1 }

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

fn check_scopes(root: &Path, paths: Vec<PathBuf>) -> Result<CheckReport> {
    let config = load_config(root);
    check_with_config(root, &config, paths, None, None)
}

fn check_with_traversal_policy(
    root: &Path,
    paths: Vec<PathBuf>,
    traversal_error_policy: crate::config::TraversalErrorPolicy,
) -> Result<CheckReport> {
    let config = load_config(root);
    check_with_config(root, &config, paths, None, Some(traversal_error_policy))
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
    traversal_error_policy: Option<crate::config::TraversalErrorPolicy>,
) -> Result<CheckReport> {
    check(CheckOptions {
        root: root.to_path_buf(),
        config,
        paths,
        require_ratchet_current,
        traversal_error_policy,
    })
}

fn count_rust_production_source(root: &Path, source_text: &str) -> CountedSource {
    count_rust_production_source_with_line_types(root, source_text, IncludedLineTypes::Code)
}

fn count_rust_production_source_with_line_types(
    root: &Path,
    source_text: &str,
    included_line_types: IncludedLineTypes,
) -> CountedSource {
    const SOURCE_PATH: &str = "src/lib.rs";

    write_rust_enabled_config(root);
    write_file(root, SOURCE_PATH, source_text);
    let config = load_config(root);
    let source = ClassifiedSource::new(
        RelativePath::new(Path::new(SOURCE_PATH)).unwrap(),
        crate::LanguageId::new("Rust").unwrap(),
        SourceKind::Production,
    );
    count_source(
        &config,
        &TokeiCodeLineCounter,
        &source,
        &root.join(SOURCE_PATH),
        included_line_types,
    )
    .unwrap()
}

fn file_violations(report: &CheckReport) -> Vec<&FileLocViolation> {
    report
        .violations()
        .iter()
        .filter_map(|violation| match violation {
            CheckViolation::FileLoc(violation) => Some(violation),
            CheckViolation::DirectoryFanout(_) | CheckViolation::Ratchet(_) => None,
        })
        .collect()
}

fn directory_violations(report: &CheckReport) -> Vec<&DirectoryFanoutViolation> {
    report
        .violations()
        .iter()
        .filter_map(|violation| match violation {
            CheckViolation::DirectoryFanout(violation) => Some(violation),
            CheckViolation::FileLoc(_) | CheckViolation::Ratchet(_) => None,
        })
        .collect()
}

fn ratchet_violations(report: &CheckReport) -> Vec<&RatchetViolation> {
    report
        .violations()
        .iter()
        .filter_map(|violation| match violation {
            CheckViolation::Ratchet(violation) => Some(violation),
            CheckViolation::FileLoc(_) | CheckViolation::DirectoryFanout(_) => None,
        })
        .collect()
}

fn single_failed_file_violation(
    report: &CheckReport,
    kind: FileViolationKind,
) -> &FileLocViolation {
    let violations = file_violations(report);
    assert_eq!(report.status(), CheckStatus::Failed);
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].kind, kind);
    violations[0]
}

fn single_directory_violation(report: &CheckReport) -> &DirectoryFanoutViolation {
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
        _included_line_types: IncludedLineTypes,
    ) -> Result<LocMeasurement> {
        mismatched_measurement()
    }

    fn count_source(
        &self,
        _source: &str,
        _language: crate::LanguageId,
        _included_line_types: IncludedLineTypes,
    ) -> Result<LocMeasurement> {
        mismatched_measurement()
    }

    fn backend_id(&self) -> &'static str {
        "test"
    }

    fn backend_version(&self) -> &'static str {
        "1"
    }
}

fn mismatched_measurement() -> Result<LocMeasurement> {
    Ok(LocMeasurement::new(
        IncludedLineTypes::Code,
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
                included_line_types = ["code", "comments"]
                production = 1
            "#},
        );
        write_file(tempdir.path(), "src/lib.rs", "fn source() {}\n");
        let config = load_config(tempdir.path());

        let report = check_with_counter(
            CheckOptions {
                root: tempdir.path().to_path_buf(),
                config: &config,
                paths: vec![],
                require_ratchet_current: None,
                traversal_error_policy: None,
            },
            &MismatchedLineTypeCounter,
        )
        .unwrap();

        let violation = single_failed_file_violation(&report, FileViolationKind::FileLoc);
        assert_eq!(
            violation.measurement.included_line_types(),
            IncludedLineTypes::CodeAndComments
        );
        assert_eq!(violation.measurement.actual().get(), 3);
    }
}

mod when_source_exceeds_the_production_file_limit {
    use super::*;

    #[test]
    fn it_reports_a_file_loc_violation() {
        let tempdir = tempfile::tempdir().unwrap();
        write_config(
            tempdir.path(),
            indoc! {r#"
                [limits.max_lines_per_file]
                production = 2

                [languages.rust]
                enabled = true

                [traversal]
                error_policy = "warn"
            "#},
        );
        write_rust_source_with_function_count(tempdir.path(), "src/lib.rs", 3);

        let report = check_root(tempdir.path()).unwrap();

        let violations = file_violations(&report);
        assert_eq!(report.status(), CheckStatus::Failed);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].kind, FileViolationKind::FileLoc);
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
                max_file_bytes = 8

                [languages.rust]
                enabled = true
            "#},
        );
        write_file(tempdir.path(), "src/lib.rs", "fn too_large() {}\n");

        let error = check_root(tempdir.path()).unwrap_err();

        assert!(matches!(
            error,
            crate::SprawlError::SourceFileTooLarge { limit: 8, .. }
        ));
    }

    #[test]
    fn it_allows_a_source_equal_to_the_byte_size_limit() {
        let tempdir = tempfile::tempdir().unwrap();
        write_config(
            tempdir.path(),
            indoc! {r#"
                [traversal]
                max_file_bytes = 10

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

    use crate::classification::TraversalIncidentSeverity;

    use super::*;

    #[test]
    fn it_collects_incidents_before_failing_in_fail_mode() {
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
                error_policy = "warn"

                [languages.rust]
                enabled = true
            "#},
        );
        write_file(tempdir.path(), "real/lib.rs", "fn a() {}\n");
        symlink("..", tempdir.path().join("real/cycle")).unwrap();

        let report = check_root(tempdir.path()).unwrap();

        assert_eq!(report.status(), CheckStatus::Passed);
        assert!(!report.analysis_complete());
        assert_eq!(report.traversal_incidents().len(), 1);
        assert_eq!(
            report.traversal_incidents()[0].severity,
            TraversalIncidentSeverity::Warning
        );
    }
}

mod when_test_code_exceeds_the_test_file_limit {
    use super::*;

    #[test]
    fn it_reports_a_test_file_loc_violation() {
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

        let violation = single_failed_file_violation(&report, FileViolationKind::TestFileLoc);
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

        let violation = single_failed_file_violation(&report, FileViolationKind::FileLoc);
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

        let violation = single_failed_file_violation(&report, FileViolationKind::FileLoc);
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
            IncludedLineTypes::All,
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

        let violation = single_failed_file_violation(&report, FileViolationKind::TestFileLoc);
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
                included_line_types = ["code", "comments"]
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
        let violation = single_failed_file_violation(&report, FileViolationKind::FileLoc);
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

mod when_checking_directory_fan_out {
    use super::*;

    mod when_the_documentation_group_uses_its_default_limit {
        use super::*;

        #[test]
        fn it_does_not_enforce_the_global_production_limit() {
            let tempdir = tempfile::tempdir().unwrap();
            write_config(
                tempdir.path(),
                indoc! {r#"
                    [limits]
                    max_leaf_files_per_directory = { production = 1 }

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
                    max_leaf_files_per_directory = { production = 8 }

                    [[overrides]]
                    paths = ["docs/**"]
                    max_leaf_files_per_directory = { production = 1 }
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
                max_leaf_files_per_directory = { production = 1 }

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
        assert_eq!(violations[0].kind, DirectoryViolationKind::DirectoryFanout);
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
                max_leaf_files_per_directory = { production = 1 }

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
                max_leaf_files_per_directory = { production = 1 }

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
                max_leaf_files_per_directory = { production = 1, test = 1 }

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
                .all(|violation| violation.kind == DirectoryViolationKind::TestDirectoryFanout)
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
                max_leaf_files_per_directory = { production = 1 }

                [[overrides]]
                paths = ["src/**"]
                max_leaf_files_per_directory = { production = 2 }
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
                max_leaf_files_per_directory = { production = 1 }

                [[overrides]]
                paths = ["src/*"]
                max_leaf_files_per_directory = { production = 2 }
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
                max_leaf_files_per_directory = { test = 1 }

                [[overrides]]
                paths = ["tests/**"]
                max_leaf_files_per_directory = { test = 2 }
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
                max_leaf_files_per_directory = { production = 1, test = 1 }

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
                CheckViolation::DirectoryFanout(violation) => Some(violation),
                CheckViolation::FileLoc(_) | CheckViolation::Ratchet(_) => None,
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
                included_line_types = ["code", "comments"]
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
                included_line_types = ["code"]
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
                max_leaf_files_per_directory = { production = 1 }

                [languages.rust]
                enabled = true

                [[overrides]]
                paths = ["src/**"]
                max_leaf_files_per_directory = { production = 2 }
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
    fn it_does_not_apply_language_file_limits_to_directory_fan_out() {
        let tempdir = tempfile::tempdir().unwrap();
        write_config(
            tempdir.path(),
            indoc! {r#"
                [limits]
                max_leaf_files_per_directory = { production = 1 }

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
        assert_eq!(violation.kind, DirectoryViolationKind::DirectoryFanout);
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

        let report = check_scopes(tempdir.path(), vec![PathBuf::from("checked")]).unwrap();

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
            crate::config::TraversalErrorPolicy::Warn,
        )
        .unwrap_err();

        assert!(error.to_string().contains("check scope does not exist"));
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

        let report = check_scopes(tempdir.path(), vec![PathBuf::from("checked")]).unwrap();

        let violations = file_violations(&report);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].path.as_str(), "checked/lib.rs");
    }
}

mod when_checking_with_a_ratchet_file {
    use super::*;

    fn write_file_ratchet(root: &Path, path: &str, non_test_code_lines: u32) {
        write_ratchet(
            root,
            &formatdoc! {r#"
                version = 1

                [[files]]
                path = "{path}"
                non_test_code_lines = {non_test_code_lines}
            "#},
        );
    }

    fn write_rust_directory_ratchet_fixture(root: &Path, leaf_source_files: u32) {
        write_config(
            root,
            indoc! {r#"
                [limits]
                max_leaf_files_per_directory = { production = 1 }

                [languages.rust]
                enabled = true
            "#},
        );
        write_ratchet(
            root,
            &formatdoc! {r#"
                version = 1

                [[directories]]
                path = "src"
                group = "source"
                leaf_source_files = {leaf_source_files}
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
        let ratchet_violations = ratchet_violations(&report);
        assert_eq!(report.status(), CheckStatus::Failed);
        assert_eq!(file_violations.len(), 1);
        assert_eq!(file_violations[0].ratchet.unwrap().get(), 2);
        assert_eq!(ratchet_violations.len(), 1);
        assert_eq!(
            ratchet_violations[0].kind(),
            RatchetViolationKind::FileLocIncreased
        );
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
            RatchetViolationKind::FileLocStale
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
            RatchetViolationKind::FileLocStale
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
                version = 1

                [[files]]
                path = "checked/lib.rs"
                non_test_code_lines = 2

                [[files]]
                path = "skipped/deleted.rs"
                non_test_code_lines = 2
            "#},
        );
        write_rust_source_with_function_count(tempdir.path(), "checked/lib.rs", 2);

        let report = check_scopes(tempdir.path(), vec![PathBuf::from("checked")]).unwrap();

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
            write_rust_directory_fanout_fixture(tempdir.path());
            write_ratchet(
                tempdir.path(),
                indoc! {r#"
                    version = 1

                    [[directories]]
                    path = "src"
                    group = "documentation"
                    leaf_source_files = 2
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
        fn it_carries_the_source_group_through_the_diagnostic() {
            let tempdir = tempfile::tempdir().unwrap();
            write_rust_directory_ratchet_fixture(tempdir.path(), 1);

            let report = check_root(tempdir.path()).unwrap();

            let ratchets = ratchet_violations(&report);
            let [ratchet] = ratchets.as_slice() else {
                panic!("expected one ratchet violation");
            };
            let RatchetViolation::Increased(crate::RatchetIncreasedViolation::Directory(violation)) =
                ratchet
            else {
                panic!("expected an increased directory ratchet");
            };
            assert_eq!(violation.group, crate::LanguageGroupId::Source);
            assert_eq!(violation.actual.get(), 2);
            assert_eq!(
                serde_json::to_value(ratchet).unwrap()["group"],
                serde_json::json!("source")
            );
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
                    version = 1

                    [[directories]]
                    path = "docs"
                    group = "documentation"
                    leaf_source_files = 3
                "#},
            );
            write_file(tempdir.path(), "docs/a.md", "# A\n");
            write_file(tempdir.path(), "docs/b.md", "# B\n");

            let report = check_root(tempdir.path()).unwrap();

            let ratchets = ratchet_violations(&report);
            let [ratchet] = ratchets.as_slice() else {
                panic!("expected one ratchet violation");
            };
            let RatchetViolation::Stale(crate::RatchetStaleViolation::Measured(
                crate::RatchetStaleMeasuredViolation::Directory(violation),
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
                check_path_tests = false

                [limits]
                max_lines_per_file = { test = 1 }
                max_leaf_files_per_directory = { test = 1 }
            "#},
        );
        write_ratchet(
            tempdir.path(),
            indoc! {r#"
                version = 1

                [[test_files]]
                path = "tests/integration.rs"
                code_lines = 2

                [[test_directories]]
                path = "tests"
                group = "source"
                leaf_test_files = 1
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
