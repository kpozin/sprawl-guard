//! Readable ratchet-file model for grandfathered current violations.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::Path;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};

use crate::classification::SourceKind;
use crate::file_io::write_text_atomically;
use crate::value::schema::{TomlSchemaDirective, TomlSchemaUrl, authored_toml_schema_generator};
use crate::{CountedLoc, LanguageGroupId, LeafFiles, LocLimit, RelativePath, Result, SprawlError};

/// Measured ceilings for current violations.
///
/// The file omits language, ownership, reasons, and configured limits. Checks derive those values
/// from source classification and policy.
#[derive(Clone, Debug, Default, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
#[schemars(title = "Sprawl Guard ratchet")]
pub struct RatchetFile {
    /// Ratchet file schema version.
    pub schema_version: RatchetSchemaVersion,
    /// File LOC ceilings for both source roles.
    #[serde(default)]
    pub files: Vec<FileRatchet>,
    /// Directory fan-out ceilings for both source roles.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub directories: Vec<DirectoryRatchet>,
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
        let document =
            toml::from_str::<toml::Value>(input).map_err(|source| SprawlError::ParseRatchet {
                path: path.to_path_buf(),
                source,
            })?;
        validate_schema_version(&document)?;
        let file = document
            .try_into::<Self>()
            .map_err(|source| SprawlError::ParseRatchet {
                path: path.to_path_buf(),
                source,
            })?;
        file.validate()?;
        Ok(file)
    }

    /// Writes this ratchet file in stable TOML form.
    pub fn write(&self, path: &Path) -> Result<()> {
        let output = self.to_toml()?;
        write_text_atomically(path, &output).map_err(|source| SprawlError::WriteRatchet {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Renders this ratchet file in stable TOML form with its editor schema directive.
    pub fn to_toml(&self) -> Result<String> {
        self.validate()?;
        let mut stable = self.clone();
        stable.sort();
        let toml = toml::to_string_pretty(&stable).expect("ratchet file must serialize");
        Ok(format!(
            "{}\n{toml}",
            RatchetSchemaVersion::CURRENT.schema_directive()
        ))
    }

    /// Sorts all entries by their complete ratchet identities.
    pub fn sort(&mut self) {
        self.files.sort_by_key(FileRatchet::key);
        self.directories.sort_by_key(DirectoryRatchet::key);
    }

    /// Returns true when the file contains no ratchet ceilings.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty() && self.directories.is_empty()
    }

    fn validate(&self) -> Result<()> {
        reject_empty_file_paths(&self.files)?;
        reject_duplicate_file_keys(&self.files)?;
        reject_duplicate_directory_keys(&self.directories)
    }
}

/// Supported ratchet file schema versions.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "i64", into = "i64")]
pub enum RatchetSchemaVersion {
    /// Ratchet schema version 1.
    V1,
}

impl RatchetSchemaVersion {
    /// Schema version emitted by this build.
    pub const CURRENT: Self = Self::V1;

    /// Returns the published JSON Schema for this version.
    pub fn schema_url(self) -> TomlSchemaUrl {
        match self {
            Self::V1 => TomlSchemaUrl::RatchetV1,
        }
    }

    /// Returns the Taplo directive for this version's published schema.
    pub fn schema_directive(self) -> TomlSchemaDirective {
        TomlSchemaDirective::new(self.schema_url())
    }
}

impl JsonSchema for RatchetSchemaVersion {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> std::borrow::Cow<'static, str> {
        "RatchetSchemaVersion".into()
    }

    fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "const": 1,
            "title": "Ratchet schema version",
            "type": "integer"
        })
    }
}

/// Generates the JSON Schema for authored ratchet files.
pub fn authored_ratchet_json_schema() -> Schema {
    let mut schema = authored_toml_schema_generator().into_root_schema_for::<RatchetFile>();
    schema.insert(
        "id".to_owned(),
        RatchetSchemaVersion::CURRENT.schema_url().as_str().into(),
    );
    schema
}

