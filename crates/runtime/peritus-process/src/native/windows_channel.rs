//! Protected Windows helper status and terminal-control channels.

#![allow(
    unsafe_code,
    reason = "CreatePipe and inherited HANDLE ownership are the narrow Windows channel boundary"
)]

use std::{
    fs::File,
    io::{Read, Write},
    os::windows::io::{AsRawHandle, FromRawHandle, RawHandle},
    sync::{Arc, Mutex},
};

use windows_sys::Win32::{
    Foundation::{
        ERROR_INVALID_PARAMETER, ERROR_MORE_DATA, GetHandleInformation, GetLastError, HANDLE,
        WAIT_OBJECT_0, WAIT_TIMEOUT,
    },
    System::{
        JobObjects::{
            IsProcessInJob, JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
            JOBOBJECT_BASIC_PROCESS_ID_LIST, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JobObjectBasicAccountingInformation, JobObjectBasicProcessIdList,
            JobObjectExtendedLimitInformation, QueryInformationJobObject,
        },
        Pipes::{CreatePipe, PIPE_NOWAIT, PeekNamedPipe, SetNamedPipeHandleState},
        Threading::{
            GetProcessHandleCount, GetProcessTimes, OpenProcess,
            PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, WaitForSingleObject,
        },
    },
};

use crate::{
    ErrorCode, NativePostActivationFailure, NativeProtectedHandle,
    NativeWindowsContainmentIdentity, ProcessError, ProcessOperation, ProcessTreeIdentity,
    RecoveryClass, TerminalSize,
};
use peritus_types::Sha256Digest;

/// Reserved child environment key carrying the digest-bound started-status writer.
pub const NATIVE_WINDOWS_STATUS_HANDLE_ENV: &str = "PERITUS_NATIVE_WINDOWS_STATUS_V1";
/// Reserved child environment key carrying the terminal resize-control reader.
pub const NATIVE_WINDOWS_CONTROL_HANDLE_ENV: &str = "PERITUS_NATIVE_WINDOWS_CONTROL_V1";
/// Reserved child environment key carrying the independently retained Job Object handle.
pub const NATIVE_WINDOWS_JOB_HANDLE_ENV: &str = "PERITUS_NATIVE_WINDOWS_JOB_V1";
/// Exact protected-handle role used for the independently retained Job Object.
pub const NATIVE_WINDOWS_JOB_HANDLE_LABEL: &str = "windows-containment-job-v1";

const TARGET_ADOPTION_BYTES: usize = Sha256Digest::LENGTH + 4 + 8;
const TARGET_ADOPTION_ACK: u8 = 5;
const SECRET_FILES_ACK: u8 = 6;
const SECRET_FILE_HEADER_BYTES: usize = Sha256Digest::LENGTH + 4;
const SECRET_FILE_ENTRY_BYTES: usize = Sha256Digest::LENGTH + 8 + 8 + 16;
const WINDOWS_COMPLETION_VERSION: u8 = 2;
const WINDOWS_COMPLETION_TAIL_BYTES: usize = 7;
const WINDOWS_COMPLETION_HAS_STATUS: u8 = 1;
const WINDOWS_COMPLETION_HAS_FAILURE: u8 = 2;

/// Authenticated Windows target completion, independent of the helper's process exit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeWindowsCompletion {
    target_status: Option<u32>,
    failure: Option<NativePostActivationFailure>,
    legacy: bool,
}

impl NativeWindowsCompletion {
    /// Creates an exact target-only completion.
    #[must_use]
    pub const fn target(target_status: u32) -> Self {
        Self { target_status: Some(target_status), failure: None, legacy: false }
    }

    /// Creates an exact failure observed before a target status became available.
    #[must_use]
    pub const fn failure(failure: NativePostActivationFailure) -> Self {
        Self { target_status: None, failure: Some(failure), legacy: false }
    }

    /// Creates an exact target status paired with an independent later failure.
    #[must_use]
    pub const fn target_with_failure(
        target_status: u32,
        failure: NativePostActivationFailure,
    ) -> Self {
        Self { target_status: Some(target_status), failure: Some(failure), legacy: false }
    }

    const fn legacy_quiesced() -> Self {
        Self { target_status: None, failure: None, legacy: true }
    }

    const fn legacy_failed() -> Self {
        Self {
            target_status: None,
            failure: Some(NativePostActivationFailure::LegacyUnspecified),
            legacy: true,
        }
    }

    /// Returns the exact native target status when the version-two producer observed it.
    #[must_use]
    pub const fn target_status(self) -> Option<u32> {
        self.target_status
    }

    /// Returns the structured post-activation failure when one was reported.
    #[must_use]
    pub const fn post_activation_failure(self) -> Option<NativePostActivationFailure> {
        self.failure
    }

    pub(crate) const fn is_legacy(self) -> bool {
        self.legacy
    }
}

/// Exact live quiescence facts read through the retained target and Job Object handles.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeWindowsQuiescence {
    target_process_quiescent: bool,
    job_quiescent: bool,
}

impl NativeWindowsQuiescence {
    /// Reports whether no adopted target process remains live.
    #[must_use]
    pub const fn target_process_quiescent(self) -> bool {
        self.target_process_quiescent
    }

    /// Reports whether the retained Job Object has no active process.
    #[must_use]
    pub const fn job_quiescent(self) -> bool {
        self.job_quiescent
    }
}

/// One exact resource sample read from the retained Windows Job Object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeWindowsResourceSnapshot {
    cpu_time_millis: u64,
    peak_memory_bytes: u64,
    active_processes: u64,
    open_handles: u64,
}

impl NativeWindowsResourceSnapshot {
    /// Returns cumulative Job user-plus-kernel time in milliseconds.
    #[must_use]
    pub const fn cpu_time_millis(self) -> u64 {
        self.cpu_time_millis
    }

    /// Returns peak committed memory charged to the Job.
    #[must_use]
    pub const fn peak_memory_bytes(self) -> u64 {
        self.peak_memory_bytes
    }

