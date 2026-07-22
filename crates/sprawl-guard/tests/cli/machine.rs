use assert_fs::TempDir;
use assert_fs::prelude::*;
use indoc::indoc;
use serde_json::json;

use super::common::*;

const MACHINE_PROTOCOL_VERSION: u8 = 1;

mod run {
    use super::*;

    fn machine_request(root: &TempDir, argv: &[&str]) -> serde_json::Value {
        json!({
            "protocol_version": MACHINE_PROTOCOL_VERSION,
            "workspace_root": root.path(),
            "host_path_mapping": {
                "kind": "none",
            },
            "argv": argv,
        })
    }

    fn machine_alias_request(
        root: &TempDir,
        host_root: &str,
        style: &str,
        argv: &[&str],
    ) -> serde_json::Value {
        json!({
            "protocol_version": MACHINE_PROTOCOL_VERSION,
            "workspace_root": root.path(),
            "host_path_mapping": {
                "kind": "alias",
                "host_root": host_root,
                "style": style,
            },
            "argv": argv,
        })
    }

    fn assert_completed_result_matches_direct_output(
        output: &std::process::Output,
        json: &serde_json::Value,
        direct: &std::process::Output,
        direct_payload: serde_json::Value,
    ) {
        assert_machine_protocol_version(json);
        assert_exit_code(output, exit_codes::SUCCESS);
        assert_eq!(
            json!({
                "kind": json["result"]["kind"],
                "exit_code": json["result"]["exit_code"],
                "payload": json["result"]["payload"],
                "stdout": json["result"]["stdout"],
                "stderr": json["result"]["stderr"],
            }),
            json!({
                "kind": "completed",
                "exit_code": direct.status.code().unwrap(),
                "payload": direct_payload,
                "stdout": stdout(direct),
                "stderr": stderr(direct),
            })
        );
    }

    fn assert_completed_result_matches_direct_streams(
        output: &std::process::Output,
        json: &serde_json::Value,
        direct: &std::process::Output,
    ) {
        assert_machine_protocol_version(json);
        assert_exit_code(output, exit_codes::SUCCESS);
        assert_eq!(
            json!({
                "kind": json["result"]["kind"],
                "exit_code": json["result"]["exit_code"],
                "stdout": json["result"]["stdout"],
                "stderr": json["result"]["stderr"],
            }),
            json!({
                "kind": "completed",
                "exit_code": direct.status.code().unwrap(),
                "stdout": stdout(direct),
                "stderr": stderr(direct),
            })
        );
    }

    fn request_error_messages(request: &serde_json::Value) -> Vec<String> {
        let (output, json) = run_json_stdin(&["machine", "run"], request);

        assert_machine_protocol_version(&json);
        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(
            json!({
                "kind": json["result"]["kind"],
                "exit_code": json["result"]["exit_code"],
                "stdout": json["result"]["stdout"],
            }),
            json!({
                "kind": "request_error",
                "exit_code": exit_codes::OPERATIONAL_ERROR,
                "stdout": "",
            })
        );
        assert_eq!(stderr(&output), "");
        json["result"]["issues"]
            .as_array()
            .unwrap()
            .iter()
            .map(|issue| issue["message"].as_str().unwrap().to_owned())
            .collect()
    }

    fn write_enabled_rust_config(root: &TempDir) {
        write_config(
            root,
            indoc! {r#"
                [languages.rust]
                enabled = true
            "#},
        );
    }

    fn write_enabled_rust_file(root: &TempDir, path: &str) {
        write_enabled_rust_config(root);
        write_file(root, path, "fn production() {}\n");
    }

    fn direct_json_command(
        root: &TempDir,
        args: &[&str],
    ) -> (std::process::Output, serde_json::Value) {
        let output = sprawl_guard(root).args(args).output().unwrap();
        let payload = json_stdout(&output);
        (output, payload)
    }

    fn run_machine_request(
        request: &serde_json::Value,
    ) -> (std::process::Output, serde_json::Value) {
        run_json_stdin(&["machine", "run"], request)
    }

    /// Ratchet-producing commands whose machine payloads share one schema.
    #[derive(Clone, Copy)]
    enum RatchetPayloadKind {
        Baseline,
        Ratchet,
    }

    impl RatchetPayloadKind {
        fn as_str(self) -> &'static str {
            match self {
                Self::Baseline => "baseline",
                Self::Ratchet => "ratchet",
            }
        }
    }

