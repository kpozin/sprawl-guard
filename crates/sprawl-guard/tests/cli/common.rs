use std::io::Write;
use std::process::{Command, Output, Stdio};

use assert_fs::TempDir;
use assert_fs::prelude::*;
use indoc::{formatdoc, indoc};
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
    Command::new(sprawl_guard_binary())
}

pub(crate) fn write_config(root: &TempDir, contents: &str) {
    root.child("sprawl-guard.toml").write_str(contents).unwrap();
}

pub(crate) fn write_alias_config(root: &TempDir, contents: &str) {
    root.child(".sprawlrc.toml").write_str(contents).unwrap();
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

pub(crate) fn write_rust_file_limit_config(root: &TempDir, code_non_test: u32) {
    write_config(
        root,
        &formatdoc! {r#"
            [limits.max_lines_per_file]
            code_non_test = {code_non_test}

            [languages.rust]
            enabled = true
        "#},
    );
}

pub(crate) fn write_rust_file_limit_config_and_file_ratchet(
    root: &TempDir,
    configured_limit: u32,
    ratchet_path: &str,
    ratchet_non_test_code_lines: u32,
) {
    write_config(
        root,
        &formatdoc! {r#"
            [limits.max_lines_per_file]
            code_non_test = {configured_limit}

            [languages.rust]
            enabled = true
        "#},
    );
    write_ratchet(
        root,
        &formatdoc! {r#"
            version = 1

            [[files]]
            path = "{ratchet_path}"
            non_test_code_lines = {ratchet_non_test_code_lines}
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

pub(crate) fn ratchet_file_entry<'file>(
    ratchet: &'file RatchetFile,
    path: &str,
) -> Option<&'file FileRatchet> {
    ratchet
        .files
        .iter()
        .find(|entry| entry.path.as_str() == path)
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

pub(crate) fn run_json_stdin(
    args: &[&str],
    stdin_json: &serde_json::Value,
) -> (Output, serde_json::Value) {
    let mut child = sprawl_guard_no_root()
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(serde_json::to_string(stdin_json).unwrap().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
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
