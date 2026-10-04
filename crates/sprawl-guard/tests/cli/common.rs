use std::process::{Command, Output};

use assert_fs::TempDir;
use assert_fs::prelude::*;
use indoc::{formatdoc, indoc};
use sprawl_guard_lib::classification::SourceKind;
use sprawl_guard_lib::ratchet::{FileRatchet, RatchetFile};
use toml::Value as TomlValue;

pub(crate) mod exit_codes {
    pub(crate) const CHECK_FAILURE: i32 = 1;
    pub(crate) const OPERATIONAL_ERROR: i32 = 2;
    pub(crate) const SUCCESS: i32 = 0;
}

fn sprawl_guard_binary() -> &'static str {
    option_env!("CARGO_BIN_EXE_sprawl-guard")
        .or(option_env!("BAZEL_BIN_EXE_sprawl-guard"))
        .expect("sprawl-guard binary path must be provided by Cargo or Bazel")
}

pub(crate) fn sprawl_guard(root: &TempDir) -> Command {
    let mut command = sprawl_guard_no_root();
    command.arg("--root").arg(root.path());
    command
}

pub(crate) fn sprawl_guard_no_root() -> Command {
    let binary = std::path::Path::new(sprawl_guard_binary());
    let binary = if binary.is_absolute() {
        binary.to_path_buf()
    } else {
        std::env::current_dir().unwrap().join(binary)
    };
    Command::new(binary)
}

pub(crate) fn write_config(root: &TempDir, contents: &str) {
    root.child("sprawl-guard.toml")
        .write_str(&format!("schema_version = 2\n{contents}"))
        .unwrap();
    ignore_fixture_file(root, "sprawl-guard.toml");
}

fn ignore_fixture_file(root: &TempDir, path: &str) {
    let ignore_file = root.child(".gitignore");
    let mut contents = std::fs::read_to_string(ignore_file.path()).unwrap_or_default();
    if !contents.is_empty() && !contents.ends_with('\n') {
        contents.push('\n');
    }
    contents.push_str(path);
    contents.push('\n');
    ignore_file.write_str(&contents).unwrap();
}

pub(crate) fn read_config(root: &TempDir) -> String {
    std::fs::read_to_string(root.child("sprawl-guard.toml").path()).unwrap()
}

pub(crate) fn read_config_toml(root: &TempDir) -> TomlValue {
    parse_toml(&read_config(root))
}

pub(crate) fn parse_toml(input: &str) -> TomlValue {
    toml::from_str(input).unwrap()
}

pub(crate) fn toml_value(table: toml::Table) -> TomlValue {
    TomlValue::Table(table)
}

pub(crate) fn assert_toml_contains(actual: &TomlValue, expected: toml::Table) {
    let expected = TomlValue::Table(expected);
    assert_toml_value_contains(actual, &expected, &mut Vec::new());
}

fn assert_toml_value_contains(actual: &TomlValue, expected: &TomlValue, path: &mut Vec<String>) {
    match (actual, expected) {
        (TomlValue::Table(actual), TomlValue::Table(expected)) => {
            for (key, expected_value) in expected {
                let actual_value = actual.get(key).unwrap_or_else(|| {
                    panic!("missing TOML key at {}", toml_assertion_path(path, key))
                });
                path.push(key.clone());
                assert_toml_value_contains(actual_value, expected_value, path);
                path.pop();
            }
        }
        _ => assert_eq!(
            actual,
            expected,
            "TOML mismatch at {}",
            toml_assertion_path(path, "")
        ),
    }
}

fn toml_assertion_path(path: &[String], key: &str) -> String {
    let mut segments = path.to_vec();
    if !key.is_empty() {
        segments.push(key.to_owned());
    }
    if segments.is_empty() {
        "<root>".to_owned()
    } else {
        segments.join(".")
    }
}

pub(crate) fn toml_from_stdout_config(output: &Output) -> TomlValue {
    let stdout = stdout(output);
    let config_start = stdout.find("# sprawl-guard.toml").unwrap_or(0);
    let config = stdout[config_start..]
        .lines()
        .take_while(|line| {
            !line.starts_with("Included source files:")
                && !line.starts_with("Current tree ")
                && !line.starts_with("Large generated/vendor-like directories ")
        })
        .collect::<Vec<_>>()
        .join("\n");
    parse_toml(&config)
}

pub(crate) fn write_file(root: &TempDir, path: &str, contents: &str) {
    let file = root.child(path);
    std::fs::create_dir_all(file.path().parent().unwrap()).unwrap();
    file.write_str(contents).unwrap();
}

