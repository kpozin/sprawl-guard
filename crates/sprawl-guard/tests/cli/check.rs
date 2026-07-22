use assert_fs::TempDir;
use indoc::indoc;
use serde_json::json;

use super::common::*;

mod when_rust_production_source_has_file_wide_cfg_test {
    use super::*;

    #[test]
    fn it_excludes_the_file_from_production_loc() {
        let root = TempDir::new().unwrap();
        write_config(
            &root,
            indoc! {r#"
                [limits.max_lines_per_file]
                code_non_test = 1

                [languages.rust]
                enabled = true
            "#},
        );
        write_file(
            &root,
            "src/lib.rs",
            indoc! {r#"
                #![cfg(test)]

                fn fixture_one() {}
                fn fixture_two() {}
            "#},
        );

        let output = sprawl_guard(&root).arg("check").output().unwrap();

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(stdout(&output), "No sprawl-guard violations found.\n");
        assert_eq!(stderr(&output), "");
    }
}

mod when_rust_cfg_test_preprocessing_fails {
    use super::*;

    #[test]
    fn it_serializes_a_structured_warning_in_json_output() {
        let root = TempDir::new().unwrap();
        write_config(
            &root,
            indoc! {r#"
                [limits.max_lines_per_file]
                code_non_test = 1

                [languages.rust]
                enabled = true
            "#},
        );
        write_file(
            &root,
            "src/lib.rs",
            indoc! {r#"
                #[cfg(test)]
                fn broken( {
            "#},
        );

        let (output, json) = run_check_json(&root);

        assert_exit_code(&output, exit_codes::CHECK_FAILURE);
        let warning = &json["warnings"][0];
        let detail = warning["detail"].as_str().unwrap();
        assert!(!detail.is_empty());
        assert_eq!(
            warning,
            &json!({
                "detail": detail,
                "kind": "rust_cfg_test_parse_failed",
                "path": "src/lib.rs",
            })
        );
        assert_eq!(stderr(&output), "");
    }
}

#[cfg(unix)]
mod when_traversal_error_policy_is_overridden {
    use super::*;

    #[test]
    fn it_reports_partial_json_without_failing_on_traversal_warnings() {
        let root = TempDir::new().unwrap();
        write_directory_symlink_loop_fixture(&root);

        let (output, json) = run_json(
            &root,
            &[
                "check",
                "--format",
                "json",
                "--traversal-error-policy",
                "warn",
            ],
        );

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(
            json["coverage"],
            json!({
                "kind": "partial",
                "traversal_incidents": [{
                    "detail": "symlink loop found",
                    "kind": "symlink_loop",
                    "path": "real/cycle",
                    "severity": "warning",
                }],
            })
        );
        assert_eq!(stderr(&output), "");
    }
}

#[cfg(unix)]
mod when_default_traversal_error_policy_fails {
    use super::*;

    #[test]
    fn it_serializes_collected_incidents_for_json_output() {
        let root = TempDir::new().unwrap();
        write_directory_symlink_loop_fixture(&root);

        let (output, json) = run_check_json(&root);

        assert_exit_code(&output, exit_codes::OPERATIONAL_ERROR);
        assert_eq!(
            json,
            json!({
                "status": "error",
                "root": root.path().canonicalize().unwrap().display().to_string(),
                "coverage": {
                    "kind": "partial",
                    "traversal_incidents": [{
                        "detail": "symlink loop found",
                        "kind": "symlink_loop",
                        "path": "real/cycle",
                        "severity": "error",
                    }],
                },
                "warnings": [],
                "error": {
                    "kind": "traversal_failed",
                    "message": "traversal completed with 1 incident(s)",
                },
                "violations": [],
            })
        );
        assert_eq!(stderr(&output), "");
    }

    #[test]
    fn it_prints_collected_incidents_for_human_output() {
        let root = TempDir::new().unwrap();
        write_directory_symlink_loop_fixture(&root);

        let output = sprawl_guard(&root).arg("check").output().unwrap();
        let stderr = stderr(&output);

        assert_exit_code(&output, exit_codes::OPERATIONAL_ERROR);
        assert_eq!(stdout(&output), "");
        assert!(stderr.contains("Traversal failed under"));
        assert!(stderr.contains("symlink loop found at real/cycle"));
    }
}

mod when_production_directory_exceeds_the_leaf_file_limit {
    use super::*;

    #[test]
    fn it_serializes_a_directory_violation_in_json_output() {
        let root = TempDir::new().unwrap();
        write_config(
            &root,
            indoc! {r#"
                [limits]
                max_leaf_files_per_directory = { production = 1 }

                [languages.rust]
                enabled = true
            "#},
        );
        write_file(&root, "src/a.rs", "fn a() {}\n");
        write_file(&root, "src/b.rs", "fn b() {}\n");

        let (output, json) = run_check_json(&root);

        assert_exit_code(&output, exit_codes::CHECK_FAILURE);
        assert_eq!(
            json["violations"][0],
            json!({
                "kind": "directory_fanout",
                "path": "src",
                "actual": 2,
                "limit": 1,
                "files": ["src/a.rs", "src/b.rs"],
                "ratchet": null,
            })
        );
        assert_eq!(stderr(&output), "");
    }
}

mod when_sarif_output_is_requested {
    use super::*;

    #[test]
    fn it_serializes_check_violations_as_sarif_results() {
        let root = TempDir::new().unwrap();
        write_rust_file_limit_config(&root, 1);
        write_rust_source_with_function_count(&root, "src/lib.rs", 2);

        let output = sprawl_guard(&root)
            .args(["check", "--format", "sarif"])
            .output()
            .unwrap();
        let sarif = serde_json::from_str::<serde_json::Value>(&stdout(&output)).unwrap();

        assert_exit_code(&output, exit_codes::CHECK_FAILURE);
        assert_eq!(
            json!({
                "version": sarif["version"],
                "semantic_version": sarif["runs"][0]["tool"]["driver"]["semanticVersion"],
                "result": sarif["runs"][0]["results"][0],
            }),
            json!({
                "version": "2.1.0",
                "semantic_version": env!("CARGO_PKG_VERSION"),
                "result": {
                    "level": "error",
                    "locations": [{
                        "physicalLocation": {
                            "artifactLocation": {
                                "uri": "src/lib.rs",
                            },
                            "region": {
                                "startLine": 1,
                            },
                        },
                    }],
                    "message": {
                        "text": "src/lib.rs has 2 code lines, exceeding limit 1.",
                    },
                    "properties": {
                        "actual": 2,
                        "kind": "file_loc",
                        "language": "Rust",
                        "limit": 1,
                        "path": "src/lib.rs",
                        "ratchet": null,
                    },
                    "ruleId": "sprawl-guard/file-loc",
                },
            })
        );
        assert_eq!(stderr(&output), "");
    }
}
