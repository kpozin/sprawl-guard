//! Shared file size measurement and source-file read guards.

use std::fs::{self, File};
use std::io::{self, Read as _};
use std::path::Path;

use crate::config::ParsedCodeFileByteLimit;
use crate::{FileBytes, FileSizeSource, MeasuredFileSize, Result, SprawlError};

mod lfs_pointer;

/// Measures the size of a file for its byte limit.
///
/// A regular file small enough to be a Git LFS pointer is read, and a valid pointer is measured by
/// the size it declares. Any other file is measured by its size on disk, without reading it, so a
/// special file such as a FIFO is never opened.
pub(crate) fn measure_file_size(path: &Path) -> io::Result<MeasuredFileSize> {
    let metadata = fs::metadata(path)?;
    let on_disk = FileBytes::new(metadata.len());
    if metadata.is_file() && on_disk.into_inner() < lfs_pointer::POINTER_SIZE_BOUND {
        let mut content = Vec::new();
        File::open(path)?
            .take(lfs_pointer::POINTER_SIZE_BOUND)
            .read_to_end(&mut content)?;
        if let Some(declared) = lfs_pointer::declared_size(&content) {
            return Ok(MeasuredFileSize {
                bytes: declared,
                source: FileSizeSource::LfsPointer {
                    pointer_bytes: on_disk,
                },
            });
        }
    }
    Ok(MeasuredFileSize {
        bytes: on_disk,
        source: FileSizeSource::OnDisk,
    })
}

/// Whether a source file can be read and counted.
pub(crate) enum ParsedCodeFileSize {
    /// The file is within `traversal.max_parsed_code_file_bytes`.
    WithinLimit,
    /// The file is larger than the limit, so it is not read.
    TooLarge(FileBytes),
    /// The file is a Git LFS pointer, so its content is not the source. Holds the size the pointer
    /// declares.
    LfsPointer(FileBytes),
}

/// Decides whether a source file can be counted, reading only a file small enough to be a Git LFS
/// pointer.
///
/// `check` and `explain` call this once per source before counting it.
pub(crate) fn parsed_code_file_size(
    path: &Path,
    limit: ParsedCodeFileByteLimit,
) -> Result<ParsedCodeFileSize> {
    let size = measure_file_size(path).map_err(|source| SprawlError::ReadFileType {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(match size.source {
        FileSizeSource::LfsPointer { .. } => ParsedCodeFileSize::LfsPointer(size.bytes),
        FileSizeSource::OnDisk if size.bytes.into_inner() > limit.into_inner() => {
            ParsedCodeFileSize::TooLarge(size.bytes)
        }
        FileSizeSource::OnDisk => ParsedCodeFileSize::WithinLimit,
    })
}

/// Ensures a source file is within `traversal.max_parsed_code_file_bytes` before it is read.
///
/// Only the file's size on disk is checked, since callers have already handled Git LFS pointers.
pub(crate) fn ensure_parsed_code_file_size(
    path: &Path,
    limit: ParsedCodeFileByteLimit,
) -> Result<()> {
    let size = fs::metadata(path)
        .map_err(|source| SprawlError::ReadFileType {
            path: path.to_path_buf(),
            source,
        })?
        .len();
    if size > limit.into_inner() {
        return Err(SprawlError::ParsedCodeFileTooLarge {
            path: path.to_path_buf(),
            size: FileBytes::new(size),
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
