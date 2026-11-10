use std::{
    ffi::{OsStr, c_void},
    fs::File,
    io, mem,
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        io::{AsRawHandle, FromRawHandle},
    },
    path::{Path, PathBuf},
    ptr,
};
use windows_sys::Win32::{
    Foundation::*,
    Security::Authorization::*,
    Security::*,
    Storage::FileSystem::*,
    System::{Com::CoTaskMemFree, Threading::*},
    UI::Shell::*,
};

pub(crate) fn wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(Some(0)).collect()
}
fn check(ok: i32) -> io::Result<()> {
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
struct Allocation(*mut c_void);
impl Drop for Allocation {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}
fn user() -> io::Result<Vec<u64>> {
    unsafe {
        let mut token = ptr::null_mut();
        check(OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_QUERY,
            &mut token,
        ))?;
        let mut size = 0;
        GetTokenInformation(token, TokenUser, ptr::null_mut(), 0, &mut size);
        let mut data = vec![0u64; (size as usize).div_ceil(8)];
        let result = check(GetTokenInformation(
            token,
            TokenUser,
            data.as_mut_ptr().cast(),
            size,
            &mut size,
        ));
        CloseHandle(token);
        result?;
        Ok(data)
    }
}
pub(crate) struct Security {
    descriptor: Allocation,
}
impl Security {
    pub(crate) fn new(directory: bool) -> io::Result<Self> {
        unsafe {
            let user = user()?;
            let sid = (*(user.as_ptr().cast::<TOKEN_USER>())).User.Sid;
            let mut text = ptr::null_mut();
            check(ConvertSidToStringSidW(sid, &mut text))?;
            let allocation = Allocation(text.cast());
            let mut len = 0;
            while *text.add(len) != 0 {
                len += 1;
            }
            let sid = String::from_utf16_lossy(std::slice::from_raw_parts(text, len));
            let inherit = if directory { "OICI" } else { "" };
            let sddl = wide(OsStr::new(&format!("O:{sid}D:P(A;{inherit};FA;;;{sid})")));
            let mut descriptor = ptr::null_mut();
            check(ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                ptr::null_mut(),
            ))?;
            drop(allocation);
            Ok(Self {
                descriptor: Allocation(descriptor),
            })
        }
    }
    pub(crate) fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.descriptor.0,
            bInheritHandle: 0,
        }
    }
}
pub(crate) fn validate_owner(handle: HANDLE, private: bool) -> io::Result<()> {
    unsafe {
        let mut owner = ptr::null_mut();
        let mut dacl = ptr::null_mut();
        let mut sd = ptr::null_mut();
        let code = GetSecurityInfo(
            handle,
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            ptr::null_mut(),
            &mut dacl,
            ptr::null_mut(),
            &mut sd,
        );
        if code != 0 {
            return Err(io::Error::from_raw_os_error(code as i32));
        }
        let _sd = Allocation(sd);
        let user = user()?;
        let sid = (*(user.as_ptr().cast::<TOKEN_USER>())).User.Sid;
        if EqualSid(owner, sid) == 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "foreign object owner",
            ));
        }
        if private {
            let mut control = 0;
            let mut revision = 0;
            check(GetSecurityDescriptorControl(
                sd,
                &mut control,
                &mut revision,
            ))?;
            if dacl.is_null() || control & SE_DACL_PROTECTED == 0 || (*dacl).AceCount != 1 {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "object must have a protected current-user DACL",
                ));
            }
            let mut ace = ptr::null_mut();
            check(GetAce(dacl, 0, &mut ace))?;
            let allowed = &*(ace.cast::<ACCESS_ALLOWED_ACE>());
            if allowed.Header.AceType != 0 /* ACCESS_ALLOWED_ACE_TYPE */ || EqualSid(ptr::addr_of!(allowed.SidStart).cast_mut().cast(), sid) == 0
            {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "foreign ACL entry",
                ));
            }
        }
        Ok(())
    }
}
fn local_appdata() -> io::Result<PathBuf> {
    unsafe {
        let mut path = ptr::null_mut();
        let result = SHGetKnownFolderPath(&FOLDERID_LocalAppData, 0, ptr::null_mut(), &mut path);
        if result < 0 {
            return Err(io::Error::from_raw_os_error(result));
        }
        let mut len = 0;
        while *path.add(len) != 0 {
            len += 1;
        }
        let result = PathBuf::from(std::ffi::OsString::from_wide(std::slice::from_raw_parts(
            path, len,
        )));
        CoTaskMemFree(path.cast());
        Ok(result.join("Omochi"))
    }
}
pub fn default_runtime_root() -> io::Result<PathBuf> {
    if let Some(root) = std::env::var_os("OMOCHI_RUNTIME_DIR").filter(|s| !s.is_empty()) {
        return Ok(root.into());
    }
    Ok(local_appdata()?.join("Runtime/v2"))
}
pub fn config_root() -> io::Result<PathBuf> {
    Ok(local_appdata()?.join("rust-v1"))
}
fn open(path: &Path, access: u32, disposition: u32, directory: bool) -> io::Result<File> {
    let full = std::path::absolute(path)?;
    let text = full.to_string_lossy();
    if text.starts_with(r"\\") && !text.starts_with(r"\\?\") {
        return Err(crate::invalid(
            "runtime and state require a local filesystem",
        ));
    }
    unsafe {
        let path = wide(full.as_os_str());
        let mut volume = vec![0; 32768];
        check(GetVolumePathNameW(
            path.as_ptr(),
            volume.as_mut_ptr(),
            volume.len() as u32,
        ))?;
        if GetDriveTypeW(volume.as_ptr()) == 4 {
            return Err(crate::invalid("remote drives are not supported"));
        }
        let mut flags = 0;
        check(GetVolumeInformationW(
            volume.as_ptr(),
            ptr::null_mut(),
            0,
            ptr::null_mut(),
            ptr::null_mut(),
            &mut flags,
            ptr::null_mut(),
            0,
        ))?;
        if flags & 8 /* FILE_PERSISTENT_ACLS */ == 0 {
            return Err(crate::invalid("filesystem must support persistent ACLs"));
        }
        let security = Security::new(directory)?;
        let attributes = security.attributes();
        let handle = CreateFileW(
            path.as_ptr(),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            &attributes,
            disposition,
            FILE_FLAG_OPEN_REPARSE_POINT
                | if directory {
                    FILE_FLAG_BACKUP_SEMANTICS
                } else {
                    0
                },
            ptr::null_mut(),
        );
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        let file = File::from_raw_handle(handle);
        let mut info: BY_HANDLE_FILE_INFORMATION = mem::zeroed();
        check(GetFileInformationByHandle(handle, &mut info))?;
        if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
            || (info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0) != directory
        {
            return Err(crate::invalid("unexpected object type or reparse point"));
        }
        Ok(file)
    }
}
pub fn validate_directory(path: &Path) -> io::Result<()> {
    let f = open(path, READ_CONTROL, OPEN_EXISTING, true)?;
    validate_owner(f.as_raw_handle(), true)
}
pub fn private_directory(path: &Path) -> io::Result<()> {
    // A single relative component has an empty parent, which is not a directory.
    // Resolve it before recursion without managing the existing cwd itself.
    let path = std::path::absolute(path)?;
    if !path.exists() {
        let parent = path
            .parent()
            .ok_or_else(|| crate::invalid("directory needs a parent"))?;
        if !parent.exists() {
            private_directory(parent)?;
        }
        let security = Security::new(true)?;
        unsafe {
            if CreateDirectoryW(wide(path.as_os_str()).as_ptr(), &security.attributes()) == 0 {
                let e = io::Error::last_os_error();
                if e.raw_os_error() != Some(ERROR_ALREADY_EXISTS as i32) {
                    return Err(e);
                }
            }
        }
    }
    validate_directory(&path)
}
pub fn private_file(path: &Path, new: bool) -> io::Result<File> {
    let file = open(
        path,
        GENERIC_READ | GENERIC_WRITE | READ_CONTROL,
        if new { CREATE_NEW } else { OPEN_ALWAYS },
        false,
    )?;
    validate_owner(file.as_raw_handle(), true)?;
    Ok(file)
}
pub fn open_read(path: &Path) -> io::Result<File> {
    let file = open(path, GENERIC_READ | READ_CONTROL, OPEN_EXISTING, false)?;
    validate_owner(file.as_raw_handle(), true)?;
    Ok(file)
}
pub fn file_identity(path: &Path) -> io::Result<(u64, u64)> {
    let f = open(path, READ_CONTROL, OPEN_EXISTING, path.is_dir())?;
    unsafe {
        let mut info = mem::zeroed::<BY_HANDLE_FILE_INFORMATION>();
        check(GetFileInformationByHandle(f.as_raw_handle(), &mut info))?;
        Ok((
            info.dwVolumeSerialNumber as u64,
            (info.nFileIndexHigh as u64) << 32 | info.nFileIndexLow as u64,
        ))
    }
}
pub fn replace(from: &Path, to: &Path) -> io::Result<()> {
    // MoveFileEx rejects an existing destination while readers hold it open.
    // Windows 11 supports POSIX rename: existing readers keep the old object,
    // and subsequent opens see the replacement, matching Unix rename semantics.
    let file = open(
        from,
        DELETE | GENERIC_WRITE | READ_CONTROL,
        OPEN_EXISTING,
        false,
    )?;
    validate_owner(file.as_raw_handle(), true)?;
    let name = wide(std::path::absolute(to)?.as_os_str());
    let size = mem::size_of::<FILE_RENAME_INFO>() + name.len() * 2;
    let mut storage = vec![0usize; size.div_ceil(mem::size_of::<usize>())];
    let info = storage.as_mut_ptr().cast::<FILE_RENAME_INFO>();
    unsafe {
        (*info).Anonymous.Flags = 0x1 /* REPLACE_IF_EXISTS */ | 0x2 /* POSIX_SEMANTICS */;
        (*info).FileNameLength = u32::try_from((name.len() - 1) * 2)
            .map_err(|_| crate::invalid("replacement path is too long"))?;
        ptr::copy_nonoverlapping(name.as_ptr(), (*info).FileName.as_mut_ptr(), name.len());
    }
    for attempt in 0..6 {
        if unsafe {
            SetFileInformationByHandle(
                file.as_raw_handle(),
                FileRenameInfoEx,
                info.cast(),
                u32::try_from(size).map_err(|_| crate::invalid("replacement path is too long"))?,
            )
        } != 0
        {
            return file.sync_all();
        }
        let e = io::Error::last_os_error();
        if e.raw_os_error() != Some(ERROR_SHARING_VIOLATION as i32) || attempt == 5 {
            return Err(e);
        }
        std::thread::sleep(std::time::Duration::from_millis(10 << attempt));
    }
    unreachable!()
}
pub fn sync_directory(_path: &Path) -> io::Result<()> {
    Ok(())
}

pub fn handle_identity(file: &File) -> io::Result<(u64, u64)> {
    unsafe {
        let mut info = mem::zeroed::<BY_HANDLE_FILE_INFORMATION>();
        check(GetFileInformationByHandle(file.as_raw_handle(), &mut info))?;
        Ok((
            info.dwVolumeSerialNumber as u64,
            (info.nFileIndexHigh as u64) << 32 | info.nFileIndexLow as u64,
        ))
    }
}

pub fn prepare_default_runtime() -> io::Result<PathBuf> {
    let root = default_runtime_root()?;
    if std::env::var_os("OMOCHI_RUNTIME_DIR").is_some_and(|s| !s.is_empty()) {
        private_directory(&root)?;
    } else {
        let base = local_appdata()?;
        for path in [&base, &base.join("Runtime"), &root] {
            private_directory(path)?;
        }
    }
    Ok(root)
}

pub fn new_private_directory(path: &Path) -> io::Result<()> {
    let security = Security::new(true)?;
    unsafe {
        check(CreateDirectoryW(
            wide(path.as_os_str()).as_ptr(),
            &security.attributes(),
        ))?;
    }
    validate_directory(path)
}