    /// Returns the exact active process count reported by the Job.
    #[must_use]
    pub const fn active_processes(self) -> u64 {
        self.active_processes
    }

    /// Returns the sum of open handles across the current Job process list.
    #[must_use]
    pub const fn open_handles(self) -> u64 {
        self.open_handles
    }
}

/// Manifest-bound nonsensitive identity expected for one private secret file.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativeWindowsSecretFileBinding {
    path_digest: Sha256Digest,
    payload_len: u64,
}

impl NativeWindowsSecretFileBinding {
    /// Creates one exact path and finite-length binding.
    ///
    /// # Errors
    /// Rejects an empty payload or zero path identity.
    pub fn new(path_digest: Sha256Digest, payload_len: u64) -> Result<Self, ProcessError> {
        if path_digest == Sha256Digest::new([0; 32]) || payload_len == 0 {
            return Err(channel_error("Windows secret-file binding is incomplete"));
        }
        Ok(Self { path_digest, payload_len })
    }

    /// Returns the canonical native-path digest.
    #[must_use]
    pub const fn path_digest(self) -> Sha256Digest {
        self.path_digest
    }

    /// Returns the exact delivered byte length.
    #[must_use]
    pub const fn payload_len(self) -> u64 {
        self.payload_len
    }
}

/// Exact native identity of one helper-created private secret file.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativeWindowsSecretFileIdentity {
    binding: NativeWindowsSecretFileBinding,
    volume_serial: u64,
    file_id: [u8; 16],
}

impl NativeWindowsSecretFileIdentity {
    /// Creates one exact native file identity.
    ///
    /// # Errors
    /// Rejects zero volume or file identity.
    pub fn new(
        binding: NativeWindowsSecretFileBinding,
        volume_serial: u64,
        file_id: [u8; 16],
    ) -> Result<Self, ProcessError> {
        if volume_serial == 0 || file_id == [0; 16] {
            return Err(channel_error("Windows secret-file native identity is incomplete"));
        }
        Ok(Self { binding, volume_serial, file_id })
    }

    /// Returns the manifest path and payload binding.
    #[must_use]
    pub const fn binding(self) -> NativeWindowsSecretFileBinding {
        self.binding
    }

    /// Returns the native volume serial number.
    #[must_use]
    pub const fn volume_serial(self) -> u64 {
        self.volume_serial
    }

    /// Returns the 128-bit native file identifier.
    #[must_use]
    pub const fn file_id(self) -> [u8; 16] {
        self.file_id
    }
}

/// C2-owned parent endpoints and their exact protected helper endpoints.
#[derive(Clone, Debug)]
pub struct NativeWindowsHelperChannels {
    status_reader: Arc<File>,
    control_writer: Arc<Mutex<File>>,
    child_handles: Vec<NativeProtectedHandle>,
    status_handle: u64,
    control_handle: u64,
    containment_job_handle: Option<u64>,
    containment_job_identity: Option<Sha256Digest>,
    containment_job_name: Option<String>,
    adoption: Arc<Mutex<Option<ContainmentAdoption>>>,
    expected_secret_files: Arc<Vec<NativeWindowsSecretFileBinding>>,
    secret_files: Arc<Mutex<Option<Vec<NativeWindowsSecretFileIdentity>>>>,
}

#[derive(Debug)]
struct ContainmentAdoption {
    identity: NativeWindowsContainmentIdentity,
    target: File,
}

impl NativeWindowsHelperChannels {
    /// Creates one status pipe and one resize-control pipe.
    ///
    /// # Errors
    /// Returns a typed spawn failure if either anonymous pipe cannot be created.
    pub fn new() -> Result<Self, ProcessError> {
        let (status_reader, status_writer) = pipe()?;
        let (control_reader, control_writer) = pipe()?;
        set_nonblocking(&status_reader)?;
        set_nonblocking(&control_writer)?;
        let status = NativeProtectedHandle::from_file("windows-helper-status-v1", status_writer)?;
        let control =
            NativeProtectedHandle::from_file("windows-terminal-control-v1", control_reader)?;
        let status_handle = status.raw_handle();
        let control_handle = control.raw_handle();
        Ok(Self {
            status_reader: Arc::new(status_reader),
            control_writer: Arc::new(Mutex::new(control_writer)),
            child_handles: vec![status, control],
            status_handle,
            control_handle,
            containment_job_handle: None,
            containment_job_identity: None,
            containment_job_name: None,
            adoption: Arc::new(Mutex::new(None)),
            expected_secret_files: Arc::new(Vec::new()),
            secret_files: Arc::new(Mutex::new(None)),
        })
    }

    /// Creates protected helper channels with one parent-owned authority-bound Job Object.
    ///
    /// # Errors
    /// Rejects a staged object with the wrong role or a malformed native object name.
    pub fn new_with_containment(
        containment_job: NativeProtectedHandle,
        job_identity: Sha256Digest,
        object_name: String,
    ) -> Result<Self, ProcessError> {
        if containment_job.label() != NATIVE_WINDOWS_JOB_HANDLE_LABEL
            || containment_job.payload_len().is_some()
        {
            return Err(channel_error("Windows containment Job Object role is invalid"));
        }
        let provisional = ProcessTreeIdentity::new(1, Some(1), None, true);
        NativeWindowsContainmentIdentity::new(job_identity, object_name.clone(), provisional)?;
        let job_handle = containment_job.raw_handle();
        let mut channels = Self::new()?;
        channels.child_handles.push(containment_job);
        channels.containment_job_handle = Some(job_handle);
        channels.containment_job_identity = Some(job_identity);
        channels.containment_job_name = Some(object_name);
        Ok(channels)
    }

    pub(crate) fn take_child_handles(&mut self) -> Vec<NativeProtectedHandle> {
        core::mem::take(&mut self.child_handles)
    }

    pub(crate) const fn status_handle(&self) -> u64 {
        self.status_handle
    }

    pub(crate) const fn control_handle(&self) -> u64 {
        self.control_handle
    }

