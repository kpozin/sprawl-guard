use assert_fs::TempDir;
use indoc::indoc;
use serde_json::json;

use super::super::common::*;

#[cfg(unix)]
mod when_ignore_policy_is_a_symlink {
    use super::*;

    #[test]
    fn it_refuses_external_or_broken_policy_before_ratchet_mutation() {
        for filename in [".ignore", ".gitignore", ".rgignore", ".fdignore"] {
            let outer = TempDir::new().unwrap();
            let root = TempDir::new().unwrap();
            std::fs::write(root.join("lib.rs"), "fn a() {}\nfn b() {}\n").unwrap();
            std::fs::write(root.join("sprawl-guard.toml"),
                "schema_version = 2\n[ignore]\nrespect_rgignore = true\nrespect_fdignore = true\n[limits.max_lines_per_file]\nproduction = 1\n").unwrap();
            let policy = outer.path().join("policy");
            std::os::unix::fs::symlink(&policy, root.join(filename)).unwrap();
            for rules in [Some("lib.rs\n"), Some(""), None] {
                if let Some(rules) = rules {
                    std::fs::write(&policy, rules).unwrap();
                } else {
                    std::fs::remove_file(&policy).unwrap();
                }
                let result = sprawl_guard(&root)
                    .args(["update", "--write"])
                    .output()
                    .unwrap();

                assert_exit_code(&result, exit_codes::OPERATIONAL_ERROR);
                assert!(stderr(&result).contains("symlink"), "{}", stderr(&result));
                assert!(!root.join("sprawl-guard.ratchet.toml").exists());
            }
        }
    }
}

mod when_an_ignore_file_has_a_malformed_inclusion {
    use super::*;

    const IGNORE_PATHS: &[&str] = &[
        ".ignore",
        "src/.gitignore",
        "src/.rgignore",
        "src/.fdignore",
    ];

    fn fixture(ignore_path: &str) -> TempDir {
        let root = TempDir::new().unwrap();
        write_rust_file_limit_config(&root, 1);
        write_rust_source_with_function_count(&root, "src/lib.rs", 2);
        write_file(&root, ignore_path, "*.rs\n!{lib.rs\n");
        root
    }

    mod and_check_uses_the_default_error_policy {
        use super::*;

        #[test]
        fn it_fails_with_the_offending_ignore_path() {
            for path in IGNORE_PATHS {
                let root = fixture(path);

                let (output, report) = run_check_json(&root);

                assert_exit_code(&output, exit_codes::OPERATIONAL_ERROR);
                assert_eq!(report["coverage"]["kind"], "partial");
                let incident = &report["coverage"]["traversal_incidents"][0];
                assert_eq!(incident["kind"], "ignore_parse_failed");
                assert_eq!(incident["path"], *path);
                assert_eq!(incident["severity"], "error");
                assert!(incident["detail"].as_str().unwrap().contains("unclosed"));
            }
        }
    }

    mod and_check_allows_warnings {
        use super::*;

        #[test]
        fn it_reports_partial_coverage_and_still_measures_visible_files() {
            for path in IGNORE_PATHS {
                let root = fixture(path);
                write_file(&root, "web/app.ts", "export const answer = 42;\n");

                let (output, report) = run_json(
                    &root,
                    &[
                        "check",
                        "--format",
                        "json",
                        "--traversal-error-policy",
                        "warn",
                    ],
                );

                assert_exit_code(&output, exit_codes::SUCCESS);
                assert_eq!(report["coverage"]["kind"], "partial");
                assert_eq!(report["summary"]["files_checked"], 1);
                let incident = &report["coverage"]["traversal_incidents"][0];
                assert_eq!(incident["path"], *path);
                assert_eq!(incident["severity"], "warning");
            }
        }
    }

    mod and_a_ratchet_is_requested {
        use super::*;

        #[test]
        fn it_rejects_previews_and_writes_despite_warning_only_configuration() {
            for path in IGNORE_PATHS {
                let root = fixture(path);
                write_config(&root, "[traversal]\nerror_policy = \"warn\"\n");
                for command in ["update", "tighten"] {
                    for args in [vec![command], vec![command, "--write"]] {
                        let output = sprawl_guard(&root).args(args).output().unwrap();

                        assert_exit_code(&output, exit_codes::OPERATIONAL_ERROR);
                        assert_eq!(stdout(&output), "");
                        assert!(!root.path().join("sprawl-guard.ratchet.toml").exists());
                    }
                }

                write_ratchet(
                    &root,
                    "schema_version = 1\n# preserve this comment\nfiles = []\n",
                );
                let before = read_ratchet(&root);
                for command in ["update", "tighten"] {
                    for args in [vec![command], vec![command, "--write"]] {
                        let output = sprawl_guard(&root).args(args).output().unwrap();

                        assert_exit_code(&output, exit_codes::OPERATIONAL_ERROR);
                        assert!(stderr(&output).contains(path));
                        assert_eq!(read_ratchet(&root), before);
                    }
                }
            }
        }
    }

