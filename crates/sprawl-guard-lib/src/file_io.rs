//! Shared filesystem commit primitives for Sprawl Guard-owned text files.

use std::fs::{self, OpenOptions};
use std::io::{self, Write as _};
use std::path::Path;

use tempfile::NamedTempFile;

/// Replaces an existing text file without exposing a partially written replacement.
///
/// The replacement is written and synced in a temporary sibling before an atomic rename over the
/// target. Existing permissions are copied to the replacement. This guarantees atomic replacement,
/// not full crash durability of the rename itself; callers that require persistence across power
/// loss must additionally arrange for parent-directory syncing on supported platforms.
pub fn replace_text_atomically(path: &Path, contents: &str) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("path has no parent directory: {}", path.display()),
        )
    })?;
    #[cfg(not(target_os = "wasi"))]
    let permissions = fs::metadata(path)?.permissions();
    let mut temporary = NamedTempFile::new_in(parent)?;
    temporary.write_all(contents.as_bytes())?;
    // WASI Preview 1 cannot apply file permissions; the host chooses the replacement file mode.
    #[cfg(not(target_os = "wasi"))]
    temporary.as_file().set_permissions(permissions)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

/// Creates a new text file without overwriting an existing path.
///
/// The destination is opened with create-new semantics, so concurrent creators cannot turn a
/// preflight existence check into an overwrite.
pub fn create_text_new(path: &Path, contents: &str) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(contents.as_bytes())
}

/// Writes text with atomic replacement semantics when a target already exists.
///
/// New files use create-new semantics. If another process creates the target between the existence
/// check and creation attempt, the operation retries as an atomic replacement rather than falling
/// back to a truncating write.
pub fn write_text_atomically(path: &Path, contents: &str) -> io::Result<()> {
    match fs::metadata(path) {
        Ok(_) => replace_text_atomically(path, contents),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            match create_text_new(path, contents) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    replace_text_atomically(path, contents)
                }
                Err(error) => Err(error),
            }
        }
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_text_atomically_replaces_complete_contents() {
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().join("document.toml");
        fs::write(&path, "old\n").unwrap();

        replace_text_atomically(&path, "new\ncomplete\n").unwrap();

        assert_eq!(fs::read_to_string(path).unwrap(), "new\ncomplete\n");
    }

    #[test]
    fn create_text_new_refuses_an_existing_path() {
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().join("document.toml");
        fs::write(&path, "existing\n").unwrap();

        let error = create_text_new(&path, "replacement\n").unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read_to_string(path).unwrap(), "existing\n");
    }

    #[test]
    fn write_text_atomically_creates_a_missing_file() {
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().join("document.toml");

        write_text_atomically(&path, "created\n").unwrap();

        assert_eq!(fs::read_to_string(path).unwrap(), "created\n");
    }
}
