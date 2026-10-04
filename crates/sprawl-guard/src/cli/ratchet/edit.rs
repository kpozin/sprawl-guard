use std::collections::BTreeMap;

use sprawl_guard_lib::{
    DirectoryRatchet, FileRatchet, RatchetFile, RatchetLookup, RatchetSchemaVersion,
};

use super::{CeilingUpdatePolicy, scope::RatchetScope};

/// Adds newly discovered entries while preserving existing ceilings.
///
/// When increases are allowed, an existing ceiling can rise to its current measurement. This
/// operation never lowers or removes an existing ceiling.
pub(super) fn update_ratchet(
    previous: RatchetFile,
    current: RatchetFile,
    ceiling_policy: CeilingUpdatePolicy,
) -> RatchetFile {
    let mut files = previous
        .files
        .into_iter()
        .map(|entry| (entry.key(), entry))
        .collect::<BTreeMap<_, _>>();
    for current in current.files {
        files
            .entry(current.key())
            .and_modify(|previous| {
                if ceiling_policy.allows_increases() {
                    previous.max_loc = std::cmp::max(previous.max_loc, current.max_loc);
                }
            })
            .or_insert(current);
    }

    let mut directories = previous
        .directories
        .into_iter()
        .map(|entry| (entry.key(), entry))
        .collect::<BTreeMap<_, _>>();
    for current in current.directories {
        directories
            .entry(current.key())
            .and_modify(|previous| {
                if ceiling_policy.allows_increases() {
                    previous.leaf_files = std::cmp::max(previous.leaf_files, current.leaf_files);
                }
            })
            .or_insert(current);
    }

    let mut next = RatchetFile {
        schema_version: RatchetSchemaVersion::CURRENT,
        files: files.into_values().collect(),
        directories: directories.into_values().collect(),
    };
    next.sort();
    next
}

/// Applies `tighten` semantics: lower existing scoped entries and remove stale ones.
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
                    max_loc: std::cmp::min(entry.max_loc, value),
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
        fn updating_increases_only_the_changed_group_when_allowed() {
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

            let next = update_ratchet(previous, current, CeilingUpdatePolicy::AllowIncreases);

            assert_eq!(next.directories[0].group, LanguageGroupId::Source);
            assert_eq!(next.directories[0].leaf_files.get(), 5);
            assert_eq!(next.directories[1].group, LanguageGroupId::Documentation);
            assert_eq!(next.directories[1].leaf_files.get(), 3);
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
        fn updating_preserves_existing_roles_without_explicit_increases() {
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

            let next = update_ratchet(previous, current, CeilingUpdatePolicy::Preserve);

            assert_eq!(next.directories[0].role, SourceKind::Production);
            assert_eq!(next.directories[0].leaf_files.get(), 5);
            assert_eq!(next.directories[1].role, SourceKind::Test);
            assert_eq!(next.directories[1].leaf_files.get(), 2);
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
                max_loc = 1

                [[files]]
                path = "z.rs"
                role = "production"
                max_loc = 1

                [[files]]
                path = "a.rs"
                role = "test"
                max_loc = 1
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
