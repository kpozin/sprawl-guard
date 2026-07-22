//! Shared source-file read guards.

use std::fs;
use std::path::Path;

use crate::config::SourceFileByteLimit;
use crate::{Result, SprawlError};

/// Ensures a source file is below the configured byte-size limit before content reads.
pub(crate) fn ensure_source_file_size(path: &Path, limit: SourceFileByteLimit) -> Result<()> {
    let metadata = fs::metadata(path).map_err(|source| SprawlError::ReadFileType {
        path: path.to_path_buf(),
        source,
    })?;
    let size = metadata.len();
    let limit = limit.get();
    if size > limit {
        return Err(SprawlError::SourceFileTooLarge {
            path: path.to_path_buf(),
            size,
            limit,
        });
    }
    Ok(())
}

/// Reads a source file after applying the shared source byte-size guard.
pub(crate) fn read_source_to_string(path: &Path, limit: SourceFileByteLimit) -> Result<String> {
    ensure_source_file_size(path, limit)?;
    fs::read_to_string(path).map_err(|source| SprawlError::ReadSource {
        path: path.to_path_buf(),
        source,
    })
}
