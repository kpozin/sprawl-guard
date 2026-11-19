//! How the ratchet file reads and writes byte ceilings.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::fmt;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::value::MapAccessDeserializer;
use serde::de::{self, MapAccess, SeqAccess, Unexpected, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

use crate::value::path::NONEMPTY_RELATIVE_PATH_PATTERN;
use crate::{FileByteLimit, FileStorage, RelativePath};

/// Byte ceiling for one file that was already larger than its byte limit.
///
/// The ratchet file writes a regular file's ceiling as a bare byte count and a Git LFS file's
/// ceiling as a table that also names its storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(into = "FileSizeEntry")]
pub struct FileSizeRatchet {
    /// How the file's content was stored, which decides the byte limit it was over.
    pub storage: FileStorage,
    /// Largest size, in bytes, the file may grow back to.
    pub max_bytes: FileByteLimit,
}

/// Reads a byte ceiling in either spelling.
///
/// A visitor picks the spelling from the value's type, so a mistake inside a table reports its
/// own field and position instead of a generic mismatch.
impl<'de> Deserialize<'de> for FileSizeRatchet {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct EntryVisitor;

        impl<'de> Visitor<'de> for EntryVisitor {
            type Value = FileSizeRatchet;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a byte count, or a table with `max_bytes` and `storage`")
            }

            fn visit_i64<E: de::Error>(self, count: i64) -> Result<Self::Value, E> {
                let count = u64::try_from(count)
                    .map_err(|_| E::invalid_value(Unexpected::Signed(count), &self))?;
                self.visit_u64(count)
            }

            fn visit_u64<E: de::Error>(self, count: u64) -> Result<Self::Value, E> {
                FileByteLimit::try_new(count)
                    .map(FileSizeRatchet::regular)
                    .map_err(E::custom)
            }

            fn visit_map<A: MapAccess<'de>>(self, table: A) -> Result<Self::Value, A::Error> {
                let StoredFileSize { max_bytes, storage } =
                    StoredFileSize::deserialize(MapAccessDeserializer::new(table))?;
                Ok(FileSizeRatchet { storage, max_bytes })
            }
        }

        deserializer.deserialize_any(EntryVisitor)
    }
}

impl FileSizeRatchet {
    const fn regular(max_bytes: FileByteLimit) -> Self {
        Self {
            storage: FileStorage::Regular,
            max_bytes,
        }
    }
}

/// How the ratchet file writes one byte ceiling.
#[derive(JsonSchema, Serialize)]
#[serde(untagged)]
enum FileSizeEntry {
    /// Ceiling for a regular file, in bytes.
    Regular(FileByteLimit),
    /// Ceiling for a file with the storage it names.
    Stored(StoredFileSize),
}

/// Byte ceiling that names how the file's content was stored.
#[derive(Deserialize, JsonSchema, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredFileSize {
    /// Largest size, in bytes, the file may grow back to.
    max_bytes: FileByteLimit,
    /// How the file's content was stored, which decides the byte limit it was over.
    storage: FileStorage,
}

impl From<FileSizeRatchet> for FileSizeEntry {
    fn from(ratchet: FileSizeRatchet) -> Self {
        match ratchet.storage {
            FileStorage::Regular => Self::Regular(ratchet.max_bytes),
            storage @ FileStorage::Lfs => Self::Stored(StoredFileSize {
                storage,
                max_bytes: ratchet.max_bytes,
            }),
        }
    }
}

/// One byte ceiling as Sprawl Guard 0.6.0 wrote it, in an array of tables.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyFileSizeRatchet {
    path: RelativePath,
    storage: LegacyFileStorage,
    max_bytes: FileByteLimit,
}

/// The storage names Sprawl Guard 0.6.0 wrote, where `git` meant a regular file.
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum LegacyFileStorage {
    Git,
    Lfs,
}

impl From<LegacyFileStorage> for FileStorage {
    fn from(storage: LegacyFileStorage) -> Self {
        match storage {
            LegacyFileStorage::Git => Self::Regular,
            LegacyFileStorage::Lfs => Self::Lfs,
        }
    }
}

/// Reads `file_sizes` as a table keyed by path, or as the array of tables that 0.6.0 wrote.
///
/// Each path may have one ceiling. Two keys can name one path, such as `a.png` and `./a.png`, and
/// the 0.6.0 form could record a ceiling for each storage of one path, so both forms reject a
/// path they see twice.
pub(super) fn deserialize_file_sizes<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<RelativePath, FileSizeRatchet>, D::Error> {
    struct FileSizesVisitor;

    impl<'de> Visitor<'de> for FileSizesVisitor {
        type Value = BTreeMap<RelativePath, FileSizeRatchet>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a table of byte ceilings keyed by path")
        }

        fn visit_map<A: MapAccess<'de>>(self, mut table: A) -> Result<Self::Value, A::Error> {
            let mut file_sizes = BTreeMap::new();
            while let Some((path, ceiling)) = table.next_entry()? {
                insert_once(&mut file_sizes, path, ceiling)?;
            }
            Ok(file_sizes)
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut entries: A) -> Result<Self::Value, A::Error> {
            let mut file_sizes = BTreeMap::new();
            while let Some(LegacyFileSizeRatchet {
                path,
                storage,
                max_bytes,
            }) = entries.next_element()?
            {
                let ceiling = FileSizeRatchet {
                    storage: storage.into(),
                    max_bytes,
                };
                insert_once(&mut file_sizes, path, ceiling)?;
            }
            Ok(file_sizes)
        }
    }

    deserializer.deserialize_any(FileSizesVisitor)
}

fn insert_once<E: de::Error>(
    file_sizes: &mut BTreeMap<RelativePath, FileSizeRatchet>,
    path: RelativePath,
    ceiling: FileSizeRatchet,
) -> Result<(), E> {
    match file_sizes.entry(path) {
        Entry::Vacant(entry) => {
            entry.insert(ceiling);
            Ok(())
        }
        Entry::Occupied(entry) => Err(E::custom(format!(
            "`file_sizes` lists `{}` more than once; keep one byte ceiling for it",
            entry.key()
        ))),
    }
}

/// Describes the map that Sprawl Guard writes.
///
/// The schema declares Draft 4, which has no `propertyNames`, so a key pattern limits the paths.
pub(super) fn file_sizes_schema(generator: &mut SchemaGenerator) -> Schema {
    let entry = generator.subschema_for::<FileSizeEntry>();
    json_schema!({
        "type": "object",
        "patternProperties": { NONEMPTY_RELATIVE_PATH_PATTERN: entry },
        "additionalProperties": false
    })
}

/// Writes each byte ceiling that names its storage as an inline table on the path's own line.
///
/// The TOML serializer writes a table inside a map as a separate `[file_sizes."path"]` section,
/// which would split the map and move those entries out of path order.
pub(super) fn inline_file_sizes(toml: &str) -> String {
    let mut document = toml
        .parse::<toml_edit::DocumentMut>()
        .expect("serialized ratchet file must parse");
    if let Some(file_sizes) = document
        .get_mut("file_sizes")
        .and_then(toml_edit::Item::as_table_mut)
    {
        for (mut path, entry) in file_sizes.iter_mut() {
            path.leaf_decor_mut().clear();
            entry.make_value();
        }
        file_sizes.sort_values();
    }
    document.to_string()
}
