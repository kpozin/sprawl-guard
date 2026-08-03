//! Readable ratchet-file model for grandfathered current violations.

use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{CountedLoc, LanguageGroupId, LeafFiles, LocLimit, RelativePath, Result, SprawlError};

const SUPPORTED_RATCHET_VERSION: u32 = 1;

/// Parsed `sprawl-guard.ratchet.toml` contents.
///
/// A ratchet file stores only measured ceilings for current violations. It
/// deliberately omits language, owner, reason, and configured limits because
/// those are either derived during `check` or belong in policy config.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RatchetFile {
    /// Ratchet file schema version.
    pub version: RatchetVersion,
    /// Production source file LOC ceilings.
    pub files: Vec<FileRatchet>,
    /// Path-classified test file LOC ceilings.
    pub test_files: Vec<TestFileRatchet>,
    /// Production directory fan-out ceilings.
    pub directories: Vec<DirectoryRatchet>,
    /// Path-classified test directory fan-out ceilings.
    pub test_directories: Vec<TestDirectoryRatchet>,
}

impl RatchetFile {
    /// Reads and parses a ratchet file.
    pub fn read(path: &Path) -> Result<Self> {
        let input = fs::read_to_string(path).map_err(|source| SprawlError::ReadRatchet {
            path: path.to_path_buf(),
            source,
        })?;
        Self::parse(path, &input)
    }

    /// Parses ratchet TOML and validates the schema version.
    ///
    /// The `path` parameter is used only for diagnostics so parse errors can
    /// point at the file that produced them.
    pub fn parse(path: &Path, input: &str) -> Result<Self> {
        let file = toml::from_str::<RawRatchetFile>(input).map_err(|source| {
            SprawlError::ParseRatchet {
                path: path.to_path_buf(),
                source,
            }
        })?;
        file.try_into()
    }

