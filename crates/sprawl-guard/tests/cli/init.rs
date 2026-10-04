use assert_fs::TempDir;
use assert_fs::prelude::*;
use std::process::Output;

use super::common::*;

fn assert_init_writes_config(root: &TempDir, expected: toml::Table) -> Output {
    let output = sprawl_guard(root).arg("init").output().unwrap();
    let authored_config = read_config(root);
    let config = read_config_toml(root);

    assert_exit_code(&output, exit_codes::SUCCESS);
    assert!(authored_config.starts_with(concat!(
        "#:schema https://kpozin.github.io/sprawl-guard/schemas/v2/",
        "sprawl-guard.schema.json\n"
    )));
    assert_eq!(config, toml_value(expected));
    assert_eq!(stderr(&output), "");
    output
}

mod when_the_repository_has_supported_sources {
    use super::*;

    #[test]
    fn it_writes_a_minimal_config_without_scanning_sources() {
        let root = TempDir::new().unwrap();
        write_file(&root, "src/lib.rs", "fn main() {}\n");

        let output = assert_init_writes_config(
            &root,
            toml::toml! {
                schema_version = 2
            },
        );

        assert!(stdout(&output).contains("Wrote "));
        assert!(!stdout(&output).contains("Included source files:"));
        assert!(!stdout(&output).contains("Current tree"));
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
                schema_version = 2
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
        assert!(stdout(&output).contains(concat!(
            "#:schema https://kpozin.github.io/sprawl-guard/schemas/v2/",
            "sprawl-guard.schema.json\n"
        )));
        let config = toml_from_stdout_config(&output);
        assert_eq!(
            config,
            toml_value(toml::toml! {
                schema_version = 2
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
    fn an_unselected_alternative_filename_does_not_block_init() {
        let root = TempDir::new().unwrap();
        write_file(
            &root,
            ".sprawlrc.toml",
            "schema_version = 1\n[languages.rust]\nenabled = true\n",
        );

        let output = sprawl_guard(&root).arg("init").output().unwrap();

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert!(root.child("sprawl-guard.toml").path().exists());
        assert!(root.child(".sprawlrc.toml").path().exists());
        assert_eq!(stderr(&output), "");
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
                schema_version = 2
            },
        );

        assert!(stdout(&output).contains("Wrote "));
    }
}

mod when_repository_sources_cannot_be_counted {
    use super::*;

    #[test]
    fn it_still_creates_the_config() {
        let root = TempDir::new().unwrap();
        write_oversized_rust_source(&root, "src/huge.rs");

        let output = sprawl_guard(&root).arg("init").output().unwrap();

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert!(root.child("sprawl-guard.toml").path().exists());
        assert!(stdout(&output).contains("Wrote "));
        assert_eq!(stderr(&output), "");
    }
}
