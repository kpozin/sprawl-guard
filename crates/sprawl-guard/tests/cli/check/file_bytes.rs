use assert_fs::TempDir;
use indoc::indoc;
use serde_json::json;

use super::super::common::*;

/// Byte limit that most fixtures here configure.
const LIMIT: u64 = 10;

/// Writes a file of `size` bytes.
fn write_file_of_size(root: &TempDir, path: &str, size: usize) {
    write_file(root, path, &"x".repeat(size));
}

/// Returns the JSON violations of `kind` from a check report.
fn violations_of_kind(report: &serde_json::Value, kind: &str) -> Vec<serde_json::Value> {
    report["violations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|violation| violation["kind"] == kind)
        .cloned()
        .collect()
}

/// Returns the paths of the JSON violations of `kind` from a check report.
fn violation_paths(report: &serde_json::Value, kind: &str) -> Vec<serde_json::Value> {
    violations_of_kind(report, kind)
        .iter()
        .map(|violation| violation["path"].clone())
        .collect()
}

mod when_only_the_default_limit_is_set {
    use super::*;

    fn write_default_limit_config(root: &TempDir) {
        write_config(root, &format!("[limits]\nmax_bytes_per_file = {LIMIT}\n"));
    }

    mod and_a_binary_file_is_over_it {
        use super::*;

        #[test]
        fn it_reports_the_file_in_json_output() {
            let root = TempDir::new().unwrap();
            write_default_limit_config(&root);
            write_file_of_size(&root, "assets/logo.png", 20);

            let (output, report) = run_check_json(&root);

            assert_exit_code(&output, exit_codes::CHECK_FAILURE);
            assert_eq!(
                violations_of_kind(&report, "file_bytes"),
                vec![json!({
                    "kind": "file_bytes",
                    "path": "assets/logo.png",
                    "actual": 20,
                    "limit": LIMIT,
                    "ratchet": null,
                })],
            );
        }

        #[test]
        fn it_lists_the_file_in_human_output() {
            let root = TempDir::new().unwrap();
            write_default_limit_config(&root);
            write_file_of_size(&root, "assets/logo.png", 2_500);

            let output = sprawl_guard(&root).arg("check").output().unwrap();

            assert_exit_code(&output, exit_codes::CHECK_FAILURE);
            assert!(
                stdout(&output).contains(indoc! {"
                    Files over the byte limit (1):

                      assets/logo.png: 2500 bytes (2.5 kB)
                        limit 10 bytes
                "}),
                "{}",
                stdout(&output)
            );
        }
    }

    mod and_a_file_of_a_disabled_language_is_over_it {
        use super::*;

        #[test]
        fn it_reports_the_file() {
            let root = TempDir::new().unwrap();
            write_default_limit_config(&root);
            write_file_of_size(&root, "src/generated.rs", 20);

            let (output, report) = run_check_json(&root);

            assert_exit_code(&output, exit_codes::CHECK_FAILURE);
            assert_eq!(
                violations_of_kind(&report, "file_bytes")[0]["path"],
                "src/generated.rs"
            );
        }
    }

    mod and_every_file_is_within_it {
        use super::*;

        #[test]
        fn it_passes() {
            let root = TempDir::new().unwrap();
            write_default_limit_config(&root);
            write_file_of_size(&root, "assets/logo.png", 10);

            let (output, _) = run_check_json(&root);

            assert_exit_code(&output, exit_codes::SUCCESS);
        }
    }
}

mod when_an_override_changes_the_default {
    use super::*;

    fn write_override_config(root: &TempDir, default: &str, assets: &str) {
        write_config(
            root,
            &format!(
                "[limits]\nmax_bytes_per_file = {default}\n\n[[overrides]]\npaths = [\"assets/**\"]\nmax_bytes_per_file = {assets}\nreason = \"asset budget\"\n"
            ),
        );
    }

    mod and_it_tightens_the_limit {
        use super::*;

        #[test]
        fn it_reports_files_over_the_override() {
            let root = TempDir::new().unwrap();
            write_override_config(&root, "\"unlimited\"", "5");
            write_file_of_size(&root, "assets/logo.png", 6);
            write_file_of_size(&root, "docs/manual.pdf", 600);

            let (output, report) = run_check_json(&root);

            assert_exit_code(&output, exit_codes::CHECK_FAILURE);
            let paths = violation_paths(&report, "file_bytes");
            assert_eq!(paths, vec![json!("assets/logo.png")]);
        }
    }

    mod and_it_loosens_the_limit_to_unlimited {
        use super::*;

        #[test]
        fn it_reports_only_files_outside_the_override() {
            let root = TempDir::new().unwrap();
            write_override_config(&root, &LIMIT.to_string(), "\"unlimited\"");
            write_file_of_size(&root, "assets/logo.png", 600);
            write_file_of_size(&root, "docs/manual.pdf", 600);

            let (output, report) = run_check_json(&root);

            assert_exit_code(&output, exit_codes::CHECK_FAILURE);
            let paths = violation_paths(&report, "file_bytes");
            assert_eq!(paths, vec![json!("docs/manual.pdf")]);
        }
    }
}

mod when_limits_are_sizes_with_units {
    use super::*;

    /// Writes a default of 1 kB, which is 1000 bytes, and 1 KiB, which is 1024 bytes, for assets.
    fn write_unit_config(root: &TempDir) {
        write_config(
            root,
            indoc! {r#"
                [limits]
                max_bytes_per_file = "1 kB"

                [[overrides]]
                paths = ["assets/**"]
                max_bytes_per_file = "1 KiB"
                reason = "asset budget"
            "#},
        );
    }

    #[test]
    fn it_reports_files_over_each_limit_in_bytes() {
        let root = TempDir::new().unwrap();
        write_unit_config(&root);
        write_file_of_size(&root, "assets/at_limit.bin", 1_024);
        write_file_of_size(&root, "assets/over_limit.bin", 1_025);
        write_file_of_size(&root, "docs/at_limit.txt", 1_000);
        write_file_of_size(&root, "docs/over_limit.txt", 1_001);

        let (output, report) = run_check_json(&root);

        assert_exit_code(&output, exit_codes::CHECK_FAILURE);
        assert_eq!(
            violations_of_kind(&report, "file_bytes")
                .iter()
                .map(|violation| (violation["path"].clone(), violation["limit"].clone()))
                .collect::<Vec<_>>(),
            vec![
                (json!("assets/over_limit.bin"), json!(1_024)),
                (json!("docs/over_limit.txt"), json!(1_000)),
            ]
        );
    }
}

mod when_the_ratchet_has_a_byte_entry {
    use super::*;

    fn write_byte_ratchet(root: &TempDir, path: &str, max_bytes: u64) {
        write_config(root, &format!("[limits]\nmax_bytes_per_file = {LIMIT}\n"));
        write_ratchet(
            root,
            &format!("schema_version = 1\n\n[file_sizes]\n\"{path}\" = {max_bytes}\n"),
        );
    }

    mod and_the_file_grew_past_the_ceiling {
        use super::*;

        #[test]
        fn it_reports_the_violation_with_the_ceiling() {
            let root = TempDir::new().unwrap();
            write_byte_ratchet(&root, "assets/logo.png", 20);
            write_file_of_size(&root, "assets/logo.png", 21);

            let (output, report) = run_check_json(&root);

            assert_exit_code(&output, exit_codes::CHECK_FAILURE);
            assert_eq!(violations_of_kind(&report, "file_bytes")[0]["ratchet"], 20);
        }
    }

    mod and_the_file_is_at_the_ceiling {
        use super::*;

        #[test]
        fn it_passes() {
            let root = TempDir::new().unwrap();
            write_byte_ratchet(&root, "assets/logo.png", 20);
            write_file_of_size(&root, "assets/logo.png", 20);

            let (output, _) = run_check_json(&root);

            assert_exit_code(&output, exit_codes::SUCCESS);
        }
    }

    mod and_the_file_shrank_below_the_ceiling {
        use super::*;

        #[test]
        fn it_reports_the_stale_entry_with_its_size() {
            let root = TempDir::new().unwrap();
            write_byte_ratchet(&root, "assets/logo.png", 20);
            write_file_of_size(&root, "assets/logo.png", 15);

            let (output, report) = run_check_json(&root);

            assert_exit_code(&output, exit_codes::CHECK_FAILURE);
            assert_eq!(
                violations_of_kind(&report, "file_bytes_stale"),
                vec![json!({
                    "kind": "file_bytes_stale",
                    "path": "assets/logo.png",
                    "actual": 15,
                    "limit": LIMIT,
                    "ratchet": 20,
                })],
            );
        }
    }

    mod and_the_file_is_gone {
        use super::*;

        #[test]
        fn it_reports_the_stale_entry_without_a_size() {
            let root = TempDir::new().unwrap();
            write_byte_ratchet(&root, "assets/logo.png", 20);

            let (output, report) = run_check_json(&root);

            assert_exit_code(&output, exit_codes::CHECK_FAILURE);
            assert_eq!(
                violations_of_kind(&report, "file_bytes_stale"),
                vec![json!({
                    "kind": "file_bytes_stale",
                    "path": "assets/logo.png",
                    "actual": null,
                    "limit": null,
                    "ratchet": 20,
                })],
            );
        }
    }
}

mod when_a_source_is_over_both_its_line_and_byte_limits {
    use super::*;

    fn write_fixture(root: &TempDir) {
        write_config(
            root,
            &format!(
                "[limits]\nmax_bytes_per_file = {LIMIT}\n\n[limits.max_lines_per_file]\nproduction = 1\n\n[languages.rust]\nenabled = true\n"
            ),
        );
        write_rust_source_with_function_count(root, "src/lib.rs", 3);
    }

    #[test]
    fn update_records_both_ceilings_and_check_then_passes() {
        let root = TempDir::new().unwrap();
        write_fixture(&root);

        let (output, ratchet) = run_ratchet_file_write(&root, "update");

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(
            production_file_ratchet_entry(&ratchet, "src/lib.rs")
                .map(|entry| entry.max_lines.get()),
            Some(3)
        );
        let size = std::fs::metadata(root.path().join("src/lib.rs"))
            .unwrap()
            .len();
        let sizes = ratchet
            .file_sizes
            .iter()
            .map(|(path, entry)| (path.as_str(), entry.max_bytes.into_inner()))
            .collect::<Vec<_>>();
        assert_eq!(sizes, vec![("src/lib.rs", size)]);
        let (output, _) = run_check_json(&root);
        assert_exit_code(&output, exit_codes::SUCCESS);
    }

    #[test]
    fn tighten_lowers_both_ceilings_after_the_file_shrinks() {
        let root = TempDir::new().unwrap();
        write_fixture(&root);
        run_ratchet_file_write(&root, "update");
        write_rust_source_with_function_count(&root, "src/lib.rs", 2);

        let (output, ratchet) = run_ratchet_file_write(&root, "tighten");

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(
            production_file_ratchet_entry(&ratchet, "src/lib.rs")
                .map(|entry| entry.max_lines.get()),
            Some(2)
        );
        let size = std::fs::metadata(root.path().join("src/lib.rs"))
            .unwrap()
            .len();
        assert_eq!(
            ratchet
                .file_sizes
                .values()
                .next()
                .unwrap()
                .max_bytes
                .into_inner(),
            size
        );
        let (output, _) = run_check_json(&root);
        assert_exit_code(&output, exit_codes::SUCCESS);
    }
}

mod when_an_uncounted_source_is_over_its_byte_limit_and_has_a_line_ceiling {
    use super::*;

    /// Path of the source that is too large to count and over its byte limit.
    const LARGE_SOURCE: &str = "src/large.rs";

    /// Writes a source too large to count, with an existing line ceiling, next to a small source,
    /// in a directory that allows one direct file.
    ///
    /// Unreadable paths only warn, so the uncounted source is a gap that leaves the check passing
    /// once its byte and directory violations are ratcheted.
    fn write_fixture(root: &TempDir) {
        write_config(
            root,
            indoc! {r#"
                [traversal]
                max_parsed_code_file_bytes = 20
                unreadable_paths = "warn"

                [limits]
                max_bytes_per_file = 30

                [limits.max_lines_per_file]
                production = 1

                [limits.max_direct_files_per_directory]
                production = 1

                [languages.rust]
                enabled = true
            "#},
        );
        write_ratchet(
            root,
            "schema_version = 1\n\n[[files]]\npath = \"src/large.rs\"\nrole = \"production\"\nmax_lines = 5\n",
        );
        write_file_of_size(root, LARGE_SOURCE, 60);
        write_file(root, "src/small.rs", "fn a() {}\n");
    }

    fn violation_kinds(report: &serde_json::Value) -> Vec<serde_json::Value> {
        report["violations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|violation| violation["kind"].clone())
            .collect()
    }

    fn uncounted_paths(report: &serde_json::Value) -> Vec<serde_json::Value> {
        report["coverage"]["uncounted_files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|file| file["path"].clone())
            .collect()
    }

    fn ceilings(root: &TempDir) -> (Option<u32>, Vec<(String, u64)>, Vec<(String, u32)>) {
        let ratchet = read_ratchet_file(root);
        (
            production_file_ratchet_entry(&ratchet, LARGE_SOURCE)
                .map(|entry| entry.max_lines.get()),
            ratchet
                .file_sizes
                .iter()
                .map(|(path, entry)| (path.as_str().to_owned(), entry.max_bytes.into_inner()))
                .collect(),
            production_directory_ratchet_entries(&ratchet)
                .into_iter()
                .map(|(_, path, max_direct_files)| (path.to_owned(), max_direct_files))
                .collect(),
        )
    }

    mod and_check_runs_first {
        use super::*;

        #[test]
        fn it_reports_the_byte_violation_the_gap_and_the_crowded_directory() {
            let root = TempDir::new().unwrap();
            write_fixture(&root);

            let (output, report) = run_check_json(&root);

            assert_exit_code(&output, exit_codes::CHECK_FAILURE);
            assert_eq!(
                violation_kinds(&report),
                vec![json!("file_bytes"), json!("directory_direct_files")]
            );
            assert_eq!(uncounted_paths(&report), vec![json!(LARGE_SOURCE)]);
        }
    }

    mod and_update_writes_the_ratchet {
        use super::*;

        #[test]
        fn it_adds_only_the_new_ceilings_and_check_then_passes_with_the_gap() {
            let root = TempDir::new().unwrap();
            write_fixture(&root);

            let (output, _) = run_ratchet_file_write(&root, "update");

            assert_exit_code(&output, exit_codes::SUCCESS);
            assert_eq!(
                ceilings(&root),
                (
                    Some(5),
                    vec![(LARGE_SOURCE.to_owned(), 60)],
                    vec![("src".to_owned(), 2)]
                )
            );
            let (output, report) = run_check_json(&root);
            assert_exit_code(&output, exit_codes::SUCCESS);
            assert_eq!(uncounted_paths(&report), vec![json!(LARGE_SOURCE)]);
        }

        mod and_the_source_shrinks_but_stays_over_both_limits {
            use super::*;

            #[test]
            fn tighten_lowers_only_the_byte_ceiling() {
                let root = TempDir::new().unwrap();
                write_fixture(&root);
                run_ratchet_file_write(&root, "update");
                write_file_of_size(&root, LARGE_SOURCE, 50);

                let (output, _) = run_ratchet_file_write(&root, "tighten");

                assert_exit_code(&output, exit_codes::SUCCESS);
                assert_eq!(
                    ceilings(&root),
                    (
                        Some(5),
                        vec![(LARGE_SOURCE.to_owned(), 50)],
                        vec![("src".to_owned(), 2)]
                    )
                );
                let (output, _) = run_check_json(&root);
                assert_exit_code(&output, exit_codes::SUCCESS);
            }
        }
    }
}

mod when_the_ratchet_file_uses_the_0_6_0_array_of_tables {
    use super::*;

    #[test]
    fn each_ratchet_command_rewrites_it_as_a_table_keyed_by_path() {
        for command in ["update", "tighten"] {
            let root = TempDir::new().unwrap();
            write_config(&root, &format!("[limits]\nmax_bytes_per_file = {LIMIT}\n"));
            write_file_of_size(&root, "assets/a.bin", 20);
            write_ratchet(
                &root,
                indoc! {r#"
                    schema_version = 1

                    [[file_sizes]]
                    path = "assets/a.bin"
                    storage = "git"
                    max_bytes = 20
                "#},
            );

            let output = sprawl_guard(&root)
                .args([command, "--write"])
                .output()
                .unwrap();

            assert_exit_code(&output, exit_codes::SUCCESS);
            assert_eq!(
                stdout(&output),
                "No ratchet changes; rewrote sprawl-guard.ratchet.toml in the current form\n"
            );
            assert!(
                read_ratchet(&root).ends_with("[file_sizes]\n\"assets/a.bin\" = 20\n"),
                "{command}"
            );
        }
    }
}