    pub(crate) const fn containment_job_handle(&self) -> Option<u64> {
        self.containment_job_handle
    }

    pub(crate) const fn containment_job_identity(&self) -> Option<Sha256Digest> {
        self.containment_job_identity
    }

    pub(crate) fn status_reader(&self) -> Result<File, ProcessError> {
        self.status_reader
            .try_clone()
            .map_err(|_| channel_error("Windows helper status reader cannot be cloned"))
    }

    /// Binds the exact private-file set expected before target creation.
    ///
    /// # Errors
    /// Rejects duplicate native paths.
    pub fn with_secret_file_bindings(
        mut self,
        mut bindings: Vec<NativeWindowsSecretFileBinding>,
    ) -> Result<Self, ProcessError> {
        bindings.sort_by(|left, right| {
            left.path_digest.as_bytes().cmp(right.path_digest.as_bytes())
        });
        if bindings
            .windows(2)
            .any(|pair| pair[0].path_digest == pair[1].path_digest)
        {
            return Err(channel_error("Windows secret-file bindings contain a duplicate path"));
        }
        self.expected_secret_files = Arc::new(bindings);
        Ok(self)
    }

    pub(crate) fn verify_secret_files(
        &self,
        mut reader: Box<dyn Read + Send>,
        expected_record: Sha256Digest,
        should_continue: &mut dyn FnMut() -> bool,
    ) -> Result<Box<dyn Read + Send>, crate::platform::HandshakeError> {
        let mut header = [0_u8; SECRET_FILE_HEADER_BYTES];
        read_exact_while(&mut *reader, &mut header, should_continue)?;
        if &header[..Sha256Digest::LENGTH] != expected_record.as_bytes() {
            return Err(crate::platform::HandshakeError::Failed(channel_error(
                "Windows secret-file custody record mismatched",
            )));
        }
        let count = usize::try_from(u32::from_le_bytes(
            header[Sha256Digest::LENGTH..]
                .try_into()
                .map_err(|_| crate::platform::HandshakeError::Failed(channel_error(
                    "Windows secret-file custody count is malformed",
                )))?,
        ))
        .map_err(|_| crate::platform::HandshakeError::Failed(channel_error(
            "Windows secret-file custody count is not representable",
        )))?;
        if count != self.expected_secret_files.len() {
            return Err(crate::platform::HandshakeError::Failed(channel_error(
                "Windows secret-file custody count differs from the manifest",
            )));
        }
        let mut identities = Vec::new();
        identities.try_reserve_exact(count).map_err(|_| {
            crate::platform::HandshakeError::Failed(channel_error(
                "Windows secret-file custody allocation is unavailable",
            ))
        })?;
        for expected in self.expected_secret_files.iter().copied() {
            let mut entry = [0_u8; SECRET_FILE_ENTRY_BYTES];
            read_exact_while(&mut *reader, &mut entry, should_continue)?;
            let path_digest = Sha256Digest::new(
                entry[..Sha256Digest::LENGTH]
                    .try_into()
                    .expect("fixed secret-file path digest"),
            );
            let payload_len = u64::from_le_bytes(
                entry[Sha256Digest::LENGTH..Sha256Digest::LENGTH + 8]
                    .try_into()
                    .expect("fixed secret-file payload length"),
            );
            let volume_at = Sha256Digest::LENGTH + 8;
            let volume_serial = u64::from_le_bytes(
                entry[volume_at..volume_at + 8]
                    .try_into()
                    .expect("fixed secret-file volume identity"),
            );
            let file_id = entry[volume_at + 8..]
                .try_into()
                .expect("fixed secret-file identifier");
            let binding = NativeWindowsSecretFileBinding::new(path_digest, payload_len)
                .map_err(crate::platform::HandshakeError::Failed)?;
            if binding != expected {
                return Err(crate::platform::HandshakeError::Failed(channel_error(
                    "Windows secret-file custody identity differs from the manifest",
                )));
            }
            identities.push(
                NativeWindowsSecretFileIdentity::new(binding, volume_serial, file_id)
                    .map_err(crate::platform::HandshakeError::Failed)?,
            );
        }
        let mut retained = self.secret_files.lock().map_err(|_| {
            crate::platform::HandshakeError::Failed(channel_error(
                "Windows secret-file custody state was poisoned",
            ))
        })?;
        if retained.is_some() {
            return Err(crate::platform::HandshakeError::Failed(channel_error(
                "Windows secret-file custody was already published",
            )));
        }
        *retained = Some(identities);
        drop(retained);
        Ok(reader)
    }

    pub(crate) fn acknowledge_secret_files(&self) -> Result<(), ProcessError> {
        self.write_control(&[SECRET_FILES_ACK])
    }

    pub(crate) fn secret_file_identities(
        &self,
    ) -> Result<Option<Vec<NativeWindowsSecretFileIdentity>>, ProcessError> {
        self.secret_files
            .lock()
            .map(|files| files.clone())
            .map_err(|_| channel_error("Windows secret-file custody state was poisoned"))
    }

