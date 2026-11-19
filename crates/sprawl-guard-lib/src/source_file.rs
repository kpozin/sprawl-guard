//! Shared source-file read guards.

use std::fs;
use std::path::Path;

use crate::config::ParsedCodeFileByteLimit;
use crate::{Result, SprawlError};

/// Ensures a source file is within `traversal.max_parsed_code_file_bytes` before it is read.
pub(crate) fn ensure_parsed_code_file_size(
    path: &Path,
    limit: ParsedCodeFileByteLimit,
) -> Result<()> {
    let metadata = fs::metadata(path).map_err(|source| SprawlError::ReadFileType {
        path: path.to_path_buf(),
        source,
    })?;
    let size = metadata.len();
    if size > limit.into_inner() {
        return Err(SprawlError::ParsedCodeFileTooLarge {
            path: path.to_path_buf(),
            size,
            limit,
        });
    }
    Ok(())
}

/// Reads a source file after checking it against `traversal.max_parsed_code_file_bytes`.
pub(crate) fn read_source_to_string(path: &Path, limit: ParsedCodeFileByteLimit) -> Result<String> {
    ensure_parsed_code_file_size(path, limit)?;
    fs::read_to_string(path).map_err(|source| SprawlError::ReadSource {
        path: path.to_path_buf(),
        source,
    })
}
