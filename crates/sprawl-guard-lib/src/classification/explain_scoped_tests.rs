#![cfg(unix)]

use std::path::Path;

use crate::classification::{PathClassification, explain_path};
use crate::config::SymlinkTraversal;
use crate::test_support::{load_config, write_rust_symlink_traversal_config, write_source_file};

mod when_explaining_a_target_with_an_unrelated_traversal_failure {
    use super::*;

    #[test]
    fn it_ignores_the_unrelated_failure() {
        let tempdir = tempfile::tempdir().unwrap();
        write_rust_symlink_traversal_config(tempdir.path(), SymlinkTraversal::Directories);
        write_source_file(tempdir.path(), "src/lib.rs");
        std::fs::create_dir_all(tempdir.path().join("unrelated")).unwrap();
        std::os::unix::fs::symlink("..", tempdir.path().join("unrelated/cycle")).unwrap();

        let classification = explain_path(
            tempdir.path(),
            Path::new("src/lib.rs"),
            &load_config(tempdir.path()),
        )
        .unwrap();

        assert!(matches!(classification, PathClassification::Source(_)));
    }
}
