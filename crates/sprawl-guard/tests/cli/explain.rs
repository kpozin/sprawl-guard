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
            "nested/ratchet.toml: excluded because it is the configured ratchet file\n"
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
                src/lib.rs: production source (Rust; group: source); file LOC limit: 400; included line types: code; directory leaf-file limit: 10; configured LOC: 1 (code: 1, comments: 0, blank lines: 1); rust cfg(test): parsed; excluded top-level items: 1
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
                "included_line_types": json["measurement"]["included_line_types"],
                "line_counts": json["measurement"]["line_counts"],
                "actual": json["measurement"]["actual"],
            }),
            serde_json::json!({
                "kind": "source",
                "state": "measured",
                "included_line_types": ["code"],
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
                max_leaf_files_per_directory = { production = 1 }

                [languages.rust]
                enabled = true

                [[overrides]]
                paths = ["src/**"]
                max_lines_per_file = { production = 2 }
                max_leaf_files_per_directory = { production = 3 }
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
                src/legacy/lib.rs: production source (Rust; group: source); file LOC limit: 4; included line types: code; directory leaf-file limit: 3; matched file overrides: #1 paths=[src/**], #2 paths=[src/legacy/**]; matched directory overrides: #1 paths=[src/**], #2 paths=[src/legacy/**]; configured LOC: 1 (code: 1, comments: 0, blank lines: 0); rust cfg(test): skipped by prefilter; excluded top-level items: 0
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
                max_leaf_files_per_directory = { production = 1 }

                [languages.rust]
                enabled = true

                [[overrides]]
                paths = ["src"]
                max_leaf_files_per_directory = { production = 3 }
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
                src/lib.rs: production source (Rust; group: source); file LOC limit: 400; included line types: code; directory leaf-file limit: 3; matched directory overrides: #1 paths=[src]; configured LOC: 1 (code: 1, comments: 0, blank lines: 0); rust cfg(test): skipped by prefilter; excluded top-level items: 0
            "}
        );
        assert_eq!(stderr(&output), "");
    }
}
