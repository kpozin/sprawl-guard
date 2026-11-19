use std::path::Path;

use assert_fs::TempDir;
use indoc::indoc;

use super::common::*;

mod when_explaining_the_configured_ratchet {
    use super::*;

    fn write_fixture(root: &TempDir) {
        write_config(
            root,
            indoc! {r#"
                [ratchet]
                file = "./nested//ratchet.toml"
            "#},
        );
        write_file(root, "nested/ratchet.toml", "schema_version = 1\n");
    }

    #[test]
    fn it_reports_the_control_file_reason_for_humans() {
        let root = TempDir::new().unwrap();
        write_fixture(&root);

        let output = sprawl_guard(&root)
            .args(["explain", "nested/ratchet.toml"])
            .output()
            .unwrap();

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(
            stdout(&output),
            "nested/ratchet.toml: excluded because it is the ratchet file\n"
        );
    }

    #[test]
    fn it_reports_the_control_file_reason_as_structured_output() {
        let root = TempDir::new().unwrap();
        write_fixture(&root);

        let (output, json) = run_json(
            &root,
            &["explain", "--format", "json", "nested/ratchet.toml"],
        );

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(
            json,
            serde_json::json!({
                "kind": "ratchet_excluded",
                "path": "nested/ratchet.toml",
            })
        );
    }
}

mod when_explaining_rust_cfg_test_preprocessing {
    use super::*;

