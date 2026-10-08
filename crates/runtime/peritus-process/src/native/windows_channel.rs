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
    Foundation::{GetHandleInformation, HANDLE},
    System::{
        JobObjects::IsProcessInJob,
        Pipes::{CreatePipe, PIPE_NOWAIT, PeekNamedPipe, SetNamedPipeHandleState},
        Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    },
};

use crate::{
    ErrorCode, NativeProtectedHandle, NativeWindowsContainmentIdentity, ProcessError,
    ProcessOperation, ProcessTreeIdentity, RecoveryClass, TerminalSize,
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
    _target: File,
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
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
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
        *adoption = Some(ContainmentAdoption { identity, _target: target });
        drop(adoption);
        Ok(reader)
    }

    pub(crate) fn acknowledge_target_adoption(&self) -> Result<(), ProcessError> {
        self.write_control(&[TARGET_ADOPTION_ACK])
    }

    pub(crate) fn containment_identity(
        &self,
    ) -> Result<Option<NativeWindowsContainmentIdentity>, ProcessError> {
        self.adoption
            .lock()
            .map(|adoption| adoption.as_ref().map(|value| value.identity.clone()))
            .map_err(|_| channel_error("Windows containment adoption state was poisoned"))
    }

    #[must_use]
    pub(crate) fn retains_containment_job(&self, expected: Sha256Digest) -> bool {
        self.containment_job_identity == Some(expected) && self.raw_containment_job().is_ok()
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

    pub(crate) fn verify_helper_quiescence(
        &self,
        quiesced: Sha256Digest,
        worker_failed: Sha256Digest,
    ) -> Result<(), ProcessError> {
        let mut record = [0_u8; Sha256Digest::LENGTH];
        let mut reader = &*self.status_reader;
        reader
            .read_exact(&mut record)
            .map_err(|_| worker_error("Windows helper quiescence record is missing"))?;
        if record == quiesced.into_bytes() {
            return Ok(());
        }
        if record == worker_failed.into_bytes() {
            return Err(worker_error("Windows helper worker failed after target activation"));
        }
        Err(worker_error("Windows helper quiescence record is malformed"))
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