    pub(crate) fn verify_target_adoption(
        &self,
        mut reader: Box<dyn Read + Send>,
        expected: Sha256Digest,
        helper: ProcessTreeIdentity,
        should_continue: &mut dyn FnMut() -> bool,
    ) -> Result<Box<dyn Read + Send>, crate::platform::HandshakeError> {
        let mut frame = [0_u8; TARGET_ADOPTION_BYTES];
        read_exact_while(&mut *reader, &mut frame, should_continue)?;
        if &frame[..Sha256Digest::LENGTH] != expected.as_bytes() {
            return Err(crate::platform::HandshakeError::Failed(channel_error(
                "Windows target adoption record mismatched",
            )));
        }
        let pid = u32::from_le_bytes(
            frame[Sha256Digest::LENGTH..Sha256Digest::LENGTH + 4]
                .try_into()
                .map_err(|_| crate::platform::HandshakeError::Failed(channel_error(
                    "Windows target adoption process identity is malformed",
                )))?,
        );
        let start_token = u64::from_le_bytes(
            frame[Sha256Digest::LENGTH + 4..]
                .try_into()
                .map_err(|_| crate::platform::HandshakeError::Failed(channel_error(
                    "Windows target adoption birth identity is malformed",
                )))?,
        );
        if pid == 0 || pid == helper.root_pid() || start_token == 0 {
            return Err(crate::platform::HandshakeError::Failed(channel_error(
                "Windows target adoption identity aliases or omits its birth",
            )));
        }
        let job_handle = self.raw_containment_job().map_err(
            crate::platform::HandshakeError::Failed,
        )?;
        // SAFETY: access is query-only and handle inheritance is disabled for this C2 handle.
        let process = unsafe {
            OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE, 0, pid)
        };
        if process.is_null() {
            return Err(crate::platform::HandshakeError::Failed(channel_error(
                "Windows target cannot be opened for adoption",
            )));
        }
        // SAFETY: the non-null query handle transfers to File and is closed with the adoption.
        let target = unsafe { File::from_raw_handle(process.cast()) };
        if process_start_token(&target) != Some(start_token) {
            return Err(crate::platform::HandshakeError::Failed(channel_error(
                "Windows target PID was reused before adoption",
            )));
        }
        let mut member = 0;
        // SAFETY: both handles remain live and `member` is writable for the duration of the call.
        if unsafe {
            IsProcessInJob(
                target.as_raw_handle().cast(),
                job_handle,
                &raw mut member,
            )
        } == 0
            || member == 0
        {
            return Err(crate::platform::HandshakeError::Failed(channel_error(
                "Windows target is not contained by the retained Job Object",
            )));
        }
        let target_identity = ProcessTreeIdentity::new(pid, Some(start_token), None, true);
        let identity = NativeWindowsContainmentIdentity::new(
            self.containment_job_identity.ok_or_else(|| {
                crate::platform::HandshakeError::Failed(channel_error(
                    "Windows containment Job Object identity is missing",
                ))
            })?,
            self.containment_job_name.clone().ok_or_else(|| {
                crate::platform::HandshakeError::Failed(channel_error(
                    "Windows containment Job Object name is missing",
                ))
            })?,
            target_identity,
        )
        .map_err(crate::platform::HandshakeError::Failed)?;
        let mut adoption = self.adoption.lock().map_err(|_| {
            crate::platform::HandshakeError::Failed(channel_error(
                "Windows containment adoption state was poisoned",
            ))
        })?;
        if adoption.is_some() {
            return Err(crate::platform::HandshakeError::Failed(channel_error(
                "Windows target adoption was already published",
            )));
        }
        *adoption = Some(ContainmentAdoption { identity, target });
        drop(adoption);
        Ok(reader)
    }

    pub(crate) fn acknowledge_target_adoption(&self) -> Result<(), ProcessError> {
        self.write_control(&[TARGET_ADOPTION_ACK])
    }

    pub(crate) fn containment_identity(
        &self,
    ) -> Result<Option<NativeWindowsContainmentIdentity>, ProcessError> {
        let adoption = self
            .adoption
            .lock()
            .map_err(|_| channel_error("Windows containment adoption state was poisoned"))?;
        let Some(adoption) = adoption.as_ref() else {
            return Ok(None);
        };
        if process_start_token(&adoption.target) != adoption.identity.target_identity().start_token()
        {
            return Err(channel_error("Windows target birth identity changed after adoption"));
        }
        let mut member = 0;
        let job = self.raw_containment_job()?;
        // SAFETY: both retained handles remain live and `member` is writable for the call.
        if unsafe {
            IsProcessInJob(
                adoption.target.as_raw_handle().cast(),
                job,
                &raw mut member,
            )
        } == 0
            || member == 0
        {
            return Err(channel_error(
                "Windows target is no longer contained by the retained Job Object",
            ));
        }
        Ok(Some(adoption.identity.clone()))
    }

    pub(crate) fn owner_identity_digest(&self) -> Result<Sha256Digest, ProcessError> {
        let status_reader = self.status_reader.as_raw_handle().cast();
        let control = self
            .control_writer
            .lock()
            .map_err(|_| channel_error("Windows terminal control channel was poisoned"))?;
        let control_writer = control.as_raw_handle().cast();
        if !handle_is_live(status_reader) || !handle_is_live(control_writer) {
            return Err(channel_error("Windows retained helper channel is no longer live"));
        }
        let mut bytes = Vec::from(b"PERITUS-WINDOWS-HELPER-CHANNEL-OWNER-V1\0".as_slice());
        bytes.extend_from_slice(&(status_reader as usize as u64).to_be_bytes());
        bytes.extend_from_slice(&(control_writer as usize as u64).to_be_bytes());
        bytes.extend_from_slice(&self.status_handle.to_be_bytes());
        bytes.extend_from_slice(&self.control_handle.to_be_bytes());
        bytes.extend_from_slice(
            &u64::try_from(self.expected_secret_files.len())
                .unwrap_or(u64::MAX)
                .to_be_bytes(),
        );
        for binding in self.expected_secret_files.iter().copied() {
            bytes.extend_from_slice(binding.path_digest().as_bytes());
            bytes.extend_from_slice(&binding.payload_len().to_be_bytes());
        }
        Ok(peritus_codec::sha256(&bytes))
    }

    pub(crate) fn containment_job_owner_digest(&self) -> Option<Sha256Digest> {
        let raw = self.raw_containment_job().ok()?;
        let identity = self.containment_job_identity?;
        let name = self.containment_job_name.as_ref()?;
        let mut bytes = Vec::from(b"PERITUS-WINDOWS-JOB-OWNER-V1\0".as_slice());
        bytes.extend_from_slice(&(raw as usize as u64).to_be_bytes());
        bytes.extend_from_slice(identity.as_bytes());
        bytes.extend_from_slice(name.as_bytes());
        Some(peritus_codec::sha256(&bytes))
    }

    #[must_use]
    pub(crate) fn retains_containment_job(&self, expected: Sha256Digest) -> bool {
        self.containment_job_identity == Some(expected) && self.raw_containment_job().is_ok()
    }

    pub(crate) fn quiescence(&self) -> Result<NativeWindowsQuiescence, ProcessError> {
        let target_process_quiescent = {
            let adoption = self
                .adoption
                .lock()
                .map_err(|_| channel_error("Windows containment adoption state was poisoned"))?;
            match adoption.as_ref() {
                None => true,
                Some(adoption) => {
                    // SAFETY: adoption retains a live SYNCHRONIZE-capable process handle.
                    match unsafe { WaitForSingleObject(adoption.target.as_raw_handle().cast(), 0) } {
                        WAIT_OBJECT_0 => true,
                        WAIT_TIMEOUT => false,
                        _ => {
                            return Err(channel_error(
                                "Windows target quiescence cannot be observed",
                            ));
                        }
                    }
                }
            }
        };
        let accounting = query_job_accounting(self.raw_containment_job()?)?;
        Ok(NativeWindowsQuiescence {
            target_process_quiescent,
            job_quiescent: accounting.ActiveProcesses == 0,
        })
    }

    pub(crate) fn resource_snapshot(
        &self,
    ) -> Result<NativeWindowsResourceSnapshot, ProcessError> {
        let job = self.raw_containment_job()?;
        let accounting = query_job_accounting(job)?;
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        let limits_size = u32::try_from(core::mem::size_of_val(&limits))
            .map_err(|_| channel_error("Windows Job resource record size overflowed"))?;
        let mut returned = 0_u32;
        // SAFETY: the retained Job handle is live and the complete output record is writable.
        if unsafe {
            QueryInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&raw mut limits).cast(),
                limits_size,
                &raw mut returned,
            )
        } == 0
            || returned != limits_size
        {
            return Err(channel_error("Windows Job memory accounting cannot be observed"));
        }
        let cpu_ticks = accounting
            .TotalUserTime
            .checked_add(accounting.TotalKernelTime)
            .and_then(|value| u64::try_from(value).ok())
            .ok_or_else(|| channel_error("Windows Job CPU accounting is invalid"))?;
        let process_ids = query_job_process_ids(
            job,
            usize::try_from(accounting.ActiveProcesses)
                .map_err(|_| channel_error("Windows Job process count is not representable"))?,
        )?;
        let open_handles = process_ids.into_iter().try_fold(0_u64, |total, process_id| {
            let process_id = u32::try_from(process_id)
                .map_err(|_| channel_error("Windows Job process identity is not representable"))?;
            // SAFETY: access is query-only and inheritance is disabled for this temporary handle.
            let process = unsafe {
                OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                    0,
                    process_id,
                )
            };
            if process.is_null() {
                // SAFETY: this reads the immediately preceding OpenProcess result on this thread.
                if unsafe { GetLastError() } == ERROR_INVALID_PARAMETER {
                    return Ok(total);
                }
                return Err(channel_error(
                    "Windows Job process handle count changed during observation",
                ));
            }
            // SAFETY: the non-null query handle transfers to File and is closed after this sample.
            let process = unsafe { File::from_raw_handle(process.cast()) };
            let mut handles = 0_u32;
            // SAFETY: the process handle remains live and the output count is writable.
            if unsafe { GetProcessHandleCount(process.as_raw_handle().cast(), &raw mut handles) }
                == 0
            {
                // SAFETY: the retained temporary handle includes synchronization access.
                if unsafe { WaitForSingleObject(process.as_raw_handle().cast(), 0) }
                    == WAIT_OBJECT_0
                {
                    return Ok(total);
                }
                return Err(channel_error("Windows process handle count cannot be observed"));
            }
            Ok(total.saturating_add(u64::from(handles)))
        })?;
        Ok(NativeWindowsResourceSnapshot {
            cpu_time_millis: cpu_ticks / 10_000,
            peak_memory_bytes: u64::try_from(limits.PeakJobMemoryUsed)
                .map_err(|_| channel_error("Windows Job memory accounting is not representable"))?,
            active_processes: u64::from(accounting.ActiveProcesses),
            open_handles,
        })
    }

    pub(crate) fn resize(&self, size: TerminalSize) -> Result<(), ProcessError> {
        if i16::try_from(size.columns()).is_err() || i16::try_from(size.rows()).is_err() {
            return Err(terminal_control_error(
                "Windows terminal resize exceeds native signed coordinates",
            ));
        }
        let mut frame = [0_u8; 5];
        frame[0] = 1;
        frame[1..3].copy_from_slice(&size.columns().to_le_bytes());
        frame[3..5].copy_from_slice(&size.rows().to_le_bytes());
        self.write_control(&frame)
    }

    pub(crate) fn graceful(&self, action: crate::GracefulAction) -> Result<(), ProcessError> {
        let tag = match action {
            crate::GracefulAction::Interrupt => 2,
            crate::GracefulAction::Terminate => 3,
            crate::GracefulAction::CloseInput => 4,
        };
        self.write_control(&[tag])
    }

    fn write_control(&self, frame: &[u8]) -> Result<(), ProcessError> {
        let mut writer = self
            .control_writer
            .lock()
            .map_err(|_| channel_error("Windows terminal control channel was poisoned"))?;
        match writer.write(frame) {
            Ok(written) if written == frame.len() => Ok(()),
            Ok(_) | Err(_) => {
                Err(channel_error("Windows terminal control frame cannot be delivered"))
            }
        }
    }

    pub(crate) fn verify_helper_completion(
        &self,
        completion: Sha256Digest,
        quiesced: Sha256Digest,
        worker_failed: Sha256Digest,
    ) -> Result<NativeWindowsCompletion, ProcessError> {
        let mut record = [0_u8; Sha256Digest::LENGTH];
        let mut reader = &*self.status_reader;
        reader
            .read_exact(&mut record)
            .map_err(|_| worker_error("Windows helper completion record is missing"))?;
        if record == completion.into_bytes() {
            let mut tail = [0_u8; WINDOWS_COMPLETION_TAIL_BYTES];
            reader
                .read_exact(&mut tail)
                .map_err(|_| worker_error("Windows helper completion frame is truncated"))?;
            if tail[0] != WINDOWS_COMPLETION_VERSION
                || tail[1] & !(WINDOWS_COMPLETION_HAS_STATUS | WINDOWS_COMPLETION_HAS_FAILURE) != 0
            {
                return Err(worker_error("Windows helper completion frame is malformed"));
            }
            let has_status = tail[1] & WINDOWS_COMPLETION_HAS_STATUS != 0;
            let has_failure = tail[1] & WINDOWS_COMPLETION_HAS_FAILURE != 0;
            let raw_status = u32::from_le_bytes(
                tail[2..6]
                    .try_into()
                    .expect("fixed Windows target status field"),
            );
            let target_status = has_status.then_some(raw_status);
            let failure = if has_failure {
                crate::terminal::decode_native_failure(tail[6]).filter(|failure| {
                    *failure != NativePostActivationFailure::LegacyUnspecified
                })
            } else {
                None
            };
            if (!has_status && raw_status != 0)
                || has_failure != failure.is_some()
                || (!has_status && !has_failure)
            {
                return Err(worker_error("Windows helper completion frame is noncanonical"));
            }
            return Ok(NativeWindowsCompletion {
                target_status,
                failure,
                legacy: false,
            });
        }
        if record == quiesced.into_bytes() {
            return Ok(NativeWindowsCompletion::legacy_quiesced());
        }
        if record == worker_failed.into_bytes() {
            return Ok(NativeWindowsCompletion::legacy_failed());
        }
        Err(worker_error("Windows helper completion record is malformed"))
    }

    fn raw_containment_job(&self) -> Result<HANDLE, ProcessError> {
        let raw = self
            .containment_job_handle
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| *value != 0 && *value != usize::MAX)
            .ok_or_else(|| channel_error("Windows containment Job Object handle is missing"))?
            as HANDLE;
        let mut flags = 0;
        // SAFETY: the raw value remains owned by the protected launch handle while queried.
        if unsafe { GetHandleInformation(raw, &raw mut flags) } == 0 {
            return Err(channel_error("Windows containment Job Object handle is no longer live"));
        }
        Ok(raw)
    }
}

