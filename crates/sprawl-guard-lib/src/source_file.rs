//! Shared source-file read guards.

use std::fs;
use std::path::Path;

use crate::config::ParsedCodeFileByteLimit;
use crate::{FileBytes, Result, SprawlError};

/// Whether a source file is small enough to read and count.
pub(crate) enum ParsedCodeFileSize {
    /// The file is within `traversal.max_parsed_code_file_bytes`.
    WithinLimit,
    /// The file is larger than the limit, so it is not read.
    TooLarge(FileBytes),
}

/// Compares a source file's size with `traversal.max_parsed_code_file_bytes` without reading it.
pub(crate) fn parsed_code_file_size(
    path: &Path,
    limit: ParsedCodeFileByteLimit,
) -> Result<ParsedCodeFileSize> {
    let metadata = fs::metadata(path).map_err(|source| SprawlError::ReadFileType {
        path: path.to_path_buf(),
        source,
    })?;
    let size = metadata.len();
    if size > limit.into_inner() {
        return Ok(ParsedCodeFileSize::TooLarge(FileBytes::new(size)));
    }
    Ok(ParsedCodeFileSize::WithinLimit)
}

/// Ensures a source file is within `traversal.max_parsed_code_file_bytes` before it is read.
pub(crate) fn ensure_parsed_code_file_size(
    path: &Path,
    limit: ParsedCodeFileByteLimit,
) -> Result<()> {
    match parsed_code_file_size(path, limit)? {
        ParsedCodeFileSize::WithinLimit => Ok(()),
        ParsedCodeFileSize::TooLarge(size) => Err(SprawlError::ParsedCodeFileTooLarge {
            path: path.to_path_buf(),
            size,
            limit,
        }),
    }
}

/// Reads a source file after checking it against `traversal.max_parsed_code_file_bytes`.
pub(crate) fn read_source_to_string(path: &Path, limit: ParsedCodeFileByteLimit) -> Result<String> {
    ensure_parsed_code_file_size(path, limit)?;
    fs::read_to_string(path).map_err(|source| SprawlError::ReadSource {
        path: path.to_path_buf(),
        source,
    })
}
