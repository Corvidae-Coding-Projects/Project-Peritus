//! Uniquely owned kernel handles, job accounting, and pseudoconsole lifetime.
use super::error;
use crate::{ProcessError, TerminalSize};
use std::{
    fs::File,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    ptr,
};
use windows_sys::Win32::{
    Foundation::HANDLE,
    System::{
        Console::{COORD, ClosePseudoConsole, CreatePseudoConsole, HPCON, ResizePseudoConsole},
        JobObjects::{
            CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JobObjectBasicAccountingInformation, JobObjectExtendedLimitInformation,
            QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
        },
        Pipes::CreatePipe,
    },
};

pub(super) struct Handle(OwnedHandle);
impl Handle {
    pub(super) unsafe fn from_raw(raw: HANDLE) -> Self {
        // SAFETY: callers transfer one valid, uniquely owned kernel handle.
        Self(unsafe { OwnedHandle::from_raw_handle(raw.cast()) })
    }
    pub(super) fn raw(&self) -> HANDLE {
        self.0.as_raw_handle().cast()
    }
}

pub(super) struct Job(Handle);
impl Job {
    pub(super) fn create() -> Result<Self, ProcessError> {
        // SAFETY: null security and name request a private non-inheritable Job Object.
        let raw = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
        if raw.is_null() {
            return Err(error("ConPTY job creation failed"));
        }
        // SAFETY: successful CreateJobObjectW transfers a single owned handle.
        let job = Self(unsafe { Handle::from_raw(raw) });
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: the exact initialized record and live job are valid for this operation.
        if unsafe {
            SetInformationJobObject(
                job.raw(),
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast(),
                u32::try_from(size_of_val(&limits)).expect("Windows structure size"),
            )
        } == 0
        {
            return Err(error("ConPTY kill-on-close job configuration failed"));
        }
        Ok(job)
    }
    pub(super) fn raw(&self) -> HANDLE {
        self.0.raw()
    }
    pub(super) fn active(&self) -> Result<u64, ProcessError> {
        let mut info = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
        // SAFETY: the record is writable at its exact size and the job handle stays live.
        if unsafe {
            QueryInformationJobObject(
                self.raw(),
                JobObjectBasicAccountingInformation,
                (&raw mut info).cast(),
                u32::try_from(size_of_val(&info)).expect("Windows structure size"),
                ptr::null_mut(),
            )
        } == 0
        {
            return Err(error("ConPTY job accounting failed"));
        }
        Ok(u64::from(info.ActiveProcesses))
    }
    pub(super) fn terminate(&self) -> Result<(), ProcessError> {
        // SAFETY: only processes assigned to this uniquely owned private job are affected.
        if unsafe { TerminateJobObject(self.raw(), 1) } == 0 {
            return Err(error("ConPTY job termination failed"));
        }
        Ok(())
    }
}

pub(super) struct Console(pub(super) HPCON);
impl Console {
    pub(super) fn create(size: TerminalSize) -> Result<(Self, File, File), ProcessError> {
        let (input, input_writer) = pipe()?;
        let (output_reader, output) = pipe()?;
        let mut console = 0;
        let coordinate = coordinate(size)?;
        // SAFETY: private pipe handles and output HPCON slot remain live during creation.
        if unsafe {
            CreatePseudoConsole(
                coordinate,
                input.as_raw_handle().cast(),
                output.as_raw_handle().cast(),
                0,
                &raw mut console,
            )
        } < 0
        {
            return Err(error("native ConPTY creation failed"));
        }
        Ok((Self(console), input_writer, output_reader))
    }
    pub(super) fn resize(&self, size: TerminalSize) -> Result<(), ProcessError> {
        let coordinate = coordinate(size)?;
        // SAFETY: self retains the live pseudoconsole, and no close runs concurrently.
        if unsafe { ResizePseudoConsole(self.0, coordinate) } < 0 {
            return Err(error("native ConPTY resize failed"));
        }
        Ok(())
    }
}
impl Drop for Console {
    fn drop(&mut self) {
        // SAFETY: releases exactly the live HPCON uniquely owned by this value.
        unsafe { ClosePseudoConsole(self.0) };
    }
}
fn coordinate(size: TerminalSize) -> Result<COORD, ProcessError> {
    Ok(COORD {
        X: i16::try_from(size.columns())
            .map_err(|_| error("ConPTY columns exceed native coordinate capacity"))?,
        Y: i16::try_from(size.rows())
            .map_err(|_| error("ConPTY rows exceed native coordinate capacity"))?,
    })
}
fn pipe() -> Result<(File, File), ProcessError> {
    let mut reader = ptr::null_mut();
    let mut writer = ptr::null_mut();
    // SAFETY: writable output slots and null attributes create two private non-inheritable handles.
    if unsafe { CreatePipe(&raw mut reader, &raw mut writer, ptr::null(), 0) } == 0 {
        return Err(error("ConPTY transport pipe creation failed"));
    }
    // SAFETY: successful CreatePipe transfers each distinct owned handle exactly once.
    Ok(unsafe { (File::from_raw_handle(reader.cast()), File::from_raw_handle(writer.cast())) })
}
