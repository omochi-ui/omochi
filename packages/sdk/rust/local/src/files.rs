//! Filesystem policy expressed as current-user privacy, rather than permission bits.
use fs2::FileExt;
use std::{
    fs::{self, File},
    io::{self, Write},
    path::Path,
};

pub use crate::platform::{
    config_root, file_identity, handle_identity, new_private_directory, private_directory,
    private_file, validate_directory,
};

/// Permanent files are never unlinked or replaced. Each independent open contends.
pub struct FileLock(File);
impl FileLock {
    pub fn acquire(path: &Path, nonblocking: bool) -> io::Result<Self> {
        let file = private_file(path, false)?;
        if nonblocking {
            file.try_lock_exclusive().map_err(|error| {
                if error.raw_os_error() == fs2::lock_contended_error().raw_os_error() {
                    io::Error::new(io::ErrorKind::WouldBlock, error)
                } else {
                    error
                }
            })?;
        } else {
            FileExt::lock_exclusive(&file)?;
        }
        Ok(Self(file))
    }
    pub fn file(&self) -> &File {
        &self.0
    }
}
impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.0);
    }
}

/// The caller holds its domain lock throughout read/modify/replace. Errors after
/// replacement do not imply rollback. Temporary files are on the same filesystem.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let root = path
        .parent()
        .ok_or_else(|| crate::invalid("file needs a parent"))?;
    validate_directory(root)?;
    let temporary = root.join(format!(".state-{}.tmp", crate::random_name()?));
    let result = (|| {
        let mut file = private_file(&temporary, true)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        crate::platform::replace(&temporary, path)?;
        crate::platform::sync_directory(root)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

pub fn read_bounded(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    use std::io::Read;
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_file() || meta.len() > limit {
        return Err(crate::invalid("expected a bounded regular file"));
    }
    let file = crate::platform::open_read(path)?;
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(crate::invalid("file exceeds size limit"));
    }
    Ok(bytes)
}
