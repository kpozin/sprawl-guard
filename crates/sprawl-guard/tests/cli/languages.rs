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
    fn it_prints_bundled_path_metadata_for_humans() {
        let root = TempDir::new().unwrap();

        let output = sprawl_guard(&root).arg("languages").output().unwrap();
        let output = stdout(&output);

        assert!(output.contains("bundled extensions: rs"));
        assert!(output.contains("bundled filenames: build, workspace, module"));
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
    fn it_matches_init_style_detection_instead_of_resolved_policy() {
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
