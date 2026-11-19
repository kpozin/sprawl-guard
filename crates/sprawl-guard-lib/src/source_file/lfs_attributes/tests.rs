use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::*;

/// A temporary work tree whose top holds `.git`, and whose `root` subdirectory is checked.
struct WorkTree {
    _directory: TempDir,
    top: PathBuf,
}

impl WorkTree {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let top = directory.path().canonicalize().unwrap();
        fs::create_dir(top.join(GIT_DIR)).unwrap();
        Self {
            _directory: directory,
            top,
        }
    }

    fn write(&self, path: &str, contents: &str) {
        let path = self.top.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    /// Returns how `path` is stored, relative to `root`, which is relative to the top.
    fn storage_under(&self, root: &str, path: &str) -> FileStorage {
        let root = self.top.join(root);
        fs::create_dir_all(&root).unwrap();
        LfsAttributes::new(&root)
            .storage(&RelativePath::new(Path::new(path)).unwrap())
            .unwrap()
    }

    fn storage(&self, path: &str) -> FileStorage {
        self.storage_under("", path)
    }
}

/// Returns how `path` is stored when the top `.gitattributes` holds `attributes`.
fn storage_with(attributes: &str, path: &str) -> FileStorage {
    let tree = WorkTree::new();
    tree.write(ATTRIBUTES_FILE, attributes);
    tree.storage(path)
}

mod when_no_gitattributes_exists {
    use super::*;

    #[test]
    fn it_returns_regular() {
        assert_eq!(WorkTree::new().storage("big.bin"), FileStorage::Regular);
    }
}

mod when_a_line_sets_filter_lfs {
    use super::*;

    #[test]
    fn it_returns_lfs_for_a_matching_path() {
        assert_eq!(
            storage_with(
                "*.bin filter=lfs diff=lfs merge=lfs -text\n",
                "assets/big.bin"
            ),
            FileStorage::Lfs
        );
    }

    #[test]
    fn it_returns_regular_for_a_path_that_does_not_match() {
        assert_eq!(
            storage_with("*.bin filter=lfs\n", "big.png"),
            FileStorage::Regular
        );
    }

    #[test]
    fn it_matches_a_space_class_written_by_git_lfs_track() {
        assert_eq!(
            storage_with("my[[:space:]]file.bin filter=lfs\n", "my file.bin"),
            FileStorage::Lfs
        );
    }

    #[test]
    fn it_does_not_apply_a_directory_pattern_to_files_under_it() {
        assert_eq!(
            storage_with("assets filter=lfs\nassets/ filter=lfs\n", "assets/big.bin"),
            FileStorage::Regular
        );
    }

    #[test]
    fn it_applies_a_recursive_pattern_to_files_under_the_directory() {
        assert_eq!(
            storage_with("assets/** filter=lfs\n", "assets/deep/big.bin"),
            FileStorage::Lfs
        );
    }
}

mod when_the_file_starts_with_a_byte_order_mark {
    use super::*;

    #[test]
    fn it_reads_the_first_line_like_git() {
        assert_eq!(
            storage_with("\u{FEFF}*.bin filter=lfs\n", "big.bin"),
            FileStorage::Lfs
        );
    }
}

mod when_a_pattern_has_braces {
    use super::*;

    #[test]
    fn it_matches_them_literally_like_git() {
        let attributes = "{a,b}.bin filter=lfs\n";
        assert_eq!(storage_with(attributes, "{a,b}.bin"), FileStorage::Lfs);
        assert_eq!(storage_with(attributes, "a.bin"), FileStorage::Regular);
    }

    #[test]
    fn it_matches_them_inside_a_character_class() {
        let attributes = "[{]a.bin filter=lfs\n";
        assert_eq!(storage_with(attributes, "{a.bin"), FileStorage::Lfs);
        assert_eq!(storage_with(attributes, "\\a.bin"), FileStorage::Regular);
    }

    #[test]
    fn it_matches_a_range_between_braces_like_git() {
        // `|` sorts between `{` and `}`.
        assert_eq!(
            storage_with("[{-}]a.bin filter=lfs\n", "|a.bin"),
            FileStorage::Lfs
        );
    }

    #[test]
    fn it_reads_a_leading_closing_bracket_as_part_of_the_class() {
        let attributes = "[!]{]a.bin filter=lfs\n";
        assert_eq!(storage_with(attributes, "xa.bin"), FileStorage::Lfs);
        assert_eq!(storage_with(attributes, "{a.bin"), FileStorage::Regular);
    }