    /// Writes this ratchet file in stable TOML form.
    pub fn write(&self, path: &Path) -> Result<()> {
        let output = toml::to_string_pretty(self).expect("ratchet file must serialize");
        fs::write(path, output).map_err(|source| SprawlError::WriteRatchet {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Returns true when the file contains no ratchet ceilings.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
            && self.test_files.is_empty()
            && self.directories.is_empty()
            && self.test_directories.is_empty()
    }
}

/// Supported ratchet file schema version.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct RatchetVersion(u32);

impl RatchetVersion {
    /// Creates the current supported schema version.
    pub const fn current() -> Self {
        Self(SUPPORTED_RATCHET_VERSION)
    }

    /// Returns the raw schema version.
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl Default for RatchetVersion {
    fn default() -> Self {
        Self::current()
    }
}

/// Numeric value used when reporting ratchet comparisons.
///
/// Ratchet failures can compare either configured LOC or leaf-file counts. The
/// violation kind identifies the metric, while this wrapper keeps comparison
/// values branded instead of passing raw integers through the check report.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct RatchetCount(u32);

impl RatchetCount {
    /// Creates a reportable ratchet comparison count.
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Converts a configured LOC limit to a ratchet comparison count.
    pub const fn from_code_lines(value: LocLimit) -> Self {
        Self(value.get())
    }

    /// Converts a measured LOC count to a ratchet comparison count.
    pub const fn from_counted_loc(value: CountedLoc) -> Self {
        Self(value.get())
    }

    /// Converts a leaf-file count to a ratchet comparison count.
    pub const fn from_leaf_files(value: LeafFiles) -> Self {
        Self(value.get())
    }

    /// Returns the raw count.
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl fmt::Display for RatchetCount {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Production source file ratchet ceiling.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileRatchet {
    /// Repository-relative source file path.
    pub path: RelativePath,
    /// Grandfathered production LOC ceiling.
    pub non_test_code_lines: LocLimit,
}

/// Path-classified test file ratchet ceiling.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestFileRatchet {
    /// Repository-relative test file path.
    pub path: RelativePath,
    /// Grandfathered test LOC ceiling.
    pub code_lines: LocLimit,
}

/// Identity of one persisted directory fan-out measurement.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct DirectoryRatchetKey {
    group: LanguageGroupId,
    path: RelativePath,
}

impl DirectoryRatchetKey {
    /// Creates a directory-ratchet key from its language group and path.
    pub fn new(group: LanguageGroupId, path: RelativePath) -> Self {
        Self { group, path }
    }

    /// Returns the language group measured at the directory.
    pub fn group(&self) -> &LanguageGroupId {
        &self.group
    }

    /// Returns the repository-relative directory path.
    pub fn path(&self) -> &RelativePath {
        &self.path
    }
}

/// Production directory fan-out ratchet ceiling.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirectoryRatchet {
    /// Repository-relative directory path. The repository root is `""`.
    pub path: RelativePath,
    /// Language group measured at this directory.
    pub group: LanguageGroupId,
    /// Grandfathered direct production leaf-file ceiling.
    pub leaf_source_files: LeafFiles,
}

impl DirectoryRatchet {
    /// Returns the group-and-path identity of this ratchet entry.
    pub fn key(&self) -> DirectoryRatchetKey {
        DirectoryRatchetKey::new(self.group.clone(), self.path.clone())
    }
}

/// Path-classified test directory fan-out ratchet ceiling.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestDirectoryRatchet {
    /// Repository-relative directory path. The repository root is `""`.
    pub path: RelativePath,
    /// Language group measured at this directory.
    pub group: LanguageGroupId,
    /// Grandfathered direct test leaf-file ceiling.
    pub leaf_test_files: LeafFiles,
}

impl TestDirectoryRatchet {
    /// Returns the group-and-path identity of this ratchet entry.
    pub fn key(&self) -> DirectoryRatchetKey {
        DirectoryRatchetKey::new(self.group.clone(), self.path.clone())
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRatchetFile {
    version: u32,
    #[serde(default)]
    files: Vec<FileRatchet>,
    #[serde(default)]
    test_files: Vec<TestFileRatchet>,
    #[serde(default)]
    directories: Vec<DirectoryRatchet>,
    #[serde(default)]
    test_directories: Vec<TestDirectoryRatchet>,
}

impl TryFrom<RawRatchetFile> for RatchetFile {
    type Error = SprawlError;

    fn try_from(value: RawRatchetFile) -> Result<Self> {
        if value.version != SUPPORTED_RATCHET_VERSION {
            return Err(SprawlError::UnsupportedRatchetVersion {
                version: value.version,
            });
        }
        reject_duplicate_paths("files", value.files.iter().map(|entry| &entry.path))?;
        reject_duplicate_paths(
            "test_files",
            value.test_files.iter().map(|entry| &entry.path),
        )?;
        reject_duplicate_directory_keys(
            "directories",
            value
                .directories
                .iter()
                .map(|entry| (&entry.group, &entry.path)),
        )?;
        reject_duplicate_directory_keys(
            "test_directories",
            value
                .test_directories
                .iter()
                .map(|entry| (&entry.group, &entry.path)),
        )?;
        Ok(Self {
            version: RatchetVersion::current(),
            files: value.files,
            test_files: value.test_files,
            directories: value.directories,
            test_directories: value.test_directories,
        })
    }
}

fn reject_duplicate_paths<'path>(
    section: &'static str,
    paths: impl Iterator<Item = &'path RelativePath>,
) -> Result<()> {
    let mut seen = BTreeSet::new();
    for path in paths {
        if !seen.insert(path) {
            return Err(SprawlError::DuplicateRatchetEntry {
                section,
                path: path.clone(),
            });
        }
    }
    Ok(())
}

fn reject_duplicate_directory_keys<'entry>(
    section: &'static str,
    entries: impl Iterator<Item = (&'entry LanguageGroupId, &'entry RelativePath)>,
) -> Result<()> {
    let mut seen = BTreeSet::new();
    for (group, path) in entries {
        if !seen.insert((group, path)) {
            return Err(SprawlError::DuplicateDirectoryRatchetEntry {
                section,
                group: group.clone(),
                path: path.clone(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use indoc::indoc;

    use super::*;

    mod parse {
        use super::*;

        mod when_the_file_uses_v1_schema {
            use super::*;

            #[test]
            fn it_reads_all_metric_sections() {
                let file = RatchetFile::parse(
                    Path::new("sprawl-guard.ratchet.toml"),
                    indoc! {r#"
                        version = 1

                        [[files]]
                        path = "src/client/api.ts"
                        non_test_code_lines = 731

                        [[test_files]]
                        path = "src/client/api.test.ts"
                        code_lines = 1200

                        [[directories]]
                        path = "src/api"
                        group = "source"
                        leaf_source_files = 17

                        [[test_directories]]
                        path = "src/api/tests"
                        group = "documentation"
                        leaf_test_files = 31
                    "#},
                )
                .unwrap();

                assert_eq!(file.version.get(), 1);
                assert_eq!(file.files[0].path.as_str(), "src/client/api.ts");
                assert_eq!(file.files[0].non_test_code_lines.get(), 731);
                assert_eq!(file.test_files[0].code_lines.get(), 1200);
                assert_eq!(file.directories[0].leaf_source_files.get(), 17);
                assert_eq!(file.test_directories[0].leaf_test_files.get(), 31);
            }
        }

        mod when_a_directory_entry_omits_its_group {
            use super::*;

            #[test]
            fn it_rejects_the_file() {
                let error = RatchetFile::parse(
                    Path::new("sprawl-guard.ratchet.toml"),
                    indoc! {r#"
                        version = 1

                        [[directories]]
                        path = "src/api"
                        leaf_source_files = 17
                    "#},
                )
                .unwrap_err();

                assert!(error.to_string().contains("missing field `group`"));
            }
        }

        mod when_directory_groups_share_a_path {
            use super::*;

            #[test]
            fn it_reads_each_group_as_a_distinct_entry() {
                let file = RatchetFile::parse(
                    Path::new("sprawl-guard.ratchet.toml"),
                    indoc! {r#"
                        version = 1

                        [[directories]]
                        path = "src/api"
                        group = "source"
                        leaf_source_files = 17

                        [[directories]]
                        path = "src/api"
                        group = "documentation"
                        leaf_source_files = 4
                    "#},
                )
                .unwrap();

                assert_eq!(file.directories.len(), 2);
            }
        }

        mod when_a_directory_section_repeats_a_group_and_path {
            use super::*;

            #[test]
            fn it_rejects_the_duplicate_key() {
                let error = RatchetFile::parse(
                    Path::new("sprawl-guard.ratchet.toml"),
                    indoc! {r#"
                        version = 1

                        [[directories]]
                        path = "src/api"
                        group = "source"
                        leaf_source_files = 17

                        [[directories]]
                        path = "src/api"
                        group = "source"
                        leaf_source_files = 4
                    "#},
                )
                .unwrap_err();

                assert_eq!(
                    error.to_string(),
                    "duplicate directory ratchet entry in [[directories]] for group source and path src/api"
                );
            }
        }

        mod when_optional_sections_are_absent {
            use super::*;

            #[test]
            fn it_defaults_them_to_empty_lists() {
                let file =
                    RatchetFile::parse(Path::new("sprawl-guard.ratchet.toml"), "version = 1")
                        .unwrap();

                assert!(file.is_empty());
            }
        }

        mod when_the_version_is_not_supported {
            use super::*;

            #[test]
            fn it_rejects_the_file() {
                let error =
                    RatchetFile::parse(Path::new("sprawl-guard.ratchet.toml"), "version = 2")
                        .unwrap_err();

                assert!(
                    error
                        .to_string()
                        .contains("unsupported ratchet file version 2")
                );
            }
        }

        mod when_the_file_contains_unknown_fields {
            use super::*;

            #[test]
            fn it_rejects_the_file() {
                let error = RatchetFile::parse(
                    Path::new("sprawl-guard.ratchet.toml"),
                    indoc! {r#"
                        version = 1
                        owner = "team"
                    "#},
                )
                .unwrap_err();

                assert!(error.to_string().contains("unknown field"));
            }
        }

        mod when_a_section_contains_duplicate_paths {
            use super::*;

            #[test]
            fn it_rejects_the_file() {
                let error = RatchetFile::parse(
                    Path::new("sprawl-guard.ratchet.toml"),
                    indoc! {r#"
                        version = 1

                        [[files]]
                        path = "src/lib.rs"
                        non_test_code_lines = 2

                        [[files]]
                        path = "src/lib.rs"
                        non_test_code_lines = 3
                    "#},
                )
                .unwrap_err();

                assert!(
                    error
                        .to_string()
                        .contains("duplicate ratchet entry in [[files]]")
                );
            }
        }
    }
}