fn query_job_accounting(
    job: HANDLE,
) -> Result<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, ProcessError> {
    let mut accounting = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
    let size = u32::try_from(core::mem::size_of_val(&accounting))
        .map_err(|_| channel_error("Windows Job accounting record size overflowed"))?;
    let mut returned = 0_u32;
    // SAFETY: the retained Job handle is live and the complete output record is writable.
    if unsafe {
        QueryInformationJobObject(
            job,
            JobObjectBasicAccountingInformation,
            (&raw mut accounting).cast(),
            size,
            &raw mut returned,
        )
    } == 0
        || returned != size
    {
        return Err(channel_error("Windows Job accounting cannot be observed"));
    }
    Ok(accounting)
}

fn query_job_process_ids(job: HANDLE, initial_capacity: usize) -> Result<Vec<usize>, ProcessError> {
    let mut capacity = initial_capacity.max(1);
    loop {
        let bytes = core::mem::offset_of!(JOBOBJECT_BASIC_PROCESS_ID_LIST, ProcessIdList)
            .checked_add(
                capacity
                    .checked_mul(core::mem::size_of::<usize>())
                    .ok_or_else(|| channel_error("Windows Job process-list size overflowed"))?,
            )
            .ok_or_else(|| channel_error("Windows Job process-list size overflowed"))?;
        let words = bytes
            .checked_add(core::mem::size_of::<usize>() - 1)
            .map(|value| value / core::mem::size_of::<usize>())
            .ok_or_else(|| channel_error("Windows Job process-list size overflowed"))?;
        let mut storage = Vec::new();
        storage
            .try_reserve_exact(words)
            .map_err(|_| channel_error("Windows Job process-list allocation is unavailable"))?;
        storage.resize(words, 0_usize);
        let length = u32::try_from(words.saturating_mul(core::mem::size_of::<usize>()))
            .map_err(|_| channel_error("Windows Job process-list exceeds native capacity"))?;
        let list = storage.as_mut_ptr().cast::<JOBOBJECT_BASIC_PROCESS_ID_LIST>();
        let mut returned = 0_u32;
        // SAFETY: usize storage is suitably aligned, the byte length covers the header and the
        // requested flexible-array capacity, and the Job handle remains live.
        let observed = unsafe {
            QueryInformationJobObject(
                job,
                JobObjectBasicProcessIdList,
                list.cast(),
                length,
                &raw mut returned,
            )
        };
        // SAFETY: Windows initializes the two fixed header fields for a successful or short query.
        let (assigned, listed) = unsafe {
            (
                usize::try_from((*list).NumberOfAssignedProcesses).unwrap_or(usize::MAX),
                usize::try_from((*list).NumberOfProcessIdsInList).unwrap_or(usize::MAX),
            )
        };
        if observed == 0 {
            // SAFETY: this reads the immediately preceding query result on the same thread.
            if unsafe { GetLastError() } != ERROR_MORE_DATA {
                return Err(channel_error("Windows Job process list cannot be observed"));
            }
        }
        if observed == 0 || assigned > capacity || listed > capacity || listed != assigned {
            let next = assigned.max(capacity.saturating_mul(2));
            if next <= capacity {
                return Err(channel_error("Windows Job process list cannot be observed exactly"));
            }
            capacity = next;
            continue;
        }
        // SAFETY: the successful query reported `listed <= capacity` initialized identifiers.
        let identifiers =
            unsafe { core::slice::from_raw_parts((*list).ProcessIdList.as_ptr(), listed) };
        let mut result = Vec::new();
        result
            .try_reserve_exact(listed)
            .map_err(|_| channel_error("Windows Job process-list allocation is unavailable"))?;
        result.extend_from_slice(identifiers);
        return Ok(result);
    }
}

