//! Shared file size measurement and source-file read guards.

use std::fs::{self, File};
use std::io::{self, Read as _};
use std::path::Path;

use crate::config::ParsedCodeFileByteLimit;
use crate::{FileBytes, FileStorage, Result, SprawlError};

mod lfs_attributes;
mod lfs_pointer;

pub(crate) use lfs_attributes::LfsAttributes;

/// Measures the size of a file for its byte limit.
///
/// A file in Git LFS is measured by the size of its content: the size its pointer declares when
/// Git LFS hasn't downloaded it, or its size on disk when it has. The two agree unless a Git LFS
/// extension transforms the content, in which case the pointer declares the stored object's size.
/// Any other file is measured on disk without being read.
pub(crate) fn measure_file_size(path: &Path, storage: FileStorage) -> io::Result<FileBytes> {
    let metadata = fs::metadata(path)?;
    let on_disk = FileBytes::new(metadata.len());
    Ok(match storage {
        FileStorage::Regular => on_disk,
        FileStorage::Lfs => declared_lfs_size(path, &metadata)?.unwrap_or(on_disk),
    })
}

/// Returns the size that the file at `path` declares, if it is a Git LFS pointer.
///
/// Only a regular file small enough to be a pointer is read, so a special file such as a FIFO is
/// never opened.
fn declared_lfs_size(path: &Path, metadata: &fs::Metadata) -> io::Result<Option<FileBytes>> {
    if !metadata.is_file() || metadata.len() >= lfs_pointer::POINTER_SIZE_BOUND {
        return Ok(None);
    }
    let mut content = Vec::new();
    File::open(path)?
        .take(lfs_pointer::POINTER_SIZE_BOUND)
        .read_to_end(&mut content)?;
    Ok(lfs_pointer::declared_size(&content))
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
    let read_error = |source| SprawlError::ReadFileType {
        path: path.to_path_buf(),
        source,
    };
    let metadata = fs::metadata(path).map_err(read_error)?;
    let on_disk = FileBytes::new(metadata.len());
    Ok(
        match declared_lfs_size(path, &metadata).map_err(read_error)? {
            Some(declared) => ParsedCodeFileSize::LfsPointer(declared),
            None if on_disk.into_inner() > limit.into_inner() => {
                ParsedCodeFileSize::TooLarge(on_disk)
            }
            None => ParsedCodeFileSize::WithinLimit,
        },
    )
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
