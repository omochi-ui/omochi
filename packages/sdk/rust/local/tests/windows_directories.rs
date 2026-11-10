#![cfg(windows)]

use omochi_local::{LocalRuntime, files};
use std::{os::windows::ffi::OsStrExt, path::Path, process::Command};
use windows_sys::Win32::Security::{
    DACL_SECURITY_INFORMATION, GetFileSecurityW, OWNER_SECURITY_INFORMATION,
};

fn current_directory_security() -> Vec<u64> {
    let path: Vec<u16> = std::env::current_dir()
        .unwrap()
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let information = OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;
    let mut size = 0;
    unsafe {
        GetFileSecurityW(
            path.as_ptr(),
            information,
            std::ptr::null_mut(),
            0,
            &mut size,
        );
        assert!(size > 0, "{}", std::io::Error::last_os_error());
        let mut descriptor = vec![0u64; (size as usize).div_ceil(8)];
        assert_ne!(
            GetFileSecurityW(
                path.as_ptr(),
                information,
                descriptor.as_mut_ptr().cast(),
                size,
                &mut size,
            ),
            0,
            "{}",
            std::io::Error::last_os_error()
        );
        descriptor
    }
}

#[test]
fn creates_missing_relative_directories_without_changing_current_directory_security() {
    // Resolve relative paths in a child so parallel tests retain their own cwd.
    for root in ["runtime", "nested/runtime"] {
        let sandbox = tempfile::tempdir().unwrap();
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "relative_directory_fixture",
                "--nocapture",
            ])
            .current_dir(sandbox.path())
            .env("OMOCHI_RELATIVE_DIRECTORY_TEST_ROOT", root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{root}: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(sandbox.path().join(root).join("runtime.json").is_file());
        assert!(sandbox.path().join("config").is_dir());
    }
}

#[test]
#[ignore]
fn relative_directory_fixture() {
    let root = std::env::var_os("OMOCHI_RELATIVE_DIRECTORY_TEST_ROOT").unwrap();
    let root = Path::new(&root);
    let security = current_directory_security();
    assert!(!root.exists());
    let runtime = LocalRuntime::prepare(Some(root.to_path_buf())).unwrap();
    assert_eq!(runtime.root(), root);
    for ancestor in root.ancestors().filter(|path| !path.as_os_str().is_empty()) {
        files::validate_directory(ancestor).unwrap();
    }
    files::private_directory(Path::new("config")).unwrap();
    files::validate_directory(Path::new("config")).unwrap();
    assert_eq!(current_directory_security(), security);
}