fn handle_is_live(raw: HANDLE) -> bool {
    let mut flags = 0_u32;
    // SAFETY: the caller retains the handle and the query writes only `flags`.
    unsafe { GetHandleInformation(raw, &raw mut flags) } != 0
}

/// Helper-owned inherited status/control endpoints opened from C2-reserved environment values.
#[derive(Debug)]
pub struct NativeWindowsHelperAttachment {
    status: File,
    control: Option<File>,
    containment_job: Option<File>,
}

impl NativeWindowsHelperAttachment {
    /// Opens the exact inherited channel handles.
    ///
    /// # Errors
    /// Rejects missing, malformed, or non-handle environment values.
    pub fn from_environment() -> Result<Self, ProcessError> {
        let status = inherited_file(NATIVE_WINDOWS_STATUS_HANDLE_ENV)?;
        let control = inherited_file(NATIVE_WINDOWS_CONTROL_HANDLE_ENV)?;
        let containment_job = inherited_optional_file(NATIVE_WINDOWS_JOB_HANDLE_ENV)?;
        Ok(Self { status, control: Some(control), containment_job })
    }

    /// Writes the digest-bound record proving that the target was successfully resumed.
    ///
    /// # Errors
    /// Returns a protocol error if C2 can no longer observe the record.
    pub fn signal_started(&mut self, record: [u8; 32]) -> Result<(), ProcessError> {
        self.status
            .write_all(&record)
            .and_then(|()| self.status.flush())
            .map_err(|_| channel_error("Windows target-started record cannot be written"))
    }

