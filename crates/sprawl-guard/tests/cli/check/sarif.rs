use std::process::Command;

use assert_fs::TempDir;
use serde_json::{Value, json};

use super::super::common::*;

/// The SARIF 2.1.0 schema as OASIS published it with Errata 01, unchanged, from
/// <https://docs.oasis-open.org/sarif/sarif/v2.1.0/errata01/os/schemas/sarif-schema-2.1.0.json>.
/// Copyright © OASIS Open 2023, distributed under the OASIS IPR Policy.
const SARIF_SCHEMA: &str = include_str!("../fixtures/sarif-schema-2.1.0.json");

/// Runs `check --format sarif` on `paths` and asserts that the log satisfies the SARIF 2.1.0 schema,
/// including the `uri` and `uri-reference` formats that GitHub code scanning rejects when they are
/// malformed.
fn sarif_log(mut command: Command, paths: &[&str]) -> Value {
    let output = command
        .args(["check", "--format", "sarif"])
        .args(paths)
        .output()
        .unwrap();
    assert_eq!(stderr(&output), "");
    let log = json_stdout(&output);
    let schema = serde_json::from_str(SARIF_SCHEMA).unwrap();
    // The schema declares Draft 4, which has no `uri-reference` format, so a Draft 4 validator
    // would skip `artifactLocation.uri`. The schema uses nothing that Draft 6 reads differently.
    let validator = jsonschema::options()
        .with_draft(jsonschema::Draft::Draft6)
        .should_validate_formats(true)
        .build(&schema)
        .unwrap();
    let errors = validator
        .iter_errors(&log)
        .map(|error| format!("{}: {error}", error.instance_path()))
        .collect::<Vec<_>>();
    assert!(errors.is_empty(), "{errors:#?}\n{log:#}");
    log
}

fn first_result(log: &Value) -> &Value {
    &log["runs"][0]["results"][0]
}

fn notifications(log: &Value) -> &Value {
    &log["runs"][0]["invocations"][0]["toolExecutionNotifications"]
}

mod when_sarif_output_is_requested {
    use super::*;

    mod and_a_file_is_over_its_limit {
        use super::*;

        #[test]
        fn it_writes_a_valid_log_with_a_line_region() {
            let root = TempDir::new().unwrap();
            write_rust_file_limit_config(&root, 1);
            write_rust_source_with_function_count(&root, "src/lib.rs", 2);

            let log = sarif_log(sprawl_guard(&root), &[]);

            assert_eq!(
                first_result(&log)["locations"][0]["physicalLocation"]["region"],
                json!({ "startLine": 1 })
            );
        }
    }

    mod and_a_file_name_has_characters_that_a_uri_reserves {
        use super::*;

        #[test]
        fn it_writes_a_valid_log_with_a_percent_encoded_uri() {
            let root = TempDir::new().unwrap();
            write_rust_file_limit_config(&root, 1);
            write_rust_source_with_function_count(&root, "src/my dir/a#1 100%.rs", 2);

            let log = sarif_log(sprawl_guard(&root), &[]);

            assert_eq!(
                first_result(&log)["locations"][0]["physicalLocation"]["artifactLocation"]["uri"],
                "src/my%20dir/a%231%20100%25.rs"
            );
        }
    }

    mod and_a_directory_has_too_many_files {
        use super::*;

        #[test]
        fn it_writes_a_valid_log_with_related_file_locations() {
            let root = TempDir::new().unwrap();
            write_rust_directory_direct_files_fixture(&root);

            let log = sarif_log(sprawl_guard(&root), &[]);

            let result = first_result(&log);
            assert_eq!(
                result["locations"][0]["physicalLocation"],
                json!({ "artifactLocation": { "uri": "src" } })
            );
            assert_eq!(result["relatedLocations"].as_array().unwrap().len(), 2);
        }
    }

    mod and_a_ratchet_entry_is_stale {
        use super::*;

        #[test]
        fn it_writes_a_valid_log_with_the_stale_ratchet_rule() {
            let root = TempDir::new().unwrap();
            write_rust_file_limit_config_and_file_ratchet(&root, 1, "src/lib.rs", 3);
            write_rust_source_with_function_count(&root, "src/lib.rs", 2);

            let log = sarif_log(sprawl_guard(&root), &[]);

            assert_eq!(first_result(&log)["ruleId"], "sprawl-guard/ratchet-stale");
        }
    }

    mod and_the_root_is_outside_the_declared_workspace {
        use super::*;

        #[test]
        fn it_writes_a_valid_log_with_no_locations() {
            let root = TempDir::new().unwrap();
            let workspace = TempDir::new().unwrap();
            write_rust_file_limit_config(&root, 1);
            write_rust_source_with_function_count(&root, "src/lib.rs", 2);

            let mut command = sprawl_guard(&root);
            command.env("GITHUB_WORKSPACE", workspace.path());
            let log = sarif_log(command, &[]);

            assert_eq!(first_result(&log)["locations"], json!([]));
        }
    }

    mod and_a_path_cannot_be_read {
        use super::*;

        #[test]
        fn it_writes_a_valid_log_with_an_error_notification() {
            let root = TempDir::new().unwrap();
            write_rust_file_limit_config(&root, 1);
            write_file(&root, "src/.ignore", "!{lib.rs\n");

            let log = sarif_log(sprawl_guard(&root), &[]);

            assert_eq!(notifications(&log)[0]["level"], "error");
        }
    }

    mod and_a_selected_path_is_skipped {
        use super::*;

        #[test]
        fn it_writes_a_valid_log_with_a_warning_notification() {
            let root = TempDir::new().unwrap();
            write_rust_file_limit_config(&root, 1);
            write_rust_source_with_function_count(&root, ".hidden.rs", 2);

            let log = sarif_log(sprawl_guard(&root), &[".hidden.rs"]);

            assert_eq!(notifications(&log)[0]["level"], "warning");
        }
    }
}
