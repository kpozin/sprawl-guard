use assert_fs::TempDir;
use indoc::indoc;
use serde_json::json;
use sprawl_guard_lib::FileStorage;
use sprawl_guard_lib::classification::SourceKind;

use super::super::common::*;

/// Path of the large asset that Git LFS stores.
const ASSET: &str = "assets/model.bin";

/// `.gitattributes` line that `git lfs track "*.bin"` writes.
const ATTRIBUTES: &str = "*.bin filter=lfs diff=lfs merge=lfs -text\n";

/// Byte limit for files that Git stores: above the size of `.gitattributes`, below a pointer's.
const LIMIT: u64 = 100;

/// Byte limit for Git LFS objects.
const LFS_LIMIT: u64 = 1_000;

/// Size of the object that the asset's pointer declares.
const DECLARED: u64 = 5_000;

/// Returns the text of a Git LFS pointer to an object of `size` bytes.
fn pointer(size: u64) -> String {
    format!(
        "version https://git-lfs.github.com/spec/v1\noid sha256:{}\nsize {size}\n",
        "a".repeat(64)
    )
}

fn write_limit_config(root: &TempDir, lfs_limit: &str) {
    write_config(
        root,
        &format!(
            "[limits]\nmax_bytes_per_file = {LIMIT}\nmax_bytes_per_lfs_object = {lfs_limit}\n"
        ),
    );
}

fn violations(report: &serde_json::Value) -> Vec<serde_json::Value> {
    report["violations"].as_array().unwrap().clone()
}

fn violation_kinds(report: &serde_json::Value) -> Vec<serde_json::Value> {
    violations(report)
        .iter()
        .map(|violation| violation["kind"].clone())
        .collect()
}

fn byte_ceilings(root: &TempDir) -> Vec<(String, FileStorage, u64)> {
    read_ratchet_file(root)
        .file_sizes
        .iter()
        .map(|(path, entry)| {
            (
                path.as_str().to_owned(),
                entry.storage,
                entry.max_bytes.into_inner(),
            )
        })
        .collect()
}

/// Replaces the asset's pointer with the content that Git LFS downloads.
fn smudge(root: &TempDir) {
    write_file(root, ASSET, &"x".repeat(DECLARED.try_into().unwrap()));
}

/// The asset's form on disk: the pointer, or the content after Git LFS downloads it.
#[derive(Clone, Copy, Debug)]
enum Checkout {
    Unsmudged,
    Smudged,
}

const CHECKOUTS: [Checkout; 2] = [Checkout::Unsmudged, Checkout::Smudged];

/// Writes the asset that `.gitattributes` assigns to Git LFS, in the form `checkout` names.
fn write_lfs_asset(root: &TempDir, checkout: Checkout) {
    write_file(root, ".gitattributes", ATTRIBUTES);
    write_file(root, ASSET, &pointer(DECLARED));
    if let Checkout::Smudged = checkout {
        smudge(root);
    }
}

mod when_gitattributes_assigns_a_large_asset_to_lfs {
    use super::*;

    mod and_only_the_limit_for_files_in_git_is_set {
        use super::*;

        #[test]
        fn check_passes_whether_or_not_the_asset_is_smudged() {
            for checkout in CHECKOUTS {
                let root = TempDir::new().unwrap();
                write_config(&root, &format!("[limits]\nmax_bytes_per_file = {LIMIT}\n"));
                write_lfs_asset(&root, checkout);

                let (output, report) = run_check_json(&root);

                assert_exit_code(&output, exit_codes::SUCCESS);
                assert_eq!(
                    violations(&report),
                    Vec::<serde_json::Value>::new(),
                    "{checkout:?}"
                );
            }
        }
    }

    mod and_the_asset_is_over_the_limit_for_lfs_objects {
        use super::*;

        fn write_fixture(root: &TempDir, checkout: Checkout) {
            // "1 kB" is LFS_LIMIT bytes, written as a size with a unit.
            write_limit_config(root, "\"1 kB\"");
            write_lfs_asset(root, checkout);
        }

        #[test]
        fn it_reports_the_content_size_whether_or_not_the_asset_is_smudged() {
            for checkout in CHECKOUTS {
                let root = TempDir::new().unwrap();
                write_fixture(&root, checkout);

                let (output, report) = run_check_json(&root);

                assert_exit_code(&output, exit_codes::CHECK_FAILURE);
                assert_eq!(
                    violations(&report),
                    vec![json!({
                        "kind": "lfs_object_bytes",
                        "path": ASSET,
                        "actual": DECLARED,
                        "limit": LFS_LIMIT,
                        "ratchet": null,
                    })],
                    "{checkout:?}"
                );
            }
        }