impl From<RatchetSchemaVersion> for i64 {
    fn from(version: RatchetSchemaVersion) -> Self {
        match version {
            RatchetSchemaVersion::V1 => 1,
        }
    }
}

impl TryFrom<i64> for RatchetSchemaVersion {
    type Error = SprawlError;

    fn try_from(version: i64) -> Result<Self> {
        match version {
            1 => Ok(Self::V1),
            version => Err(SprawlError::UnsupportedRatchetSchemaVersion { version }),
        }
    }
}

impl Default for RatchetSchemaVersion {
    fn default() -> Self {
        Self::CURRENT
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
    pub const fn from_loc_limit(value: LocLimit) -> Self {
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

/// Identity of one persisted file LOC measurement.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct FileRatchetKey {
    path: RelativePath,
    role: SourceKind,
}

impl FileRatchetKey {
    /// Creates a file-ratchet key from its path and source role.
    pub fn new(path: RelativePath, role: SourceKind) -> Self {
        Self { path, role }
    }

    /// Returns the repository-relative file path.
    pub fn path(&self) -> &RelativePath {
        &self.path
    }

    /// Returns the source role measured at the file.
    pub const fn role(&self) -> SourceKind {
        self.role
    }
}

/// One file LOC ratchet ceiling.
#[derive(Clone, Debug, Eq, JsonSchema, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileRatchet {
    /// Repository-relative source file path.
    #[schemars(schema_with = "nonempty_relative_path_schema")]
    pub path: RelativePath,
    /// Production or test role measured at the file.
    pub role: SourceKind,
    /// Allowed configured LOC ceiling for an existing violation.
    pub max_loc: LocLimit,
}

fn nonempty_relative_path_schema(generator: &mut SchemaGenerator) -> Schema {
    let relative_path = RelativePath::json_schema(generator);
    json_schema!({
        "allOf": [relative_path, { "minLength": 1 }]
    })
}

impl FileRatchet {
    /// Returns the path-and-role identity of this ratchet entry.
    pub fn key(&self) -> FileRatchetKey {
        FileRatchetKey::new(self.path.clone(), self.role)
    }
}

/// Identity of one persisted directory fan-out measurement.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct DirectoryRatchetKey {
    path: RelativePath,
    role: SourceKind,
    group: LanguageGroupId,
}

impl DirectoryRatchetKey {
    /// Creates a directory-ratchet key from its path, source role, and language group.
    pub fn new(path: RelativePath, role: SourceKind, group: LanguageGroupId) -> Self {
        Self { path, role, group }
    }

    /// Returns the language group measured at the directory.
    pub fn group(&self) -> &LanguageGroupId {
        &self.group
    }

    /// Returns the source role measured at the directory.
    pub const fn role(&self) -> SourceKind {
        self.role
    }

    /// Returns the repository-relative directory path.
    pub fn path(&self) -> &RelativePath {
        &self.path
    }
}

/// One directory fan-out ratchet ceiling.
#[derive(Clone, Debug, Eq, JsonSchema, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirectoryRatchet {
    /// Repository-relative directory path. The repository root is `""`.
    pub path: RelativePath,
    /// Production or test role measured at the directory.
    pub role: SourceKind,
    /// Language group measured at this directory.
    pub group: LanguageGroupId,
    /// Allowed direct leaf-file ceiling for an existing violation.
    pub leaf_files: LeafFiles,
}

impl DirectoryRatchet {
    /// Returns the path, role, and group identity of this ratchet entry.
    pub fn key(&self) -> DirectoryRatchetKey {
        DirectoryRatchetKey::new(self.path.clone(), self.role, self.group.clone())
    }
}

/// Ratchet ceilings indexed by their complete measurement identities.
#[derive(Debug)]
pub struct RatchetLookup {
    files: BTreeMap<FileRatchetKey, LocLimit>,
    directories: BTreeMap<DirectoryRatchetKey, LeafFiles>,
}

