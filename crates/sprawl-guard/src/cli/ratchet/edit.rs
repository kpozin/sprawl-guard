use std::collections::BTreeMap;

use sprawl_guard_lib::{
    DirectFiles, DirectoryRatchet, DirectoryRatchetKey, FileByteLimit, FileRatchet, FileRatchetKey,
    FileSizeRatchet, FileSizeRatchetKey, LineLimit, RatchetFile, RatchetLookup,
    RatchetSchemaVersion, RelativePath,
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
    let mut next = RatchetFile {
        schema_version: RatchetSchemaVersion::CURRENT,
        files: merge_entries(previous.files, current.files, ceiling_policy),
        directories: merge_entries(previous.directories, current.directories, ceiling_policy),
        file_sizes: merge_entries(previous.file_sizes, current.file_sizes, ceiling_policy),
    };
    next.sort();
    next
}

/// Applies `tighten` semantics: lower selected entries and remove stale ones.
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
        files: lower_entries(previous.files, current.files(), scope),
        directories: lower_entries(previous.directories, current.directories(), scope),
        file_sizes: lower_entries(previous.file_sizes, current.file_sizes(), scope),
    };
    next.sort();
    Ok(next)
}

/// One kind of ratchet entry: an identity and the ceiling recorded for it.
trait CeilingEntry {
    type Key: Ord;
    type Ceiling: Copy + Ord;

    fn key(&self) -> Self::Key;
    fn path(&self) -> &RelativePath;
    fn ceiling_mut(&mut self) -> &mut Self::Ceiling;
}

impl CeilingEntry for FileRatchet {
    type Key = FileRatchetKey;
    type Ceiling = LineLimit;

    fn key(&self) -> Self::Key {
        FileRatchet::key(self)
    }

    fn path(&self) -> &RelativePath {
        &self.path
    }

    fn ceiling_mut(&mut self) -> &mut Self::Ceiling {
        &mut self.max_lines
    }
}

impl CeilingEntry for DirectoryRatchet {
    type Key = DirectoryRatchetKey;
    type Ceiling = DirectFiles;

    fn key(&self) -> Self::Key {
        DirectoryRatchet::key(self)
    }

    fn path(&self) -> &RelativePath {
        &self.path
    }

    fn ceiling_mut(&mut self) -> &mut Self::Ceiling {
        &mut self.max_direct_files
    }
}

impl CeilingEntry for FileSizeRatchet {
    type Key = FileSizeRatchetKey;
    type Ceiling = FileByteLimit;

    fn key(&self) -> Self::Key {
        Self::key(self)
    }

    fn path(&self) -> &RelativePath {
        &self.path
    }

    fn ceiling_mut(&mut self) -> &mut Self::Ceiling {
        &mut self.max_bytes
    }
}

/// Adds current entries that `previous` lacks, raising existing ceilings only when allowed.
fn merge_entries<Entry: CeilingEntry>(
    previous: Vec<Entry>,
    current: Vec<Entry>,
    ceiling_policy: CeilingUpdatePolicy,
) -> Vec<Entry> {
    let mut entries = previous
        .into_iter()
        .map(|entry| (entry.key(), entry))
        .collect::<BTreeMap<_, _>>();
    for mut current in current {
        entries
            .entry(current.key())
            .and_modify(|previous| {
                if ceiling_policy.allows_increases() {
                    let ceiling = previous.ceiling_mut();
                    *ceiling = std::cmp::max(*ceiling, *current.ceiling_mut());
                }
            })
            .or_insert(current);
    }
    entries.into_values().collect()
}

