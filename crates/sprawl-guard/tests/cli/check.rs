use assert_fs::TempDir;
use indoc::indoc;
use serde_json::json;

use super::common::*;

mod ignore;

mod when_a_check_succeeds {
    use super::*;

    fn write_mixed_source_fixture(root: &TempDir) {
        write_config(root, "");
        write_file(root, "src/lib.rs", "fn main() {}\n");
        write_file(root, "web/app.ts", "export const answer = 42;\n");
    }

    #[test]
    fn it_reports_the_checked_files_directories_and_languages_in_human_output() {
        let root = TempDir::new().unwrap();
        write_mixed_source_fixture(&root);

        let output = sprawl_guard(&root).arg("check").output().unwrap();

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(
            stdout(&output),
            format!(
                "Policy: authored config at {}.\n\n{}{}",
                root.path().join("sprawl-guard.toml").display(),
                "No sprawl-guard violations found.\n",
                "Checked 2 source files in 2 directories across 2 languages.\n",
            ),
        );
        assert_eq!(stderr(&output), "");
    }

    #[test]
    fn it_reports_the_checked_files_directories_and_languages_in_json_output() {
        let root = TempDir::new().unwrap();
        write_mixed_source_fixture(&root);

        let (output, report) = run_check_json(&root);

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(
            report["summary"],
            json!({
                "files_checked": 2,
                "directories_checked": 2,
                "languages_detected": 2,
            }),
        );
        assert_eq!(
            report["config_source"],
            json!({
                "kind": "authored",
                "path": root.path().join("sprawl-guard.toml").display().to_string(),
            })
        );
        assert_eq!(stderr(&output), "");
    }
}

mod when_the_working_directory_changes {
    use super::*;

    fn write_root_policy_fixture(root: &TempDir) {
        write_rust_file_limit_config(root, 1);
        write_rust_source_with_function_count(root, "src/lib.rs", 2);
    }

    #[test]
    fn it_makes_built_in_fallback_visible_from_a_nested_directory() {
        let root = TempDir::new().unwrap();
        write_root_policy_fixture(&root);
        let nested = root.path().join("src");

        let output = sprawl_guard_no_root()
            .current_dir(&nested)
            .args(["check", "--format", "json"])
            .output()
            .unwrap();
        let report = json_stdout(&output);
        let human = sprawl_guard_no_root()
            .current_dir(&nested)
            .arg("check")
            .output()
            .unwrap();

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(
            report["root"],
            nested.canonicalize().unwrap().display().to_string()
        );
        assert_eq!(report["config_source"], json!({"kind": "built_in"}));
        assert!(stdout(&human).contains("Policy: built-in defaults"));
    }

    #[test]
    fn it_honors_an_explicit_root_and_config_from_a_nested_directory() {
        let root = TempDir::new().unwrap();
        write_root_policy_fixture(&root);
        let nested = root.path().join("src");

        let output = sprawl_guard_no_root()
            .current_dir(&nested)
            .arg("--root")
            .arg(root.path())
            .args(["--config", "sprawl-guard.toml", "check", "--format", "json"])
            .output()
            .unwrap();
        let report = json_stdout(&output);

        assert_exit_code(&output, exit_codes::CHECK_FAILURE);
        assert_eq!(
            report["root"],
            root.path().canonicalize().unwrap().display().to_string()
        );
        assert_eq!(
            report["config_source"],
            json!({
                "kind": "authored",
                "path": root.path().join("sprawl-guard.toml").display().to_string(),
            })
        );
    }

    #[test]
    fn it_selects_an_intentional_nested_project_config() {
        let root = TempDir::new().unwrap();
        write_root_policy_fixture(&root);
        let nested = root.path().join("src");
        write_file(
            &root,
            "src/sprawl-guard.toml",
            indoc! {r#"
                schema_version = 2

                [limits.max_lines_per_file]
                production = 1

                [languages.rust]
                enabled = true
            "#},
        );

        let output = sprawl_guard_no_root()
            .current_dir(&nested)
            .args(["check", "--format", "json"])
            .output()
            .unwrap();
        let report = json_stdout(&output);

        assert_exit_code(&output, exit_codes::CHECK_FAILURE);
        assert_eq!(
            report["config_source"],
            json!({
                "kind": "authored",
                "path": nested.join("sprawl-guard.toml").display().to_string(),
            })
        );
    }
}

mod when_a_check_is_scoped {
    use super::*;

