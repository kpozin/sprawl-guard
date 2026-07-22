use assert_fs::TempDir;
use indoc::indoc;

use super::common::*;

fn write_sample_config(root: &TempDir) {
    write_config(
        root,
        indoc! {r#"
            [limits]
            max_lines_per_file = { code_non_test = 123 }

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
    let config = parse_toml(&stdout(&output));

    assert_exit_code(&output, exit_codes::SUCCESS);
    assert_toml_contains(
        &config,
        toml::toml! {
            [limits.max_lines_per_file]
            code_non_test = 123
            code = 800

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
