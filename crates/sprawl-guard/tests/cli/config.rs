use assert_fs::TempDir;
use indoc::indoc;

use super::common::*;

fn write_sample_config(root: &TempDir) {
    write_config(
        root,
        indoc! {r#"
            [limits]
            max_lines_per_file = { production = 123 }

            [languages.rust]
            enabled = true
        "#},
    );
}

/// Runs one `config` subcommand on the sample config and checks its line-limit settings.
///
/// `count_lines` is the top-level line-type list the subcommand is expected to print.
fn full_config_output(root: &TempDir, command: &str, count_lines: &[&str]) -> String {
    let output = sprawl_guard(root)
        .args(["config", command])
        .output()
        .unwrap();
    let stdout = stdout(&output);
    let config = parse_toml(&stdout);
    let count_lines = toml::Value::from(count_lines.to_vec());

    assert_exit_code(&output, exit_codes::SUCCESS);
    assert_toml_contains(
        &config,
        toml::toml! {
            [limits.max_lines_per_file]
            count_lines = count_lines
            production = 123
            test = 800

            [languages.Rust]
            enabled = true
        },
    );
    assert_eq!(stderr(&output), "");
    stdout
}

mod resolved {
    use super::*;

    #[test]
    fn it_prints_the_resolved_config_used_by_checks() {
        let root = TempDir::new().unwrap();
        write_sample_config(&root);

        let stdout = full_config_output(&root, "resolved", &["code"]);

        assert!(stdout.starts_with(
            "# Effective settings, for reading only. \
             Run `sprawl-guard config export` for a file you can load.\n\
             schema_version = 2\n"
        ));
    }

    #[test]
    fn it_rejects_legacy_config_and_names_the_upgrade_command() {
        let root = TempDir::new().unwrap();
        write_file(&root, "sprawl-guard.toml", "schema_version = 1\n");

        for command in [
            &["config", "resolved"][..],
            &["check"],
            &["update"],
            &["tighten"],
        ] {
            let output = sprawl_guard(&root).args(command).output().unwrap();

            assert_exit_code(&output, exit_codes::OPERATIONAL_ERROR);
            assert!(
                stderr(&output).contains(
                    "uses schema version 1; this version reads version 2.\n\
                     Run `sprawl-guard config upgrade --write` to upgrade it."
                ),
                "{command:?}: {}",
                stderr(&output)
            );
        }
    }

    #[test]
    fn it_names_the_config_path_when_the_root_is_another_directory() {
        let root = TempDir::new().unwrap();
        write_file(&root, "repo/sprawl-guard.toml", "schema_version = 1\n");

        let output = sprawl_guard(&root)
            .args(["check", "--root", "repo"])
            .output()
            .unwrap();

        assert_exit_code(&output, exit_codes::OPERATIONAL_ERROR);
        assert!(
            stderr(&output).contains(
                "repo/sprawl-guard.toml uses schema version 1; this version reads version 2.\n\
                 Run `sprawl-guard config upgrade --write --config repo/sprawl-guard.toml` to \
                 upgrade it."
            ),
            "{}",
            stderr(&output)
        );
    }

    #[test]
    fn it_shows_inherited_and_automatic_language_line_limits_with_every_counted_type() {
        let root = TempDir::new().unwrap();
        write_sample_config(&root);

        let output = sprawl_guard(&root)
            .args(["config", "resolved"])
            .output()
            .unwrap();
        let config = parse_toml(&stdout(&output));

        assert_eq!(
            config["languages"]["Rust"]["limits"]["max_lines_per_file"],
            toml::Value::Table(toml::toml! {
                count_lines = ["code"]
                production = 123
                test = 800
            })
        );
        assert_eq!(
            config["languages"]["Markdown"]["limits"]["max_lines_per_file"],
            toml::Value::Table(toml::toml! {
                count_lines = ["code", "comments"]
                production = 123
                test = 800
            })
        );
    }

    #[test]
    fn it_lists_the_built_in_and_authored_file_excludes_as_one_list() {
        let root = TempDir::new().unwrap();
        write_config(
            &root,
            indoc! {r#"
                [files]
                extend_exclude = ["generated/**"]
            "#},
        );

        let output = sprawl_guard(&root)
            .args(["config", "resolved"])
            .output()
            .unwrap();
        let config = parse_toml(&stdout(&output));

        assert_exit_code(&output, exit_codes::SUCCESS);
        let excludes = config["files"]["exclude"].as_array().unwrap();
        assert!(
            excludes
                .iter()
                .any(|value| value.as_str() == Some("generated/**"))
        );
        assert!(
            excludes
                .iter()
                .any(|value| value.as_str() == Some("**/pnpm-lock.yaml"))
        );
        assert_eq!(stderr(&output), "");
    }

    mod when_the_removed_detected_option_is_passed {
        use super::*;

        #[test]
        fn it_rejects_the_option() {
            let root = TempDir::new().unwrap();
            write_sample_config(&root);

            let output = sprawl_guard(&root)
                .args(["config", "resolved", "--detected"])
                .output()
                .unwrap();

            assert_exit_code(&output, exit_codes::OPERATIONAL_ERROR);
            assert!(stderr(&output).contains("unexpected argument '--detected'"));
        }
    }
}

mod export {
    use super::*;

    #[test]
    fn it_prints_the_full_explicit_config_suitable_for_committing() {
        let root = TempDir::new().unwrap();
        write_sample_config(&root);

        let stdout = full_config_output(&root, "export", &[]);

        assert!(stdout.starts_with(concat!(
            "#:schema https://kpozin.github.io/sprawl-guard/schemas/v2/",
            "sprawl-guard.schema.json\n",
            "schema_version = 2\n"
        )));
    }
}

mod upgrade {
    use super::*;

    const CURRENT_DIRECTIVE: &str = concat!(
        "#:schema https://kpozin.github.io/sprawl-guard/schemas/v2/",
        "sprawl-guard.schema.json\n"
    );

    fn migrated_v1(source: &str) -> String {
        format!(
            "{CURRENT_DIRECTIVE}{}",
            source.replacen("schema_version = 1", "schema_version = 2", 1)
        )
    }

    mod when_migration_is_required {
        use super::*;

        #[test]
        fn check_reports_a_v1_structural_migration_without_changing_the_file() {
            let root = TempDir::new().unwrap();
            let source = indoc! {r#"
                #:schema https://kpozin.github.io/sprawl-guard/schemas/v1/sprawl-guard.schema.json
                schema_version = 1

                [output]
                format = "json"
                color = "always"
            "#};
            write_file(&root, "sprawl-guard.toml", source);

            let output = sprawl_guard(&root)
                .args(["config", "upgrade", "--check"])
                .output()
                .unwrap();

            assert_exit_code(&output, exit_codes::CHECK_FAILURE);
            assert!(
                stdout(&output).contains("Configuration needs an upgrade: sprawl-guard.toml\n")
            );
            assert_eq!(read_config(&root), source);
            assert_eq!(stderr(&output), "");
        }

        #[test]
        fn upgrade_migrates_v1_structure_and_preserves_unrelated_authored_content() {
            let root = TempDir::new().unwrap();
            let source = indoc! {r#"
                #:schema https://kpozin.github.io/sprawl-guard/schemas/v1/sprawl-guard.schema.json
                # Owned by the platform team.
                schema_version = 1

                [limits.max_lines_per_file]
                production = 321

                [output]
                format = "json"
                color = "never"

                [traversal]
                max_depth = 32
                max_entries = 5000
            "#};
            write_file(&root, "sprawl-guard.toml", source);

            let output = sprawl_guard(&root)
                .args(["config", "upgrade", "--write"])
                .output()
                .unwrap();

            assert_exit_code(&output, exit_codes::SUCCESS);
            assert!(stdout(&output).contains("Upgraded sprawl-guard.toml\n"));
            let migrated = read_config(&root);
            assert!(migrated.starts_with(CURRENT_DIRECTIVE));
            assert!(migrated.contains("# Owned by the platform team."));
            assert!(migrated.contains("schema_version = 2"));
            assert!(migrated.contains("production = 321"));
            assert!(migrated.contains("format = \"json\""));
            assert!(migrated.contains("max_entries = 5000"));
            assert!(!migrated.contains("color ="));
            assert!(!migrated.contains("max_depth"));
            assert_eq!(stderr(&output), "");
        }

        #[test]
        fn upgrade_without_write_previews_the_migration_without_changing_the_file() {
            let root = TempDir::new().unwrap();
            let source = "schema_version = 1\n[output]\ncolor = \"always\"\n";
            write_file(&root, "sprawl-guard.toml", source);

            let output = sprawl_guard(&root)
                .args(["config", "upgrade"])
                .output()
                .unwrap();

            assert_exit_code(&output, exit_codes::SUCCESS);
            assert!(stdout(&output).contains("Would upgrade "));
            assert!(stdout(&output).contains("schema_version = 2"));
            assert!(stdout(&output).contains("Preview only; use --write to apply."));
            assert_eq!(read_config(&root), source);
            assert_eq!(stderr(&output), "");
        }

        #[test]
        fn write_and_check_together_are_rejected() {
            let root = TempDir::new().unwrap();
            let source = "schema_version = 1\n";
            write_file(&root, "sprawl-guard.toml", source);

            let output = sprawl_guard(&root)
                .args(["config", "upgrade", "--write", "--check"])
                .output()
                .unwrap();

            assert_exit_code(&output, exit_codes::OPERATIONAL_ERROR);
            assert!(stderr(&output).contains("cannot be used with"));
            assert_eq!(read_config(&root), source);
        }

        #[test]
        fn upgrade_migrates_an_inline_output_table_without_losing_other_fields() {
            let root = TempDir::new().unwrap();
            let source = indoc! {r#"
                schema_version = 1
                output = { format = "json", color = "never" }
                traversal = { max_depth = 32, max_entries = 5000 }
            "#};
            write_file(&root, "sprawl-guard.toml", source);

            let check_output = sprawl_guard(&root)
                .args(["config", "upgrade", "--check"])
                .output()
                .unwrap();

            assert_exit_code(&check_output, exit_codes::CHECK_FAILURE);
            assert_eq!(read_config(&root), source);

            let upgrade_output = sprawl_guard(&root)
                .args(["config", "upgrade", "--write"])
                .output()
                .unwrap();

            assert_exit_code(&upgrade_output, exit_codes::SUCCESS);
            let migrated = read_config(&root);
            let migrated_toml = parse_toml(&migrated);
            assert!(migrated.contains("schema_version = 2"));
            assert_eq!(migrated_toml["output"]["format"].as_str(), Some("json"));
            assert!(migrated_toml["output"].get("color").is_none());
            assert_eq!(
                migrated_toml["traversal"]["max_entries"].as_integer(),
                Some(5000)
            );
            assert!(migrated_toml["traversal"].get("max_depth").is_none());
        }

        #[test]
        fn upgrade_renames_v1_keys_in_place_and_keeps_their_comments() {
            let root = TempDir::new().unwrap();
            write_file(
                &root,
                "sprawl-guard.toml",
                indoc! {r#"
                    schema_version = 1

                    [limits.max_lines_per_file]
                    included_line_types = ["code", "comments"]
                    production = 300

                    [limits.max_leaf_files_per_directory]
                    production = 20

                    [files]
                    # Generated code is owned elsewhere.
                    exclude = ["generated/**"]

                    [tests]
                    check_path_tests = false
                    globs = ["tests/**"]

                    [traversal]
                    error_policy = "warn"

                    [languages.rust]
                    additional_globs = ["*.rs.in"]
                    limits.max_lines_per_file.included_line_types = ["code", "blank_lines"]

                    [language_groups.documentation]
                    limits = { max_leaf_files_per_directory = { production = 30 } }

                    [[overrides]]
                    paths = ["vendor/**"]
                    reason = "Vendored code."
                    max_leaf_files_per_directory = { production = "unlimited" }
                "#},
            );

            let output = sprawl_guard(&root)
                .args(["config", "upgrade", "--write"])
                .output()
                .unwrap();

            assert_exit_code(&output, exit_codes::SUCCESS);
            assert_eq!(
                read_config(&root),
                format!(
                    "{CURRENT_DIRECTIVE}{}",
                    indoc! {r#"
                        schema_version = 2

                        [limits.max_lines_per_file]
                        count_lines = ["comments"]
                        production = 300

                        [limits.max_direct_files_per_directory]
                        production = 20

                        [files]
                        # Generated code is owned elsewhere.
                        extend_exclude = ["generated/**"]

                        [tests]
                        check = false
                        globs = ["tests/**"]

                        [traversal]
                        unreadable_paths = "warn"

                        [languages.rust]
                        extend_include = ["*.rs.in"]
                        limits.max_lines_per_file.count_lines = ["blank_lines"]

                        [language_groups.documentation]
                        limits = { max_direct_files_per_directory = { production = 30 } }

                        [[overrides]]
                        paths = ["vendor/**"]
                        reason = "Vendored code."
                        max_direct_files_per_directory = { production = "unlimited" }
                    "#}
                )
            );
            assert_eq!(stderr(&output), "");
        }

        #[test]
        fn upgrade_renames_keys_in_inline_override_arrays() {
            let root = TempDir::new().unwrap();
            write_file(
                &root,
                "sprawl-guard.toml",
                indoc! {r#"
                    schema_version = 1
                    overrides = [
                      { paths = ["vendor/**"], reason = "Vendored code.", max_leaf_files_per_directory = { production = 5 } },
                    ]
                "#},
            );

            let output = sprawl_guard(&root)
                .args(["config", "upgrade", "--write"])
                .output()
                .unwrap();

            assert_exit_code(&output, exit_codes::SUCCESS);
            assert_eq!(
                read_config(&root),
                format!(
                    "{CURRENT_DIRECTIVE}{}",
                    indoc! {r#"
                        schema_version = 2
                        overrides = [
                          { paths = ["vendor/**"], reason = "Vendored code.", max_direct_files_per_directory = { production = 5 } },
                        ]
                    "#}
                )
            );
        }

        #[test]
        fn upgrade_rejects_a_key_written_under_both_its_old_and_new_names() {
            let root = TempDir::new().unwrap();
            let source = indoc! {r#"
                schema_version = 1

                [tests]
                check_path_tests = false
                check = true
            "#};
            write_file(&root, "sprawl-guard.toml", source);

            let output = sprawl_guard(&root)
                .args(["config", "upgrade", "--write"])
                .output()
                .unwrap();

            assert_exit_code(&output, exit_codes::OPERATIONAL_ERROR);
            assert!(stderr(&output).contains("check_path_tests"));
            assert_eq!(read_config(&root), source);
        }

        #[test]
        fn check_reports_a_missing_current_directive_without_changing_the_file() {
            let root = TempDir::new().unwrap();
            write_config(&root, "# Keep this comment.\n[limits]\n");
            let original = read_config(&root);

            let output = sprawl_guard(&root)
                .args(["config", "upgrade", "--check"])
                .output()
                .unwrap();

            assert_exit_code(&output, exit_codes::CHECK_FAILURE);
            assert_eq!(read_config(&root), original);
            assert_eq!(stderr(&output), "");
        }

        #[test]
        fn upgrade_adds_the_directive_without_reformatting_authored_toml() {
            let root = TempDir::new().unwrap();
            write_config(&root, "# Keep this comment.\n[limits]\n");
            let original = read_config(&root);

            let output = sprawl_guard(&root)
                .args(["config", "upgrade", "--write"])
                .output()
                .unwrap();

            assert_exit_code(&output, exit_codes::SUCCESS);
            assert_eq!(read_config(&root), format!("{CURRENT_DIRECTIVE}{original}"));
            assert_eq!(stderr(&output), "");
        }

        #[test]
        fn upgrade_preserves_comments_before_the_first_config_key() {
            let root = TempDir::new().unwrap();
            let original = "# Owned by the platform team.\n\nschema_version = 1\n";
            write_file(&root, "sprawl-guard.toml", original);

            let output = sprawl_guard(&root)
                .args(["config", "upgrade", "--write"])
                .output()
                .unwrap();

            assert_exit_code(&output, exit_codes::SUCCESS);
            assert_eq!(read_config(&root), migrated_v1(original));
            assert_eq!(stderr(&output), "");
        }

        #[test]
        fn upgrade_replaces_a_stale_directive() {
            let root = TempDir::new().unwrap();
            write_file(
                &root,
                "sprawl-guard.toml",
                "#:schema https://example.com/old.json\nschema_version = 2\n",
            );

            let output = sprawl_guard(&root)
                .args(["config", "upgrade", "--write"])
                .output()
                .unwrap();

            assert_exit_code(&output, exit_codes::SUCCESS);
            assert_eq!(
                read_config(&root),
                format!("{CURRENT_DIRECTIVE}schema_version = 2\n")
            );
            assert_eq!(stderr(&output), "");
        }

        #[test]
        fn upgrade_preserves_crlf_line_endings() {
            let root = TempDir::new().unwrap();
            write_file(
                &root,
                "sprawl-guard.toml",
                "schema_version = 1\r\n# Authored comment.\r\n",
            );

            let output = sprawl_guard(&root)
                .args(["config", "upgrade", "--write"])
                .output()
                .unwrap();

            assert_exit_code(&output, exit_codes::SUCCESS);
            assert_eq!(
                read_config(&root),
                format!(
                    "{}\r\nschema_version = 2\r\n# Authored comment.\r\n",
                    CURRENT_DIRECTIVE.trim_end()
                )
            );
            assert_eq!(stderr(&output), "");
        }

        fn assert_upgrade_rejects_config(source: &str, expected_error: &str) {
            let root = TempDir::new().unwrap();
            write_file(&root, "sprawl-guard.toml", source);

            let output = sprawl_guard(&root)
                .args(["config", "upgrade", "--write"])
                .output()
                .unwrap();

            assert_exit_code(&output, exit_codes::OPERATIONAL_ERROR);
            assert_eq!(read_config(&root), source);
            assert!(stderr(&output).contains(expected_error));
        }

        #[test]
        fn invalid_config_is_rejected_without_changing_the_file() {
            assert_upgrade_rejects_config("schema_version = 1\nunknown = true\n", "unknown field");
        }

        #[test]
        fn future_schema_versions_remain_typed_errors() {
            assert_upgrade_rejects_config(
                "schema_version = 3\n",
                "unsupported config schema version 3",
            );
        }
    }

    mod when_the_config_is_current {
        use super::*;

        #[test]
        fn check_succeeds_without_changing_the_file() {
            let root = TempDir::new().unwrap();
            let source = format!("{CURRENT_DIRECTIVE}schema_version = 2\n# Authored comment.\n");
            write_file(&root, "sprawl-guard.toml", &source);

            let output = sprawl_guard(&root)
                .args(["config", "upgrade", "--check"])
                .output()
                .unwrap();

            assert_exit_code(&output, exit_codes::SUCCESS);
            assert!(stdout(&output).contains("Configuration is current:"));
            assert_eq!(read_config(&root), source);
            assert_eq!(stderr(&output), "");
        }
    }

    mod when_no_authored_config_exists {
        use super::*;

        #[test]
        fn upgrade_reports_an_operational_error() {
            let root = TempDir::new().unwrap();

            let output = sprawl_guard(&root)
                .args(["config", "upgrade", "--write"])
                .output()
                .unwrap();

            assert_exit_code(&output, exit_codes::OPERATIONAL_ERROR);
            assert!(stderr(&output).contains("no config file found to upgrade"));
        }
    }

    mod when_the_config_path_is_explicit {
        use super::*;

        #[test]
        fn upgrade_updates_only_the_selected_config() {
            let root = TempDir::new().unwrap();
            let default = "schema_version = 1\n# Default.\n";
            let selected = "schema_version = 1\n# Selected.\n";
            write_file(&root, "sprawl-guard.toml", default);
            write_file(&root, "selected.toml", selected);

            let output = sprawl_guard(&root)
                .args(["config", "upgrade", "--write", "--config", "selected.toml"])
                .output()
                .unwrap();

            assert_exit_code(&output, exit_codes::SUCCESS);
            assert_eq!(read_config(&root), default);
            assert_eq!(
                std::fs::read_to_string(root.path().join("selected.toml")).unwrap(),
                migrated_v1(selected)
            );
            assert_eq!(stderr(&output), "");
        }

        #[test]
        fn upgrade_accepts_the_old_alias_only_when_selected_explicitly() {
            let root = TempDir::new().unwrap();
            let selected = "schema_version = 1\n# Explicit alternative.\n";
            write_file(&root, ".sprawlrc.toml", selected);

            let output = sprawl_guard(&root)
                .args(["config", "upgrade", "--write", "--config", ".sprawlrc.toml"])
                .output()
                .unwrap();

            assert_exit_code(&output, exit_codes::SUCCESS);
            assert_eq!(
                std::fs::read_to_string(root.path().join(".sprawlrc.toml")).unwrap(),
                migrated_v1(selected)
            );
            assert_eq!(stderr(&output), "");
        }
    }
}