    fn write_sibling_fanout_fixture(root: &TempDir) {
        write_config(
            root,
            indoc! {r#"
                [limits.max_leaf_files_per_directory]
                production = 1

                [languages.rust]
                enabled = true
            "#},
        );
        write_file(root, "safe/lib.rs", "fn safe() {}\n");
        write_file(root, "crowded/a.rs", "fn a() {}\n");
        write_file(root, "crowded/b.rs", "fn b() {}\n");
    }

    #[test]
    fn it_distinguishes_scoped_success_from_a_failing_full_repository_check() {
        let root = TempDir::new().unwrap();
        write_sibling_fanout_fixture(&root);

        let (scoped_output, scoped) =
            run_json(&root, &["check", "safe/lib.rs", "--format", "json"]);
        let (full_output, full) = run_check_json(&root);
        let human = sprawl_guard(&root)
            .args(["check", "safe/lib.rs"])
            .output()
            .unwrap();

        assert_exit_code(&scoped_output, exit_codes::SUCCESS);
        assert_eq!(
            scoped["scope"],
            json!({"kind": "paths", "paths": ["safe/lib.rs"]})
        );
        assert_exit_code(&full_output, exit_codes::CHECK_FAILURE);
        assert_eq!(full["scope"], json!({"kind": "all"}));
        assert_eq!(full["violations"][0]["kind"], "directory_fanout");
        assert_exit_code(&human, exit_codes::SUCCESS);
        assert!(stdout(&human).contains("Policy: authored config at"));
        assert!(
            stdout(&human).contains("No sprawl-guard violations found in the requested scope.\n")
        );
    }
}

mod when_file_loc_is_reported_as_json {
    use super::*;

