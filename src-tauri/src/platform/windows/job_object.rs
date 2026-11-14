//! Windows Job Object wrapper — binds the engine child so it dies with us.

use std::io;
use std::os::windows::io::RawHandle;

use thiserror::Error;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_BASIC_LIMIT_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
};

#[derive(Debug, Error)]
pub enum JobObjectError {
    #[error("CreateJobObjectW failed: {0}")]
    CreateFailed(io::Error),
    #[error("SetInformationJobObject failed: {0}")]
    SetInfoFailed(io::Error),
}

pub struct JobObject {
    handle: HANDLE,
}

unsafe impl Send for JobObject {}
unsafe impl Sync for JobObject {}

impl JobObject {
    pub fn create() -> Result<Self, JobObjectError> {
        let handle = unsafe { CreateJobObjectW(None, None) }.map_err(|e| {
            JobObjectError::CreateFailed(io::Error::other(e.to_string()))
        })?;
        if handle.is_invalid() {
            return Err(JobObjectError::CreateFailed(io::Error::last_os_error()));
        }
        let info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
            BasicLimitInformation: JOBOBJECT_BASIC_LIMIT_INFORMATION {
                LimitFlags: JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                ..Default::default()
            },
            ..Default::default()
        };
        if unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        }
        .is_err()
        {
            unsafe { let _ = CloseHandle(handle); };
            return Err(JobObjectError::SetInfoFailed(io::Error::last_os_error()));
        }
        Ok(JobObject { handle })
    }

    pub fn assign_raw_handle(&self, raw: RawHandle) -> Result<(), JobObjectError> {
        unsafe { AssignProcessToJobObject(self.handle, HANDLE(raw)) }.map_err(|e| {
            JobObjectError::SetInfoFailed(io::Error::other(e.to_string()))
        })
    }

    /// Best-effort termination of every process in the job.
    pub fn terminate(&self, code: u32) {
        unsafe {
            let _ = TerminateJobObject(self.handle, code);
        }
    }
}

impl Drop for JobObject {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}