/// Lowers each in-scope entry to its current measurement and drops in-scope entries with none.
fn lower_entries<Entry: CeilingEntry>(
    previous: Vec<Entry>,
    current: &BTreeMap<Entry::Key, Entry::Ceiling>,
    scope: &RatchetScope,
) -> Vec<Entry> {
    previous
        .into_iter()
        .filter_map(|mut entry| {
            if !scope.contains(entry.path()) {
                return Some(entry);
            }
            let measured = *current.get(&entry.key())?;
            let ceiling = entry.ceiling_mut();
            *ceiling = std::cmp::min(*ceiling, measured);
            Some(entry)
        })
        .collect()
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

    mod when_file_size_entries_change {
        use super::*;

        const PREVIOUS: &str = indoc! {r#"
            schema_version = 1

            [[file_sizes]]
            path = "assets/kept.png"
            storage = "git"
            max_bytes = 50

            [[file_sizes]]
            path = "assets/logo.png"
            storage = "git"
            max_bytes = 100

            [[file_sizes]]
            path = "assets/removed.png"
            storage = "git"
            max_bytes = 70
        "#};

        const CURRENT: &str = indoc! {r#"
            schema_version = 1

            [[file_sizes]]
            path = "assets/kept.png"
            storage = "git"
            max_bytes = 40

            [[file_sizes]]
            path = "assets/logo.png"
            storage = "git"
            max_bytes = 120

            [[file_sizes]]
            path = "assets/new.png"
            storage = "git"
            max_bytes = 30
        "#};

        fn sizes(ratchet: &RatchetFile) -> Vec<(&str, u64)> {
            ratchet
                .file_sizes
                .iter()
                .map(|entry| (entry.path.as_str(), entry.max_bytes.into_inner()))
                .collect()
        }

        #[test]
        fn updating_adds_new_files_and_keeps_existing_ceilings() {
            let next = update_ratchet(
                parse_ratchet(PREVIOUS),
                parse_ratchet(CURRENT),
                CeilingUpdatePolicy::Preserve,
            );

            assert_eq!(
                sizes(&next),
                vec![
                    ("assets/kept.png", 50),
                    ("assets/logo.png", 100),
                    ("assets/new.png", 30),
                    ("assets/removed.png", 70),
                ]
            );
        }

        #[test]
        fn updating_raises_ceilings_when_allowed() {
            let next = update_ratchet(
                parse_ratchet(PREVIOUS),
                parse_ratchet(CURRENT),
                CeilingUpdatePolicy::AllowIncreases,
            );

            assert_eq!(sizes(&next)[1], ("assets/logo.png", 120));
        }

        #[test]
        fn lowering_shrinks_ceilings_and_drops_unmeasured_files_in_scope() {
            let scope = RatchetScope::Paths(vec![
                RelativePath::new("assets/kept.png").unwrap(),
                RelativePath::new("assets/logo.png").unwrap(),
            ]);

            let next = lower_ratchet_scope(parse_ratchet(PREVIOUS), parse_ratchet(CURRENT), &scope)
                .unwrap();

            assert_eq!(
                sizes(&next),
                vec![
                    ("assets/kept.png", 40),
                    ("assets/logo.png", 100),
                    ("assets/removed.png", 70),
                ]
            );
        }

        #[test]
        fn lowering_everything_drops_files_that_are_no_longer_measured() {
            let next = lower_ratchet_scope(
                parse_ratchet(PREVIOUS),
                parse_ratchet(CURRENT),
                &RatchetScope::All,
            )
            .unwrap();

            assert_eq!(
                sizes(&next),
                vec![("assets/kept.png", 40), ("assets/logo.png", 100)]
            );
        }
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
                max_direct_files = 5

                [[directories]]
                path = "src"
                role = "production"
                group = "documentation"
                max_direct_files = 7
            "#});
            let current = parse_ratchet(indoc! {r#"
                schema_version = 1

                [[directories]]
                path = "src"
                role = "production"
                group = "source"
                max_direct_files = 3
            "#});

            let next = lower_ratchet_scope(previous, current, &RatchetScope::All).unwrap();

            assert_eq!(next.directories.len(), 1);
            assert_eq!(next.directories[0].group, LanguageGroupId::Source);
            assert_eq!(next.directories[0].max_direct_files.get(), 3);
        }

        #[test]
        fn updating_increases_only_the_changed_group_when_allowed() {
            let previous = parse_ratchet(indoc! {r#"
                schema_version = 1

                [[directories]]
                path = "src"
                role = "production"
                group = "source"
                max_direct_files = 5

                [[directories]]
                path = "src"
                role = "production"
                group = "documentation"
                max_direct_files = 2
            "#});
            let current = parse_ratchet(indoc! {r#"
                schema_version = 1

                [[directories]]
                path = "src"
                role = "production"
                group = "source"
                max_direct_files = 4

                [[directories]]
                path = "src"
                role = "production"
                group = "documentation"
                max_direct_files = 3
            "#});

            let next = update_ratchet(previous, current, CeilingUpdatePolicy::AllowIncreases);

            assert_eq!(next.directories[0].group, LanguageGroupId::Source);
            assert_eq!(next.directories[0].max_direct_files.get(), 5);
            assert_eq!(next.directories[1].group, LanguageGroupId::Documentation);
            assert_eq!(next.directories[1].max_direct_files.get(), 3);
        }

        #[test]
        fn sorting_orders_paths_before_roles_and_groups() {
            let mut ratchet = parse_ratchet(indoc! {r#"
                schema_version = 1

                [[directories]]
                path = "z"
                role = "test"
                group = "source"
                max_direct_files = 1

                [[directories]]
                path = "z"
                role = "production"
                group = "documentation"
                max_direct_files = 1

                [[directories]]
                path = "a"
                role = "test"
                group = "source"
                max_direct_files = 1

                [[directories]]
                path = "z"
                role = "production"
                group = "source"
                max_direct_files = 1
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
                max_direct_files = 5

                [[directories]]
                path = "src"
                role = "test"
                group = "source"
                max_direct_files = 2
            "#});
            let current = parse_ratchet(indoc! {r#"
                schema_version = 1

                [[directories]]
                path = "src"
                role = "production"
                group = "source"
                max_direct_files = 4

                [[directories]]
                path = "src"
                role = "test"
                group = "source"
                max_direct_files = 3
            "#});

            let next = update_ratchet(previous, current, CeilingUpdatePolicy::Preserve);

            assert_eq!(next.directories[0].role, SourceKind::Production);
            assert_eq!(next.directories[0].max_direct_files.get(), 5);
            assert_eq!(next.directories[1].role, SourceKind::Test);
            assert_eq!(next.directories[1].max_direct_files.get(), 2);
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
                max_lines = 1

                [[files]]
                path = "z.rs"
                role = "production"
                max_lines = 1

                [[files]]
                path = "a.rs"
                role = "test"
                max_lines = 1
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
