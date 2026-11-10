//! C ABI v1. Inputs are copied before submit returns; callback output is borrowed
//! only for its duration. destroy joins all callbacks before returning.
use std::{ffi::c_void, sync::Arc};
use tokio::runtime::Runtime;
type Completion = unsafe extern "C" fn(*mut c_void, i32, *const u8, usize);
pub struct Context {
    runtime: Runtime,
    inner: Arc<omochi_local::binding::Context>,
    tasks: std::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>,
}
#[unsafe(no_mangle)]
pub extern "C" fn omochi_local_abi_version() -> u32 {
    1
}
#[unsafe(no_mangle)]
pub extern "C" fn omochi_local_create() -> *mut Context {
    match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    {
        Ok(runtime) => Box::into_raw(Box::new(Context {
            runtime,
            inner: Arc::default(),
            tasks: Default::default(),
        })),
        Err(_) => std::ptr::null_mut(),
    }
}
/// # Safety
/// Context is live until destroy; input buffers are valid for their lengths;
/// callback and userdata remain valid until completion or destroy returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn omochi_local_submit(
    context: *mut Context,
    command: *const u8,
    command_len: usize,
    data: *const u8,
    data_len: usize,
    callback: Completion,
    userdata: *mut c_void,
) -> i32 {
    if context.is_null()
        || command.is_null()
        || command_len > 128 * 1024
        || data_len > 65536
        || (data.is_null() && data_len != 0)
    {
        return -1;
    }
    let context = unsafe { &*context };
    let command = unsafe { std::slice::from_raw_parts(command, command_len) }.to_vec();
    let data = if data_len == 0 {
        vec![]
    } else {
        unsafe { std::slice::from_raw_parts(data, data_len) }.to_vec()
    };
    let userdata = userdata as usize;
    let inner = context.inner.clone();
    let task = context.runtime.spawn(async move {
        let (code, bytes) = match inner.request(&command, &data).await {
            Ok(bytes) => (0, bytes),
            Err(error) => (-1, format!("{:?}: {error}", error.kind()).into_bytes()),
        };
        unsafe {
            callback(userdata as *mut c_void, code, bytes.as_ptr(), bytes.len());
        }
    });
    let mut tasks = context.tasks.lock().unwrap_or_else(|e| e.into_inner());
    tasks.retain(|task| !task.is_finished());
    tasks.push(task);
    0
}
/// # Safety
/// No concurrent submit/destroy calls may use this context. Callback must not call destroy.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn omochi_local_destroy(context: *mut Context) {
    if context.is_null() {
        return;
    }
    let context = unsafe { Box::from_raw(context) };
    context.runtime.block_on(context.inner.shutdown());
    let tasks = std::mem::take(&mut *context.tasks.lock().unwrap_or_else(|e| e.into_inner()));
    context.runtime.block_on(async {
        for task in tasks {
            let _ = task.await;
        }
    });
}

/// Return the UTF-8 JSON encoded runtime path length, or -1 on error. A zero
/// capacity queries the size without writing. No native context is required.
/// # Safety
/// output points to at least capacity writable bytes, unless capacity is zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn omochi_local_runtime_root(output: *mut u8, capacity: usize) -> isize {
    let bytes = match omochi_local::default_runtime_root()
        .and_then(|path| serde_json::to_vec(&path).map_err(std::io::Error::from))
    {
        Ok(bytes) => bytes,
        Err(_) => return -1,
    };
    if capacity != 0 {
        if output.is_null() || capacity < bytes.len() {
            return -1;
        }
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), output, bytes.len());
        }
    }
    bytes.len() as isize
}

/// # Safety
/// path is a valid UTF-8 JSON path buffer of length bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn omochi_local_create_private_directory(
    path: *const u8,
    length: usize,
) -> i32 {
    if path.is_null() || length > 128 * 1024 {
        return -1;
    }
    let bytes = unsafe { std::slice::from_raw_parts(path, length) };
    let result = serde_json::from_slice::<std::path::PathBuf>(bytes)
        .map_err(std::io::Error::from)
        .and_then(|path| omochi_local::files::new_private_directory(&path));
    match result {
        Ok(()) => 0,
        Err(error) => error.raw_os_error().unwrap_or(-1),
    }
}
