//! Cancel synchronous console/pipe reads without occupying Tokio's blocking pool.
use std::{
    fs::File,
    io::{self, Read},
    os::windows::io::{AsRawHandle, FromRawHandle},
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
};
use tokio::{
    io::{AsyncRead, ReadBuf},
    sync::mpsc,
};
use windows_sys::Win32::{
    Foundation::*,
    System::{Console::*, IO::CancelSynchronousIo, Threading::*},
};

fn read_console(file: &File, decoder: &mut crate::input_utf16::Decoder) -> io::Result<Vec<u8>> {
    loop {
        let mut units = [0u16; 4096];
        let mut count = 0;
        if unsafe {
            ReadConsoleW(
                file.as_raw_handle(),
                units.as_mut_ptr().cast(),
                units.len() as u32,
                &mut count,
                std::ptr::null(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        if count == 0 {
            return Ok(decoder.finish());
        }
        let bytes = decoder.write(&units[..count as usize]);
        if !bytes.is_empty() {
            return Ok(bytes);
        }
        // A read containing only a high surrogate is not EOF. Keep reading.
    }
}

pub struct Input {
    receiver: mpsc::Receiver<io::Result<Vec<u8>>>,
    pending: Vec<u8>,
    offset: usize,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Input {
    pub fn stdin() -> io::Result<Self> {
        let mut handle = std::ptr::null_mut();
        unsafe {
            if DuplicateHandle(
                GetCurrentProcess(),
                GetStdHandle(STD_INPUT_HANDLE),
                GetCurrentProcess(),
                &mut handle,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        let mut file = unsafe { File::from_raw_handle(handle) };
        let mut mode = 0;
        // ReadFile uses the console code page; wide reads preserve Unicode.
        // Redirected files/pipes continue to supply their original bytes.
        let console = unsafe { GetConsoleMode(file.as_raw_handle(), &mut mode) } != 0;
        let (sender, receiver) = mpsc::channel(1);
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let thread = std::thread::Builder::new()
            .name("omochi-stdin".into())
            .spawn(move || {
                let mut decoder = crate::input_utf16::Decoder::default();
                while !stopped.load(Ordering::Acquire) {
                    let result = if console {
                        read_console(&file, &mut decoder)
                    } else {
                        let mut bytes = vec![0; 8192];
                        file.read(&mut bytes).map(|count| {
                            bytes.truncate(count);
                            bytes
                        })
                    };
                    match result {
                        Ok(bytes) if bytes.is_empty() => break,
                        Ok(bytes) => {
                            if sender.blocking_send(Ok(bytes)).is_err() {
                                break;
                            }
                        }
                        Err(error) => {
                            if !stopped.load(Ordering::Acquire) {
                                let _ = sender.blocking_send(Err(error));
                            }
                            break;
                        }
                    }
                }
            })?;
        Ok(Self {
            receiver,
            pending: vec![],
            offset: 0,
            stop,
            thread: Some(thread),
        })
    }
}
impl AsyncRead for Input {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if buffer.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        if this.offset == this.pending.len() {
            match std::task::ready!(this.receiver.poll_recv(cx)) {
                None => return Poll::Ready(Ok(())),
                Some(Err(error)) => return Poll::Ready(Err(error)),
                Some(Ok(bytes)) => {
                    this.pending = bytes;
                    this.offset = 0;
                }
            }
        }
        let count = buffer.remaining().min(this.pending.len() - this.offset);
        buffer.put_slice(&this.pending[this.offset..this.offset + count]);
        this.offset += count;
        Poll::Ready(Ok(()))
    }
}
impl Drop for Input {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.receiver.close();
        if let Some(thread) = self.thread.take() {
            // Retry to cover cancellation racing entry into ReadFile/ReadConsoleW.
            while !thread.is_finished() {
                unsafe {
                    CancelSynchronousIo(thread.as_raw_handle());
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            let _ = thread.join();
        }
    }
}
