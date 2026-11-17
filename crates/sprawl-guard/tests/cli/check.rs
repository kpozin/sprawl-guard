use assert_fs::TempDir;
use indoc::indoc;
use serde_json::json;

use super::common::*;

mod github;
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
                "{}No sprawl-guard violations found.\n{}",
                AUTHORED_CONFIG_HEADER,
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
                "ratchet_entries": 0,
            }),
        );
        assert_eq!(
            report["config_source"],
            json!({
                "kind": "authored",
                "path": root.path().canonicalize().unwrap().join("sprawl-guard.toml").display().to_string(),
            })
        );
        assert_eq!(stderr(&output), "");
    }
}

mod when_the_working_directory_changes {
    use super::*;

    fn write_root_config_fixture(root: &TempDir) {
        write_rust_file_limit_config(root, 1);
        write_rust_source_with_function_count(root, "src/lib.rs", 2);
    }

    #[test]
    fn it_makes_built_in_fallback_visible_from_a_nested_directory() {
        let root = TempDir::new().unwrap();
        write_root_config_fixture(&root);
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
        assert!(
            stdout(&human).contains("Configuration: built-in defaults (no sprawl-guard.toml in ")
        );
    }

    #[test]
    fn it_honors_an_explicit_root_and_config_from_a_nested_directory() {
        let root = TempDir::new().unwrap();
        write_root_config_fixture(&root);
        let nested = root.path().join("src");

        let output = sprawl_guard_no_root()
            .current_dir(&nested)
            .args([
                "check",
                "--format",
                "json",
                "--config",
                "sprawl-guard.toml",
                "--root",
            ])
            .arg(root.path())
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
                "path": root.path().canonicalize().unwrap().join("sprawl-guard.toml").display().to_string(),
            })
        );
    }

    #[test]
    fn it_selects_an_intentional_nested_project_config() {
        let root = TempDir::new().unwrap();
        write_root_config_fixture(&root);
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
                "path": nested.canonicalize().unwrap().join("sprawl-guard.toml").display().to_string(),
            })
        );
    }
}

mod when_a_check_is_scoped {
    use super::*;