    #[test]
    fn it_keeps_braces_the_pattern_already_escapes() {
        assert_eq!(
            storage_with("\\{a\\}.bin filter=lfs\n", "{a}.bin"),
            FileStorage::Lfs
        );
    }
}

mod when_a_character_class_is_never_closed {
    use super::*;

    #[test]
    fn it_matches_nothing_like_git() {
        assert_eq!(
            storage_with("file[.bin filter=lfs\n", "file[.bin"),
            FileStorage::Regular
        );
    }

    #[test]
    fn it_keeps_later_lines() {
        assert_eq!(
            storage_with("file[.bin -filter\n*.bin filter=lfs\n", "file[.bin"),
            FileStorage::Lfs
        );
    }
}

mod when_a_later_line_changes_the_filter {
    use super::*;

    #[test]
    fn it_returns_regular_when_the_filter_is_unset() {
        assert_eq!(
            storage_with("*.bin filter=lfs\nsmall.bin -filter\n", "small.bin"),
            FileStorage::Regular
        );
    }

    #[test]
    fn it_returns_regular_when_the_filter_is_unspecified() {
        assert_eq!(
            storage_with("*.bin filter=lfs\nsmall.bin !filter\n", "small.bin"),
            FileStorage::Regular
        );
    }

    #[test]
    fn it_returns_regular_when_the_filter_is_another_one() {
        assert_eq!(
            storage_with("*.bin filter=lfs\nsmall.bin filter=crypt\n", "small.bin"),
            FileStorage::Regular
        );
    }

    #[test]
    fn it_returns_lfs_when_the_last_line_sets_it_again() {
        assert_eq!(
            storage_with("*.bin -filter\n*.bin filter=lfs\n", "big.bin"),
            FileStorage::Lfs
        );
    }

    #[test]
    fn it_keeps_lfs_when_the_later_line_does_not_mention_the_filter() {
        assert_eq!(
            storage_with("*.bin filter=lfs\n*.bin -text\n", "big.bin"),
            FileStorage::Lfs
        );
    }
}

mod when_one_line_mentions_the_filter_twice {
    use super::*;

    #[test]
    fn it_uses_the_last_mention() {
        assert_eq!(
            storage_with("*.bin -filter filter=lfs\n", "big.bin"),
            FileStorage::Lfs
        );
    }
}

mod when_a_line_is_skipped {
    use super::*;

    #[test]
    fn it_skips_comments() {
        assert_eq!(
            storage_with("# *.bin filter=lfs\n", "big.bin"),
            FileStorage::Regular
        );
    }

    #[test]
    fn it_skips_macro_definitions() {
        assert_eq!(
            storage_with("[attr]binary filter=lfs\n", "binary"),
            FileStorage::Regular
        );
    }

    #[test]
    fn it_skips_quoted_patterns() {
        assert_eq!(
            storage_with("\"big.bin\" filter=lfs\n", "big.bin"),
            FileStorage::Regular
        );
    }

    #[test]
    fn it_skips_negated_patterns() {
        assert_eq!(
            storage_with("!big.bin filter=lfs\n", "big.bin"),
            FileStorage::Regular
        );
    }
}

mod when_a_nested_gitattributes_mentions_the_filter {
    use super::*;

    #[test]
    fn it_overrides_the_parent_file() {
        let tree = WorkTree::new();
        tree.write(ATTRIBUTES_FILE, "*.bin filter=lfs\n");
        tree.write("vendor/.gitattributes", "*.bin -filter\n");
        assert_eq!(tree.storage("vendor/big.bin"), FileStorage::Regular);
    }

    #[test]
    fn it_matches_patterns_relative_to_its_own_directory() {
        let tree = WorkTree::new();
        tree.write("assets/.gitattributes", "/big.bin filter=lfs\n");
        assert_eq!(tree.storage("assets/big.bin"), FileStorage::Lfs);
    }
}

mod when_a_nested_gitattributes_does_not_match {
    use super::*;

    #[test]
    fn it_falls_through_to_the_parent_file() {
        let tree = WorkTree::new();
        tree.write(ATTRIBUTES_FILE, "*.bin filter=lfs\n");
        tree.write("vendor/.gitattributes", "*.txt -filter\n");
        assert_eq!(tree.storage("vendor/big.bin"), FileStorage::Lfs);
    }
}

