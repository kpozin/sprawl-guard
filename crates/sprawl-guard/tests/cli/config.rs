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

fn assert_config_command_outputs_full_config(root: &TempDir, command: &str) {
    let output = sprawl_guard(root)
        .args(["config", command])
        .output()
        .unwrap();
    let stdout = stdout(&output);
    let config = parse_toml(&stdout);

    assert_exit_code(&output, exit_codes::SUCCESS);
    assert!(stdout.starts_with("schema_version = 1\n"));
    assert_toml_contains(
        &config,
        toml::toml! {
            [limits.max_lines_per_file]
            included_line_types = ["code"]
            production = 123
            test = 800

            [languages.Rust]
            enabled = true
        },
    );
    assert_eq!(stderr(&output), "");
}

mod resolved {
    use super::*;

    #[test]
    fn it_prints_the_resolved_config_used_by_checks() {
        let root = TempDir::new().unwrap();
        write_sample_config(&root);

        assert_config_command_outputs_full_config(&root, "resolved");
    }

    #[test]
    fn it_materializes_inherited_and_automatic_language_loc_policy() {
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
                included_line_types = ["code"]
                production = 123
                test = 800
            })
        );
        assert_eq!(
            config["languages"]["Markdown"]["limits"]["max_lines_per_file"],
            toml::Value::Table(toml::toml! {
                included_line_types = ["code", "comments"]
                production = 123
                test = 800
            })
        );
    }

    #[test]
    fn it_preserves_a_caller_authored_files_table() {
        let root = TempDir::new().unwrap();
        write_config(
            &root,
            indoc! {r#"
                [files]
                exclude = ["generated/**"]
            "#},
        );

        let output = sprawl_guard(&root)
            .args(["config", "resolved"])
            .output()
            .unwrap();
        let config = parse_toml(&stdout(&output));

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(
            config["files"]["exclude"],
            toml::Value::Array(vec![toml::Value::String("generated/**".to_owned())])
        );
        assert_eq!(stderr(&output), "");
    }
}

mod export {
    use super::*;

    #[test]
    fn it_prints_the_full_explicit_config_suitable_for_committing() {
        let root = TempDir::new().unwrap();
        write_sample_config(&root);

        assert_config_command_outputs_full_config(&root, "export");
    }
}
