#![cfg(test)]

use std::fs;
use std::path::Path;

use indoc::formatdoc;

use crate::GlobPattern;
use crate::classification::ClassifiedSource;
use crate::classification::{TraversalIncidentKind, TraversalIncidentSeverity};
use crate::config::{
    Config, LoadOptions, SymlinkTraversal, TraversalErrorPolicy, load_resolved_config,
};

pub(crate) fn load_config(root: &Path) -> Config {
    let mut config = load_resolved_config(&LoadOptions {
        root: root.to_path_buf(),
        config: None,
    })
    .unwrap()
    .config;
    for fixture_path in ["sprawl-guard.toml", ".sprawlrc.toml"] {
        config
            .files
            .exclude
            .push(GlobPattern::new(fixture_path).unwrap());
    }
    config
}

pub(crate) fn write_config(root: &Path, contents: &str) {
    fs::write(
        root.join("sprawl-guard.toml"),
        format!("schema_version = 1\n{contents}"),
    )
    .unwrap();
}

pub(crate) fn write_rust_enabled_config(root: &Path) {
    write_config(
        root,
        indoc::indoc! {r#"
            [languages.rust]
            enabled = true
        "#},
    );
}

pub(crate) fn write_rust_symlink_traversal_config(root: &Path, policy: SymlinkTraversal) {
    let policy = symlink_traversal_name(policy);
    write_config(
        root,
        &formatdoc! {r#"
            [traversal]
            follow_symlinks = "{policy}"

            [languages.rust]
            enabled = true
        "#},
    );
}

pub(crate) fn write_rust_symlink_traversal_warn_config(root: &Path, policy: SymlinkTraversal) {
    let policy = symlink_traversal_name(policy);
    let error_policy = traversal_error_policy_name(TraversalErrorPolicy::Warn);
    write_config(
        root,
        &formatdoc! {r#"
            [traversal]
            follow_symlinks = "{policy}"
            error_policy = "{error_policy}"

            [languages.rust]
            enabled = true
        "#},
    );
}

pub(crate) fn write_rust_entry_limit_config(root: &Path, max_entries: u32) {
    write_config(
        root,
        &formatdoc! {r#"
            [traversal]
            max_entries = {max_entries}

            [languages.rust]
            enabled = true
        "#},
    );
}

pub(crate) fn write_rust_depth_limit_config(root: &Path, max_depth: u32) {
    write_config(
        root,
        &formatdoc! {r#"
            [traversal]
            max_depth = {max_depth}

            [languages.rust]
            enabled = true
        "#},
    );
}

pub(crate) fn write_ambiguous_rust_go_config(root: &Path, path: &str) {
    write_config(
        root,
        &formatdoc! {r#"
            [languages.rust]
            enabled = true
            include = ["{path}"]

            [languages.go]
            enabled = true
            include = ["{path}"]
        "#},
    );
}

pub(crate) fn write_file(root: &Path, path: &str, contents: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

pub(crate) fn write_source_file(root: &Path, path: &str) {
    write_file(root, path, "fn main() {}\n");
}

pub(crate) fn source_paths(sources: &[ClassifiedSource]) -> Vec<&str> {
    sources.iter().map(|source| source.path.as_str()).collect()
}

pub(crate) fn assert_symlink_loop_error(error: crate::SprawlError) {
    let crate::SprawlError::TraversalFailed { incidents, .. } = error else {
        panic!("expected traversal failure, got {error}");
    };
    assert_eq!(incidents.len(), 1);
    assert_eq!(incidents[0].kind, TraversalIncidentKind::SymlinkLoop);
    assert_eq!(incidents[0].severity, TraversalIncidentSeverity::Error);
}

fn symlink_traversal_name(policy: SymlinkTraversal) -> &'static str {
    match policy {
        SymlinkTraversal::None => "none",
        SymlinkTraversal::Files => "files",
        SymlinkTraversal::Directories => "directories",
        SymlinkTraversal::FilesAndDirectories => "files-and-directories",
    }
}

fn traversal_error_policy_name(policy: TraversalErrorPolicy) -> &'static str {
    match policy {
        TraversalErrorPolicy::Fail => "fail",
        TraversalErrorPolicy::Warn => "warn",
    }
}
