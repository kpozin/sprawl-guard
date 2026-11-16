use std::fs::{File, OpenOptions};
use std::hash::{Hash, Hasher};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
use std::path::Path;

/// Returns the device and inode numbers of an open file.
#[cfg(any(target_os = "redox", unix))]
fn identity(file: &File) -> io::Result<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;

    let md = file.metadata()?;
    Ok((md.dev(), md.ino()))
}

/// Returns the device and inode numbers of an open file.
///
/// The standard library's WASI metadata extension is unstable, so this asks wasi-libc.
#[cfg(target_os = "wasi")]
fn identity(file: &File) -> io::Result<(u64, u64)> {
    let mut status = std::mem::MaybeUninit::<libc::stat>::uninit();
    // SAFETY: `file` keeps the descriptor open during the call, and `status` is a valid `stat`.
    if unsafe { libc::fstat(file.as_raw_fd(), status.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `fstat` succeeded, so it initialized `status`.
    let status = unsafe { status.assume_init() };
    Ok((status.st_dev, status.st_ino))
}

#[derive(Debug)]
pub struct Handle {
    file: Option<File>,
    // If is_std is true, then we don't drop the corresponding File since it
    // will close the handle.
    is_std: bool,
    dev: u64,
    ino: u64,
}

impl Drop for Handle {
    fn drop(&mut self) {
        if self.is_std {
            // unwrap() will not panic. Since we were able to open an
            // std stream successfully, then `file` is guaranteed to be Some()
            self.file.take().unwrap().into_raw_fd();
        }
    }
}

impl Eq for Handle {}

impl PartialEq for Handle {
    fn eq(&self, other: &Handle) -> bool {
        (self.dev, self.ino) == (other.dev, other.ino)
    }
}

impl AsRawFd for crate::Handle {
    fn as_raw_fd(&self) -> RawFd {
        // unwrap() will not panic. Since we were able to open the
        // file successfully, then `file` is guaranteed to be Some()
        self.0.file.as_ref().take().unwrap().as_raw_fd()
    }
}

impl IntoRawFd for crate::Handle {
    fn into_raw_fd(mut self) -> RawFd {
        // unwrap() will not panic. Since we were able to open the
        // file successfully, then `file` is guaranteed to be Some()
        self.0.file.take().unwrap().into_raw_fd()
    }
}

impl Hash for Handle {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.dev.hash(state);
        self.ino.hash(state);
    }
}

impl Handle {
    pub fn from_path<P: AsRef<Path>>(p: P) -> io::Result<Handle> {
        Handle::from_file(OpenOptions::new().read(true).open(p)?)
    }

    pub fn from_file(file: File) -> io::Result<Handle> {
        let (dev, ino) = identity(&file)?;
        Ok(Handle {
            file: Some(file),
            is_std: false,
            dev,
            ino,
        })
    }

    pub fn from_std(file: File) -> io::Result<Handle> {
        Handle::from_file(file).map(|mut h| {
            h.is_std = true;
            h
        })
    }

    pub fn stdin() -> io::Result<Handle> {
        Handle::from_std(unsafe { File::from_raw_fd(0) })
    }

    pub fn stdout() -> io::Result<Handle> {
        Handle::from_std(unsafe { File::from_raw_fd(1) })
    }

    pub fn stderr() -> io::Result<Handle> {
        Handle::from_std(unsafe { File::from_raw_fd(2) })
    }

    pub fn as_file(&self) -> &File {
        // unwrap() will not panic. Since we were able to open the
        // file successfully, then `file` is guaranteed to be Some()
        self.file.as_ref().take().unwrap()
    }

    pub fn as_file_mut(&mut self) -> &mut File {
        // unwrap() will not panic. Since we were able to open the
        // file successfully, then `file` is guaranteed to be Some()
        self.file.as_mut().take().unwrap()
    }

    pub fn dev(&self) -> u64 {
        self.dev
    }

    pub fn ino(&self) -> u64 {
        self.ino
    }
}