    mod and_the_path_is_explained {
        use super::*;

        #[test]
        fn it_rejects_the_incomplete_traversal() {
            for path in IGNORE_PATHS {
                let root = fixture(path);

                let output = sprawl_guard(&root)
                    .args(["explain", "src/lib.rs"])
                    .output()
                    .unwrap();

                assert_exit_code(&output, exit_codes::OPERATIONAL_ERROR);
                assert!(stderr(&output).contains(path));
            }
        }
    }
}

mod when_ambient_ignore_rules_hide_a_ratchet_file {
    use super::*;

    #[test]
    fn it_keeps_check_and_tighten_independent_of_machine_and_parent_settings() {
        for ambient_path in [
            ".ignore",
            ".gitignore",
            "repo/.git/info/exclude",
            "xdg/git/ignore",
        ] {
            let outer = TempDir::new().unwrap();
            write_file(&outer, "repo/.git/HEAD", "ref: refs/heads/main\n");
            write_file(&outer, "repo/.gitignore", "sprawl-guard.toml\n");
            write_file(
                &outer,
                "repo/sprawl-guard.toml",
                indoc! {r#"
                schema_version = 2
                [limits.max_lines_per_file]
                production = 1
            "#},
            );
            write_file(&outer, "repo/src/lib.rs", "fn first() {}\nfn second() {}\n");
            let ratchet = indoc! {r#"
                schema_version = 1
                [[files]]
                path = "src/lib.rs"
                role = "production"
                max_loc = 2
            "#};
            write_file(&outer, "repo/sprawl-guard.ratchet.toml", ratchet);
            let command = || {
                let mut command = sprawl_guard_no_root();
                command
                    .arg("--root")
                    .arg(outer.path().join("repo"))
                    .env("HOME", outer.path().join("home"))
                    .env("USERPROFILE", outer.path().join("home"))
                    .env("XDG_CONFIG_HOME", outer.path().join("xdg"));
                command
            };
            let before = command()
                .args(["check", "--format", "json"])
                .output()
                .unwrap();
            write_file(&outer, ambient_path, "*.rs\n");

            let after = command()
                .args(["check", "--format", "json"])
                .output()
                .unwrap();
            let tightened = command().args(["tighten", "--write"]).output().unwrap();
            let written =
                std::fs::read_to_string(outer.path().join("repo/sprawl-guard.ratchet.toml"))
                    .unwrap();

            assert_exit_code(&before, exit_codes::SUCCESS);
            assert_exit_code(&after, exit_codes::SUCCESS);
            assert_eq!(json_stdout(&before), json_stdout(&after), "{ambient_path}");
            assert_exit_code(&tightened, exit_codes::SUCCESS);
            assert_eq!(parse_toml(&written), parse_toml(ratchet));
            assert_eq!(json_stdout(&after)["summary"]["files_checked"], json!(1));
        }
    }
}

mod when_parent_ignore_files_are_malformed {
    use super::*;

    #[test]
    fn it_ignores_parent_errors_without_spending_the_traversal_budget() {
        let root = TempDir::new().unwrap();
        write_file(&root, "repo/lib.rs", "fn first() {}\n");
        write_file(
            &root,
            "repo/sprawl-guard.toml",
            indoc! {r#"
            schema_version = 2
            [files]
            exclude = ["sprawl-guard.toml"]
            [traversal]
            max_entries = 4
        "#},
        );
        write_file(
            &root,
            "repo/sprawl-guard.ratchet.toml",
            "schema_version = 1\n",
        );
        for filename in [".ignore", ".gitignore", ".rgignore", ".fdignore"] {
            write_file(&root, filename, "*.rs\n!{lib.rs\n");
        }
        for args in [
            vec!["check"],
            vec!["explain", "lib.rs"],
            vec!["update", "--write"],
            vec!["tighten"],
        ] {
            let output = sprawl_guard_no_root()
                .arg("--root")
                .arg(root.path().join("repo"))
                .args(args)
                .output()
                .unwrap();

            assert_exit_code(&output, exit_codes::SUCCESS);
        }
    }
}
