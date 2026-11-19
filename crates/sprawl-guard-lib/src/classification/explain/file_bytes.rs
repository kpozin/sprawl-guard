//! The byte limit and size that `explain` reports for one file.

use std::io::ErrorKind;
use std::path::Path;

use serde::Serialize;

use crate::config::Config;
use crate::resolved_limits::LimitResolver;
use crate::value::path::resolve_root;
use crate::{FileBytes, ResolvedFileBytesLimit, Result, SprawlError};

use super::super::types::PathClassification;

/// How `check` applies its byte limit to one file.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FileBytesExplanation {
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
    /// The file exists and has this size.
    Measured {
        /// Size of the file on disk.
        bytes: FileBytes,
    },
}

/// Explains the byte limit for the path that `classification` describes.
///
/// Returns `None` for paths that byte limits never apply to, such as directories, ignored files
/// and the ratchet file.
pub fn explain_file_bytes(
    root: &Path,
    classification: &PathClassification,
    config: &Config,
) -> Result<Option<FileBytesExplanation>> {
    let Some(path) = classification.byte_limited_path(|explanation| &explanation.source().path)
    else {
        return Ok(None);
    };
    let absolute_path = resolve_root(root)?.join(path.to_path_buf());
    let size = match absolute_path.metadata() {
        Ok(metadata) => FileSize::Measured {
            bytes: FileBytes::new(metadata.len()),
        },
        Err(source) if source.kind() == ErrorKind::NotFound => FileSize::NotFound,
        Err(source) => {
            return Err(SprawlError::ReadFileType {
                path: absolute_path,
                source,
            });
        }
    };
    Ok(Some(FileBytesExplanation {
        limit: LimitResolver::new(config)?.file_bytes_limit(path),
        size,
    }))
}
