use std::collections::BTreeMap;

use sprawl_guard_lib::{
    CodeLines, DirectoryRatchet, FileRatchet, LeafFiles, RatchetFile, RatchetVersion, RelativePath,
    TestDirectoryRatchet, TestFileRatchet,
};

use super::scope::RatchetScope;

/// Applies `baseline` semantics: replace every entry in scope with current violations.
///
/// Entries outside `scope` are preserved. Entries inside `scope` disappear when
/// they are no longer current violations. New current violations are added.
pub(super) fn replace_ratchet_scope(
    previous: RatchetFile,
    current: RatchetFile,
    scope: &RatchetScope,
) -> RatchetFile {
    let mut next = preserve_out_of_scope(previous, scope);
    next.files.extend(current.files);
    next.test_files.extend(current.test_files);
    next.directories.extend(current.directories);
    next.test_directories.extend(current.test_directories);
    sort_ratchet(&mut next);
    next
}

/// Applies `ratchet` semantics: lower existing scoped entries and remove stale ones.
///
/// Entries outside `scope` are preserved. Entries inside `scope` are never
/// raised and no new entries are created.
pub(super) fn lower_ratchet_scope(
    previous: RatchetFile,
    current: RatchetFile,
    scope: &RatchetScope,
) -> RatchetFile {
    let current = RatchetLookup::from_file(current);
    let mut next = RatchetFile {
        version: RatchetVersion::current(),
        files: previous
            .files
            .into_iter()
            .filter_map(|entry| {
                if !scope.contains(&entry.path) {
                    return Some(entry);
                }
                current.file_value(&entry.path).map(|value| FileRatchet {
                    path: entry.path,
                    non_test_code_lines: std::cmp::min(entry.non_test_code_lines, value),
                })
            })
            .collect(),
        test_files: previous
            .test_files
            .into_iter()
            .filter_map(|entry| {
                if !scope.contains(&entry.path) {
                    return Some(entry);
                }
                current
                    .test_file_value(&entry.path)
                    .map(|value| TestFileRatchet {
                        path: entry.path,
                        code_lines: std::cmp::min(entry.code_lines, value),
                    })
            })
            .collect(),
        directories: previous
            .directories
            .into_iter()
            .filter_map(|entry| {
                if !scope.contains(&entry.path) {
                    return Some(entry);
                }
                current
                    .directory_value(&entry.path)
                    .map(|value| DirectoryRatchet {
                        path: entry.path,
                        leaf_source_files: std::cmp::min(entry.leaf_source_files, value),
                    })
            })
            .collect(),
        test_directories: previous
            .test_directories
            .into_iter()
            .filter_map(|entry| {
                if !scope.contains(&entry.path) {
                    return Some(entry);
                }
                current
                    .test_directory_value(&entry.path)
                    .map(|value| TestDirectoryRatchet {
                        path: entry.path,
                        leaf_test_files: std::cmp::min(entry.leaf_test_files, value),
                    })
            })
            .collect(),
    };
    sort_ratchet(&mut next);
    next
}

/// Counts existing ratchet ceilings that `baseline` would raise.
///
/// New entries are not raises. Missing entries are handled by scoped/full
/// replacement, not by this guard.
pub(super) fn ratchet_raise_count(previous: &RatchetFile, current: &RatchetFile) -> usize {
    let previous = RatchetLookup::from_file(previous.clone());
    current
        .files
        .iter()
        .filter(|entry| {
            previous
                .file_value(&entry.path)
                .is_some_and(|value| entry.non_test_code_lines > value)
        })
        .count()
        + current
            .test_files
            .iter()
            .filter(|entry| {
                previous
                    .test_file_value(&entry.path)
                    .is_some_and(|value| entry.code_lines > value)
            })
            .count()
        + current
            .directories
            .iter()
            .filter(|entry| {
                previous
                    .directory_value(&entry.path)
                    .is_some_and(|value| entry.leaf_source_files > value)
            })
            .count()
        + current
            .test_directories
            .iter()
            .filter(|entry| {
                previous
                    .test_directory_value(&entry.path)
                    .is_some_and(|value| entry.leaf_test_files > value)
            })
            .count()
}

fn preserve_out_of_scope(previous: RatchetFile, scope: &RatchetScope) -> RatchetFile {
    RatchetFile {
        version: RatchetVersion::current(),
        files: previous
            .files
            .into_iter()
            .filter(|entry| !scope.contains(&entry.path))
            .collect(),
        test_files: previous
            .test_files
            .into_iter()
            .filter(|entry| !scope.contains(&entry.path))
            .collect(),
        directories: previous
            .directories
            .into_iter()
            .filter(|entry| !scope.contains(&entry.path))
            .collect(),
        test_directories: previous
            .test_directories
            .into_iter()
            .filter(|entry| !scope.contains(&entry.path))
            .collect(),
    }
}

struct RatchetLookup {
    files: BTreeMap<RelativePath, CodeLines>,
    test_files: BTreeMap<RelativePath, CodeLines>,
    directories: BTreeMap<RelativePath, LeafFiles>,
    test_directories: BTreeMap<RelativePath, LeafFiles>,
}

impl RatchetLookup {
    fn from_file(file: RatchetFile) -> Self {
        Self {
            files: file
                .files
                .into_iter()
                .map(|entry| (entry.path, entry.non_test_code_lines))
                .collect(),
            test_files: file
                .test_files
                .into_iter()
                .map(|entry| (entry.path, entry.code_lines))
                .collect(),
            directories: file
                .directories
                .into_iter()
                .map(|entry| (entry.path, entry.leaf_source_files))
                .collect(),
            test_directories: file
                .test_directories
                .into_iter()
                .map(|entry| (entry.path, entry.leaf_test_files))
                .collect(),
        }
    }

    fn file_value(&self, path: &RelativePath) -> Option<CodeLines> {
        self.files.get(path).copied()
    }

    fn test_file_value(&self, path: &RelativePath) -> Option<CodeLines> {
        self.test_files.get(path).copied()
    }

    fn directory_value(&self, path: &RelativePath) -> Option<LeafFiles> {
        self.directories.get(path).copied()
    }

    fn test_directory_value(&self, path: &RelativePath) -> Option<LeafFiles> {
        self.test_directories.get(path).copied()
    }
}

pub(super) fn sort_ratchet(ratchet: &mut RatchetFile) {
    ratchet
        .files
        .sort_by(|left, right| left.path.cmp(&right.path));
    ratchet
        .test_files
        .sort_by(|left, right| left.path.cmp(&right.path));
    ratchet
        .directories
        .sort_by(|left, right| left.path.cmp(&right.path));
    ratchet
        .test_directories
        .sort_by(|left, right| left.path.cmp(&right.path));
}
