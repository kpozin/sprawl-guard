use std::collections::BTreeMap;

use sprawl_guard_lib::{
    DirectoryRatchet, DirectoryRatchetKey, FileRatchet, LeafFiles, LocLimit, RatchetFile,
    RatchetVersion, RelativePath, TestDirectoryRatchet, TestFileRatchet,
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
                    .directory_value(&entry.key())
                    .map(|value| DirectoryRatchet {
                        path: entry.path,
                        group: entry.group,
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
                    .test_directory_value(&entry.key())
                    .map(|value| TestDirectoryRatchet {
                        path: entry.path,
                        group: entry.group,
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
                    .directory_value(&entry.key())
                    .is_some_and(|value| entry.leaf_source_files > value)
            })
            .count()
        + current
            .test_directories
            .iter()
            .filter(|entry| {
                previous
                    .test_directory_value(&entry.key())
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
    files: BTreeMap<RelativePath, LocLimit>,
    test_files: BTreeMap<RelativePath, LocLimit>,
    directories: BTreeMap<DirectoryRatchetKey, LeafFiles>,
    test_directories: BTreeMap<DirectoryRatchetKey, LeafFiles>,
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
                .map(|entry| (entry.key(), entry.leaf_source_files))
                .collect(),
            test_directories: file
                .test_directories
                .into_iter()
                .map(|entry| (entry.key(), entry.leaf_test_files))
                .collect(),
        }
    }

    fn file_value(&self, path: &RelativePath) -> Option<LocLimit> {
        self.files.get(path).copied()
    }

    fn test_file_value(&self, path: &RelativePath) -> Option<LocLimit> {
        self.test_files.get(path).copied()
    }

    fn directory_value(&self, key: &DirectoryRatchetKey) -> Option<LeafFiles> {
        self.directories.get(key).copied()
    }

    fn test_directory_value(&self, key: &DirectoryRatchetKey) -> Option<LeafFiles> {
        self.test_directories.get(key).copied()
    }
}

pub(super) fn sort_ratchet(ratchet: &mut RatchetFile) {
    ratchet
        .files
        .sort_by(|left, right| left.path.cmp(&right.path));
    ratchet
        .test_files
        .sort_by(|left, right| left.path.cmp(&right.path));
    ratchet.directories.sort_by_key(DirectoryRatchet::key);
    ratchet
        .test_directories
        .sort_by_key(TestDirectoryRatchet::key);
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use indoc::indoc;
    use sprawl_guard_lib::LanguageGroupId;

    use super::*;

    fn parse_ratchet(input: &str) -> RatchetFile {
        RatchetFile::parse(Path::new("sprawl-guard.ratchet.toml"), input).unwrap()
    }

    mod when_directory_groups_share_a_path {
        use super::*;

        #[test]
        fn lowering_matches_the_complete_group_and_path_key() {
            let previous = parse_ratchet(indoc! {r#"
                version = 1

                [[directories]]
                path = "src"
                group = "source"
                leaf_source_files = 5

                [[directories]]
                path = "src"
                group = "documentation"
                leaf_source_files = 7
            "#});
            let current = parse_ratchet(indoc! {r#"
                version = 1

                [[directories]]
                path = "src"
                group = "source"
                leaf_source_files = 3
            "#});

            let next = lower_ratchet_scope(previous, current, &RatchetScope::All);

            assert_eq!(next.directories.len(), 1);
            assert_eq!(next.directories[0].group, LanguageGroupId::Source);
            assert_eq!(next.directories[0].leaf_source_files.get(), 3);
        }

        #[test]
        fn raise_detection_compares_each_group_independently() {
            let previous = parse_ratchet(indoc! {r#"
                version = 1

                [[directories]]
                path = "src"
                group = "source"
                leaf_source_files = 5

                [[directories]]
                path = "src"
                group = "documentation"
                leaf_source_files = 2
            "#});
            let current = parse_ratchet(indoc! {r#"
                version = 1

                [[directories]]
                path = "src"
                group = "source"
                leaf_source_files = 4

                [[directories]]
                path = "src"
                group = "documentation"
                leaf_source_files = 3
            "#});

            assert_eq!(ratchet_raise_count(&previous, &current), 1);
        }

        #[test]
        fn sorting_orders_groups_before_paths() {
            let mut ratchet = parse_ratchet(indoc! {r#"
                version = 1

                [[directories]]
                path = "z"
                group = "source"
                leaf_source_files = 1

                [[directories]]
                path = "z"
                group = "documentation"
                leaf_source_files = 1

                [[directories]]
                path = "a"
                group = "source"
                leaf_source_files = 1
            "#});

            sort_ratchet(&mut ratchet);

            assert_eq!(
                ratchet
                    .directories
                    .iter()
                    .map(|entry| (entry.group.as_str(), entry.path.as_str()))
                    .collect::<Vec<_>>(),
                vec![("source", "a"), ("source", "z"), ("documentation", "z"),]
            );
        }
    }
}
