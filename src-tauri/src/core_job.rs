#![cfg(windows)]

use std::{
    io,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle},
};
use windows_sys::Win32::{
    Foundation::HANDLE,
    System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    },
};

pub(crate) struct CoreProcessJob {
    handle: OwnedHandle,
}

impl CoreProcessJob {
    pub(crate) fn new_kill_on_close() -> io::Result<Self> {
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

    pub(crate) fn assign_process(&self, process: RawHandle) -> io::Result<()> {
        let succeeded = unsafe {
            AssignProcessToJobObject(self.handle.as_raw_handle() as HANDLE, process as HANDLE)
        };
        if succeeded == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    pub(crate) fn terminate(&self) -> io::Result<()> {
        let succeeded = unsafe { TerminateJobObject(self.handle.as_raw_handle() as HANDLE, 1) };
        if succeeded == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
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

    fn long_running_child() -> std::process::Child {
        Command::new("cmd.exe")
            .args(["/d", "/s", "/c", "ping -t 127.0.0.1 > nul"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    }

    fn wait_for_exit(child: &mut std::process::Child) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if child.try_wait().unwrap().is_some() {
                return true;
            }
            thread::sleep(Duration::from_millis(20));
        }
        false
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
    fn kill_on_close_job_terminates_its_assigned_process() {
        let job = CoreProcessJob::new_kill_on_close().unwrap();
        let mut child = long_running_child();
        job.assign_process(child.as_raw_handle()).unwrap();
        drop(job);
        assert!(wait_for_exit(&mut child));
    }

    #[test]
    fn forced_owner_process_termination_closes_the_job_and_kills_its_child() {
        if std::env::var_os(OWNER_HELPER_ENV).is_some() {
            let pid_file = std::env::var_os(OWNER_PID_FILE_ENV).unwrap();
            let job = CoreProcessJob::new_kill_on_close().unwrap();
            let child = long_running_child();
            job.assign_process(child.as_raw_handle()).unwrap();
            fs::write(pid_file, child.id().to_string()).unwrap();
            std::mem::forget(child);
            std::mem::forget(job);
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
}