pub(crate) fn write_rust_file_limit_config(root: &TempDir, production: u32) {
    write_config(
        root,
        &formatdoc! {r#"
            [limits.max_lines_per_file]
            production = {production}

            [languages.rust]
            enabled = true
        "#},
    );
}

pub(crate) fn write_rust_file_limit_config_and_file_ratchet(
    root: &TempDir,
    configured_limit: u32,
    ratchet_path: &str,
    ratchet_loc: u32,
) {
    write_config(
        root,
        &formatdoc! {r#"
            [limits.max_lines_per_file]
            production = {configured_limit}

            [languages.rust]
            enabled = true
        "#},
    );
    write_ratchet(
        root,
        &formatdoc! {r#"
            schema_version = 1

            [[files]]
            path = "{ratchet_path}"
            role = "production"
            max_loc = {ratchet_loc}
        "#},
    );
}

pub(crate) fn write_rust_source_with_function_count(
    root: &TempDir,
    path: &str,
    function_count: u32,
) {
    let contents = (0..function_count)
        .map(|index| format!("fn function_{index}() {{}}\n"))
        .collect::<String>();
    write_file(root, path, &contents);
}

pub(crate) fn write_rust_directory_fanout_fixture(root: &TempDir) {
    write_config(
        root,
        indoc! {r#"
            [limits]
            max_leaf_files_per_directory = { production = 1 }

            [languages.rust]
            enabled = true
        "#},
    );
    write_file(root, "src/a.rs", "fn a() {}\n");
    write_file(root, "src/b.rs", "fn b() {}\n");
}

pub(crate) fn write_mixed_group_directory_fanout_fixture(root: &TempDir) {
    write_config(
        root,
        indoc! {r#"
            [limits]
            max_leaf_files_per_directory = { production = 1 }

            [[overrides]]
            paths = ["src/**"]
            max_leaf_files_per_directory = { production = 1 }
            reason = "exercise grouped directory fan-out"

            [languages.rust]
            enabled = true

            [languages.markdown]
            enabled = true
        "#},
    );
    write_file(root, "src/a.rs", "fn a() {}\n");
    write_file(root, "src/b.rs", "fn b() {}\n");
    write_file(root, "src/a.md", "# A\n");
    write_file(root, "src/b.md", "# B\n");
}

pub(crate) fn write_oversized_rust_source(root: &TempDir, path: &str) {
    let source = root.child(path);
    std::fs::create_dir_all(source.path().parent().unwrap()).unwrap();
    std::fs::File::create(source.path())
        .unwrap()
        .set_len(10 * 1024 * 1024 + 1)
        .unwrap();
}

#[cfg(unix)]
pub(crate) fn write_directory_symlink_loop_fixture(root: &TempDir) {
    use indoc::indoc;
    use std::os::unix::fs::symlink;

    write_config(
        root,
        indoc! {r#"
            [traversal]
            follow_symlinks = "directories"

            [languages.rust]
            enabled = true
        "#},
    );
    write_file(root, "real/lib.rs", "fn a() {}\n");
    symlink("..", root.path().join("real/cycle")).unwrap();
}

pub(crate) fn write_ratchet(root: &TempDir, contents: &str) {
    root.child("sprawl-guard.ratchet.toml")
        .write_str(contents)
        .unwrap();
}

pub(crate) fn write_dependency_lockfile_ratchet_fixture(root: &TempDir) {
    write_config(
        root,
        indoc! {r#"
            [limits.max_leaf_files_per_directory]
            production = 1

            [languages.yaml]
            enabled = true
        "#},
    );
    write_file(root, "src/config.yaml", "setting: value\n");
    write_file(
        root,
        "src/pnpm-lock.yaml",
        "lockfileVersion: '9.0'\npackages:\n  dependency: {}\n",
    );
    write_ratchet(
        root,
        indoc! {r#"
            schema_version = 1

            [[files]]
            path = "src/pnpm-lock.yaml"
            role = "production"
            max_loc = 3

            [[directories]]
            path = "src"
            role = "production"
            group = "source"
            leaf_files = 2
        "#},
    );
}

pub(crate) fn read_ratchet(root: &TempDir) -> String {
    std::fs::read_to_string(root.child("sprawl-guard.ratchet.toml").path()).unwrap()
}

pub(crate) fn read_ratchet_file(root: &TempDir) -> RatchetFile {
    RatchetFile::parse(
        root.child("sprawl-guard.ratchet.toml").path(),
        &read_ratchet(root),
    )
    .unwrap()
}

pub(crate) fn run_ratchet_file_write(root: &TempDir, command: &str) -> (Output, RatchetFile) {
    let output = sprawl_guard(root)
        .args([command, "--write"])
        .output()
        .unwrap();
    let ratchet = read_ratchet_file(root);
    (output, ratchet)
}

pub(crate) fn production_file_ratchet_entry<'file>(
    ratchet: &'file RatchetFile,
    path: &str,
) -> Option<&'file FileRatchet> {
    ratchet
        .files
        .iter()
        .find(|entry| entry.path.as_str() == path && entry.role == SourceKind::Production)
}

pub(crate) fn production_directory_ratchet_entries(
    ratchet: &RatchetFile,
) -> Vec<(&str, &str, u32)> {
    ratchet
        .directories
        .iter()
        .filter(|entry| entry.role == SourceKind::Production)
        .map(|entry| {
            (
                entry.group.as_str(),
                entry.path.as_str(),
                entry.leaf_files.get(),
            )
        })
        .collect()
}

pub(crate) fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

pub(crate) fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

pub(crate) fn json_stdout(output: &Output) -> serde_json::Value {
    serde_json::from_str::<serde_json::Value>(&stdout(output)).unwrap()
}

pub(crate) fn run_json(root: &TempDir, args: &[&str]) -> (Output, serde_json::Value) {
    let output = sprawl_guard(root).args(args).output().unwrap();
    let json = json_stdout(&output);
    (output, json)
}

pub(crate) fn run_check_json(root: &TempDir) -> (Output, serde_json::Value) {
    run_json(root, &["check", "--format", "json"])
}

pub(crate) fn json_language<'a>(json: &'a serde_json::Value, id: &str) -> &'a serde_json::Value {
    json["languages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|language| language["id"] == id)
        .unwrap()
}

pub(crate) fn assert_detected_source_count(json: &serde_json::Value, id: &str, expected: u64) {
    let language = json_language(json, id);
    assert_eq!(
        serde_json::json!({
            "source_files": language["source_files"],
        }),
        serde_json::json!({
            "source_files": expected,
        })
    );
}

pub(crate) fn assert_exit_code(output: &Output, code: i32) {
    assert_eq!(output.status.code(), Some(code));
}