    fn write_sibling_direct_files_fixture(root: &TempDir) {
        write_config(
            root,
            indoc! {r#"
                [limits.max_direct_files_per_directory]
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
        write_sibling_direct_files_fixture(&root);

        let (scoped_output, scoped) =
            run_json(&root, &["check", "safe/lib.rs", "--format", "json"]);
        let (full_output, full) = run_check_json(&root);
        let human = sprawl_guard(&root)
            .args(["check", "safe/lib.rs"])
            .output()
            .unwrap();

        assert_exit_code(&scoped_output, exit_codes::SUCCESS);
        assert_eq!(scoped["paths"], json!(["safe/lib.rs"]));
        assert_exit_code(&full_output, exit_codes::CHECK_FAILURE);
        assert_eq!(full["paths"], json!("all"));
        assert_eq!(full["violations"][0]["kind"], "directory_direct_files");
        assert_exit_code(&human, exit_codes::SUCCESS);
        assert!(stdout(&human).starts_with(AUTHORED_CONFIG_HEADER));
        assert!(
            stdout(&human).contains("No sprawl-guard violations found in the selected paths.\n")
        );
    }
}

mod when_file_lines_is_reported_as_json {
    use super::*;

    #[test]
    fn it_includes_the_effective_line_types_and_complete_breakdown_for_each_file_role() {
        let root = TempDir::new().unwrap();
        write_config(
            &root,
            indoc! {r#"
                [limits.max_lines_per_file]
                count_lines = ["comments"]
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
                    Some("file_lines" | "test_file_lines")
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(file_violations.len(), 2);
        for violation in file_violations {
            assert_eq!(violation["count_lines"], json!(["comments"]));
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

    fn violations(root: &TempDir, ceiling: u32) -> Vec<serde_json::Value> {
        write_rust_file_limit_config_and_file_ratchet(root, 1, "src/lib.rs", ceiling);
        write_file(
            root,
            "src/lib.rs",
            "// comment\nfn one() {}\nfn two() {}\nfn three() {}\n",
        );
        let (_, json) = run_check_json(root);
        json["violations"].as_array().unwrap().clone()
    }

    #[test]
    fn it_reports_one_limit_violation_with_the_ceiling_when_the_file_grew() {
        let root = TempDir::new().unwrap();

        let violations = violations(&root, 2);

        let [violation] = violations.as_slice() else {
            panic!("expected one violation, got {violations:?}");
        };
        assert_eq!(
            json!({
                "kind": violation["kind"],
                "actual": violation["actual"],
                "limit": violation["limit"],
                "ratchet": violation["ratchet"],
            }),
            json!({
                "kind": "file_lines",
                "actual": 3,
                "limit": 1,
                "ratchet": 2,
            })
        );
    }

    #[test]
    fn it_includes_the_measurement_when_the_ceiling_is_stale() {
        let root = TempDir::new().unwrap();

        let violations = violations(&root, 4);
        let violation = violations
            .iter()
            .find(|violation| violation["kind"] == "file_lines_stale")
            .unwrap();

        assert_eq!(
            json!({
                "kind": violation["kind"],
                "count_lines": violation["count_lines"],
                "line_counts": violation["line_counts"],
                "actual": violation["actual"],
            }),
            json!({
                "kind": "file_lines_stale",
                "count_lines": [],
                "line_counts": {"code": 3, "comments": 1, "blank_lines": 0},
                "actual": 3,
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
            .find(|violation| violation["kind"] == "file_lines_stale")
            .unwrap();
        assert_eq!(file["path"], "src/pnpm-lock.yaml");
        assert_eq!(file["actual"], serde_json::Value::Null);
        let directory = violations
            .iter()
            .find(|violation| violation["kind"] == "directory_direct_files_stale")
            .unwrap();
        assert_eq!(directory["path"], "src");
        assert_eq!(directory["actual"], 1);
    }
}

mod when_rust_production_source_has_file_wide_cfg_test {
    use super::*;

    #[test]
    fn it_excludes_the_file_from_production_lines() {
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
                "{}No sprawl-guard violations found.\n{}",
                AUTHORED_CONFIG_HEADER, "Checked 1 source file in 1 directory across 1 language.\n",
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
mod when_unreadable_paths_is_overridden {
    use super::*;

    #[test]
    fn it_reports_partial_json_without_failing_on_traversal_warnings() {
        let root = TempDir::new().unwrap();
        write_directory_symlink_loop_fixture(&root);

        let (output, json) = run_json(
            &root,
            &["check", "--format", "json", "--unreadable-paths", "warn"],
        );

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(
            json["coverage"],
            json!({
                "kind": "partial",
                "unreadable_paths": [{
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
mod when_default_unreadable_paths_fails {
    use super::*;

    #[test]
    fn it_serializes_collected_unreadable_paths_for_json_output() {
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
                    "path": root.path().canonicalize().unwrap().join("sprawl-guard.toml").display().to_string(),
                },
                "coverage": {
                    "kind": "partial",
                    "unreadable_paths": [{
                        "detail": "symlink loop found",
                        "kind": "symlink_loop",
                        "path": "real/cycle",
                        "severity": "error",
                    }],
                },
                "warnings": [],
                "error": {
                    "kind": "traversal_failed",
                    "message": "could not read 1 path(s)",
                },
                "violations": [],
            })
        );
        assert_eq!(stderr(&output), "");
    }

    #[test]
    fn it_prints_collected_unreadable_paths_for_human_output() {
        let root = TempDir::new().unwrap();
        write_directory_symlink_loop_fixture(&root);

        let output = sprawl_guard(&root).arg("check").output().unwrap();
        let stderr = stderr(&output);

        assert_exit_code(&output, exit_codes::OPERATIONAL_ERROR);
        assert_eq!(stdout(&output), "");
        assert!(stderr.starts_with(AUTHORED_CONFIG_HEADER));
        assert!(stderr.contains("Could not read 1 path(s) under "));
        assert!(stderr.contains("symlink loop found at real/cycle"));
    }
}

mod when_production_directory_exceeds_the_direct_file_limit {
    use super::*;

    #[test]
    fn it_serializes_a_directory_violation_in_json_output() {
        let root = TempDir::new().unwrap();
        write_rust_directory_direct_files_fixture(&root);

        let (output, json) = run_check_json(&root);

        assert_exit_code(&output, exit_codes::CHECK_FAILURE);
        assert_eq!(
            json["violations"][0],
            json!({
                "kind": "directory_direct_files",
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
                        "text": "src/lib.rs has 2 lines (counting code), over the limit of 1.",
                    },
                    "properties": {
                        "actual": 2,
                        "count_lines": [],
                        "kind": "file_lines",
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
                    "ruleId": "sprawl-guard/file-lines",
                },
            })
        );
        assert_eq!(stderr(&output), "");
    }
}

mod when_the_config_file_is_under_the_root {
    use super::*;

    const CONFIG: &str = "schema_version = 2\n\n[limits.max_lines_per_file]\nproduction = 1\n";

    #[test]
    fn it_does_not_check_the_default_config_as_source() {
        let root = TempDir::new().unwrap();
        write_file(&root, "sprawl-guard.toml", CONFIG);

        let (output, report) = run_json(&root, &["check", "--format", "json"]);

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(report["summary"]["files_checked"], json!(0));
    }

    #[test]
    fn it_does_not_check_an_explicit_config_as_source() {
        let root = TempDir::new().unwrap();
        write_file(&root, "config/custom.toml", CONFIG);

        let (output, report) = run_json(
            &root,
            &[
                "check",
                "--format",
                "json",
                "--config",
                "config/custom.toml",
            ],
        );

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(report["summary"]["files_checked"], json!(0));
    }

    #[cfg(unix)]
    #[test]
    fn it_does_not_check_a_symlinked_config_as_source() {
        let root = TempDir::new().unwrap();
        let shared = TempDir::new().unwrap();
        write_file(
            &shared,
            "shared.toml",
            &format!("{CONFIG}\n[traversal]\nfollow_symlinks = \"files\"\n"),
        );
        std::os::unix::fs::symlink(
            shared.path().join("shared.toml"),
            root.path().join("sprawl-guard.toml"),
        )
        .unwrap();

        let (output, report) = run_json(&root, &["check", "--format", "json"]);

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(report["summary"]["files_checked"], json!(0));
    }

    #[cfg(unix)]
    #[test]
    fn it_does_not_check_a_config_under_a_symlinked_directory_as_source() {
        let root = TempDir::new().unwrap();
        let shared = TempDir::new().unwrap();
        write_file(
            &shared,
            "sprawl.toml",
            &format!("{CONFIG}\n[traversal]\nfollow_symlinks = \"directories\"\n"),
        );
        std::os::unix::fs::symlink(shared.path(), root.path().join("shared")).unwrap();

        let (output, report) = run_json(
            &root,
            &[
                "check",
                "--config",
                "shared/sprawl.toml",
                "--format",
                "json",
            ],
        );

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(report["summary"]["files_checked"], json!(0));
    }

    #[test]
    fn explain_reports_the_config_exclusion() {
        let root = TempDir::new().unwrap();
        write_file(&root, "sprawl-guard.toml", CONFIG);

        let (output, report) =
            run_json(&root, &["explain", "--format", "json", "sprawl-guard.toml"]);

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(report["kind"], "config_excluded");
    }
}

mod when_a_check_passes_with_ratchet_entries {
    use super::*;

    #[test]
    fn it_reports_how_many_entries_remain() {
        let root = TempDir::new().unwrap();
        write_config(&root, "[limits.max_lines_per_file]\nproduction = 1\n");
        write_file(&root, "src/lib.rs", "fn first() {}\nfn second() {}\n");
        let update = sprawl_guard(&root)
            .args(["update", "--write"])
            .output()
            .unwrap();
        assert_exit_code(&update, exit_codes::SUCCESS);

        let output = sprawl_guard(&root).arg("check").output().unwrap();

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert!(stdout(&output).ends_with("Passed with 1 ratchet entry remaining.\n"));
    }
}
