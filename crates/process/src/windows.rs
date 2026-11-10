use super::*;
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::File,
    mem,
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle},
        process::ExitStatusExt,
    },
    ptr,
};
use windows_sys::Win32::{
    Foundation::*,
    Security::SECURITY_ATTRIBUTES,
    Storage::FileSystem::*,
    System::{Console::*, JobObjects::*, Threading::*},
    UI::Shell::ShellExecuteW,
};
fn wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(Some(0)).collect()
}
fn check(ok: i32) -> io::Result<()> {
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
fn quote(s: &OsStr, out: &mut Vec<u16>) -> io::Result<()> {
    out.push(b'"' as u16);
    let mut slashes = 0;
    for c in s.encode_wide() {
        if c == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "NUL in argument",
            ));
        }
        if c == b'\\' as u16 {
            slashes += 1;
            continue;
        }
        out.extend(std::iter::repeat_n(
            b'\\' as u16,
            if c == b'"' as u16 {
                slashes * 2 + 1
            } else {
                slashes
            },
        ));
        out.push(c);
        slashes = 0;
    }
    out.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
    out.push(b'"' as u16);
    Ok(())
}
fn stdio(mode: Stdio, standard: u32) -> io::Result<(File, Option<omochi_local::LocalStream>)> {
    unsafe {
        if matches!(mode, Stdio::Piped) {
            let endpoint = omochi_local::LocalEndpoint::for_socket("")?;
            let omochi_local::LocalEndpoint::NamedPipe { name } = &endpoint else {
                unreachable!()
            };
            let listener = omochi_local::LocalListener::bind(endpoint.clone())?;
            let attributes = SECURITY_ATTRIBUTES {
                nLength: mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: ptr::null_mut(),
                bInheritHandle: 1,
            };
            let handle = CreateFileW(
                wide(OsStr::new(name)).as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                &attributes,
                OPEN_EXISTING,
                0,
                ptr::null_mut(),
            );
            if handle == INVALID_HANDLE_VALUE {
                return Err(io::Error::last_os_error());
            }
            let child = File::from_raw_handle(handle);
            // CreateFile has already connected this instance; accept completes immediately.
            let stream = futures::executor::block_on(listener.accept())?;
            return Ok((child, Some(stream)));
        }
        let source = if matches!(mode, Stdio::Inherit) {
            GetStdHandle(standard)
        } else {
            ptr::null_mut()
        };
        if source.is_null() || source == INVALID_HANDLE_VALUE {
            let attributes = SECURITY_ATTRIBUTES {
                nLength: mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: ptr::null_mut(),
                bInheritHandle: 1,
            };
            let handle = CreateFileW(
                wide(OsStr::new("NUL")).as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                &attributes,
                OPEN_EXISTING,
                0,
                ptr::null_mut(),
            );
            if handle == INVALID_HANDLE_VALUE {
                return Err(io::Error::last_os_error());
            }
            return Ok((File::from_raw_handle(handle), None));
        }
        let mut handle = ptr::null_mut();
        check(DuplicateHandle(
            GetCurrentProcess(),
            source,
            GetCurrentProcess(),
            &mut handle,
            0,
            1,
            DUPLICATE_SAME_ACCESS,
        ))?;
        Ok((File::from_raw_handle(handle), None))
    }
}
pub(super) fn spawn(command: &Command) -> io::Result<Child> {
    unsafe {
        let search_path = command
            .std
            .get_envs()
            .find(|(key, _)| key.eq_ignore_ascii_case("PATH"))
            .map(|(_, value)| value.map(OsString::from))
            .unwrap_or_else(|| std::env::var_os("PATH"));
        let program = which::which_in(
            command.std.get_program(),
            search_path,
            command
                .std
                .get_current_dir()
                .map(Path::to_path_buf)
                .unwrap_or(std::env::current_dir()?),
        )
        .map_err(io::Error::other)?;
        if program
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("cmd") || s.eq_ignore_ascii_case("bat"))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "batch scripts require an explicit shell command",
            ));
        }
        for (key, value) in command.std.get_envs() {
            if key.encode_wide().any(|c| c == 0 || c == 61)
                || value.is_some_and(|value| value.encode_wide().any(|c| c == 0))
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "invalid environment entry",
                ));
            }
        }
        let job = CreateJobObjectW(ptr::null(), ptr::null());
        if job.is_null() {
            return Err(io::Error::last_os_error());
        }
        let job = File::from_raw_handle(job);
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = mem::zeroed();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        check(SetInformationJobObject(
            job.as_raw_handle(),
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            mem::size_of_val(&limits) as u32,
        ))?;
        let (input, stdin) = stdio(command.input, STD_INPUT_HANDLE)?;
        let (output, stdout) = stdio(command.output, STD_OUTPUT_HANDLE)?;
        let (error, stderr) = stdio(command.error, STD_ERROR_HANDLE)?;
        let handles = [
            input.as_raw_handle(),
            output.as_raw_handle(),
            error.as_raw_handle(),
        ];
        let jobs = [job.as_raw_handle()];
        let mut size = 0;
        InitializeProcThreadAttributeList(ptr::null_mut(), 2, 0, &mut size);
        let mut allocation = vec![0usize; size.div_ceil(mem::size_of::<usize>())];
        let list = allocation.as_mut_ptr().cast();
        check(InitializeProcThreadAttributeList(list, 2, 0, &mut size))?;
        struct Attributes(LPPROC_THREAD_ATTRIBUTE_LIST);
        impl Drop for Attributes {
            fn drop(&mut self) {
                unsafe {
                    DeleteProcThreadAttributeList(self.0);
                }
            }
        }
        let _attributes = Attributes(list);
        check(UpdateProcThreadAttribute(
            list,
            0,
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
            handles.as_ptr().cast(),
            mem::size_of_val(&handles),
            ptr::null_mut(),
            ptr::null(),
        ))?;
        check(UpdateProcThreadAttribute(
            list,
            0,
            PROC_THREAD_ATTRIBUTE_JOB_LIST as usize,
            jobs.as_ptr().cast(),
            mem::size_of_val(&jobs),
            ptr::null_mut(),
            ptr::null(),
        ))?;
        let mut startup: STARTUPINFOEXW = mem::zeroed();
        startup.StartupInfo.cb = mem::size_of::<STARTUPINFOEXW>() as u32;
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.StartupInfo.hStdInput = handles[0];
        startup.StartupInfo.hStdOutput = handles[1];
        startup.StartupInfo.hStdError = handles[2];
        startup.lpAttributeList = list;
        let mut line = Vec::new();
        quote(program.as_os_str(), &mut line)?;
        for arg in command.std.get_args() {
            line.push(32);
            quote(arg, &mut line)?;
        }
        line.push(0);
        let mut environment: BTreeMap<OsString, (OsString, OsString)> = std::env::vars_os()
            .map(|(k, v)| (OsString::from(k.to_string_lossy().to_lowercase()), (k, v)))
            .collect();
        for (key, value) in command.std.get_envs() {
            let lookup = OsString::from(key.to_string_lossy().to_lowercase());
            if let Some(value) = value {
                environment.insert(lookup, (key.into(), value.into()));
            } else {
                environment.remove(&lookup);
            }
        }
        let mut block = Vec::new();
        for (key, value) in environment.values() {
            block.extend(key.encode_wide());
            block.push(61);
            block.extend(value.encode_wide());
            block.push(0);
        }
        block.push(0);
        let cwd = command.std.get_current_dir().map(|p| wide(p.as_os_str()));
        let mut info: PROCESS_INFORMATION = mem::zeroed();
        check(CreateProcessW(
            wide(program.as_os_str()).as_ptr(),
            line.as_mut_ptr(),
            ptr::null(),
            ptr::null(),
            1,
            EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT | CREATE_NEW_PROCESS_GROUP,
            block.as_ptr().cast(),
            cwd.as_ref().map_or(ptr::null(), |s| s.as_ptr()),
            &startup.StartupInfo,
            &mut info,
        ))?;
        CloseHandle(info.hThread);
        let group = Group(Arc::new(GroupInner {
            pid: info.dwProcessId,
            job,
        }));
        Ok(Child {
            stdin: stdin.map(|s| Box::new(s) as _),
            stdout: stdout.map(|s| Box::new(s) as _),
            stderr: stderr.map(|s| Box::new(s) as _),
            inner: File::from_raw_handle(info.hProcess),
            group,
        })
    }
}
pub(super) fn signal(group: &GroupInner, signal: i32) {
    unsafe {
        if signal == SIGKILL {
            TerminateJobObject(group.job.as_raw_handle(), 1);
        } else {
            GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, group.pid);
        }
    }
}
pub(super) async fn wait(process: &File) -> io::Result<ExitStatus> {
    loop {
        let state = unsafe { WaitForSingleObject(process.as_raw_handle(), 0) };
        if state == WAIT_OBJECT_0 {
            let mut code = 0;
            unsafe {
                check(GetExitCodeProcess(process.as_raw_handle(), &mut code))?;
            }
            return Ok(ExitStatus::from_raw(code));
        }
        if state == WAIT_FAILED {
            return Err(io::Error::last_os_error());
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}
pub(super) fn open_browser(url: &str) -> io::Result<()> {
    let result = unsafe {
        ShellExecuteW(
            ptr::null_mut(),
            wide(OsStr::new("open")).as_ptr(),
            wide(OsStr::new(url)).as_ptr(),
            ptr::null(),
            ptr::null(),
            1,
        )
    };
    if result as isize <= 32 {
        Err(io::Error::other("could not open browser"))
    } else {
        Ok(())
    }
}

/// A thin npm launcher cannot catch TerminateProcess. Its native child monitors
/// the original process handle so killing the launcher also shuts down services.
pub(super) struct ParentMonitor {
    process: Option<File>,
    exited: bool,
}
impl ParentMonitor {
    pub(super) fn new() -> io::Result<Self> {
        let Some(pid) = std::env::var_os("OMOCHI_LAUNCHER_PID") else {
            return Ok(Self {
                process: None,
                exited: false,
            });
        };
        let pid: u32 = pid
            .to_str()
            .and_then(|p| p.parse().ok())
            .filter(|p| *p != 0)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid launcher PID"))?;
        let handle =
            unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE, 0, pid) };
        if handle.is_null() {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER as i32) {
                return Ok(Self {
                    process: None,
                    exited: true,
                });
            }
            return Err(error);
        }
        Ok(Self {
            process: Some(unsafe { File::from_raw_handle(handle) }),
            exited: false,
        })
    }
    pub(super) async fn wait(&self) {
        if self.exited {
            return;
        }
        if let Some(process) = &self.process {
            let _ = wait(process).await;
        } else {
            std::future::pending::<()>().await;
        }
    }
}
