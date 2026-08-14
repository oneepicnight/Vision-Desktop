#![cfg(windows)]

use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs::{File, OpenOptions},
    io,
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle},
    },
    path::Path,
};
use windows_sys::Win32::{
    Foundation::{
        DuplicateHandle, DUPLICATE_SAME_ACCESS, HANDLE, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT,
    },
    System::{
        JobObjects::{
            CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
            TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        },
        Threading::{
            CreateProcessW, DeleteProcThreadAttributeList, GetCurrentProcess, GetExitCodeProcess,
            InitializeProcThreadAttributeList, UpdateProcThreadAttribute, WaitForSingleObject,
            CREATE_UNICODE_ENVIRONMENT, EXTENDED_STARTUPINFO_PRESENT, INFINITE,
            PROCESS_INFORMATION, PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROC_THREAD_ATTRIBUTE_JOB_LIST,
            STARTF_USESTDHANDLES, STARTUPINFOEXW,
        },
    },
};

pub(crate) struct ContainedCoreProcess {
    process: OwnedHandle,
    job: CoreProcessJob,
    pid: u32,
}

struct CoreProcessJob {
    handle: OwnedHandle,
}

struct ProcThreadAttributeList {
    storage: Vec<usize>,
}

impl ProcThreadAttributeList {
    fn new(attribute_count: u32) -> io::Result<Self> {
        let mut bytes = 0_usize;
        unsafe {
            InitializeProcThreadAttributeList(std::ptr::null_mut(), attribute_count, 0, &mut bytes);
        }
        if bytes == 0 {
            return Err(io::Error::last_os_error());
        }
        let words = bytes.div_ceil(std::mem::size_of::<usize>());
        let mut storage = vec![0_usize; words];
        let initialized = unsafe {
            InitializeProcThreadAttributeList(
                storage.as_mut_ptr().cast(),
                attribute_count,
                0,
                &mut bytes,
            )
        };
        if initialized == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self { storage })
    }

    fn as_ptr(&mut self) -> *mut std::ffi::c_void {
        self.storage.as_mut_ptr().cast()
    }

    fn update_handles(&mut self, attribute: u32, handles: &[HANDLE]) -> io::Result<()> {
        let bytes = handles
            .len()
            .checked_mul(std::mem::size_of::<HANDLE>())
            .ok_or_else(|| io::Error::other("process attribute size overflow"))?;
        let updated = unsafe {
            UpdateProcThreadAttribute(
                self.as_ptr(),
                0,
                attribute as usize,
                handles.as_ptr().cast(),
                bytes,
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        if updated == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

impl Drop for ProcThreadAttributeList {
    fn drop(&mut self) {
        unsafe { DeleteProcThreadAttributeList(self.as_ptr()) };
    }
}

impl CoreProcessJob {
    fn new_kill_on_close() -> io::Result<Self> {
        let raw = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if raw.is_null() {
            return Err(io::Error::last_os_error());
        }
        let handle = unsafe { OwnedHandle::from_raw_handle(raw as RawHandle) };
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let size = u32::try_from(std::mem::size_of_val(&limits))
            .map_err(|_| io::Error::other("invalid Core job limit size"))?;
        let succeeded = unsafe {
            SetInformationJobObject(
                handle.as_raw_handle() as HANDLE,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size,
            )
        };
        if succeeded == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self { handle })
    }

    fn raw_handle(&self) -> HANDLE {
        self.handle.as_raw_handle() as HANDLE
    }

    fn terminate(&self) -> io::Result<()> {
        let succeeded = unsafe { TerminateJobObject(self.raw_handle(), 1) };
        if succeeded == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

impl ContainedCoreProcess {
    pub(crate) fn spawn(
        executable: &Path,
        arguments: &[OsString],
        environment_overrides: &[(OsString, OsString)],
        stdout: File,
        stderr: File,
    ) -> io::Result<Self> {
        let job = CoreProcessJob::new_kill_on_close()?;
        let stdin = OpenOptions::new().read(true).write(true).open("NUL")?;
        let inherited_stdin = duplicate_inheritable(stdin.as_raw_handle())?;
        let inherited_stdout = duplicate_inheritable(stdout.as_raw_handle())?;
        let inherited_stderr = duplicate_inheritable(stderr.as_raw_handle())?;
        let inherited_handles = [
            inherited_stdin.as_raw_handle() as HANDLE,
            inherited_stdout.as_raw_handle() as HANDLE,
            inherited_stderr.as_raw_handle() as HANDLE,
        ];
        let job_handles = [job.raw_handle()];

        let mut attributes = ProcThreadAttributeList::new(2)?;
        attributes.update_handles(PROC_THREAD_ATTRIBUTE_JOB_LIST, &job_handles)?;
        attributes.update_handles(PROC_THREAD_ATTRIBUTE_HANDLE_LIST, &inherited_handles)?;

        let mut startup = STARTUPINFOEXW::default();
        startup.StartupInfo.cb = u32::try_from(std::mem::size_of::<STARTUPINFOEXW>())
            .map_err(|_| io::Error::other("invalid process startup size"))?;
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.StartupInfo.hStdInput = inherited_handles[0];
        startup.StartupInfo.hStdOutput = inherited_handles[1];
        startup.StartupInfo.hStdError = inherited_handles[2];
        startup.lpAttributeList = attributes.as_ptr();

        let application = wide_null(executable.as_os_str())?;
        let mut command_line = windows_command_line(executable.as_os_str(), arguments)?;
        let environment = windows_environment_block(environment_overrides)?;
        let mut process_information = PROCESS_INFORMATION::default();
        let created = unsafe {
            CreateProcessW(
                application.as_ptr(),
                command_line.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1,
                EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
                environment.as_ptr().cast(),
                std::ptr::null(),
                &startup.StartupInfo as *const _,
                &mut process_information,
            )
        };
        if created == 0 {
            return Err(io::Error::last_os_error());
        }

        let process =
            unsafe { OwnedHandle::from_raw_handle(process_information.hProcess as RawHandle) };
        let thread =
            unsafe { OwnedHandle::from_raw_handle(process_information.hThread as RawHandle) };
        drop(thread);
        Ok(Self {
            process,
            job,
            pid: process_information.dwProcessId,
        })
    }

    pub(crate) fn id(&self) -> u32 {
        self.pid
    }

    pub(crate) fn as_raw_handle(&self) -> RawHandle {
        self.process.as_raw_handle()
    }

    pub(crate) fn try_wait(&self) -> io::Result<Option<i32>> {
        match unsafe { WaitForSingleObject(self.process.as_raw_handle() as HANDLE, 0) } {
            WAIT_TIMEOUT => Ok(None),
            WAIT_OBJECT_0 => self.exit_code().map(Some),
            WAIT_FAILED => Err(io::Error::last_os_error()),
            _ => Err(io::Error::other("unexpected Core process wait result")),
        }
    }

    pub(crate) fn terminate(&self) -> io::Result<()> {
        self.job.terminate()
    }

    pub(crate) fn wait(&self) -> io::Result<i32> {
        match unsafe { WaitForSingleObject(self.process.as_raw_handle() as HANDLE, INFINITE) } {
            WAIT_OBJECT_0 => self.exit_code(),
            WAIT_FAILED => Err(io::Error::last_os_error()),
            _ => Err(io::Error::other("unexpected Core process wait result")),
        }
    }

    fn exit_code(&self) -> io::Result<i32> {
        let mut code = 0_u32;
        let queried =
            unsafe { GetExitCodeProcess(self.process.as_raw_handle() as HANDLE, &mut code) };
        if queried == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(code as i32)
        }
    }

    #[cfg(test)]
    fn is_in_retained_job(&self) -> io::Result<bool> {
        use windows_sys::Win32::System::JobObjects::IsProcessInJob;
        let mut result = 0;
        let queried = unsafe {
            IsProcessInJob(
                self.process.as_raw_handle() as HANDLE,
                self.job.raw_handle(),
                &mut result,
            )
        };
        if queried == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(result != 0)
        }
    }
}

impl Drop for ContainedCoreProcess {
    fn drop(&mut self) {
        if self.try_wait().ok().flatten().is_none() && self.job.terminate().is_ok() {
            let _ = self.wait();
        }
    }
}

fn duplicate_inheritable(raw: RawHandle) -> io::Result<OwnedHandle> {
    let current = unsafe { GetCurrentProcess() };
    let mut duplicate = std::ptr::null_mut();
    let succeeded = unsafe {
        DuplicateHandle(
            current,
            raw as HANDLE,
            current,
            &mut duplicate,
            0,
            1,
            DUPLICATE_SAME_ACCESS,
        )
    };
    if succeeded == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { OwnedHandle::from_raw_handle(duplicate as RawHandle) })
    }
}

