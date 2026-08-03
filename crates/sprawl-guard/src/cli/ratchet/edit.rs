use sprawl_guard_lib::{
    DirectoryRatchet, FileRatchet, RatchetFile, RatchetLookup, RatchetSchemaVersion,
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
    next.directories.extend(current.directories);
    next.sort();
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
) -> sprawl_guard_lib::Result<RatchetFile> {
    let current = RatchetLookup::try_from(current)?;
    let mut next = RatchetFile {
        schema_version: RatchetSchemaVersion::CURRENT,
        files: previous
            .files
            .into_iter()
            .filter_map(|entry| {
                if !scope.contains(&entry.path) {
                    return Some(entry);
                }
                current.file_ceiling(&entry.key()).map(|value| FileRatchet {
                    path: entry.path,
                    role: entry.role,
                    loc: std::cmp::min(entry.loc, value),
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
                    .directory_ceiling(&entry.key())
                    .map(|value| DirectoryRatchet {
                        path: entry.path,
                        role: entry.role,
                        group: entry.group,
                        leaf_files: std::cmp::min(entry.leaf_files, value),
                    })
            })
            .collect(),
    };
    next.sort();
    Ok(next)
}

/// Counts existing ratchet ceilings that `baseline` would raise.
///
/// New entries are not raises. Missing entries are handled by scoped/full
/// replacement, not by this guard.
pub(super) fn ratchet_raise_count(
    previous: &RatchetFile,
    current: &RatchetFile,
) -> sprawl_guard_lib::Result<usize> {
    let previous = RatchetLookup::try_from(previous.clone())?;
    let current = RatchetLookup::try_from(current.clone())?;
    Ok(current
        .files()
        .iter()
        .filter(|entry| {
            let (key, current) = *entry;
            previous
                .file_ceiling(key)
                .is_some_and(|previous| *current > previous)
        })
        .count()
        + current
            .directories()
            .iter()
            .filter(|entry| {
                let (key, current) = *entry;
                previous
                    .directory_ceiling(key)
                    .is_some_and(|previous| *current > previous)
            })
            .count())
}

fn preserve_out_of_scope(previous: RatchetFile, scope: &RatchetScope) -> RatchetFile {
    RatchetFile {
        schema_version: RatchetSchemaVersion::CURRENT,
        files: previous
            .files
            .into_iter()
            .filter(|entry| !scope.contains(&entry.path))
            .collect(),
        directories: previous
            .directories
            .into_iter()
            .filter(|entry| !scope.contains(&entry.path))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use indoc::indoc;
    use sprawl_guard_lib::LanguageGroupId;
    use sprawl_guard_lib::classification::SourceKind;

    use super::*;

    fn parse_ratchet(input: &str) -> RatchetFile {
        RatchetFile::parse(Path::new("sprawl-guard.ratchet.toml"), input).unwrap()
    }

    mod when_directory_groups_share_a_path {
        use super::*;

        #[test]
        fn lowering_matches_the_complete_group_and_path_key() {
            let previous = parse_ratchet(indoc! {r#"
                schema_version = 1

                [[directories]]
                path = "src"
                role = "production"
                group = "source"
                leaf_files = 5

                [[directories]]
                path = "src"
                role = "production"
                group = "documentation"
                leaf_files = 7
            "#});
            let current = parse_ratchet(indoc! {r#"
                schema_version = 1

                [[directories]]
                path = "src"
                role = "production"
                group = "source"
                leaf_files = 3
            "#});

            let next = lower_ratchet_scope(previous, current, &RatchetScope::All).unwrap();

            assert_eq!(next.directories.len(), 1);
            assert_eq!(next.directories[0].group, LanguageGroupId::Source);
            assert_eq!(next.directories[0].leaf_files.get(), 3);
        }

        #[test]
        fn raise_detection_compares_each_group_independently() {
            let previous = parse_ratchet(indoc! {r#"
                schema_version = 1

                [[directories]]
                path = "src"
                role = "production"
                group = "source"
                leaf_files = 5

                [[directories]]
                path = "src"
                role = "production"
                group = "documentation"
                leaf_files = 2
            "#});
            let current = parse_ratchet(indoc! {r#"
                schema_version = 1

                [[directories]]
                path = "src"
                role = "production"
                group = "source"
                leaf_files = 4

                [[directories]]
                path = "src"
                role = "production"
                group = "documentation"
                leaf_files = 3
            "#});

            assert_eq!(ratchet_raise_count(&previous, &current).unwrap(), 1);
        }

        #[test]
        fn sorting_orders_paths_before_roles_and_groups() {
            let mut ratchet = parse_ratchet(indoc! {r#"
                schema_version = 1

                [[directories]]
                path = "z"
                role = "test"
                group = "source"
                leaf_files = 1

                [[directories]]
                path = "z"
                role = "production"
                group = "documentation"
                leaf_files = 1

                [[directories]]
                path = "a"
                role = "test"
                group = "source"
                leaf_files = 1

                [[directories]]
                path = "z"
                role = "production"
                group = "source"
                leaf_files = 1
            "#});

            ratchet.sort();

            assert_eq!(
                ratchet
                    .directories
                    .iter()
                    .map(|entry| (entry.path.as_str(), entry.role, entry.group.as_str()))
                    .collect::<Vec<_>>(),
                vec![
                    ("a", SourceKind::Test, "source"),
                    ("z", SourceKind::Production, "source"),
                    ("z", SourceKind::Production, "documentation"),
                    ("z", SourceKind::Test, "source"),
                ]
            );
        }
    }

    mod when_directory_roles_share_a_path_and_group {
        use super::*;

        #[test]
        fn raise_detection_compares_each_role_independently() {
            let previous = parse_ratchet(indoc! {r#"
                schema_version = 1

                [[directories]]
                path = "src"
                role = "production"
                group = "source"
                leaf_files = 5

                [[directories]]
                path = "src"
                role = "test"
                group = "source"
                leaf_files = 2
            "#});
            let current = parse_ratchet(indoc! {r#"
                schema_version = 1

                [[directories]]
                path = "src"
                role = "production"
                group = "source"
                leaf_files = 4

                [[directories]]
                path = "src"
                role = "test"
                group = "source"
                leaf_files = 3
            "#});

            assert_eq!(ratchet_raise_count(&previous, &current).unwrap(), 1);
        }
    }

    mod when_file_entries_are_sorted {
        use super::*;

        #[test]
        fn paths_precede_roles() {
            let mut ratchet = parse_ratchet(indoc! {r#"
                schema_version = 1

                [[files]]
                path = "z.rs"
                role = "test"
                loc = 1

                [[files]]
                path = "z.rs"
                role = "production"
                loc = 1

                [[files]]
                path = "a.rs"
                role = "test"
                loc = 1
            "#});

            ratchet.sort();

            assert_eq!(
                ratchet
                    .files
                    .iter()
                    .map(|entry| (entry.path.as_str(), entry.role))
                    .collect::<Vec<_>>(),
                vec![
                    ("a.rs", SourceKind::Test),
                    ("z.rs", SourceKind::Production),
                    ("z.rs", SourceKind::Test),
                ]
            );
        }
    }
}
