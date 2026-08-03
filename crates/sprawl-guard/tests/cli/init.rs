use assert_fs::TempDir;
use assert_fs::prelude::*;
use std::process::Output;

use super::common::*;

fn assert_init_writes_config(root: &TempDir, expected: toml::Table) -> Output {
    let output = sprawl_guard(root).arg("init").output().unwrap();
    let config = read_config_toml(root);

    assert_exit_code(&output, exit_codes::SUCCESS);
    assert_eq!(config, toml_value(expected));
    assert_eq!(stderr(&output), "");
    output
}

mod when_the_repository_has_supported_sources {
    use super::*;

    #[test]
    fn it_writes_a_minimal_config_without_language_tables() {
        let root = TempDir::new().unwrap();
        write_file(&root, "src/lib.rs", "fn main() {}\n");

        let output = assert_init_writes_config(
            &root,
            toml::toml! {
                schema_version = 1
            },
        );

        assert!(stdout(&output).contains("Wrote "));
        assert!(!stdout(&output).contains("Selected languages:"));
    }
}

mod when_config_path_is_explicit {
    use super::*;

    #[test]
    fn it_writes_relative_paths_under_the_root() {
        let root = TempDir::new().unwrap();
        write_file(&root, "src/lib.rs", "fn main() {}\n");

        let output = sprawl_guard(&root)
            .args(["--config", "custom-sprawl.toml", "init"])
            .output()
            .unwrap();

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert!(!root.child("sprawl-guard.toml").path().exists());
        assert_eq!(
            parse_toml(&std::fs::read_to_string(root.child("custom-sprawl.toml")).unwrap()),
            toml_value(toml::toml! {
                schema_version = 1
            }),
        );
        assert_eq!(stderr(&output), "");
    }

    #[test]
    fn it_writes_the_explicit_target_when_only_the_default_config_exists() {
        let root = TempDir::new().unwrap();
        write_config(&root, "[languages.rust]\nenabled = true\n");
        write_file(&root, "src/lib.rs", "fn main() {}\n");

        let output = sprawl_guard(&root)
            .args(["--config", "alternate.toml", "init"])
            .output()
            .unwrap();

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert!(root.child("alternate.toml").path().exists());
        assert_eq!(stderr(&output), "");
    }

    #[test]
    fn it_refuses_to_overwrite_the_explicit_target() {
        let root = TempDir::new().unwrap();
        write_file(
            &root,
            "alternate.toml",
            "[languages.rust]\nenabled = false\n",
        );
        write_file(&root, "src/lib.rs", "fn main() {}\n");

        let output = sprawl_guard(&root)
            .args(["--config", "alternate.toml", "init"])
            .output()
            .unwrap();

        assert_exit_code(&output, exit_codes::OPERATIONAL_ERROR);
        assert_eq!(
            std::fs::read_to_string(root.child("alternate.toml").path()).unwrap(),
            "[languages.rust]\nenabled = false\n"
        );
        assert!(stderr(&output).contains("config file already exists:"));
    }
}

mod when_language_selection_flags_are_supplied {
    use super::*;

    #[test]
    fn it_rejects_the_removed_flags() {
        let root = TempDir::new().unwrap();

        for args in [
            vec!["init", "--languages", "rust"],
            vec!["init", "--all-languages"],
        ] {
            let output = sprawl_guard(&root).args(args).output().unwrap();

            assert_exit_code(&output, exit_codes::OPERATIONAL_ERROR);
            assert!(stderr(&output).contains("unexpected argument"));
            assert!(!root.child("sprawl-guard.toml").path().exists());
        }
    }
}

mod when_dry_run_is_used {
    use super::*;

    #[test]
    fn it_prints_the_config_without_writing_it() {
        let root = TempDir::new().unwrap();

        let output = sprawl_guard(&root)
            .args(["init", "--dry-run"])
            .output()
            .unwrap();
        let config_path = root.child("sprawl-guard.toml");

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert!(!config_path.path().exists());
        assert!(stdout(&output).contains("Would write "));
        let config = toml_from_stdout_config(&output);
        assert_eq!(
            config,
            toml_value(toml::toml! {
                schema_version = 1
            })
        );
        assert_eq!(stderr(&output), "");
    }

    #[test]
    fn it_reports_that_a_real_write_would_refuse_an_existing_config() {
        let root = TempDir::new().unwrap();
        write_config(&root, "[languages.rust]\nenabled = true\n");

        let output = sprawl_guard(&root)
            .args(["init", "--dry-run"])
            .output()
            .unwrap();

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert!(stdout(&output).contains("A real write would be refused"));
        assert_eq!(stderr(&output), "");
    }
}