fn wide_null(value: &OsStr) -> io::Result<Vec<u16>> {
    let mut wide = value.encode_wide().collect::<Vec<_>>();
    if wide.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "process input contains a null character",
        ));
    }
    wide.push(0);
    Ok(wide)
}

fn windows_command_line(executable: &OsStr, arguments: &[OsString]) -> io::Result<Vec<u16>> {
    let mut command = Vec::new();
    append_quoted_argument(&mut command, executable)?;
    for argument in arguments {
        command.push(' ' as u16);
        append_quoted_argument(&mut command, argument)?;
    }
    command.push(0);
    Ok(command)
}

fn append_quoted_argument(output: &mut Vec<u16>, argument: &OsStr) -> io::Result<()> {
    let wide = argument.encode_wide().collect::<Vec<_>>();
    if wide.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "process argument contains a null character",
        ));
    }
    let quote = wide.is_empty()
        || wide
            .iter()
            .any(|value| matches!(*value, 0x20 | 0x09 | 0x22));
    if !quote {
        output.extend_from_slice(&wide);
        return Ok(());
    }

    output.push(0x22);
    let mut backslashes = 0_usize;
    for value in wide {
        if value == 0x5c {
            backslashes += 1;
            continue;
        }
        if value == 0x22 {
            output.extend(std::iter::repeat_n(0x5c, backslashes * 2 + 1));
            output.push(value);
        } else {
            output.extend(std::iter::repeat_n(0x5c, backslashes));
            output.push(value);
        }
        backslashes = 0;
    }
    output.extend(std::iter::repeat_n(0x5c, backslashes * 2));
    output.push(0x22);
    Ok(())
}