impl RatchetLookup {
    /// Returns a file LOC ceiling by path and role.
    pub fn file_ceiling(&self, key: &FileRatchetKey) -> Option<LocLimit> {
        self.files.get(key).copied()
    }

    /// Returns a directory fan-out ceiling by path, role, and group.
    pub fn directory_ceiling(&self, key: &DirectoryRatchetKey) -> Option<LeafFiles> {
        self.directories.get(key).copied()
    }

    /// Returns all indexed file LOC ceilings.
    pub fn files(&self) -> &BTreeMap<FileRatchetKey, LocLimit> {
        &self.files
    }

    /// Returns all indexed directory fan-out ceilings.
    pub fn directories(&self) -> &BTreeMap<DirectoryRatchetKey, LeafFiles> {
        &self.directories
    }
}

impl TryFrom<RatchetFile> for RatchetLookup {
    type Error = SprawlError;

    fn try_from(file: RatchetFile) -> Result<Self> {
        file.validate()?;
        Ok(Self {
            files: file
                .files
                .into_iter()
                .map(|entry| (entry.key(), entry.max_loc))
                .collect(),
            directories: file
                .directories
                .into_iter()
                .map(|entry| (entry.key(), entry.leaf_files))
                .collect(),
        })
    }
}

fn validate_schema_version(document: &toml::Value) -> Result<()> {
    let Some(version) = document.get("schema_version") else {
        return Err(SprawlError::MissingRatchetSchemaVersion);
    };
    if let Some(version) = version.as_integer() {
        RatchetSchemaVersion::try_from(version)?;
    }
    Ok(())
}

fn reject_duplicate_file_keys(entries: &[FileRatchet]) -> Result<()> {
    let mut seen = BTreeSet::new();
    for entry in entries {
        if !seen.insert(entry.key()) {
            return Err(SprawlError::DuplicateFileRatchetEntry {
                role: entry.role,
                path: entry.path.clone(),
            });
        }
    }
    Ok(())
}

fn reject_empty_file_paths(entries: &[FileRatchet]) -> Result<()> {
    if entries.iter().any(|entry| entry.path.as_str().is_empty()) {
        return Err(SprawlError::EmptyFileRatchetPath);
    }
    Ok(())
}

