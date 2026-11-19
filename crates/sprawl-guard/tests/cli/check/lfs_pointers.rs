use assert_fs::TempDir;
use indoc::indoc;
use serde_json::json;
use sprawl_guard_lib::classification::SourceKind;

use super::super::common::*;

/// Path of the large asset that Git LFS stores.
const ASSET: &str = "assets/model.bin";

/// Byte limit for the asset.
const LIMIT: u64 = 1_000;

/// Size of the object that the asset's pointer declares.
const DECLARED: u64 = 5_000;

/// Returns the text of a Git LFS pointer to an object of `size` bytes.
fn pointer(size: u64) -> String {
    format!(
        "version https://git-lfs.github.com/spec/v1\noid sha256:{}\nsize {size}\n",
        "a".repeat(64)
    )
}

/// Returns the size on disk of `pointer(size)`.
fn pointer_bytes(size: u64) -> u64 {
    pointer(size).len().try_into().unwrap()
}

fn write_asset_limit_config(root: &TempDir) {
    write_config(root, &format!("[limits]\nmax_bytes_per_file = {LIMIT}\n"));
}

fn file_bytes_violations(report: &serde_json::Value) -> Vec<serde_json::Value> {
    report["violations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|violation| violation["kind"] == "file_bytes")
        .cloned()
        .collect()
}

fn byte_ceilings(root: &TempDir) -> Vec<(String, u64)> {
    read_ratchet_file(root)
        .file_sizes
        .iter()
        .map(|entry| (entry.path.as_str().to_owned(), entry.max_bytes.into_inner()))
        .collect()
}

mod when_a_large_asset_is_an_unsmudged_pointer {
    use super::*;

    fn write_fixture(root: &TempDir) {
        write_asset_limit_config(root);
        write_file(root, ASSET, &pointer(DECLARED));
    }

    #[test]
    fn it_reports_the_declared_size_in_json_output() {
        let root = TempDir::new().unwrap();
        write_fixture(&root);

        let (output, report) = run_check_json(&root);

        assert_exit_code(&output, exit_codes::CHECK_FAILURE);
        assert_eq!(
            file_bytes_violations(&report),
            vec![json!({
                "kind": "file_bytes",
                "path": ASSET,
                "actual": DECLARED,
                "measured_from": "lfs_pointer",
                "pointer_bytes": pointer_bytes(DECLARED),
                "limit": LIMIT,
                "ratchet": null,
            })],
        );
    }

    #[test]
    fn it_names_the_pointer_in_human_output() {
        let root = TempDir::new().unwrap();
        write_fixture(&root);

        let output = sprawl_guard(&root).arg("check").output().unwrap();

        assert_exit_code(&output, exit_codes::CHECK_FAILURE);
        assert!(
            stdout(&output).contains(&format!(
                "declared by a {}-byte Git LFS pointer",
                pointer_bytes(DECLARED)
            )),
            "{}",
            stdout(&output)
        );
    }

    #[test]
    fn it_names_the_pointer_in_sarif_output() {
        let root = TempDir::new().unwrap();
        write_fixture(&root);

        let (output, sarif) = run_json(&root, &["check", "--format", "sarif"]);

        assert_exit_code(&output, exit_codes::CHECK_FAILURE);
        assert_eq!(
            sarif["runs"][0]["results"][0]["message"]["text"],
            format!(
                "{ASSET} has {DECLARED} bytes, declared by a {}-byte Git LFS pointer, over the limit of {LIMIT}.",
                pointer_bytes(DECLARED)
            )
        );
    }

    #[test]
    fn explain_reports_the_declared_size() {
        let root = TempDir::new().unwrap();
        write_fixture(&root);

        let output = sprawl_guard(&root)
            .args(["explain", ASSET])
            .output()
            .unwrap();

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert!(
            stdout(&output).contains(&format!(
                "size: 5000 bytes (5.0 kB), declared by a {}-byte Git LFS pointer, limit 1000 bytes (1.0 kB)",
                pointer_bytes(DECLARED)
            )),
            "{}",
            stdout(&output)
        );
    }

    mod and_update_writes_the_ratchet {
        use super::*;

        #[test]
        fn it_records_the_declared_size() {
            let root = TempDir::new().unwrap();
            write_fixture(&root);

            let (output, _) = run_ratchet_file_write(&root, "update");

            assert_exit_code(&output, exit_codes::SUCCESS);
            assert_eq!(byte_ceilings(&root), vec![(ASSET.to_owned(), DECLARED)]);
        }

        mod and_the_asset_is_then_smudged {
            use super::*;

            fn smudge(root: &TempDir) {
                write_file(root, ASSET, &"x".repeat(DECLARED.try_into().unwrap()));
            }

            #[test]
            fn check_passes_at_the_same_ceiling() {
                let root = TempDir::new().unwrap();
                write_fixture(&root);
                run_ratchet_file_write(&root, "update");
                smudge(&root);

                let (output, _) = run_check_json(&root);

                assert_exit_code(&output, exit_codes::SUCCESS);
            }

            #[test]
            fn update_keeps_the_same_ceiling() {
                let root = TempDir::new().unwrap();
                write_fixture(&root);
                run_ratchet_file_write(&root, "update");
                smudge(&root);

                let (output, _) = run_ratchet_file_write(&root, "update");

                assert_exit_code(&output, exit_codes::SUCCESS);
                assert_eq!(byte_ceilings(&root), vec![(ASSET.to_owned(), DECLARED)]);
            }
        }
    }
}

mod when_a_small_file_almost_matches_the_pointer_format {
    use super::*;

    #[test]
    fn it_is_measured_on_disk() {
        let root = TempDir::new().unwrap();
        write_config(&root, "[limits]\nmax_bytes_per_file = 10\n");
        let near_miss = pointer(DECLARED).replace('\n', "\r\n");
        write_file(&root, ASSET, &near_miss);

        let (output, report) = run_check_json(&root);

        assert_exit_code(&output, exit_codes::CHECK_FAILURE);
        assert_eq!(
            file_bytes_violations(&report),
            vec![json!({
                "kind": "file_bytes",
                "path": ASSET,
                "actual": near_miss.len(),
                "measured_from": "on_disk",
                "limit": 10,
                "ratchet": null,
            })],
        );
    }
}

mod when_a_source_with_a_line_ceiling_becomes_a_pointer {
    use super::*;

    /// Path of the Rust source that Git LFS stores.
    const SOURCE: &str = "src/lib.rs";

    /// Writes a source over its line limit with a matching line ceiling, then replaces the
    /// source with a pointer.
    fn write_fixture(root: &TempDir) {
        write_config(
            root,
            indoc! {"
                [limits.max_lines_per_file]
                production = 1

                [languages.rust]
                enabled = true
            "},
        );
        write_rust_source_with_function_count(root, SOURCE, 3);
        run_ratchet_file_write(root, "update");
        write_file(root, SOURCE, &pointer(DECLARED));
    }

    fn line_ceiling(root: &TempDir) -> Option<u32> {
        production_file_ratchet_entry(&read_ratchet_file(root), SOURCE)
            .map(|entry| entry.max_lines.get())
    }

    #[test]
    fn check_warns_without_reporting_the_ceiling_as_stale() {
        let root = TempDir::new().unwrap();
        write_fixture(&root);

        let (output, report) = run_json(
            &root,
            &["check", "--format", "json", "--require-ratchet-current"],
        );

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(
            report["warnings"],
            json!([{
                "kind": "lfs_pointer_source",
                "path": SOURCE,
                "role": "production",
                "declared_bytes": DECLARED,
            }])
        );
        assert_eq!(report["violations"], json!([]));
    }

    #[test]
    fn tighten_keeps_the_line_ceiling() {
        let root = TempDir::new().unwrap();
        write_fixture(&root);

        let (output, _) = run_ratchet_file_write(&root, "tighten");

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(line_ceiling(&root), Some(3));
    }

    #[test]
    fn explain_says_its_lines_are_not_counted() {
        let root = TempDir::new().unwrap();
        write_fixture(&root);

        let output = sprawl_guard(&root)
            .args(["explain", SOURCE])
            .output()
            .unwrap();

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert!(
            stdout(&output).contains("lines: not counted, the file is a Git LFS pointer; limit 1"),
            "{}",
            stdout(&output)
        );
    }

    #[test]
    fn explain_reports_the_pointer_state_as_structured_output() {
        let root = TempDir::new().unwrap();
        write_fixture(&root);

        let (output, json) = run_json(&root, &["explain", "--format", "json", SOURCE]);

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(json["state"], "lfs_pointer");
    }

    mod and_the_ratchet_also_has_a_ceiling_for_its_old_test_role {
        use super::*;

        fn write_test_role_ceiling(root: &TempDir) {
            let ratchet = read_ratchet(root);
            write_ratchet(
                root,
                &format!(
                    "{ratchet}\n[[files]]\npath = \"{SOURCE}\"\nrole = \"test\"\nmax_lines = 3\n"
                ),
            );
        }

        fn roles(root: &TempDir) -> Vec<SourceKind> {
            read_ratchet_file(root)
                .files
                .iter()
                .map(|entry| entry.role)
                .collect()
        }

        #[test]
        fn check_reports_only_the_test_role_ceiling_as_stale() {
            let root = TempDir::new().unwrap();
            write_fixture(&root);
            write_test_role_ceiling(&root);

            let (output, report) = run_json(
                &root,
                &["check", "--format", "json", "--require-ratchet-current"],
            );

            assert_exit_code(&output, exit_codes::CHECK_FAILURE);
            let kinds = report["violations"]
                .as_array()
                .unwrap()
                .iter()
                .map(|violation| violation["kind"].clone())
                .collect::<Vec<_>>();
            assert_eq!(kinds, vec![json!("test_file_lines_stale")]);
        }

        #[test]
        fn tighten_keeps_only_the_production_ceiling() {
            let root = TempDir::new().unwrap();
            write_fixture(&root);
            write_test_role_ceiling(&root);

            let (output, _) = run_ratchet_file_write(&root, "tighten");

            assert_exit_code(&output, exit_codes::SUCCESS);
            assert_eq!(roles(&root), vec![SourceKind::Production]);
        }
    }
}