fn windows_environment_block(overrides: &[(OsString, OsString)]) -> io::Result<Vec<u16>> {
    let mut values = BTreeMap::<String, (OsString, OsString)>::new();
    for (key, value) in std::env::vars_os().chain(overrides.iter().cloned()) {
        let canonical = key.to_string_lossy().to_uppercase();
        values.insert(canonical, (key, value));
    }

    let mut block = Vec::new();
    for (_, (key, value)) in values {
        let key = key.encode_wide().collect::<Vec<_>>();
        let value = value.encode_wide().collect::<Vec<_>>();
        if key.is_empty() || key.contains(&0) || value.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "process environment contains an invalid entry",
            ));
        }
        block.extend_from_slice(&key);
        block.push('=' as u16);
        block.extend_from_slice(&value);
        block.push(0);
    }
    block.push(0);
    Ok(block)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        process::{Command, Stdio},
        thread,
        time::{Duration, Instant},
    };
    use windows_sys::Win32::{
        Foundation::STILL_ACTIVE,
        System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };

    const OWNER_HELPER_ENV: &str = "VISION_DESKTOP_CORE_JOB_OWNER_HELPER";
    const OWNER_PID_FILE_ENV: &str = "VISION_DESKTOP_CORE_JOB_OWNER_PID_FILE";

    fn null_file() -> File {
        OpenOptions::new()
            .read(true)
            .write(true)
            .open("NUL")
            .unwrap()
    }

    fn ping_executable() -> std::path::PathBuf {
        std::path::PathBuf::from(std::env::var_os("WINDIR").unwrap())
            .join("System32")
            .join("ping.exe")
    }

    fn long_running_child() -> ContainedCoreProcess {
        ContainedCoreProcess::spawn(
            &ping_executable(),
            &[OsString::from("-t"), OsString::from("127.0.0.1")],
            &[],
            null_file(),
            null_file(),
        )
        .unwrap()
    }

    fn process_is_alive(pid: u32) -> bool {
        let raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if raw.is_null() {
            return false;
        }
        let handle = unsafe { OwnedHandle::from_raw_handle(raw as RawHandle) };
        let mut exit_code = 0_u32;
        let queried =
            unsafe { GetExitCodeProcess(handle.as_raw_handle() as HANDLE, &mut exit_code) != 0 };
        queried && exit_code == STILL_ACTIVE as u32
    }

    #[test]
    fn process_is_in_the_retained_job_when_atomic_creation_returns() {
        let child = long_running_child();
        assert!(child.is_in_retained_job().unwrap());
        assert_eq!(child.try_wait().unwrap(), None);
    }

    #[test]
    fn dropping_the_atomic_process_owner_terminates_its_child() {
        let child = long_running_child();
        let pid = child.id();
        assert!(process_is_alive(pid));
        drop(child);
        let deadline = Instant::now() + Duration::from_secs(5);
        while process_is_alive(pid) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        assert!(!process_is_alive(pid));
    }

    #[test]
    fn forced_owner_process_termination_closes_the_job_and_kills_its_child() {
        if std::env::var_os(OWNER_HELPER_ENV).is_some() {
            let pid_file = std::env::var_os(OWNER_PID_FILE_ENV).unwrap();
            let child = long_running_child();
            fs::write(pid_file, child.id().to_string()).unwrap();
            std::mem::forget(child);
            loop {
                thread::sleep(Duration::from_secs(1));
            }
        }

        let directory = tempfile::tempdir().unwrap();
        let pid_file = directory.path().join("assigned-child.pid");
        let mut owner = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "core_job::tests::forced_owner_process_termination_closes_the_job_and_kills_its_child",
                "--nocapture",
            ])
            .env(OWNER_HELPER_ENV, "1")
            .env(OWNER_PID_FILE_ENV, &pid_file)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();

        let deadline = Instant::now() + Duration::from_secs(5);
        while !pid_file.is_file() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        let assigned_pid = fs::read_to_string(&pid_file)
            .unwrap()
            .parse::<u32>()
            .unwrap();
        assert!(process_is_alive(assigned_pid));
        owner.kill().unwrap();
        owner.wait().unwrap();

        let deadline = Instant::now() + Duration::from_secs(5);
        while process_is_alive(assigned_pid) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        assert!(!process_is_alive(assigned_pid));
    }

    #[test]
    fn command_line_quoting_preserves_spaces_quotes_and_trailing_backslashes() {
        let line = windows_command_line(
            OsStr::new(r"C:\Program Files\Vision\vision-core.exe"),
            &[
                OsString::from("plain"),
                OsString::from("two words"),
                OsString::from("quoted\"value"),
                OsString::from(r"ends with slash\"),
            ],
        )
        .unwrap();
        let rendered = String::from_utf16(&line[..line.len() - 1]).unwrap();
        assert_eq!(
            rendered,
            r#""C:\Program Files\Vision\vision-core.exe" plain "two words" "quoted\"value" "ends with slash\\""#
        );
    }
}