fn reject_duplicate_directory_keys(entries: &[DirectoryRatchet]) -> Result<()> {
    let mut seen = BTreeSet::new();
    for entry in entries {
        if !seen.insert(entry.key()) {
            return Err(SprawlError::DuplicateDirectoryRatchetEntry {
                role: entry.role,
                group: entry.group.clone(),
                path: entry.path.clone(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use indoc::indoc;
    use tempfile::tempdir;

    use super::*;

    fn ratchet_with_duplicate_file_key() -> RatchetFile {
        let entry = FileRatchet {
            path: RelativePath::new("src/lib.rs").unwrap(),
            role: SourceKind::Production,
            max_loc: LocLimit::new(2).unwrap(),
        };
        RatchetFile {
            schema_version: RatchetSchemaVersion::CURRENT,
            files: vec![entry.clone(), entry],
            directories: Vec::new(),
        }
    }

    mod parse {
        use super::*;

        mod when_the_file_uses_the_version_one_shape {
            use super::*;

            #[test]
            fn it_round_trips_role_tagged_entries() {
                let input = indoc! {r#"
                    schema_version = 1

                    [[files]]
                    path = "src/client/api.ts"
                    role = "production"
                    max_loc = 731

                    [[files]]
                    path = "src/client/api.test.ts"
                    role = "test"
                    max_loc = 1200

                    [[directories]]
                    path = "src/api"
                    role = "production"
                    group = "frontend"
                    leaf_files = 17

                    [[directories]]
                    path = "src/api/tests"
                    role = "test"
                    group = "frontend"
                    leaf_files = 31
                "#};

                let file =
                    RatchetFile::parse(Path::new("sprawl-guard.ratchet.toml"), input).unwrap();

                assert_eq!(toml::to_string_pretty(&file).unwrap(), input);
            }
        }

        mod when_the_schema_version_is_missing {
            use super::*;

            #[test]
            fn it_returns_the_dedicated_error() {
                let error =
                    RatchetFile::parse(Path::new("sprawl-guard.ratchet.toml"), "").unwrap_err();

                assert!(matches!(error, SprawlError::MissingRatchetSchemaVersion));
            }
        }

        mod when_file_entries_share_a_path {
            use super::*;

            #[test]
            fn it_accepts_distinct_roles() {
                let file = RatchetFile::parse(
                    Path::new("sprawl-guard.ratchet.toml"),
                    indoc! {r#"
                        schema_version = 1

                        [[files]]
                        path = "src/lib.rs"
                        role = "production"
                        max_loc = 2

                        [[files]]
                        path = "src/lib.rs"
                        role = "test"
                        max_loc = 3
                    "#},
                )
                .unwrap();

                assert!(!file.is_empty());
            }

            #[test]
            fn it_rejects_a_duplicate_role() {
                let error = RatchetFile::parse(
                    Path::new("sprawl-guard.ratchet.toml"),
                    indoc! {r#"
                        schema_version = 1

                        [[files]]
                        path = "src/lib.rs"
                        role = "production"
                        max_loc = 2

                        [[files]]
                        path = "src/lib.rs"
                        role = "production"
                        max_loc = 3
                    "#},
                )
                .unwrap_err();

                assert_eq!(
                    error.to_string(),
                    "duplicate file ratchet entry for role production and path src/lib.rs"
                );
            }
        }

        mod when_directory_entries_share_a_path_and_role {
            use super::*;

            #[test]
            fn it_accepts_distinct_groups() {
                let file = RatchetFile::parse(
                    Path::new("sprawl-guard.ratchet.toml"),
                    indoc! {r#"
                        schema_version = 1

                        [[directories]]
                        path = "src"
                        role = "production"
                        group = "source"
                        leaf_files = 2

                        [[directories]]
                        path = "src"
                        role = "production"
                        group = "removed_group"
                        leaf_files = 3
                    "#},
                )
                .unwrap();

                assert!(!file.is_empty());
            }

            #[test]
            fn it_rejects_a_duplicate_group() {
                let error = RatchetFile::parse(
                    Path::new("sprawl-guard.ratchet.toml"),
                    indoc! {r#"
                        schema_version = 1

                        [[directories]]
                        path = "src"
                        role = "test"
                        group = "source"
                        leaf_files = 2

                        [[directories]]
                        path = "src"
                        role = "test"
                        group = "source"
                        leaf_files = 3
                    "#},
                )
                .unwrap_err();

                assert_eq!(
                    error.to_string(),
                    "duplicate directory ratchet entry for role test, group source, and path src"
                );
            }
        }

        mod when_a_directory_entry_omits_its_group {
            use super::*;

            #[test]
            fn it_rejects_the_file() {
                let error = RatchetFile::parse(
                    Path::new("sprawl-guard.ratchet.toml"),
                    indoc! {r#"
                        schema_version = 1

                        [[directories]]
                        path = "src/api"
                        role = "production"
                        leaf_files = 17
                    "#},
                )
                .unwrap_err();

                assert!(error.to_string().contains("missing field `group`"));
            }
        }

        mod when_an_entry_omits_any_required_field {
            use super::*;

            fn assert_missing_field(input: &str, field: &str) {
                let error =
                    RatchetFile::parse(Path::new("sprawl-guard.ratchet.toml"), input).unwrap_err();

                assert!(
                    error
                        .to_string()
                        .contains(&format!("missing field `{field}`")),
                    "{error}"
                );
            }

            #[test]
            fn it_rejects_incomplete_file_entries() {
                for (input, field) in [
                    (
                        "schema_version = 1\n[[files]]\nrole = \"production\"\nmax_loc = 1",
                        "path",
                    ),
                    (
                        "schema_version = 1\n[[files]]\npath = \"a.rs\"\nmax_loc = 1",
                        "role",
                    ),
                    (
                        "schema_version = 1\n[[files]]\npath = \"a.rs\"\nrole = \"production\"",
                        "max_loc",
                    ),
                ] {
                    assert_missing_field(input, field);
                }
            }

            #[test]
            fn it_rejects_incomplete_directory_entries() {
                for (input, field) in [
                    (
                        "schema_version = 1\n[[directories]]\nrole = \"production\"\ngroup = \"source\"\nleaf_files = 1",
                        "path",
                    ),
                    (
                        "schema_version = 1\n[[directories]]\npath = \"src\"\ngroup = \"source\"\nleaf_files = 1",
                        "role",
                    ),
                    (
                        "schema_version = 1\n[[directories]]\npath = \"src\"\nrole = \"production\"\nleaf_files = 1",
                        "group",
                    ),
                    (
                        "schema_version = 1\n[[directories]]\npath = \"src\"\nrole = \"production\"\ngroup = \"source\"",
                        "leaf_files",
                    ),
                ] {
                    assert_missing_field(input, field);
                }
            }
        }

        mod when_an_entry_ceiling_is_zero {
            use super::*;

            #[test]
            fn it_rejects_file_and_directory_entries() {
                for input in [
                    "schema_version = 1\n[[files]]\npath = \"a.rs\"\nrole = \"production\"\nmax_loc = 0",
                    "schema_version = 1\n[[directories]]\npath = \"src\"\nrole = \"production\"\ngroup = \"source\"\nleaf_files = 0",
                ] {
                    assert!(
                        RatchetFile::parse(Path::new("sprawl-guard.ratchet.toml"), input).is_err()
                    );
                }
            }
        }

        mod when_optional_sections_are_absent {
            use super::*;

            #[test]
            fn it_defaults_them_to_empty_lists() {
                let file = RatchetFile::parse(
                    Path::new("sprawl-guard.ratchet.toml"),
                    "schema_version = 1",
                )
                .unwrap();

                assert!(file.is_empty());
            }
        }

        mod when_the_schema_version_is_not_supported {
            use super::*;

            #[test]
            fn it_rejects_the_file() {
                let error = RatchetFile::parse(
                    Path::new("sprawl-guard.ratchet.toml"),
                    "schema_version = 2",
                )
                .unwrap_err();

                assert!(matches!(
                    error,
                    SprawlError::UnsupportedRatchetSchemaVersion { version: 2 }
                ));
            }
        }

        mod when_the_schema_version_is_not_an_integer {
            use super::*;

            #[test]
            fn it_rejects_the_file() {
                let error = RatchetFile::parse(
                    Path::new("sprawl-guard.ratchet.toml"),
                    "schema_version = \"1\"",
                )
                .unwrap_err();

                assert!(matches!(error, SprawlError::ParseRatchet { .. }));
            }
        }

        mod when_an_entry_role_is_not_supported {
            use super::*;

            #[test]
            fn it_rejects_the_file() {
                let error = RatchetFile::parse(
                    Path::new("sprawl-guard.ratchet.toml"),
                    indoc! {r#"
                        schema_version = 1

                        [[files]]
                        path = "src/lib.rs"
                        role = "fixture"
                        max_loc = 2
                    "#},
                )
                .unwrap_err();

                assert!(matches!(error, SprawlError::ParseRatchet { .. }));
            }
        }

        mod when_a_file_entry_path_is_empty {
            use super::*;

            #[test]
            fn it_rejects_the_file() {
                let error = RatchetFile::parse(
                    Path::new("sprawl-guard.ratchet.toml"),
                    indoc! {r#"
                        schema_version = 1

                        [[files]]
                        path = ""
                        role = "production"
                        max_loc = 2
                    "#},
                )
                .unwrap_err();

                assert!(matches!(error, SprawlError::EmptyFileRatchetPath));
            }
        }

        mod when_the_file_contains_unknown_fields {
            use super::*;

            #[test]
            fn it_rejects_the_file() {
                let error = RatchetFile::parse(
                    Path::new("sprawl-guard.ratchet.toml"),
                    indoc! {r#"
                        schema_version = 1
                        owner = "team"
                    "#},
                )
                .unwrap_err();

                assert!(error.to_string().contains("unknown field"));
            }

            #[test]
            fn it_rejects_configuration_tables() {
                let error = RatchetFile::parse(
                    Path::new("sprawl-guard.ratchet.toml"),
                    indoc! {r#"
                        schema_version = 1

                        [limits.max_lines_per_file]
                        production = 400
                    "#},
                )
                .unwrap_err();

                assert!(error.to_string().contains("unknown field `limits`"));
            }
        }

        mod when_an_entry_contains_unknown_fields {
            use super::*;

            #[test]
            fn it_rejects_the_file() {
                let error = RatchetFile::parse(
                    Path::new("sprawl-guard.ratchet.toml"),
                    indoc! {r#"
                        schema_version = 1

                        [[files]]
                        path = "src/lib.rs"
                        role = "production"
                        max_loc = 2
                        owner = "team"
                    "#},
                )
                .unwrap_err();

                assert!(error.to_string().contains("unknown field `owner`"));
            }

            #[test]
            fn it_rejects_the_legacy_loc_field() {
                let error = RatchetFile::parse(
                    Path::new("sprawl-guard.ratchet.toml"),
                    indoc! {r#"
                        schema_version = 1

                        [[files]]
                        path = "src/lib.rs"
                        role = "production"
                        loc = 2
                    "#},
                )
                .unwrap_err();

                assert!(error.to_string().contains("unknown field `loc`"));
            }
        }
    }

    mod write {
        use super::*;

        mod when_entries_are_not_in_stable_order {
            use super::*;

            #[test]
            fn it_sorts_them_before_serializing() {
                let file = RatchetFile::parse(
                    Path::new("input.toml"),
                    indoc! {r#"
                        schema_version = 1

                        [[files]]
                        path = "z.rs"
                        role = "production"
                        max_loc = 2

                        [[files]]
                        path = "a.rs"
                        role = "test"
                        max_loc = 3

                        [[directories]]
                        path = "z"
                        role = "production"
                        group = "source"
                        leaf_files = 2

                        [[directories]]
                        path = "a"
                        role = "test"
                        group = "source"
                        leaf_files = 3
                    "#},
                )
                .unwrap();
                let tempdir = tempdir().unwrap();
                let path = tempdir.path().join("ratchet.toml");

                file.write(&path).unwrap();

                let contents = fs::read_to_string(&path).unwrap();
                assert!(contents.starts_with(concat!(
                    "#:schema https://kpozin.github.io/sprawl-guard/schemas/v1/",
                    "sprawl-guard.ratchet.schema.json\n",
                )));
                let written = RatchetFile::read(&path).unwrap();
                assert_eq!(written.files[0].path.as_str(), "a.rs");
                assert_eq!(written.directories[0].path.as_str(), "a");
            }
        }

        mod when_public_entries_contain_duplicate_keys {
            use super::*;

            #[test]
            fn it_rejects_the_file_instead_of_writing_ambiguous_data() {
                let file = ratchet_with_duplicate_file_key();
                let tempdir = tempdir().unwrap();
                let path = tempdir.path().join("ratchet.toml");
                fs::write(&path, "existing ratchet\n").unwrap();

                let error = file.write(&path).unwrap_err();

                assert!(matches!(
                    error,
                    SprawlError::DuplicateFileRatchetEntry { .. }
                ));
                assert_eq!(fs::read_to_string(path).unwrap(), "existing ratchet\n");
            }
        }
    }

    mod lookup {
        use super::*;

        mod when_public_entries_contain_duplicate_keys {
            use super::*;

            #[test]
            fn it_rejects_them_instead_of_collapsing_a_ceiling() {
                let file = ratchet_with_duplicate_file_key();

                let error = RatchetLookup::try_from(file).unwrap_err();

                assert!(matches!(
                    error,
                    SprawlError::DuplicateFileRatchetEntry { .. }
                ));
            }
        }
    }
}