    #[test]
    fn it_reports_the_status_and_excluded_item_count() {
        let root = TempDir::new().unwrap();
        write_config(
            &root,
            indoc! {r#"
                [languages.rust]
                enabled = true
            "#},
        );
        write_file(
            &root,
            "src/lib.rs",
            indoc! {r#"
                fn production() {}

                #[cfg(test)]
                fn fixture() {}
            "#},
        );

        let output = sprawl_guard(&root)
            .arg("explain")
            .arg(Path::new("src/lib.rs"))
            .output()
            .unwrap();

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(
            stdout(&output),
            indoc! {"
                src/lib.rs
                  kind: production source
                  language: Rust
                  group: source
                  lines: 1 (limit 400, counting code)
                  line counts: 1 code, 0 comments, 1 blank
                  direct files limit for its directory: 10
                  Rust cfg(test): parsed, 1 top-level item not counted
                  size: 49 bytes, limit unlimited
            "}
        );
        assert_eq!(stderr(&output), "");
    }

    #[test]
    fn it_serializes_the_measured_source_state_and_complete_loc_measurement() {
        let root = TempDir::new().unwrap();
        write_file(&root, "src/lib.rs", "// comment\nfn production() {}\n");

        let (output, json) = run_json(&root, &["explain", "--format", "json", "src/lib.rs"]);

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(
            serde_json::json!({
                "kind": json["kind"],
                "state": json["state"],
                "count_lines": json["measurement"]["count_lines"],
                "line_counts": json["measurement"]["line_counts"],
                "actual": json["measurement"]["actual"],
            }),
            serde_json::json!({
                "kind": "source",
                "state": "measured",
                "count_lines": [],
                "line_counts": {"code": 1, "comments": 1, "blank_lines": 0},
                "actual": 1,
            })
        );
    }
}

mod when_explaining_matching_overrides {
    use super::*;

    #[test]
    fn it_reports_the_final_limits_and_override_stack() {
        let root = TempDir::new().unwrap();
        write_config(
            &root,
            indoc! {r#"
                [limits]
                max_lines_per_file = { production = 1 }
                max_direct_files_per_directory = { production = 1 }

                [languages.rust]
                enabled = true

                [[overrides]]
                paths = ["src/**"]
                max_lines_per_file = { production = 2 }
                max_direct_files_per_directory = { production = 3 }
                reason = "package exception"

                [[overrides]]
                paths = ["src/legacy/**"]
                max_lines_per_file = { production = 4 }
                reason = "legacy exception"
            "#},
        );
        write_file(&root, "src/legacy/lib.rs", "fn production() {}\n");

        let output = sprawl_guard(&root)
            .arg("explain")
            .arg(Path::new("src/legacy/lib.rs"))
            .output()
            .unwrap();

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(
            stdout(&output),
            indoc! {"
                src/legacy/lib.rs
                  kind: production source
                  language: Rust
                  group: source
                  lines: 1 (limit 4, counting code)
                  line counts: 1 code, 0 comments, 0 blank
                  direct files limit for its directory: 3
                  line limit overrides: #1 paths=[src/**], #2 paths=[src/legacy/**]
                  direct files overrides: #1 paths=[src/**], #2 paths=[src/legacy/**]
                  Rust cfg(test): skipped because the file never mentions cfg(test), 0 top-level items not counted
                  size: 19 bytes, limit unlimited
            "}
        );
        assert_eq!(stderr(&output), "");
    }

    #[test]
    fn it_reports_directory_only_override_matches() {
        let root = TempDir::new().unwrap();
        write_config(
            &root,
            indoc! {r#"
                [limits]
                max_direct_files_per_directory = { production = 1 }

                [languages.rust]
                enabled = true

                [[overrides]]
                paths = ["src"]
                max_direct_files_per_directory = { production = 3 }
                reason = "small module hub"
            "#},
        );
        write_file(&root, "src/lib.rs", "fn production() {}\n");

        let output = sprawl_guard(&root)
            .arg("explain")
            .arg(Path::new("src/lib.rs"))
            .output()
            .unwrap();

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(
            stdout(&output),
            indoc! {"
                src/lib.rs
                  kind: production source
                  language: Rust
                  group: source
                  lines: 1 (limit 400, counting code)
                  line counts: 1 code, 0 comments, 0 blank
                  direct files limit for its directory: 3
                  direct files overrides: #1 paths=[src]
                  Rust cfg(test): skipped because the file never mentions cfg(test), 0 top-level items not counted
                  size: 19 bytes, limit unlimited
            "}
        );
        assert_eq!(stderr(&output), "");
    }
}

mod when_explaining_a_missing_path {
    use super::*;

    #[test]
    fn it_says_the_path_was_classified_by_path_only() {
        let root = TempDir::new().unwrap();

        let output = sprawl_guard(&root)
            .args(["explain", "src/planned.rs"])
            .output()
            .unwrap();

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert!(stdout(&output).contains("state: not found, so classified by its path only"));
        assert_eq!(stderr(&output), "");
    }

    #[test]
    fn it_reports_the_not_found_state_as_structured_output() {
        let root = TempDir::new().unwrap();

        let (output, json) = run_json(&root, &["explain", "--format", "json", "src/planned.rs"]);

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(json["kind"], "source");
        assert_eq!(json["state"], "not_found");
        assert!(json.get("measurement").is_none());
    }
}

mod when_explaining_a_directory {
    use super::*;

    fn write_fixture(root: &TempDir) {
        write_config(
            root,
            indoc! {r#"
                [languages.rust]
                enabled = true

                [[overrides]]
                paths = ["src"]
                max_direct_files_per_directory = { production = 1 }
                reason = "flat module"
            "#},
        );
        write_file(
            root,
            "sprawl-guard.ratchet.toml",
            indoc! {r#"
                schema_version = 1

                [[directories]]
                path = "src"
                role = "production"
                group = "source"
                max_direct_files = 2
            "#},
        );
        write_file(root, "src/a.rs", "fn a() {}\n");
        write_file(root, "src/b.rs", "fn b() {}\n");
        write_file(root, "src/nested/c.rs", "fn c() {}\n");
    }

    #[test]
    fn it_reports_the_direct_file_count_limit_ceiling_and_overrides() {
        let root = TempDir::new().unwrap();
        write_fixture(&root);

        let output = sprawl_guard(&root)
            .args(["explain", "src"])
            .output()
            .unwrap();

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(
            stdout(&output),
            indoc! {"
                src
                  production: 2 direct files (limit 1, ceiling 2, group: source)
                    overrides: #1 paths=[src]
                    src/a.rs
                    src/b.rs
            "}
        );
        assert_eq!(stderr(&output), "");
    }

    #[test]
    fn it_reports_the_measurements_as_structured_output() {
        let root = TempDir::new().unwrap();
        write_fixture(&root);

        let (output, json) = run_json(&root, &["explain", "--format", "json", "src"]);

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(json["kind"], "directory");
        assert_eq!(json["path"], "src");
        let measurement = &json["measurements"][0];
        assert_eq!(measurement["kind"], "production");
        assert_eq!(measurement["group"], "source");
        assert_eq!(
            measurement["files"],
            serde_json::json!(["src/a.rs", "src/b.rs"])
        );
        assert_eq!(measurement["limit"]["value"], 1);
        assert_eq!(measurement["limit"]["matched_overrides"][0]["ordinal"], 1);
        assert_eq!(measurement["ratchet"], 2);
    }
}

mod when_explaining_a_file_with_a_byte_limit {
    use super::*;

    /// Writes an unlimited and a finite byte override for the file, with an override between them
    /// that matches the file but sets only a line limit.
    fn write_fixture(root: &TempDir) {
        write_config(
            root,
            indoc! {r#"
                [limits]
                max_bytes_per_file = 2000

                [[overrides]]
                paths = ["assets/**"]
                max_bytes_per_file = "unlimited"
                reason = "assets may be large"

                [[overrides]]
                paths = ["assets/**"]
                max_lines_per_file = { production = 10 }
                reason = "unrelated"

                [[overrides]]
                paths = ["assets/logo.*"]
                max_bytes_per_file = 1000
                reason = "logo budget"
            "#},
        );
        write_file(root, "assets/logo.png", &"x".repeat(1_500));
    }

    #[test]
    fn it_reports_the_size_limit_and_overrides_for_humans() {
        let root = TempDir::new().unwrap();
        write_fixture(&root);

        let output = sprawl_guard(&root)
            .args(["explain", "assets/logo.png"])
            .output()
            .unwrap();

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(
            stdout(&output),
            indoc! {"
                assets/logo.png: not a source file for any enabled language
                  size: 1500 bytes (1.5 kB), limit 1000 bytes (1.0 kB)
                  byte limit overrides: #1 paths=[assets/**], #3 paths=[assets/logo.*]
            "}
        );
    }

    #[test]
    fn it_reports_the_size_and_limit_as_structured_output() {
        let root = TempDir::new().unwrap();
        write_fixture(&root);

        let (output, json) = run_json(&root, &["explain", "--format", "json", "assets/logo.png"]);

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(
            json["file_bytes"],
            serde_json::json!({
                "limit": {
                    "value": 1000,
                    "matched_overrides": [
                        {
                            "ordinal": 1,
                            "paths": ["assets/**"],
                            "limits": { "max_bytes_per_file": "unlimited" },
                            "reason": "assets may be large",
                        },
                        {
                            "ordinal": 3,
                            "paths": ["assets/logo.*"],
                            "limits": { "max_bytes_per_file": 1000 },
                            "reason": "logo budget",
                        },
                    ],
                },
                "storage": "regular",
                "size": { "state": "measured", "bytes": 1500 },
            })
        );
    }
}

mod when_explaining_a_source_over_the_parsed_code_byte_limit {
    use super::*;

    #[test]
    fn it_says_its_lines_are_not_counted() {
        let root = TempDir::new().unwrap();
        write_config(
            &root,
            indoc! {r#"
                [traversal]
                max_parsed_code_file_bytes = 8

                [languages.rust]
                enabled = true
            "#},
        );
        write_file(&root, "src/lib.rs", "fn production() {}\n");

        let output = sprawl_guard(&root)
            .args(["explain", "src/lib.rs"])
            .output()
            .unwrap();

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert!(
            stdout(&output).contains(
                "  lines: not counted, larger than traversal.max_parsed_code_file_bytes (8 bytes); limit 400, counting code\n"
            ),
            "{}",
            stdout(&output)
        );
    }
}