        #[test]
        fn it_lists_the_asset_in_human_output() {
            let root = TempDir::new().unwrap();
            write_fixture(&root, Checkout::Unsmudged);

            let output = sprawl_guard(&root).arg("check").output().unwrap();

            assert_exit_code(&output, exit_codes::CHECK_FAILURE);
            assert!(
                stdout(&output).contains(indoc! {"
                    Git LFS objects over the byte limit (1):

                      assets/model.bin: 5000 bytes (5.0 kB)
                        limit 1000 bytes (1.0 kB)
                "}),
                "{}",
                stdout(&output)
            );
        }

        #[test]
        fn it_names_git_lfs_in_sarif_output() {
            let root = TempDir::new().unwrap();
            write_fixture(&root, Checkout::Unsmudged);

            let (output, sarif) = run_json(&root, &["check", "--format", "sarif"]);

            assert_exit_code(&output, exit_codes::CHECK_FAILURE);
            let result = &sarif["runs"][0]["results"][0];
            assert_eq!(
                json!({ "ruleId": result["ruleId"], "message": result["message"]["text"] }),
                json!({
                    "ruleId": "sprawl-guard/lfs-object-bytes",
                    "message": format!(
                        "{ASSET} is a Git LFS object of {DECLARED} bytes, over the limit of {LFS_LIMIT}."
                    ),
                })
            );
        }

        #[test]
        fn explain_reports_the_storage_and_the_limit_for_lfs_objects() {
            let root = TempDir::new().unwrap();
            write_fixture(&root, Checkout::Unsmudged);

            let output = sprawl_guard(&root)
                .args(["explain", ASSET])
                .output()
                .unwrap();

            assert_exit_code(&output, exit_codes::SUCCESS);
            assert!(
                stdout(&output).contains(
                    "size: 5000 bytes (5.0 kB), stored in Git LFS, limit 1000 bytes (1.0 kB)"
                ),
                "{}",
                stdout(&output)
            );
        }

        mod and_update_writes_the_ratchet {
            use super::*;

            #[test]
            fn it_records_the_content_size_for_lfs_storage() {
                let root = TempDir::new().unwrap();
                write_fixture(&root, Checkout::Unsmudged);

                let (output, _) = run_ratchet_file_write(&root, "update");

                assert_exit_code(&output, exit_codes::SUCCESS);
                assert_eq!(
                    byte_ceilings(&root),
                    vec![(ASSET.to_owned(), FileStorage::Lfs, DECLARED)]
                );
            }

            mod and_the_asset_is_then_smudged {
                use super::*;

                #[test]
                fn check_passes_at_the_same_ceiling() {
                    let root = TempDir::new().unwrap();
                    write_fixture(&root, Checkout::Unsmudged);
                    run_ratchet_file_write(&root, "update");
                    smudge(&root);

                    let (output, _) = run_json(
                        &root,
                        &["check", "--format", "json", "--require-ratchet-current"],
                    );

                    assert_exit_code(&output, exit_codes::SUCCESS);
                }
            }

            mod and_gitattributes_then_stops_assigning_it_to_lfs {
                use super::*;

                #[test]
                fn check_reports_the_lfs_ceiling_as_stale_and_the_file_over_the_git_limit() {
                    let root = TempDir::new().unwrap();
                    write_fixture(&root, Checkout::Unsmudged);
                    run_ratchet_file_write(&root, "update");
                    smudge(&root);
                    write_file(&root, ".gitattributes", "");

                    let (output, report) = run_check_json(&root);

                    assert_exit_code(&output, exit_codes::CHECK_FAILURE);
                    assert_eq!(
                        violation_kinds(&report),
                        vec![json!("file_bytes"), json!("lfs_object_bytes_stale")]
                    );
                }
            }
        }
    }

    mod and_an_override_lifts_the_limit_for_lfs_objects {
        use super::*;

        #[test]
        fn check_passes() {
            let root = TempDir::new().unwrap();
            write_config(
                &root,
                &format!(
                    "[limits]\nmax_bytes_per_lfs_object = {LFS_LIMIT}\n\n[[overrides]]\npaths = [\"assets/**\"]\nmax_bytes_per_lfs_object = \"unlimited\"\nreason = \"models are large\"\n"
                ),
            );
            write_lfs_asset(&root, Checkout::Unsmudged);

            let (output, _) = run_check_json(&root);

            assert_exit_code(&output, exit_codes::SUCCESS);
        }
    }
}

mod when_gitattributes_does_not_assign_a_pointer_to_lfs {
    use super::*;

