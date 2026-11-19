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
use crate::{
    CountedLines, DirectFiles, FileByteLimit, FileBytes, LanguageGroupId, LineLimit, RelativePath,
    Result, SprawlError,
};

/// Measured ceilings for current violations.
///
/// The file omits language, ownership, reasons, and configured limits. Checks derive those values
/// from source classification and the configured limits.
#[derive(Clone, Debug, Default, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
#[schemars(title = "Sprawl Guard ratchet")]
pub struct RatchetFile {
    /// Ratchet file schema version.
    pub schema_version: RatchetSchemaVersion,
    /// File line ceilings for both source roles.
    #[serde(default)]
    pub files: Vec<FileRatchet>,
    /// Directory direct-file count ceilings for both source roles.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub directories: Vec<DirectoryRatchet>,
    /// Byte ceilings for files that were already larger than their byte limit.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub file_sizes: Vec<FileSizeRatchet>,
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
        self.file_sizes
            .sort_by(|left, right| left.path.cmp(&right.path));
    }

    /// Returns true when the file contains no ratchet ceilings.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty() && self.directories.is_empty() && self.file_sizes.is_empty()
    }

    fn validate(&self) -> Result<()> {
        reject_empty_file_paths(self.files.iter().map(|entry| &entry.path))?;
        reject_empty_file_paths(self.file_sizes.iter().map(|entry| &entry.path))?;
        if let Some(entry) = first_duplicate(&self.files, FileRatchet::key) {
            return Err(SprawlError::DuplicateFileRatchetEntry {
                role: entry.role,
                path: entry.path.clone(),
            });
        }
        if let Some(entry) = first_duplicate(&self.directories, DirectoryRatchet::key) {
            return Err(SprawlError::DuplicateDirectoryRatchetEntry {
                role: entry.role,
                group: entry.group.clone(),
                path: entry.path.clone(),
            });
        }
        if let Some(entry) = first_duplicate(&self.file_sizes, |entry| entry.path.clone()) {
            return Err(SprawlError::DuplicateFileSizeRatchetEntry {
                path: entry.path.clone(),
            });
        }
        Ok(())
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
/// Ratchet ceilings can count lines, direct files or bytes. The
/// violation kind identifies the metric, while this wrapper keeps comparison
/// values branded instead of passing raw integers through the check report.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct RatchetCount(u64);

impl RatchetCount {
    /// Creates a reportable ratchet comparison count.
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Converts a line limit to a ratchet comparison count.
    pub fn from_loc_limit(value: LineLimit) -> Self {
        Self(u64::from(value.get()))
    }

    /// Converts a measured line count to a ratchet comparison count.
    pub fn from_counted_lines(value: CountedLines) -> Self {
        Self(u64::from(value.get()))
    }

    /// Converts a direct-file count to a ratchet comparison count.
    pub fn from_direct_files(value: DirectFiles) -> Self {
        Self(u64::from(value.get()))
    }

    /// Converts a measured file size to a ratchet comparison count.
    pub fn from_file_bytes(value: FileBytes) -> Self {
        Self(value.into_inner())
    }

    /// Converts a file byte limit to a ratchet comparison count.
    pub fn from_file_byte_limit(value: FileByteLimit) -> Self {
        Self(value.into_inner())
    }

    /// Returns the raw count.
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for RatchetCount {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Identity of one persisted file line measurement.
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

/// One file line ratchet ceiling.
#[derive(Clone, Debug, Eq, JsonSchema, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileRatchet {
    /// Repository-relative source file path.
    #[schemars(schema_with = "nonempty_relative_path_schema")]
    pub path: RelativePath,
    /// Production or test role measured at the file.
    pub role: SourceKind,
    /// Allowed line ceiling for an existing violation.
    pub max_lines: LineLimit,
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

/// Identity of one persisted directory direct-file measurement.
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

/// One directory direct-file ratchet ceiling.
#[derive(Clone, Debug, Eq, JsonSchema, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirectoryRatchet {
    /// Repository-relative directory path. The repository root is `""`.
    pub path: RelativePath,
    /// Production or test role measured at the directory.
    pub role: SourceKind,
    /// Language group measured at this directory.
    pub group: LanguageGroupId,
    /// Allowed direct-file ceiling for an existing violation.
    pub max_direct_files: DirectFiles,
}

impl DirectoryRatchet {
    /// Returns the path, role, and group identity of this ratchet entry.
    pub fn key(&self) -> DirectoryRatchetKey {
        DirectoryRatchetKey::new(self.path.clone(), self.role, self.group.clone())
    }
}

/// Byte ceiling for one file that was already larger than its byte limit.
#[derive(Clone, Debug, Eq, JsonSchema, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileSizeRatchet {
    /// Repository-relative file path.
    #[schemars(schema_with = "nonempty_relative_path_schema")]
    pub path: RelativePath,
    /// Largest size, in bytes, the file may grow back to.
    pub max_bytes: FileByteLimit,
}

/// Ratchet ceilings indexed by their complete measurement identities.
#[derive(Debug)]
pub struct RatchetLookup {
    files: BTreeMap<FileRatchetKey, LineLimit>,
    directories: BTreeMap<DirectoryRatchetKey, DirectFiles>,
    file_sizes: BTreeMap<RelativePath, FileByteLimit>,
}

impl RatchetLookup {
    /// Returns a file line ceiling by path and role.
    pub fn file_ceiling(&self, key: &FileRatchetKey) -> Option<LineLimit> {
        self.files.get(key).copied()
    }

    /// Returns a directory direct-file ceiling by path, role, and group.
    pub fn directory_ceiling(&self, key: &DirectoryRatchetKey) -> Option<DirectFiles> {
        self.directories.get(key).copied()
    }

    /// Returns all indexed file line ceilings.
    pub fn files(&self) -> &BTreeMap<FileRatchetKey, LineLimit> {
        &self.files
    }

    /// Returns all indexed directory direct-file ceilings.
    pub fn directories(&self) -> &BTreeMap<DirectoryRatchetKey, DirectFiles> {
        &self.directories
    }

    /// Returns the byte ceiling for one file, if the ratchet file records one.
    pub fn file_size_ceiling(&self, path: &RelativePath) -> Option<FileByteLimit> {
        self.file_sizes.get(path).copied()
    }

    /// Returns every byte ceiling, keyed by file path.
    pub fn file_sizes(&self) -> &BTreeMap<RelativePath, FileByteLimit> {
        &self.file_sizes
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
                .map(|entry| (entry.key(), entry.max_lines))
                .collect(),
            directories: file
                .directories
                .into_iter()
                .map(|entry| (entry.key(), entry.max_direct_files))
                .collect(),
            file_sizes: file
                .file_sizes
                .into_iter()
                .map(|entry| (entry.path, entry.max_bytes))
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

/// Returns the first entry whose key an earlier entry already used.
fn first_duplicate<Entry, Key: Ord>(
    entries: &[Entry],
    key: impl Fn(&Entry) -> Key,
) -> Option<&Entry> {
    let mut seen = BTreeSet::new();
    entries.iter().find(|entry| !seen.insert(key(entry)))
}

fn reject_empty_file_paths<'a>(mut paths: impl Iterator<Item = &'a RelativePath>) -> Result<()> {
    if paths.any(|path| path.as_str().is_empty()) {
        return Err(SprawlError::EmptyFileRatchetPath);
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
            max_lines: LineLimit::new(2).unwrap(),
        };
        RatchetFile {
            schema_version: RatchetSchemaVersion::CURRENT,
            files: vec![entry.clone(), entry],
            directories: Vec::new(),
            file_sizes: Vec::new(),
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
                    max_lines = 731

                    [[files]]
                    path = "src/client/api.test.ts"
                    role = "test"
                    max_lines = 1200

                    [[directories]]
                    path = "src/api"
                    role = "production"
                    group = "frontend"
                    max_direct_files = 17

                    [[directories]]
                    path = "src/api/tests"
                    role = "test"
                    group = "frontend"
                    max_direct_files = 31

                    [[file_sizes]]
                    path = "assets/logo.png"
                    max_bytes = 40000
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
                        max_lines = 2

                        [[files]]
                        path = "src/lib.rs"
                        role = "test"
                        max_lines = 3
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
                        max_lines = 2

                        [[files]]
                        path = "src/lib.rs"
                        role = "production"
                        max_lines = 3
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
                        max_direct_files = 2

                        [[directories]]
                        path = "src"
                        role = "production"
                        group = "removed_group"
                        max_direct_files = 3
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
                        max_direct_files = 2

                        [[directories]]
                        path = "src"
                        role = "test"
                        group = "source"
                        max_direct_files = 3
                    "#},
                )
                .unwrap_err();

                assert_eq!(
                    error.to_string(),
                    "duplicate directory ratchet entry for role test, group source, and path src"
                );
            }
        }

        mod when_file_size_entries_share_a_path {
            use super::*;

            #[test]
            fn it_rejects_the_file() {
                let error = RatchetFile::parse(
                    Path::new("sprawl-guard.ratchet.toml"),
                    indoc! {r#"
                        schema_version = 1

                        [[file_sizes]]
                        path = "assets/logo.png"
                        max_bytes = 2

                        [[file_sizes]]
                        path = "assets/logo.png"
                        max_bytes = 3
                    "#},
                )
                .unwrap_err();

                assert_eq!(
                    error.to_string(),
                    "duplicate file size ratchet entry for path assets/logo.png"
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
                        max_direct_files = 17
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
                        "schema_version = 1\n[[files]]\nrole = \"production\"\nmax_lines = 1",
                        "path",
                    ),
                    (
                        "schema_version = 1\n[[files]]\npath = \"a.rs\"\nmax_lines = 1",
                        "role",
                    ),
                    (
                        "schema_version = 1\n[[files]]\npath = \"a.rs\"\nrole = \"production\"",
                        "max_lines",
                    ),
                ] {
                    assert_missing_field(input, field);
                }
            }

            #[test]
            fn it_rejects_incomplete_directory_entries() {
                for (input, field) in [
                    (
                        "schema_version = 1\n[[directories]]\nrole = \"production\"\ngroup = \"source\"\nmax_direct_files = 1",
                        "path",
                    ),
                    (
                        "schema_version = 1\n[[directories]]\npath = \"src\"\ngroup = \"source\"\nmax_direct_files = 1",
                        "role",
                    ),
                    (
                        "schema_version = 1\n[[directories]]\npath = \"src\"\nrole = \"production\"\nmax_direct_files = 1",
                        "group",
                    ),
                    (
                        "schema_version = 1\n[[directories]]\npath = \"src\"\nrole = \"production\"\ngroup = \"source\"",
                        "max_direct_files",
                    ),
                ] {
                    assert_missing_field(input, field);
                }
            }
        }

        mod when_an_entry_ceiling_is_zero {
            use super::*;

            #[test]
            fn it_rejects_every_kind_of_entry() {
                for input in [
                    "schema_version = 1\n[[file_sizes]]\npath = \"a.png\"\nmax_bytes = 0",
                    "schema_version = 1\n[[files]]\npath = \"a.rs\"\nrole = \"production\"\nmax_lines = 0",
                    "schema_version = 1\n[[directories]]\npath = \"src\"\nrole = \"production\"\ngroup = \"source\"\nmax_direct_files = 0",
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
                        max_lines = 2
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
                        max_lines = 2
                    "#},
                )
                .unwrap_err();

                assert!(matches!(error, SprawlError::EmptyFileRatchetPath));
            }

            #[test]
            fn it_rejects_an_empty_file_size_path() {
                let error = RatchetFile::parse(
                    Path::new("sprawl-guard.ratchet.toml"),
                    "schema_version = 1\n[[file_sizes]]\npath = \"\"\nmax_bytes = 2",
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
                        max_lines = 2
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
                        max_lines = 2

                        [[files]]
                        path = "a.rs"
                        role = "test"
                        max_lines = 3

                        [[directories]]
                        path = "z"
                        role = "production"
                        group = "source"
                        max_direct_files = 2

                        [[directories]]
                        path = "a"
                        role = "test"
                        group = "source"
                        max_direct_files = 3
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