    /// Publishes the digest-bound final worker/quiescence result before helper exit.
    pub fn signal_quiescence(&mut self, record: [u8; 32]) -> Result<(), ProcessError> {
        self.status
            .write_all(&record)
            .map_err(|_| channel_error("Windows helper quiescence record cannot be written"))
    }

    /// Publishes a version-two exact target status and independent post-activation failure.
    ///
    /// # Errors
    /// Rejects legacy-only evidence and reports a protocol failure if C2 cannot receive the frame.
    pub fn signal_completion(
        &mut self,
        record: [u8; Sha256Digest::LENGTH],
        completion: NativeWindowsCompletion,
    ) -> Result<(), ProcessError> {
        if completion.is_legacy()
            || completion.target_status().is_none()
                && completion.post_activation_failure().is_none()
            || completion.post_activation_failure()
                == Some(NativePostActivationFailure::LegacyUnspecified)
        {
            return Err(channel_error("Windows helper completion evidence is not exact"));
        }
        let mut tail = [0_u8; WINDOWS_COMPLETION_TAIL_BYTES];
        tail[0] = WINDOWS_COMPLETION_VERSION;
        if let Some(status) = completion.target_status() {
            tail[1] |= WINDOWS_COMPLETION_HAS_STATUS;
            tail[2..6].copy_from_slice(&status.to_le_bytes());
        }
        if let Some(failure) = completion.post_activation_failure() {
            tail[1] |= WINDOWS_COMPLETION_HAS_FAILURE;
            tail[6] = crate::terminal::native_failure_tag(failure);
        }
        self.status
            .write_all(&record)
            .and_then(|()| self.status.write_all(&tail))
            .and_then(|()| self.status.flush())
            .map_err(|_| channel_error("Windows helper completion frame cannot be written"))
    }

    /// Publishes exact private-file identities while delete-on-close remains armed.
    ///
    /// # Errors
    /// Returns a protocol failure if the complete custody record cannot be written and flushed.
    pub fn signal_secret_files(
        &mut self,
        record: [u8; Sha256Digest::LENGTH],
        identities: &[NativeWindowsSecretFileIdentity],
    ) -> Result<(), ProcessError> {
        let count = u32::try_from(identities.len())
            .map_err(|_| channel_error("Windows secret-file custody count is not representable"))?;
        self.status
            .write_all(&record)
            .and_then(|()| self.status.write_all(&count.to_le_bytes()))
            .map_err(|_| channel_error("Windows secret-file custody header cannot be written"))?;
        for identity in identities {
            self.status
                .write_all(identity.binding.path_digest.as_bytes())
                .and_then(|()| {
                    self.status.write_all(&identity.binding.payload_len.to_le_bytes())
                })
                .and_then(|()| self.status.write_all(&identity.volume_serial.to_le_bytes()))
                .and_then(|()| self.status.write_all(&identity.file_id))
                .map_err(|_| {
                    channel_error("Windows secret-file custody identity cannot be written")
                })?;
        }
        self.status
            .flush()
            .map_err(|_| channel_error("Windows secret-file custody record cannot be flushed"))
    }

    /// Waits until C2 has retained every exact private-file identity.
    ///
    /// # Errors
    /// Returns a protocol failure if C2 disconnects or acknowledges a different transition.
    pub fn await_secret_file_adoption(&mut self) -> Result<(), ProcessError> {
        let control = self.control.as_mut().ok_or_else(|| {
            channel_error("Windows secret-file custody control channel is unavailable")
        })?;
        let mut acknowledgement = [0_u8; 1];
        control
            .read_exact(&mut acknowledgement)
            .map_err(|_| channel_error("Windows secret-file custody owner disconnected"))?;
        if acknowledgement != [SECRET_FILES_ACK] {
            return Err(channel_error(
                "Windows secret-file custody acknowledgement mismatched",
            ));
        }
        Ok(())
    }

    /// Publishes one suspended target birth identity before C2 permits it to resume.
    pub fn signal_target_adoption(
        &mut self,
        record: [u8; Sha256Digest::LENGTH],
        target: ProcessTreeIdentity,
    ) -> Result<(), ProcessError> {
        let start_token = target.start_token().ok_or_else(|| {
            channel_error("Windows target adoption birth token is missing")
        })?;
        if target.root_pid() == 0
            || target.process_group().is_some()
            || !target.complete_containment()
        {
            return Err(channel_error("Windows target adoption identity is incomplete"));
        }
        let mut frame = [0_u8; TARGET_ADOPTION_BYTES];
        frame[..Sha256Digest::LENGTH].copy_from_slice(&record);
        frame[Sha256Digest::LENGTH..Sha256Digest::LENGTH + 4]
            .copy_from_slice(&target.root_pid().to_le_bytes());
        frame[Sha256Digest::LENGTH + 4..].copy_from_slice(&start_token.to_le_bytes());
        self.status
            .write_all(&frame)
            .and_then(|()| self.status.flush())
            .map_err(|_| channel_error("Windows target adoption record cannot be written"))
    }