    #[test]
    fn it_measures_the_pointer_on_disk_against_the_limit_for_files_in_git() {
        let root = TempDir::new().unwrap();
        write_limit_config(&root, &LFS_LIMIT.to_string());
        write_file(&root, ASSET, &pointer(DECLARED));

        let (output, report) = run_check_json(&root);

        assert_exit_code(&output, exit_codes::CHECK_FAILURE);
        assert_eq!(
            violations(&report),
            vec![json!({
                "kind": "file_bytes",
                "path": ASSET,
                "actual": pointer(DECLARED).len(),
                "limit": LIMIT,
                "ratchet": null,
            })],
        );
    }
}

mod when_a_file_in_lfs_almost_matches_the_pointer_format {
    use super::*;

    #[test]
    fn it_is_measured_on_disk() {
        let root = TempDir::new().unwrap();
        write_limit_config(&root, "10");
        write_file(&root, ".gitattributes", ATTRIBUTES);
        let near_miss = pointer(DECLARED).replace('\n', "\r\n");
        write_file(&root, ASSET, &near_miss);

        let (output, report) = run_check_json(&root);

        assert_exit_code(&output, exit_codes::CHECK_FAILURE);
        assert_eq!(
            violations(&report),
            vec![json!({
                "kind": "lfs_object_bytes",
                "path": ASSET,
                "actual": near_miss.len(),
                "limit": 10,
                "ratchet": null,
            })],
        );
    }
}

mod when_a_nested_gitattributes_cannot_be_read {
    use super::*;

    /// Path of an asset under the directory whose `.gitattributes` can't be read.
    const NESTED_ASSET: &str = "vendor/model.bin";

    /// Writes an LFS ceiling for a downloaded asset over both byte limits, whose `.gitattributes`
    /// is a directory, so reading it fails.
    fn write_fixture(root: &TempDir) {
        write_config(
            root,
            &format!(
                "[traversal]\nunreadable_paths = \"warn\"\n\n[limits]\nmax_bytes_per_file = {LIMIT}\nmax_bytes_per_lfs_object = {LFS_LIMIT}\n"
            ),
        );
        write_ratchet(
            root,
            &format!(
                "schema_version = 1\n\n[file_sizes]\n\"{NESTED_ASSET}\" = {{ max_bytes = {DECLARED}, storage = \"lfs\" }}\n"
            ),
        );
        write_file(
            root,
            NESTED_ASSET,
            &"x".repeat(DECLARED.try_into().unwrap()),
        );
        std::fs::create_dir(root.path().join("vendor/.gitattributes")).unwrap();
    }

    #[test]
    fn check_reports_the_directory_as_a_gap_without_judging_the_asset() {
        let root = TempDir::new().unwrap();
        write_fixture(&root);

        let (output, report) = run_json(
            &root,
            &["check", "--format", "json", "--require-ratchet-current"],
        );

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(
            json!({
                "violations": report["violations"],
                "gap_kind": report["coverage"]["unreadable_paths"][0]["kind"],
                "gap_path": report["coverage"]["unreadable_paths"][0]["path"],
            }),
            json!({
                "violations": [],
                "gap_kind": "read_git_attributes_failed",
                "gap_path": "vendor",
            })
        );
    }

    #[test]
    fn check_still_judges_files_outside_the_directory() {
        let root = TempDir::new().unwrap();
        write_fixture(&root);
        write_file(
            &root,
            "big.txt",
            &"x".repeat((LIMIT + 1).try_into().unwrap()),
        );

        let (output, report) = run_json(&root, &["check", "--format", "json"]);

        assert_exit_code(&output, exit_codes::CHECK_FAILURE);
        assert_eq!(
            violations(&report)
                .iter()
                .map(|violation| (violation["kind"].clone(), violation["path"].clone()))
                .collect::<Vec<_>>(),
            [(json!("file_bytes"), json!("big.txt"))]
        );
    }

