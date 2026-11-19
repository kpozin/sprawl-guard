//! The byte limit and size that `explain` reports for one file.

use std::io::ErrorKind;
use std::path::Path;

use serde::Serialize;

use crate::config::Config;
use crate::resolved_limits::LimitResolver;
use crate::source_file::measure_file_size;
use crate::value::path::resolve_root;
use crate::{FileBytes, FileStorage, ResolvedFileBytesLimit, Result, SprawlError};

use super::super::types::PathClassification;
use crate::source_file::LfsAttributes;

/// How `check` applies its byte limit to one file.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FileBytesExplanation {
    /// How the file is stored, which decides the byte limit that applies.
    pub storage: FileStorage,
    /// Byte limit for the file, with the overrides that matched it.
    pub limit: ResolvedFileBytesLimit,
    /// Size of the file, if it exists.
    pub size: FileSize,
}

/// Size of a file that `explain` was asked about.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum FileSize {
    /// The file does not exist, so only its limit is known.
    NotFound,
    /// The file exists and its content has this size.
    Measured {
        /// Size of the file's content. For a file in Git LFS that isn't downloaded, it is the
        /// size that its pointer declares.
        bytes: FileBytes,
    },
}

/// Explains the byte limit for the path that `classification` describes.
///
/// Returns `None` for paths that byte limits never apply to, such as directories, ignored files
/// and the ratchet file.
///
/// A `.gitattributes` file that can't be read is an error, even when `check` would only warn about
/// it: without it, `explain` can't tell which limit applies to the one path it was asked about.
pub fn explain_file_bytes(
    root: &Path,
    classification: &PathClassification,
    config: &Config,
) -> Result<Option<FileBytesExplanation>> {
    let Some(path) = classification.byte_limited_path(|explanation| &explanation.source().path)
    else {
        return Ok(None);
    };
    let root = resolve_root(root)?;
    let mut lfs_attributes = LfsAttributes::new(&root);
    let Some(storage) = lfs_attributes.storage(path) else {
        let failure = lfs_attributes
            .take_failures()
            .into_iter()
            .next()
            .expect("an unknown storage comes from an unreadable .gitattributes file");
        return Err(SprawlError::ReadGitAttributes {
            path: failure.path,
            source: failure.source,
        });
    };
    let absolute_path = root.join(path.to_path_buf());
    let size = match measure_file_size(&absolute_path, storage) {
        Ok(bytes) => FileSize::Measured { bytes },
        Err(source) if source.kind() == ErrorKind::NotFound => FileSize::NotFound,
        Err(source) => {
            return Err(SprawlError::ReadFileType {
                path: absolute_path,
                source,
            });
        }
    };
    Ok(Some(FileBytesExplanation {
        storage,
        limit: LimitResolver::new(config)?.file_bytes_limit(path, storage),
        size,
    }))
}