    fn assert_machine_protocol_version(json: &serde_json::Value) {
        assert_eq!(json["protocol_version"], MACHINE_PROTOCOL_VERSION);
    }

    fn expected_single_file_ratchet() -> serde_json::Value {
        json!({
            "version": 1,
            "files": [{
                "path": "src/lib.rs",
                "non_test_code_lines": 2,
            }],
            "test_files": [],
            "directories": [],
            "test_directories": [],
        })
    }

    /// Verifies the common envelope for a library failure and returns its diagnostic text.
    fn assert_library_runtime_error<'a>(
        output: &std::process::Output,
        json: &'a serde_json::Value,
    ) -> &'a str {
        assert_machine_protocol_version(json);
        assert_exit_code(output, exit_codes::SUCCESS);
        assert_eq!(
            json!({
                "kind": json["result"]["kind"],
                "exit_code": json["result"]["exit_code"],
                "stdout": json["result"]["stdout"],
                "error_code": json["result"]["error_code"],
            }),
            json!({
                "kind": "runtime_error",
                "exit_code": exit_codes::OPERATIONAL_ERROR,
                "stdout": "",
                "error_code": "library_error",
            })
        );
        assert_eq!(stderr(output), "");
        json["result"]["stderr"].as_str().unwrap()
    }

    /// Verifies the shared dry-run payload emitted by baseline and ratchet commands.
    fn assert_ratchet_dry_run_result(
        root: &TempDir,
        output: &std::process::Output,
        json: &serde_json::Value,
        direct: &std::process::Output,
        kind: RatchetPayloadKind,
    ) {
        assert_completed_result_matches_direct_streams(output, json, direct);
        assert_eq!(
            json!({
                "kind": json["result"]["payload"]["kind"],
                "path": json["result"]["payload"]["path"],
                "wrote": json["result"]["payload"]["wrote"],
                "ratchet": json["result"]["payload"]["ratchet"],
            }),
            json!({
                "kind": kind.as_str(),
                "path": root.path().join("sprawl-guard.ratchet.toml"),
                "wrote": false,
                "ratchet": expected_single_file_ratchet(),
            })
        );
        assert_eq!(stderr(output), "");
    }

    /// Verifies the shared write result and persisted ratchet produced by ratchet commands.
    fn assert_ratchet_write_result(
        root: &TempDir,
        output: &std::process::Output,
        json: &serde_json::Value,
        kind: RatchetPayloadKind,
    ) {
        assert_exit_code(output, exit_codes::SUCCESS);
        assert_eq!(
            json!({
                "kind": json["result"]["kind"],
                "exit_code": json["result"]["exit_code"],
                "stdout": json["result"]["stdout"],
                "stderr": json["result"]["stderr"],
                "payload_kind": json["result"]["payload"]["kind"],
                "path": json["result"]["payload"]["path"],
                "wrote": json["result"]["payload"]["wrote"],
                "ratchet": json["result"]["payload"]["ratchet"],
            }),
            json!({
                "kind": "completed",
                "exit_code": exit_codes::SUCCESS,
                "stdout": format!(
                    "wrote {}\n",
                    root.path().join("sprawl-guard.ratchet.toml").display()
                ),
                "stderr": "",
                "payload_kind": kind.as_str(),
                "path": root.path().join("sprawl-guard.ratchet.toml"),
                "wrote": true,
                "ratchet": expected_single_file_ratchet(),
            })
        );
        let ratchet = read_ratchet_file(root);
        assert_eq!(
            serde_json::to_value(ratchet).unwrap(),
            expected_single_file_ratchet()
        );
        assert_eq!(stderr(output), "");
    }

    #[test]
    fn it_returns_request_errors_inside_the_machine_envelope() {
        let mut command = sprawl_guard_no_root();
        command.args(["machine", "run"]);
        command.stdin(std::process::Stdio::piped());
        command.stdout(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::piped());
        let mut child = command.spawn().unwrap();
        use std::io::Write as _;
        child.stdin.as_mut().unwrap().write_all(b"{").unwrap();
        let output = child.wait_with_output().unwrap();
        let json = json_stdout(&output);

        assert_machine_protocol_version(&json);
        assert_exit_code(&output, exit_codes::SUCCESS);
        let issue_message = json["result"]["issues"][0]["message"].as_str().unwrap();
        assert_eq!(
            json!({
                "kind": json["result"]["kind"],
                "exit_code": json["result"]["exit_code"],
                "stdout": json["result"]["stdout"],
                "stderr": json["result"]["stderr"],
                "message": issue_message,
            }),
            json!({
                "kind": "request_error",
                "exit_code": exit_codes::OPERATIONAL_ERROR,
                "stdout": "",
                "stderr": format!("error: {issue_message}\n"),
                "message": issue_message,
            })
        );
        assert_eq!(stderr(&output), "");
    }

    #[test]
    fn it_returns_runtime_errors_inside_the_machine_envelope() {
        let root = TempDir::new().unwrap();
        let request = machine_request(&root, &["--config", "missing.toml", "config", "resolved"]);
        let (output, json) = run_json_stdin(&["machine", "run"], &request);

        let result_stderr = assert_library_runtime_error(&output, &json);
        assert!(result_stderr.contains("config file does not exist"));
    }

    #[test]
    fn it_keeps_init_advisory_failures_inside_the_machine_envelope() {
        let root = TempDir::new().unwrap();
        write_oversized_rust_source(&root, "src/huge.rs");
        let request = machine_request(&root, &["init", "--languages", "rust"]);
        let (output, json) = run_json_stdin(&["machine", "run"], &request);
        let result_stderr = assert_library_runtime_error(&output, &json);

        assert!(result_stderr.contains("failed to run init advisory scan before writing"));
        assert!(result_stderr.contains("is too large to count"));
        assert!(!root.child("sprawl-guard.toml").path().exists());
    }

    #[test]
    fn it_reports_clap_errors_as_request_errors() {
        let root = TempDir::new().unwrap();
        let messages = request_error_messages(&machine_request(
            &root,
            &[
                "check",
                "--require-ratchet-current",
                "--no-require-ratchet-current",
            ],
        ));

        assert!(messages[0].contains("cannot be used with"));
    }

    #[test]
    fn it_rejects_nested_machine_invocation() {
        let root = TempDir::new().unwrap();
        let messages = request_error_messages(&machine_request(&root, &["machine", "run"]));

        assert!(messages[0].contains("machine run does not allow command: machine"));
    }

    #[test]
    fn it_matches_init_dry_run_output() {
        let root = TempDir::new().unwrap();
        let direct = sprawl_guard(&root)
            .args(["init", "--languages", "rust", "--dry-run"])
            .output()
            .unwrap();
        let request = machine_request(&root, &["init", "--languages", "rust", "--dry-run"]);
        let (output, json) = run_machine_request(&request);

        assert_completed_result_matches_direct_streams(&output, &json, &direct);
        assert_eq!(
            json!({
                "kind": json["result"]["payload"]["kind"],
                "target": json["result"]["payload"]["target"],
                "selected_languages": json["result"]["payload"]["selected_languages"],
                "mode": json["result"]["payload"]["mode"],
            }),
            json!({
                "kind": "init",
                "target": root.path().canonicalize().unwrap().join("sprawl-guard.toml"),
                "selected_languages": ["Rust"],
                "mode": {
                    "kind": "dry_run",
                    "overwrite_warning": serde_json::Value::Null,
                },
            })
        );
        assert_eq!(stderr(&output), "");
    }

    #[test]
    fn it_writes_init_config_inside_the_machine_envelope() {
        let root = TempDir::new().unwrap();
        let request = machine_request(&root, &["init", "--languages", "rust"]);
        let (output, json) = run_machine_request(&request);

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(
            json!({
                "kind": json["result"]["kind"],
                "exit_code": json["result"]["exit_code"],
                "stderr": json["result"]["stderr"],
                "payload_kind": json["result"]["payload"]["kind"],
                "target": json["result"]["payload"]["target"],
                "selected_languages": json["result"]["payload"]["selected_languages"],
                "mode": json["result"]["payload"]["mode"],
            }),
            json!({
                "kind": "completed",
                "exit_code": exit_codes::SUCCESS,
                "stderr": "",
                "payload_kind": "init",
                "target": root.path().canonicalize().unwrap().join("sprawl-guard.toml"),
                "selected_languages": ["Rust"],
                "mode": {
                    "kind": "written",
                },
            })
        );
        assert!(
            json["result"]["stdout"]
                .as_str()
                .unwrap()
                .contains("Wrote ")
        );
        assert_toml_contains(
            &read_config_toml(&root),
            toml::toml! {
                [languages.rust]
                enabled = true
            },
        );
        assert_eq!(stderr(&output), "");
    }

    #[test]
    fn it_accepts_machine_safe_commands() {
        let root = TempDir::new().unwrap();
        write_enabled_rust_file(&root, "src/lib.rs");
        let commands = [
            ("check", vec!["check", "--format", "json"]),
            ("explain", vec!["explain", "--format", "json", "src/lib.rs"]),
            ("init", vec!["init", "--languages", "rust", "--dry-run"]),
            ("languages", vec!["languages", "--format", "json"]),
            ("config resolved", vec!["config", "resolved"]),
            ("config export", vec!["config", "export"]),
            ("baseline", vec!["baseline"]),
        ];

        for (command, argv) in commands {
            let request = machine_request(&root, &argv);
            let (output, json) = run_machine_request(&request);

            assert_exit_code(&output, exit_codes::SUCCESS);
            assert_eq!(
                json["result"]["kind"], "completed",
                "machine-safe command should be allowed: {command}"
            );
            assert_eq!(stderr(&output), "");
        }
    }

    #[test]
    fn it_rejects_unsupported_and_nested_commands() {
        let root = TempDir::new().unwrap();
        let commands = [("machine", vec!["machine", "run"])];

        for (command, argv) in commands {
            let messages = request_error_messages(&machine_request(&root, &argv));

            assert!(
                messages[0].contains(&format!("machine run does not allow command: {command}")),
                "unsupported or nested machine command should be rejected: {command}",
            );
        }
    }

    #[test]
    fn it_returns_help_for_mutating_commands_before_policy_gating() {
        let root = TempDir::new().unwrap();
        let direct = sprawl_guard_no_root()
            .args(["baseline", "--help"])
            .output()
            .unwrap();
        let request = machine_request(&root, &["baseline", "--help"]);
        let (output, json) = run_json_stdin(&["machine", "run"], &request);

        assert_completed_result_matches_direct_output(&output, &json, &direct, json!(null));
    }

    #[test]
    fn it_rejects_argv_that_includes_the_binary_name() {
        let root = TempDir::new().unwrap();
        let request = machine_request(&root, &["sprawl-guard", "check"]);
        let messages = request_error_messages(&request);

        assert!(messages[0].contains("must exclude the sprawl-guard binary name"));
    }

    #[test]
    fn it_matches_explain_json_output() {
        let root = TempDir::new().unwrap();
        write_enabled_rust_file(&root, "src/lib.rs");
        let (direct, direct_json) =
            direct_json_command(&root, &["explain", "--format", "json", "src/lib.rs"]);
        let request = machine_request(&root, &["explain", "--format", "json", "src/lib.rs"]);
        let (output, json) = run_machine_request(&request);

        assert_completed_result_matches_direct_output(&output, &json, &direct, direct_json);
    }

    #[test]
    fn it_accepts_posix_colons_inside_argv_path_segments() {
        let root = TempDir::new().unwrap();
        write_enabled_rust_file(&root, "src/a:b.rs");
        let (direct, direct_json) =
            direct_json_command(&root, &["explain", "--format", "json", "src/a:b.rs"]);
        let request = machine_request(&root, &["explain", "--format", "json", "src/a:b.rs"]);
        let (output, json) = run_machine_request(&request);

        assert_completed_result_matches_direct_output(&output, &json, &direct, direct_json);
    }

    #[test]
    fn it_matches_check_json_output() {
        let root = TempDir::new().unwrap();
        write_rust_file_limit_config(&root, 10);
        write_file(&root, "src/lib.rs", "fn production() {}\n");
        let (direct, direct_json) = direct_json_command(&root, &["check", "--format", "json"]);
        let request = machine_request(&root, &["check", "--format", "json"]);
        let (output, json) = run_machine_request(&request);

        assert_completed_result_matches_direct_output(
            &output,
            &json,
            &direct,
            json!({
                "kind": "report",
                "report": direct_json,
            }),
        );
    }

    #[test]
    fn it_matches_config_export_output() {
        let root = TempDir::new().unwrap();
        write_enabled_rust_config(&root);
        let direct = sprawl_guard(&root)
            .args(["config", "export"])
            .output()
            .unwrap();
        let direct_config: toml::Value = toml::from_str(&stdout(&direct)).unwrap();
        let direct_payload = serde_json::to_value(direct_config).unwrap();
        let request = machine_request(&root, &["config", "export"]);
        let (output, json) = run_machine_request(&request);

        assert_completed_result_matches_direct_output(&output, &json, &direct, direct_payload);
    }

    #[test]
    fn it_matches_baseline_dry_run_output() {
        let root = TempDir::new().unwrap();
        write_rust_file_limit_config(&root, 1);
        write_rust_source_with_function_count(&root, "src/lib.rs", 2);
        let direct = sprawl_guard(&root).arg("baseline").output().unwrap();
        let request = machine_request(&root, &["baseline"]);
        let (output, json) = run_machine_request(&request);

        assert_ratchet_dry_run_result(&root, &output, &json, &direct, RatchetPayloadKind::Baseline);
    }

    #[test]
    fn it_writes_baseline_ratchet_inside_the_machine_envelope() {
        let root = TempDir::new().unwrap();
        write_rust_file_limit_config(&root, 1);
        write_rust_source_with_function_count(&root, "src/lib.rs", 2);
        let request = machine_request(&root, &["baseline", "--write"]);
        let (output, json) = run_machine_request(&request);

        assert_ratchet_write_result(&root, &output, &json, RatchetPayloadKind::Baseline);
    }

    #[test]
    fn it_matches_ratchet_dry_run_output() {
        let root = TempDir::new().unwrap();
        write_rust_file_limit_config_and_file_ratchet(&root, 1, "src/lib.rs", 4);
        write_rust_source_with_function_count(&root, "src/lib.rs", 2);
        let ratchet_before = read_ratchet(&root);
        let direct = sprawl_guard(&root).arg("ratchet").output().unwrap();
        assert_eq!(read_ratchet(&root), ratchet_before);
        let request = machine_request(&root, &["ratchet"]);
        let (output, json) = run_machine_request(&request);

        assert_ratchet_dry_run_result(&root, &output, &json, &direct, RatchetPayloadKind::Ratchet);
        assert_eq!(read_ratchet(&root), ratchet_before);
    }

    #[test]
    fn it_writes_ratchet_updates_inside_the_machine_envelope() {
        let root = TempDir::new().unwrap();
        write_rust_file_limit_config_and_file_ratchet(&root, 1, "src/lib.rs", 4);
        write_rust_source_with_function_count(&root, "src/lib.rs", 2);
        let request = machine_request(&root, &["ratchet", "--write"]);
        let (output, json) = run_machine_request(&request);

        assert_ratchet_write_result(&root, &output, &json, RatchetPayloadKind::Ratchet);
    }

    #[test]
    fn it_remaps_absolute_config_paths_under_an_alias() {
        let root = TempDir::new().unwrap();
        write_enabled_rust_config(&root);
        let config_path = root.path().join("sprawl-guard.toml");

        let direct = sprawl_guard(&root)
            .args([
                "--config",
                &config_path.display().to_string(),
                "config",
                "resolved",
            ])
            .output()
            .unwrap();
        let direct_config: toml::Value = toml::from_str(&stdout(&direct)).unwrap();
        let direct_payload = serde_json::to_value(direct_config).unwrap();

        let request = machine_alias_request(
            &root,
            &root.path().display().to_string(),
            "posix",
            &[
                "--config",
                &config_path.display().to_string(),
                "config",
                "resolved",
            ],
        );
        let (output, json) = run_machine_request(&request);

        assert_completed_result_matches_direct_output(&output, &json, &direct, direct_payload);
    }

    #[test]
    fn it_treats_dot_as_the_workspace_root_argv_path() {
        let root = TempDir::new().unwrap();
        write_rust_file_limit_config(&root, 10);
        write_file(&root, "src/lib.rs", "fn production() {}\n");
        let (direct, direct_json) = direct_json_command(&root, &["check", "--format", "json"]);
        let request = machine_request(&root, &["check", "--format", "json", "."]);
        let (output, json) = run_machine_request(&request);

        assert_completed_result_matches_direct_output(
            &output,
            &json,
            &direct,
            json!({
                "kind": "report",
                "report": direct_json,
            }),
        );
    }

    #[test]
    fn it_accepts_the_guest_workspace_root_under_an_alias() {
        let root = TempDir::new().unwrap();
        write_rust_file_limit_config(&root, 10);
        write_file(&root, "src/lib.rs", "fn production() {}\n");
        let workspace_root = root.path().display().to_string();
        let (direct, direct_json) = direct_json_command(&root, &["check", "--format", "json"]);
        let request = machine_alias_request(
            &root,
            "/host/repo",
            "posix",
            &["--root", &workspace_root, "check", "--format", "json"],
        );
        let (output, json) = run_machine_request(&request);

        assert_completed_result_matches_direct_output(
            &output,
            &json,
            &direct,
            json!({
                "kind": "report",
                "report": direct_json,
            }),
        );
    }

    #[test]
    fn it_remaps_posix_host_absolute_paths_under_an_alias() {
        let root = TempDir::new().unwrap();
        write_enabled_rust_file(&root, "src/lib.rs");
        let host_path = root.path().join("src/lib.rs");
        let (direct, direct_json) =
            direct_json_command(&root, &["explain", "--format", "json", "src/lib.rs"]);
        let request = machine_alias_request(
            &root,
            &root.path().display().to_string(),
            "posix",
            &[
                "explain",
                "--format",
                "json",
                &host_path.display().to_string(),
            ],
        );
        let (output, json) = run_machine_request(&request);

        assert_completed_result_matches_direct_output(&output, &json, &direct, direct_json);
    }

    #[test]
    fn it_rejects_posix_host_absolute_paths_outside_an_alias() {
        let root = TempDir::new().unwrap();
        let escaped_path = root.path().with_file_name("repo2").join("src/lib.rs");
        let messages = request_error_messages(&machine_alias_request(
            &root,
            &root.path().display().to_string(),
            "posix",
            &[
                "explain",
                "--format",
                "json",
                &escaped_path.display().to_string(),
            ],
        ));

        assert!(messages[0].contains("outside host_path_mapping.host_root"));
    }

    #[test]
    fn it_rejects_later_root_flags_outside_the_selected_workspace() {
        let root = TempDir::new().unwrap();
        let other = TempDir::new().unwrap();
        let messages = request_error_messages(&machine_alias_request(
            &root,
            &root.path().display().to_string(),
            "posix",
            &[
                "check",
                "--root",
                &other.path().display().to_string(),
                "--format",
                "json",
            ],
        ));

        assert!(messages[0].contains("outside host_path_mapping.host_root"));
    }

    #[test]
    fn it_remaps_windows_drive_paths_under_an_alias() {
        let root = TempDir::new().unwrap();
        write_enabled_rust_file(&root, "src/lib.rs");
        let (direct, direct_json) =
            direct_json_command(&root, &["explain", "--format", "json", "src/lib.rs"]);
        let request = machine_alias_request(
            &root,
            r"C:\repo",
            "windows",
            &["explain", "--format", "json", r"C:\repo\src\lib.rs"],
        );
        let (output, json) = run_machine_request(&request);

        assert_completed_result_matches_direct_output(&output, &json, &direct, direct_json);
    }
}