    /// Appends `entry` to the fixture's ratchet, requires it to be current, and returns the kind
    /// and path of each violation after checking that the check failed.
    fn stale_violations_with(
        root: &TempDir,
        entry: &str,
    ) -> Vec<(serde_json::Value, serde_json::Value)> {
        write_ratchet(root, &format!("{}\n{entry}", read_ratchet(root)));

        let (output, report) = run_json(
            root,
            &["check", "--format", "json", "--require-ratchet-current"],
        );

        assert_exit_code(&output, exit_codes::CHECK_FAILURE);
        violations(&report)
            .iter()
            .map(|violation| (violation["kind"].clone(), violation["path"].clone()))
            .collect()
    }

    #[test]
    fn check_still_reports_a_stale_line_ceiling_in_the_directory() {
        let root = TempDir::new().unwrap();
        write_fixture(&root);

        assert_eq!(
            stale_violations_with(
                &root,
                "[[files]]\npath = \"vendor/deleted.rs\"\nrole = \"production\"\nmax_lines = 500\n",
            ),
            [(json!("file_lines_stale"), json!("vendor/deleted.rs"))]
        );
    }

    #[test]
    fn check_still_reports_a_stale_ceiling_for_a_file_whose_storage_is_known() {
        let root = TempDir::new().unwrap();
        write_fixture(&root);
        // A deeper `.gitattributes` that can be read moves this asset to Git LFS, so its regular
        // ceiling is stale even though the parent's `.gitattributes` can't be read.
        write_file(&root, "vendor/moved/.gitattributes", ATTRIBUTES);
        write_file(&root, "vendor/moved/model.bin", "x");

        assert_eq!(
            stale_violations_with(
                &root,
                // The fixture's ratchet ends with its `[file_sizes]` table, so this line joins it.
                &format!("\"vendor/moved/model.bin\" = {DECLARED}\n"),
            ),
            [(json!("file_bytes_stale"), json!("vendor/moved/model.bin"))]
        );
    }

    #[test]
    fn tighten_refuses_to_rewrite_the_ratchet() {
        let root = TempDir::new().unwrap();
        write_fixture(&root);
        let before = read_ratchet(&root);

        let output = sprawl_guard(&root)
            .args(["tighten", "--write"])
            .output()
            .unwrap();

        assert_exit_code(&output, exit_codes::OPERATIONAL_ERROR);
        assert_eq!(read_ratchet(&root), before);
    }
}

// Unreadable files are created with Unix permissions, which WASI lacks.
#[cfg(unix)]
mod when_a_file_moved_into_lfs_cannot_be_read {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    #[test]
    fn check_reports_its_regular_ceiling_stale() {
        let root = TempDir::new().unwrap();
        write_config(
            &root,
            &format!(
                "[traversal]\nunreadable_paths = \"warn\"\n\n[limits]\nmax_bytes_per_file = {LIMIT}\nmax_bytes_per_lfs_object = {LFS_LIMIT}\n"
            ),
        );
        write_lfs_asset(&root, Checkout::Unsmudged);
        write_ratchet(
            &root,
            &format!("schema_version = 1\n\n[file_sizes]\n\"{ASSET}\" = {DECLARED}\n"),
        );
        // The pointer is small enough to be read, and reading it fails.
        std::fs::set_permissions(
            root.path().join(ASSET),
            std::fs::Permissions::from_mode(0o000),
        )
        .unwrap();

        let (output, report) = run_json(
            &root,
            &["check", "--format", "json", "--require-ratchet-current"],
        );

        assert_exit_code(&output, exit_codes::CHECK_FAILURE);
        assert_eq!(
            json!({
                "violations": violations(&report)
                    .iter()
                    .map(|violation| json!([violation["kind"], violation["path"]]))
                    .collect::<Vec<_>>(),
                "gap_kind": report["coverage"]["unreadable_paths"][0]["kind"],
            }),
            json!({
                "violations": [["file_bytes_stale", ASSET]],
                "gap_kind": "read_file_failed",
            })
        );
    }
}

mod when_an_unreadable_gitattributes_is_outside_every_finite_byte_limit {
    use super::*;

    #[test]
    fn check_does_not_read_it_or_report_a_gap() {
        let root = TempDir::new().unwrap();
        write_config(
            &root,
            &format!(
                "[[overrides]]\npaths = [\"assets/**\"]\nmax_bytes_per_file = {LIMIT}\nreason = \"assets stay small\"\n"
            ),
        );
        write_file(&root, "src/notes.txt", "notes\n");
        std::fs::create_dir_all(root.path().join("src/.gitattributes")).unwrap();

        let (output, report) = run_json(&root, &["check", "--format", "json"]);

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(report["coverage"], json!({"kind": "complete"}));
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