mod when_config_already_exists {
    use super::*;

    #[test]
    fn it_refuses_to_overwrite_the_file() {
        let root = TempDir::new().unwrap();
        write_config(&root, "[languages.rust]\nenabled = true\n");

        let output = sprawl_guard(&root).arg("init").output().unwrap();

        assert_exit_code(&output, exit_codes::OPERATIONAL_ERROR);
        assert!(stderr(&output).contains("config file already exists:"));
    }

    #[test]
    fn it_refuses_hidden_alias_config_before_language_detection() {
        let root = TempDir::new().unwrap();
        write_alias_config(&root, "[languages.rust]\nenabled = true\n");

        let output = sprawl_guard(&root).arg("init").output().unwrap();
        let stderr = stderr(&output);

        assert_exit_code(&output, exit_codes::OPERATIONAL_ERROR);
        assert!(stderr.contains("config file already exists:"));
        assert!(stderr.contains(".sprawlrc.toml"));
        assert!(!stderr.contains("no supported source files found"));
    }
}

mod when_the_repository_is_empty {
    use super::*;

    #[test]
    fn it_writes_the_minimal_config() {
        let root = TempDir::new().unwrap();

        let output = assert_init_writes_config(
            &root,
            toml::toml! {
                schema_version = 1
            },
        );

        assert!(stdout(&output).contains("Wrote "));
        assert!(!stdout(&output).contains("Selected languages:"));
    }
}

mod when_advisory_scan_fails {
    use super::*;

    #[test]
    fn it_does_not_write_the_config() {
        let root = TempDir::new().unwrap();
        write_oversized_rust_source(&root, "src/huge.rs");

        let output = sprawl_guard(&root).arg("init").output().unwrap();

        assert_exit_code(&output, exit_codes::OPERATIONAL_ERROR);
        assert!(!root.child("sprawl-guard.toml").path().exists());
        assert!(stderr(&output).contains("failed to run init advisory scan before writing"));
        assert!(stderr(&output).contains("is too large to count"));
    }
}

mod when_generated_vendor_like_paths_are_included {
    use super::*;

    #[test]
    fn it_prints_an_advisory_without_mutating_config_excludes() {
        let root = TempDir::new().unwrap();
        for index in 0..100 {
            write_file(
                &root,
                &format!("dist/file-{index}.js"),
                "export const x = 1;\n",
            );
        }

        let output = sprawl_guard(&root).arg("init").output().unwrap();
        let config = read_config_toml(&root);

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert!(stdout(&output).contains("Large generated/vendor-like directories"));
        assert!(stdout(&output).contains("dist/"));
        assert!(stdout(&output).contains("100 files javascript"));
        assert_eq!(
            config,
            toml_value(toml::toml! {
                schema_version = 1
            })
        );
        assert_eq!(stderr(&output), "");
    }
}

mod when_advisory_scan_finds_current_violations {
    use super::*;

    #[test]
    fn it_reports_violation_counts_by_group() {
        let root = TempDir::new().unwrap();
        write_rust_source_with_function_count(&root, "src/huge.rs", 401);
        write_rust_source_with_function_count(&root, "src/huge.test.rs", 801);
        for index in 0..9 {
            write_file(
                &root,
                &format!("src/leaf-{index}.rs"),
                &format!("fn leaf_{index}() {{}}\n"),
            );
        }

        let output = sprawl_guard(&root).arg("init").output().unwrap();
        let stdout = stdout(&output);

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert!(stdout.contains("Current tree has violations under the new policy:"));
        assert!(stdout.contains("production file LOC violations: 1"));
        assert!(stdout.contains("test file LOC violations: 1"));
        assert!(stdout.contains("production directory fan-out violations: 1"));
        assert!(stdout.contains("test directory fan-out violations: not enforced (unlimited)"));
        assert_eq!(stderr(&output), "");
    }

    #[test]
    fn it_uses_rust_cfg_test_preprocessing() {
        let root = TempDir::new().unwrap();
        let cfg_test_lines = (0..600)
            .map(|index| format!("    fn test_{index}() {{}}\n"))
            .collect::<String>();
        write_file(
            &root,
            "src/lib.rs",
            &format!("pub fn production() {{}}\n#[cfg(test)]\nmod tests {{\n{cfg_test_lines}}}\n"),
        );

        let output = sprawl_guard(&root).arg("init").output().unwrap();
        let stdout = stdout(&output);

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert!(stdout.contains("Current tree has no violations under the new policy."));
        assert_eq!(stderr(&output), "");
    }
}