mod when_the_root_is_below_the_top_of_the_work_tree {
    use super::*;

    #[test]
    fn it_reads_gitattributes_above_the_root() {
        let tree = WorkTree::new();
        tree.write(ATTRIBUTES_FILE, "*.bin filter=lfs\n");
        assert_eq!(
            tree.storage_under("packages/app", "big.bin"),
            FileStorage::Lfs
        );
    }

    #[test]
    fn it_lets_gitattributes_under_the_root_override_those_above() {
        let tree = WorkTree::new();
        tree.write(ATTRIBUTES_FILE, "*.bin filter=lfs\n");
        tree.write("packages/app/.gitattributes", "*.bin -filter\n");
        assert_eq!(
            tree.storage_under("packages/app", "big.bin"),
            FileStorage::Regular
        );
    }
}

mod when_no_directory_above_the_root_holds_git {
    use super::*;

    #[test]
    fn it_reads_only_gitattributes_under_the_root() {
        let directory = tempfile::tempdir().unwrap();
        let top = directory.path().canonicalize().unwrap();
        fs::write(top.join(ATTRIBUTES_FILE), "*.bin filter=lfs\n").unwrap();
        let root = top.join("app");
        fs::create_dir(&root).unwrap();
        let mut attributes = LfsAttributes::new(&root);
        assert_eq!(
            attributes.storage(&RelativePath::new(Path::new("big.bin")).unwrap()),
            Some(FileStorage::Regular)
        );
    }
}

mod when_a_gitattributes_file_cannot_be_read {
    use super::*;

    #[test]
    fn it_returns_an_unknown_storage_and_records_the_failure() {
        let tree = WorkTree::new();
        tree.write("vendor/big.bin", "");
        // A directory in place of the file makes reading it fail with an error other than
        // "not found".
        fs::create_dir(tree.top.join("vendor").join(ATTRIBUTES_FILE)).unwrap();
        let mut attributes = LfsAttributes::new(&tree.top);
        let path = RelativePath::new(Path::new("vendor/big.bin")).unwrap();
        assert_eq!(attributes.storage(&path), None);
        let failures = attributes.take_failures();
        assert_eq!(
            failures
                .iter()
                .map(|failure| failure.path.clone())
                .collect::<Vec<_>>(),
            [tree.top.join("vendor").join(ATTRIBUTES_FILE)]
        );
    }

    #[test]
    fn it_returns_the_storage_a_deeper_readable_file_decides() {
        let tree = WorkTree::new();
        fs::create_dir(tree.top.join(ATTRIBUTES_FILE)).unwrap();
        tree.write("vendor/.gitattributes", "*.bin filter=lfs\n");
        let mut attributes = LfsAttributes::new(&tree.top);
        let path = RelativePath::new(Path::new("vendor/big.bin")).unwrap();
        assert_eq!(attributes.storage(&path), Some(FileStorage::Lfs));
    }

    #[test]
    fn it_records_each_failure_once() {
        let tree = WorkTree::new();
        fs::create_dir_all(tree.top.join("vendor").join(ATTRIBUTES_FILE)).unwrap();
        let mut attributes = LfsAttributes::new(&tree.top);
        for path in ["vendor/a.bin", "vendor/b.bin"] {
            attributes.storage(&RelativePath::new(Path::new(path)).unwrap());
        }
        assert_eq!(attributes.take_failures().len(), 1);
    }
}

// Symbolic links are created with the Unix API, which WASI lacks.
#[cfg(unix)]
mod when_a_gitattributes_file_is_a_symbolic_link {
    use super::*;

    #[test]
    fn it_ignores_the_link_like_git() {
        let tree = WorkTree::new();
        tree.write("shared/attributes", "*.bin filter=lfs\n");
        std::os::unix::fs::symlink(
            tree.top.join("shared/attributes"),
            tree.top.join(ATTRIBUTES_FILE),
        )
        .unwrap();
        let mut attributes = LfsAttributes::new(&tree.top);
        let path = RelativePath::new(Path::new("big.bin")).unwrap();
        assert_eq!(attributes.storage(&path), Some(FileStorage::Regular));
        assert!(attributes.take_failures().is_empty());
    }
}