    #[test]
    fn it_includes_the_effective_line_types_and_complete_breakdown_for_each_file_role() {
        let root = TempDir::new().unwrap();
        write_config(
            &root,
            indoc! {r#"
                [limits.max_lines_per_file]
                included_line_types = ["code", "comments"]
                production = 1
                test = 1
            "#},
        );
        write_file(
            &root,
            "src/lib.rs",
            "// production\nfn one() {}\nfn two() {}\n",
        );
        write_file(&root, "tests/lib.rs", "// test\nfn one() {}\nfn two() {}\n");

        let (output, json) = run_check_json(&root);

        assert_exit_code(&output, exit_codes::CHECK_FAILURE);
        let file_violations = json["violations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|violation| {
                matches!(
                    violation["kind"].as_str(),
                    Some("file_loc" | "test_file_loc")
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(file_violations.len(), 2);
        for violation in file_violations {
            assert_eq!(
                violation["included_line_types"],
                json!(["code", "comments"])
            );
            assert_eq!(
                violation["line_counts"],
                json!({"code": 2, "comments": 1, "blank_lines": 0})
            );
            assert_eq!(violation["actual"], 3);
        }
    }
}

mod when_a_measured_file_ratchet_is_reported_as_json {
    use super::*;

    fn ratchet_violation(root: &TempDir, ceiling: u32) -> serde_json::Value {
        write_rust_file_limit_config_and_file_ratchet(root, 1, "src/lib.rs", ceiling);
        write_file(root, "src/lib.rs", "// comment\nfn one() {}\nfn two() {}\n");
        let (_, json) = run_check_json(root);
        json["violations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|violation| {
                let kind = violation["kind"].as_str().unwrap();
                kind.ends_with("_increased") || kind.ends_with("_stale")
            })
            .unwrap()
            .clone()
    }

    #[test]
    fn it_includes_the_measurement_when_the_ceiling_increased() {
        let root = TempDir::new().unwrap();

        let violation = ratchet_violation(&root, 1);

        assert_eq!(
            json!({
                "kind": violation["kind"],
                "included_line_types": violation["included_line_types"],
                "line_counts": violation["line_counts"],
                "actual": violation["actual"],
            }),
            json!({
                "kind": "file_loc_increased",
                "included_line_types": ["code"],
                "line_counts": {"code": 2, "comments": 1, "blank_lines": 0},
                "actual": 2,
            })
        );
    }

    #[test]
    fn it_includes_the_measurement_when_the_ceiling_is_stale() {
        let root = TempDir::new().unwrap();

        let violation = ratchet_violation(&root, 3);

        assert_eq!(
            json!({
                "kind": violation["kind"],
                "included_line_types": violation["included_line_types"],
                "line_counts": violation["line_counts"],
                "actual": violation["actual"],
            }),
            json!({
                "kind": "file_loc_stale",
                "included_line_types": ["code"],
                "line_counts": {"code": 2, "comments": 1, "blank_lines": 0},
                "actual": 2,
            })
        );
    }
}

mod when_a_dependency_lockfile_has_existing_ratchet_entries {
    use super::*;

    #[test]
    fn it_reports_the_file_as_absent_and_the_directory_with_its_reduced_measurement() {
        let root = TempDir::new().unwrap();
        write_dependency_lockfile_ratchet_fixture(&root);

        let (output, report) = run_check_json(&root);

        assert_exit_code(&output, exit_codes::CHECK_FAILURE);
        let violations = report["violations"].as_array().unwrap();
        let file = violations
            .iter()
            .find(|violation| violation["kind"] == "file_loc_stale")
            .unwrap();
        assert_eq!(file["path"], "src/pnpm-lock.yaml");
        assert_eq!(file["actual"], serde_json::Value::Null);
        let directory = violations
            .iter()
            .find(|violation| violation["kind"] == "directory_fanout_stale")
            .unwrap();
        assert_eq!(directory["path"], "src");
        assert_eq!(directory["actual"], 1);
    }
}

mod when_rust_production_source_has_file_wide_cfg_test {
    use super::*;

    #[test]
    fn it_excludes_the_file_from_production_loc() {
        let root = TempDir::new().unwrap();
        write_config(
            &root,
            indoc! {r#"
                [limits.max_lines_per_file]
                production = 1

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
        assert_eq!(
            stdout(&output),
            format!(
                "Policy: authored config at {}.\n\n{}{}",
                root.path().join("sprawl-guard.toml").display(),
                "No sprawl-guard violations found.\n",
                "Checked 1 source file in 1 directory across 1 language.\n",
            )
        );
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
                production = 1

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
                "config_source": {
                    "kind": "authored",
                    "path": root.path().join("sprawl-guard.toml").display().to_string(),
                },
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
        assert!(stderr.contains("Policy: authored config at"));
        assert!(stderr.contains("Traversal failed under"));
        assert!(stderr.contains("symlink loop found at real/cycle"));
    }
}

mod when_production_directory_exceeds_the_leaf_file_limit {
    use super::*;

    #[test]
    fn it_serializes_a_directory_violation_in_json_output() {
        let root = TempDir::new().unwrap();
        write_rust_directory_fanout_fixture(&root);

        let (output, json) = run_check_json(&root);

        assert_exit_code(&output, exit_codes::CHECK_FAILURE);
        assert_eq!(
            json["violations"][0],
            json!({
                "kind": "directory_fanout",
                "group": "source",
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
                        "text": "src/lib.rs has 2 configured LOC (included line types: code), exceeding limit 1.",
                    },
                    "properties": {
                        "actual": 2,
                        "included_line_types": ["code"],
                        "kind": "file_loc",
                        "language": "Rust",
                        "line_counts": {
                            "code": 2,
                            "comments": 0,
                            "blank_lines": 0,
                        },
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
