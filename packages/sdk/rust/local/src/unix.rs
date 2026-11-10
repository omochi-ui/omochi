use std::{
    fs::{self, File, OpenOptions},
    io,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

pub fn default_runtime_root() -> io::Result<PathBuf> {
    if let Some(root) = std::env::var_os("OMOCHI_RUNTIME_DIR").filter(|s| !s.is_empty()) {
        return Ok(root.into());
    }
    Ok(
        match std::env::var_os("XDG_RUNTIME_DIR").filter(|s| !s.is_empty()) {
            Some(root) => PathBuf::from(root).join("omochi/v2"),
            None => std::env::temp_dir()
                .join(format!("omochi-{}", unsafe { libc::geteuid() }))
                .join("omochi/v2"),
        },
    )
}
pub fn config_root() -> io::Result<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|s| PathBuf::from(s).join(".config")))
        .ok_or_else(|| crate::invalid("set XDG_CONFIG_HOME or --config-dir"))?;
    Ok(base.join("omochi/rust-v1"))
}
pub fn validate_directory(path: &Path) -> io::Result<()> {
    let m = fs::symlink_metadata(path)?;
    if !m.is_dir() || m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o077 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "directory must be owned and private (0700): {}",
                path.display()
            ),
        ));
    }
    Ok(())
}
pub fn private_directory(path: &Path) -> io::Result<()> {
    match fs::DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            let parent = path
                .parent()
                .ok_or_else(|| crate::invalid("directory needs a parent"))?;
            private_directory(parent)?;
            match fs::DirBuilder::new().mode(0o700).create(path) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e),
            }
        }
        Err(e) => return Err(e),
    }
    // Never tighten a writable/foreign directory; its entries may be hostile.
    let m = fs::symlink_metadata(path)?;
    if !m.is_dir() || m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o022 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "unsafe private directory",
        ));
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}
pub fn private_file(path: &Path, new: bool) -> io::Result<File> {
    let mut o = OpenOptions::new();
    o.read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    if new {
        o.create_new(true);
    } else {
        o.create(true).truncate(false);
    }
    let file = o.open(path)?;
    let m = file.metadata()?;
    if !m.is_file() || m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o077 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "file must be owned and private",
        ));
    }
    Ok(file)
}
pub fn open_read(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
}
pub fn file_identity(path: &Path) -> io::Result<(u64, u64)> {
    let m = fs::symlink_metadata(path)?;
    Ok((m.dev(), m.ino()))
}
pub fn replace(from: &Path, to: &Path) -> io::Result<()> {
    fs::rename(from, to)
}
pub fn sync_directory(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

pub fn handle_identity(file: &File) -> io::Result<(u64, u64)> {
    let m = file.metadata()?;
    Ok((m.dev(), m.ino()))
}

pub fn prepare_default_runtime() -> io::Result<PathBuf> {
    let root = default_runtime_root()?;
    if std::env::var_os("OMOCHI_RUNTIME_DIR").is_some_and(|s| !s.is_empty()) {
        private_directory(&root)?;
    } else if let Some(xdg) = std::env::var_os("XDG_RUNTIME_DIR").filter(|s| !s.is_empty()) {
        let xdg = PathBuf::from(xdg);
        private_directory(&xdg)?;
        private_directory(&xdg.join("omochi"))?;
        private_directory(&root)?;
    } else {
        let parent = root.parent().unwrap().parent().unwrap();
        for path in [parent, root.parent().unwrap(), &root] {
            private_directory(path).map_err(|e| {
                io::Error::new(
                    e.kind(),
                    format!("fallback runtime directory is unsafe: {e}"),
                )
            })?;
        }
    }
    Ok(root)
}

pub fn new_private_directory(path: &Path) -> io::Result<()> {
    fs::DirBuilder::new().mode(0o700).create(path)
}
