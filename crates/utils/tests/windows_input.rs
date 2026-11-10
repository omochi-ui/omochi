#![cfg(windows)]

use omochi_utils::input::Input;
use std::{fs::OpenOptions, os::windows::io::AsRawHandle, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
};
use windows_sys::Win32::System::Console::*;

const HELPER: &str = "OMOCHI_WINDOWS_INPUT_TEST";

#[tokio::test]
async fn console_unicode_and_idle_read_cancellation() {
    if std::env::var(HELPER).as_deref() != Ok("console") {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "console_unicode_and_idle_read_cancellation",
                "--nocapture",
            ])
            .env(HELPER, "console")
            .stdin(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_secs(10), child.wait())
                .await
                .unwrap()
                .unwrap()
                .success()
        );
        return;
    }

    // Console state is process-global: change it only in this isolated child.
    unsafe {
        FreeConsole();
        assert_ne!(AllocConsole(), 0);
        assert_ne!(SetConsoleCP(437), 0);
    }
    let console = OpenOptions::new()
        .read(true)
        .write(true)
        .open("CONIN$")
        .unwrap();
    let handle = console.as_raw_handle();
    unsafe {
        assert_ne!(SetConsoleMode(handle, 0), 0);
        assert_ne!(SetStdHandle(STD_INPUT_HANDLE, handle), 0);
    }
    let mut input = Input::stdin().unwrap();
    let text = "éあ🦀";
    let events: Vec<_> = text
        .encode_utf16()
        .map(|unit| INPUT_RECORD {
            EventType: KEY_EVENT as u16,
            Event: INPUT_RECORD_0 {
                KeyEvent: KEY_EVENT_RECORD {
                    bKeyDown: 1,
                    wRepeatCount: 1,
                    uChar: KEY_EVENT_RECORD_0 { UnicodeChar: unit },
                    ..Default::default()
                },
            },
        })
        .collect();
    let mut written = 0;
    unsafe {
        assert_ne!(
            WriteConsoleInputW(handle, events.as_ptr(), events.len() as u32, &mut written),
            0
        );
    }
    assert_eq!(written as usize, events.len());
    let mut bytes = vec![0; text.len()];
    tokio::time::timeout(Duration::from_secs(5), input.read_exact(&mut bytes))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(bytes, text.as_bytes());
    // The worker returns to an idle console read. Dropping it must cancel/join it.
    drop(input);
    let mut mode = 0;
    assert_ne!(unsafe { GetConsoleMode(handle, &mut mode) }, 0);
}

#[tokio::test]
async fn redirected_stdin_preserves_bytes_and_cancels_idle_reads() {
    let mut expected = "éあ🦀".as_bytes().to_vec();
    expected.push(0x82); // Redirection is a byte stream, even for non-UTF-8 bytes.
    if std::env::var(HELPER).as_deref() == Ok("pipe") {
        let mut input = Input::stdin().unwrap();
        let mut bytes = vec![0; expected.len()];
        tokio::time::timeout(Duration::from_secs(5), input.read_exact(&mut bytes))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(bytes, expected);
        drop(input);
        return;
    }
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "redirected_stdin_preserves_bytes_and_cancels_idle_reads",
            "--nocapture",
        ])
        .env(HELPER, "pipe")
        .stdin(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    // Keep the writer open: EOF cannot unblock the worker's next read.
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(&expected)
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_secs(10), child.wait())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
}
