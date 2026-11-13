use assert_fs::TempDir;
use indoc::indoc;
use serde_json::json;

use super::common::*;

fn write_rust_and_typescript_sources(root: &TempDir) {
    write_file(root, "src/lib.rs", "fn main() {}\n");
    write_file(root, "src/app.ts", "export const value = 1;\n");
}

fn detected_languages_json(root: &TempDir) -> (std::process::Output, serde_json::Value) {
    run_json(root, &["languages", "--detected", "--format", "json"])
}

mod when_supported_languages_are_listed {
    use super::*;

    #[test]
    fn it_prints_supported_language_metadata_as_json() {
        let root = TempDir::new().unwrap();

        let (output, json) = run_json(&root, &["languages", "--format", "json"]);
        let rust = json_language(&json, "Rust");

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(
            json!({
                "mode": json["mode"],
                "rust": {
                    "config_key": rust["config_key"],
                    "include": rust["include"],
                    "bundled_extensions": rust["bundled_extensions"],
                    "bundled_filenames": rust["bundled_filenames"],
                    "test_globs": rust["test_globs"],
                    "rust": rust["rust"],
                },
            }),
            json!({
                "mode": "supported",
                "rust": {
                    "config_key": "rust",
                    "include": null,
                    "bundled_extensions": ["rs"],
                    "bundled_filenames": [],
                    "test_globs": ["benches/**"],
                    "rust": {"exclude_cfg_test": true},
                },
            })
        );
        assert_eq!(stderr(&output), "");
    }

    #[test]
    fn it_reports_automatic_group_and_line_type_policy() {
        let root = TempDir::new().unwrap();

        let (output, json) = run_json(&root, &["languages", "--format", "json"]);
        let rust = json_language(&json, "Rust");
        let markdown = json_language(&json, "Markdown");

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(
            json!({
                "rust": {
                    "literate": rust["literate"],
                    "automatic_group": rust["automatic_group"],
                    "automatic_count_lines": rust["automatic_count_lines"],
                },
                "markdown": {
                    "literate": markdown["literate"],
                    "automatic_group": markdown["automatic_group"],
                    "automatic_count_lines": markdown["automatic_count_lines"],
                },
            }),
            json!({
                "rust": {
                    "literate": false,
                    "automatic_group": "source",
                    "automatic_count_lines": [],
                },
                "markdown": {
                    "literate": true,
                    "automatic_group": "documentation",
                    "automatic_count_lines": ["comments"],
                },
            })
        );
    }

    #[test]
    fn it_prints_one_table_row_per_language_for_humans() {
        let root = TempDir::new().unwrap();

        let output = sprawl_guard(&root).arg("languages").output().unwrap();
        let output = stdout(&output);
        let row = |language: &str| {
            output
                .lines()
                .find(|line| line.starts_with(&format!("{language} ")))
                .unwrap()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        };

        assert!(output.starts_with("Supported languages:\n\nLanguage "));
        assert_eq!(row("Rust"), "Rust rust .rs");
        assert!(row("Bazel").ends_with("build, workspace, module"));
    }

    #[test]
    fn it_adds_a_file_count_column_when_detecting() {
        let root = TempDir::new().unwrap();
        write_file(&root, "src/a.rs", "fn a() {}\n");
        write_file(&root, "src/b.rs", "fn b() {}\n");

        let output = sprawl_guard(&root)
            .args(["languages", "--detected"])
            .output()
            .unwrap();

        assert_eq!(
            stdout(&output),
            indoc! {"
                Detected languages:

                Language  Key   Files  Extensions and filenames
                Rust      rust  2      .rs

                Use --format json for globs, groups, and counted line types.
            "}
        );
    }

    #[test]
    fn it_reports_only_canonical_extension_owners() {
        let root = TempDir::new().unwrap();

        let (output, json) = run_json(&root, &["languages", "--format", "json"]);
        let phix = json_language(&json, "Phix");
        let specman_e = json_language(&json, "Specman e");

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert!(
            phix["bundled_extensions"]
                .as_array()
                .unwrap()
                .contains(&json!("e"))
        );
        assert!(
            !specman_e["bundled_extensions"]
                .as_array()
                .unwrap()
                .contains(&json!("e"))
        );
    }

    #[test]
    fn it_ignores_resolved_repo_language_overrides() {
        let root = TempDir::new().unwrap();
        write_config(
            &root,
            indoc! {r#"
                [languages.rust]
                enabled = true
                include = ["custom/**/*.rs"]
            "#},
        );

        let (output, json) = run_json(&root, &["languages", "--format", "json"]);
        let rust = json_language(&json, "Rust");

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(
            json!({
                "enabled": rust["enabled"],
                "include": rust["include"],
            }),
            json!({
                "enabled": true,
                "include": null,
            })
        );
        assert_eq!(stderr(&output), "");
    }
}

mod when_languages_are_detected_before_config_exists {
    use super::*;

    #[test]
    fn it_lists_only_languages_with_included_source_files() {
        let root = TempDir::new().unwrap();
        write_rust_and_typescript_sources(&root);

        let (output, json) = detected_languages_json(&root);

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(
            json!({
                "mode": json["mode"],
            }),
            json!({
                "mode": "detected",
            })
        );
        assert_detected_source_count(&json, "Rust", 1);
        assert_detected_source_count(&json, "TypeScript", 1);
        assert!(
            !json["languages"]
                .as_array()
                .unwrap()
                .iter()
                .any(|language| language["id"] == "JavaScript")
        );
        assert_eq!(stderr(&output), "");
    }
}

mod when_detecting_after_config_exists {
    use super::*;

    #[test]
    fn it_matches_init_style_detection_instead_of_resolved_config() {
        let root = TempDir::new().unwrap();
        write_config(
            &root,
            indoc! {r#"
                [languages.rust]
                enabled = true
            "#},
        );
        write_rust_and_typescript_sources(&root);

        let (output, json) = detected_languages_json(&root);

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_detected_source_count(&json, "Rust", 1);
        assert_detected_source_count(&json, "TypeScript", 1);
        assert_eq!(stderr(&output), "");
    }
}

mod when_root_is_passed {
    use super::*;

    #[test]
    fn it_requires_detected() {
        let root = TempDir::new().unwrap();

        let output = sprawl_guard_no_root()
            .args(["languages", "--root"])
            .arg(root.path())
            .output()
            .unwrap();

        assert_exit_code(&output, exit_codes::OPERATIONAL_ERROR);
        assert!(stderr(&output).contains("--detected"));
    }

    #[test]
    fn it_detects_languages_under_that_root() {
        let root = TempDir::new().unwrap();
        write_file(&root, "src/lib.rs", "fn main() {}\n");

        let output = sprawl_guard_no_root()
            .args(["languages", "--detected", "--format", "json", "--root"])
            .arg(root.path())
            .output()
            .unwrap();

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert!(stdout(&output).contains("\"rust\""));
    }
}