    /// Waits for C2 to publish its retained Job and target handles before target resume.
    pub fn await_target_adoption(&mut self) -> Result<(), ProcessError> {
        let control = self.control.as_mut().ok_or_else(|| {
            channel_error("Windows target adoption control channel is unavailable")
        })?;
        let mut acknowledgement = [0_u8; 1];
        control
            .read_exact(&mut acknowledgement)
            .map_err(|_| channel_error("Windows target adoption owner disconnected"))?;
        if acknowledgement != [TARGET_ADOPTION_ACK] {
            return Err(channel_error("Windows target adoption acknowledgement mismatched"));
        }
        Ok(())
    }

    /// Reports whether the C2 owner still retains the paired control endpoint.
    #[must_use]
    pub fn owner_connected(&self) -> bool {
        self.control.as_ref().is_some_and(|control| {
            // SAFETY: this only queries the live inherited pipe without consuming control bytes.
            unsafe {
                PeekNamedPipe(
                    control.as_raw_handle().cast(),
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            } != 0
        })
    }

    /// Transfers the resize reader to the `ConPTY` control loop.
    #[must_use]
    pub const fn take_control_reader(&mut self) -> Option<File> {
        self.control.take()
    }

    /// Transfers the exact inherited Job Object handle into helper activation.
    #[must_use]
    pub const fn take_containment_job(&mut self) -> Option<File> {
        self.containment_job.take()
    }
}

fn read_exact_while(
    reader: &mut dyn Read,
    bytes: &mut [u8],
    should_continue: &mut dyn FnMut() -> bool,
) -> Result<(), crate::platform::HandshakeError> {
    let mut offset = 0_usize;
    while offset < bytes.len() {
        if !should_continue() {
            return Err(crate::platform::HandshakeError::Cancelled);
        }
        match reader.read(&mut bytes[offset..]) {
            Ok(0) => {
                return Err(crate::platform::HandshakeError::Failed(channel_error(
                    "Windows target adoption stream closed",
                )));
            }
            Ok(count) => offset += count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::yield_now();
            }
            Err(_) => {
                return Err(crate::platform::HandshakeError::Failed(channel_error(
                    "Windows target adoption stream cannot be read",
                )));
            }
        }
    }
    Ok(())
}

fn process_start_token(process: &File) -> Option<u64> {
    let mut creation = windows_sys::Win32::Foundation::FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let mut exit = creation;
    let mut kernel = creation;
    let mut user = creation;
    // SAFETY: the File owns a query-capable process handle and every FILETIME is writable.
    let observed = unsafe {
        GetProcessTimes(
            process.as_raw_handle().cast(),
            &raw mut creation,
            &raw mut exit,
            &raw mut kernel,
            &raw mut user,
        )
    };
    (observed != 0)
        .then(|| (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime))
}

fn pipe() -> Result<(File, File), ProcessError> {
    let mut reader: HANDLE = std::ptr::null_mut();
    let mut writer: HANDLE = std::ptr::null_mut();
    // SAFETY: output pointers are valid; null attributes create initially non-inheritable handles.
    if unsafe { CreatePipe(&raw mut reader, &raw mut writer, std::ptr::null(), 0) } == 0 {
        return Err(channel_error("Windows anonymous helper channel cannot be created"));
    }
    // SAFETY: both non-null handles were returned by CreatePipe and ownership moves into File.
    let reader = unsafe { File::from_raw_handle(reader.cast()) };
    // SAFETY: paired writer is independently owned and also moves into File.
    let writer = unsafe { File::from_raw_handle(writer.cast()) };
    Ok((reader, writer))
}

fn set_nonblocking(writer: &File) -> Result<(), ProcessError> {
    let mode = PIPE_NOWAIT;
    // SAFETY: the File owns a live pipe endpoint and all optional output-setting pointers are null.
    if unsafe {
        SetNamedPipeHandleState(
            writer.as_raw_handle().cast(),
            &raw const mode,
            std::ptr::null(),
            std::ptr::null(),
        )
    } == 0
    {
        return Err(channel_error("Windows terminal control channel cannot be made nonblocking"));
    }
    Ok(())
}

fn inherited_file(key: &'static str) -> Result<File, ProcessError> {
    let value =
        std::env::var_os(key).ok_or_else(|| channel_error("Windows helper channel is missing"))?;
    let text = value
        .to_str()
        .ok_or_else(|| channel_error("Windows helper channel identity is not Unicode"))?;
    let raw = text
        .parse::<usize>()
        .map_err(|_| channel_error("Windows helper channel identity is malformed"))?;
    if raw == 0 || raw == usize::MAX {
        return Err(channel_error("Windows helper channel identity is invalid"));
    }
    let handle = raw as RawHandle;
    // SAFETY: C2 supplies an exact uniquely inherited child HANDLE and removes the environment
    // identity before any target command is created; this attachment assumes its ownership.
    Ok(unsafe { File::from_raw_handle(handle) })
}

fn inherited_optional_file(key: &'static str) -> Result<Option<File>, ProcessError> {
    if std::env::var_os(key).is_none() {
        return Ok(None);
    }
    inherited_file(key).map(Some)
}

const fn channel_error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::Spawn,
        ProcessOperation::Spawn,
        RecoveryClass::CancelAndReap,
        detail,
    )
}

const fn terminal_control_error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::InvalidInput,
        ProcessOperation::Control,
        RecoveryClass::CorrectRequest,
        detail,
    )
}

const fn worker_error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::Supervisor,
        ProcessOperation::Wait,
        RecoveryClass::CancelAndReap,
        detail,
    )
}
