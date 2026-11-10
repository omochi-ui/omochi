#![cfg(windows)]
use omochi_local::{LocalEndpoint, LocalListener, files};
use std::{ffi::OsStr, os::windows::ffi::OsStrExt};
use windows_sys::Win32::{
    Foundation::CloseHandle,
    Security::{ImpersonateAnonymousToken, ImpersonateLoggedOnUser, LogonUserW, RevertToSelf},
    System::Threading::GetCurrentThread,
};
fn wide(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(Some(0)).collect()
}
#[tokio::test]
async fn anonymous_token_cannot_read_private_files_or_connect_to_pipe() {
    let root = tempfile::tempdir().unwrap();
    let private = root.path().join("private");
    files::new_private_directory(&private).unwrap();
    let file = private.join("secret");
    files::atomic_write(&file, b"private").unwrap();
    let endpoint = LocalEndpoint::for_socket("").unwrap();
    let listener = LocalListener::bind(endpoint.clone()).unwrap();
    let LocalEndpoint::NamedPipe { name } = endpoint else {
        unreachable!()
    };
    unsafe {
        struct Revert;
        impl Drop for Revert {
            fn drop(&mut self) {
                unsafe {
                    RevertToSelf();
                }
            }
        }
        assert_ne!(
            ImpersonateAnonymousToken(GetCurrentThread()),
            0,
            "{}",
            std::io::Error::last_os_error()
        );
        let revert = Revert;
        assert_eq!(
            std::fs::read(&file).unwrap_err().kind(),
            std::io::ErrorKind::PermissionDenied
        );
        assert!(
            matches!(tokio::net::windows::named_pipe::ClientOptions::new().open(name), Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied)
        );
        drop(revert);
    }
    listener.close().await;
}
#[tokio::test]
async fn foreign_token_cannot_read_private_files_or_connect_to_pipe() {
    let Ok(user) = std::env::var("OMOCHI_ACL_TEST_USER") else {
        return;
    };
    let password =
        std::env::var("OMOCHI_ACL_TEST_PASSWORD").expect("ACL fixture password required");
    let root = tempfile::tempdir().unwrap();
    let private = root.path().join("private");
    files::new_private_directory(&private).unwrap();
    let file = private.join("secret");
    files::atomic_write(&file, b"private").unwrap();
    let endpoint = LocalEndpoint::for_socket("").unwrap();
    let listener = LocalListener::bind(endpoint.clone()).unwrap();
    let LocalEndpoint::NamedPipe { name } = endpoint else {
        unreachable!()
    };
    let mut token = std::ptr::null_mut();
    unsafe {
        assert_ne!(
            LogonUserW(
                wide(&user).as_ptr(),
                wide(".").as_ptr(),
                wide(&password).as_ptr(),
                2,
                0,
                &mut token
            ),
            0,
            "{}",
            std::io::Error::last_os_error()
        );
        struct Identity(windows_sys::Win32::Foundation::HANDLE);
        impl Drop for Identity {
            fn drop(&mut self) {
                unsafe {
                    RevertToSelf();
                    CloseHandle(self.0);
                }
            }
        }
        let identity = Identity(token);
        assert_ne!(ImpersonateLoggedOnUser(token), 0);
        assert_eq!(
            std::fs::read(&file).unwrap_err().kind(),
            std::io::ErrorKind::PermissionDenied
        );
        let result = tokio::net::windows::named_pipe::ClientOptions::new().open(name);
        assert!(
            matches!(result, Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied)
        );
        drop(identity);
    }
    listener.close().await;
}
