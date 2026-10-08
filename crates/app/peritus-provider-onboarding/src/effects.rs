//! Durable ownership records for provider installation and credential effects.

use std::{
    fs::{self, File, OpenOptions},
    io::Write as _,
    path::{Path, PathBuf},
};

use peritus_product_state::{ProviderKind, ProviderSelection};
use peritus_process::{NativeWindowsContainmentIdentity, ProcessTreeIdentity};
use peritus_secrets::{PlatformCredentialStore, SecretErrorKind, parse_credential_reference};
use serde::{Deserialize, Serialize};

use crate::OnboardingError;

const RECORD_VERSION: u8 = 1;

/// Protected journal owned by the launcher's durable application-state root.
#[derive(Clone, Debug)]
pub struct ProviderEffectStore {
    root: PathBuf,
}

impl ProviderEffectStore {
    /// Opens or creates the exact provider-effect journal directory.
    ///
    /// # Errors
    /// Returns a filesystem failure if the protected journal cannot be prepared.
    pub fn open(root: PathBuf) -> Result<Self, OnboardingError> {
        fs::create_dir_all(&root).map_err(|error| journal_error("create journal", &error))?;
        protect_directory(&root)?;
        let root = fs::canonicalize(root)
            .map_err(|error| journal_error("canonicalize journal", &error))?;
        Ok(Self { root })
    }

    pub(crate) fn begin_install(
        &self,
        kind: ProviderKind,
        url: &'static str,
        script_sha256: String,
        temporary_directory: PathBuf,
    ) -> Result<InstallEffect, OnboardingError> {
        let operation_id = random_operation_id()?;
        let record = InstallRecord {
            version: RECORD_VERSION,
            operation_id,
            provider: provider_name(kind)?.to_owned(),
            scope: "current-user".to_owned(),
            url: url.to_owned(),
            script_sha256,
            temporary_directory,
            phase: InstallPhase::Prepared,
            tree_root_pid: None,
            tree_start_token: None,
            tree_process_group: None,
            tree_complete_containment: None,
            windows_job_identity: None,
            windows_job_name: None,
            terminal_success: None,
            terminal_code: None,
            child_cleanup_complete: None,
            child_cleanup_detail: None,
            cleanup_complete: false,
        };
        let path = self.install_path(kind)?;
        write_new(&path, &record)?;
        Ok(InstallEffect { path, record })
    }

    pub(crate) fn reconcile_install(
        &self,
        kind: ProviderKind,
    ) -> Result<InstallRestart, OnboardingError> {
        let path = self.install_path(kind)?;
        let Some(record) = read_optional::<InstallRecord>(&path)? else {
            return Ok(InstallRestart::None);
        };
        validate_install_record(&record, kind)?;
        match record.phase {
            InstallPhase::Prepared => Ok(InstallRestart::Pending {
                effect: InstallEffect { path, record },
                identity: None,
                windows_containment: None,
                cancellation_requested: false,
            }),
            InstallPhase::Running | InstallPhase::CancelRequested => {
                let identity = install_identity(&record)?;
                let windows_containment = install_windows_containment(&record, identity)?;
                let cancellation_requested =
                    matches!(record.phase, InstallPhase::CancelRequested);
                Ok(InstallRestart::Pending {
                    effect: InstallEffect { path, record },
                    identity: Some(identity),
                    windows_containment,
                    cancellation_requested,
                })
            }
            InstallPhase::Terminal | InstallPhase::CleanupFailed => {
                let success = record.terminal_success.unwrap_or(false);
                let code = record.terminal_code;
                Ok(InstallRestart::Terminal {
                    effect: InstallEffect { path, record },
                    success,
                    code,
                })
            }
        }
    }

    pub(crate) fn begin_credential(
        &self,
        credential_reference: &str,
    ) -> Result<(), OnboardingError> {
        let path = self.credential_path(credential_reference)?;
        let record = CredentialRecord {
            version: RECORD_VERSION,
            credential_reference: credential_reference.to_owned(),
            phase: CredentialPhase::Prepared,
        };
        write_new(&path, &record)
    }

    pub(crate) fn credential_published(
        &self,
        credential_reference: &str,
    ) -> Result<(), OnboardingError> {
        self.update_credential(credential_reference, CredentialPhase::Published)
    }

    pub(crate) fn credential_cleanup_required(
        &self,
        credential_reference: &str,
    ) -> Result<(), OnboardingError> {
        let path = self.credential_path(credential_reference)?;
        if path.exists() {
            self.update_credential(credential_reference, CredentialPhase::CleanupRequired)
        } else {
            let record = CredentialRecord {
                version: RECORD_VERSION,
                credential_reference: credential_reference.to_owned(),
                phase: CredentialPhase::CleanupRequired,
            };
            write_new(&path, &record)
        }
    }

    pub(crate) fn credential_settled(
        &self,
        credential_reference: &str,
    ) -> Result<(), OnboardingError> {
        remove_record(&self.credential_path(credential_reference)?)
    }

    /// Adopts credentials present in product state and cleans every other exact journaled entry.
    ///
    /// # Errors
    /// Returns a journal, reference, or exact credential cleanup failure. Failed records remain.
    pub fn reconcile_credentials(
        &self,
        selection: &ProviderSelection,
    ) -> Result<(), OnboardingError> {
        let adopted = selection
            .direct_profiles()
            .iter()
            .map(|profile| profile.credential_reference())
            .collect::<std::collections::BTreeSet<_>>();
        let entries = fs::read_dir(&self.root)
            .map_err(|error| journal_error("list credential effects", &error))?;
        for entry in entries {
            let entry = entry.map_err(|error| journal_error("read credential effect", &error))?;
            let name = entry.file_name();
            if !name.to_string_lossy().starts_with("credential-") {
                continue;
            }
            let path = entry.path();
            let record = read_required::<CredentialRecord>(&path)?;
            validate_credential_record(&record)?;
            if adopted.contains(record.credential_reference.as_str()) {
                remove_record(&path)?;
                continue;
            }
            let reference = parse_credential_reference(&record.credential_reference)?;
            match PlatformCredentialStore::providers().remove(reference.resource_id()) {
                Ok(()) => remove_record(&path)?,
                Err(error) if error.kind() == SecretErrorKind::Missing => remove_record(&path)?,
                Err(cleanup) => {
                    return Err(OnboardingError::CredentialReconciliation {
                        credential_reference: record.credential_reference,
                        publication: format!("durable {:?} effect is not adopted", record.phase),
                        cleanup,
                    });
                }
            }
        }
        Ok(())
    }

    /// Records an exact credential removal before product-state replacement is committed.
    ///
    /// # Errors
    /// Returns a journal or reference failure.
    pub fn record_credential_cleanup(
        &self,
        credential_reference: &str,
    ) -> Result<(), OnboardingError> {
        self.credential_cleanup_required(credential_reference)
    }

    fn update_credential(
        &self,
        credential_reference: &str,
        phase: CredentialPhase,
    ) -> Result<(), OnboardingError> {
        let path = self.credential_path(credential_reference)?;
        let mut record = read_required::<CredentialRecord>(&path)?;
        validate_credential_record(&record)?;
        if record.credential_reference != credential_reference {
            return Err(journal_detail("credential effect identity differs"));
        }
        record.phase = phase;
        write_existing(&path, &record)
    }

    fn install_path(&self, kind: ProviderKind) -> Result<PathBuf, OnboardingError> {
        Ok(self.root.join(format!("install-{}.json", provider_name(kind)?)))
    }

    fn credential_path(&self, credential_reference: &str) -> Result<PathBuf, OnboardingError> {
        let reference = parse_credential_reference(credential_reference)?;
        Ok(self.root.join(format!("credential-{}.json", hex(reference.resource_id().as_bytes()))))
    }

    #[cfg(windows)]
    pub(crate) fn reopen_install_owner(
        path: &Path,
    ) -> Result<(ProviderKind, InstallEffect), OnboardingError> {
        let path = fs::canonicalize(path)
            .map_err(|error| journal_error("canonicalize install owner record", &error))?;
        let record = read_required::<InstallRecord>(&path)?;
        let kind = provider_kind(&record.provider)?;
        validate_install_record(&record, kind)?;
        if !matches!(record.phase, InstallPhase::Prepared) {
            return Err(journal_detail(
                "install owner dispatch requires an exact prepared operation",
            ));
        }
        let expected = format!("install-{}.json", provider_name(kind)?);
        if path.file_name().and_then(|name| name.to_str()) != Some(expected.as_str()) {
            return Err(journal_detail("install owner record path differs from its provider"));
        }
        Ok((kind, InstallEffect { path, record }))
    }
}

pub(crate) struct InstallEffect {
    path: PathBuf,
    record: InstallRecord,
}

impl InstallEffect {
    pub(crate) fn running(
        &mut self,
        identity: ProcessTreeIdentity,
    ) -> Result<(), OnboardingError> {
        self.record.phase = InstallPhase::Running;
        self.record.tree_root_pid = Some(identity.root_pid());
        self.record.tree_start_token = identity.start_token();
        self.record.tree_process_group = identity.process_group();
        self.record.tree_complete_containment = Some(identity.complete_containment());
        self.record.windows_job_identity = None;
        self.record.windows_job_name = None;
        write_existing(&self.path, &self.record)
    }

    #[cfg(windows)]
    pub(crate) fn running_windows(
        &mut self,
        containment: &NativeWindowsContainmentIdentity,
    ) -> Result<(), OnboardingError> {
        let identity = containment.target_identity();
        self.record.phase = InstallPhase::Running;
        self.record.tree_root_pid = Some(identity.root_pid());
        self.record.tree_start_token = identity.start_token();
        self.record.tree_process_group = identity.process_group();
        self.record.tree_complete_containment = Some(identity.complete_containment());
        self.record.windows_job_identity = Some(*containment.job_identity().as_bytes());
        self.record.windows_job_name = Some(containment.object_name().to_owned());
        write_existing(&self.path, &self.record)
    }

    pub(crate) fn cancellation_requested(&mut self) -> Result<(), OnboardingError> {
        self.record.phase = InstallPhase::CancelRequested;
        write_existing(&self.path, &self.record)
    }

    pub(crate) fn terminal(
        &mut self,
        success: bool,
        code: Option<i32>,
    ) -> Result<(), OnboardingError> {
        self.record.phase = InstallPhase::Terminal;
        self.record.terminal_success = Some(success);
        self.record.terminal_code = code;
        write_existing(&self.path, &self.record)
    }

    pub(crate) fn child_cleanup(
        &mut self,
        complete: bool,
        detail: Option<String>,
    ) -> Result<(), OnboardingError> {
        self.record.child_cleanup_complete = Some(complete);
        self.record.child_cleanup_detail = detail;
        write_existing(&self.path, &self.record)
    }

    pub(crate) fn cleanup(&mut self, complete: bool) -> Result<(), OnboardingError> {
        self.record.cleanup_complete = complete;
        if !complete {
            self.record.phase = InstallPhase::CleanupFailed;
        }
        write_existing(&self.path, &self.record)
    }

    pub(crate) fn settled(self) -> Result<(), OnboardingError> {
        remove_record(&self.path)
    }

    pub(crate) fn operation_id(&self) -> &str {
        &self.record.operation_id
    }

    pub(crate) fn journal_path(&self) -> &Path {
        &self.path
    }

    #[cfg(windows)]
    pub(crate) fn url(&self) -> &str {
        &self.record.url
    }

    #[cfg(windows)]
    pub(crate) fn script_sha256(&self) -> &str {
        &self.record.script_sha256
    }

    #[cfg(windows)]
    pub(crate) fn temporary_directory(&self) -> &Path {
        &self.record.temporary_directory
    }

    pub(crate) fn reconcile_cleanup(&mut self) -> Result<(), OnboardingError> {
        let cleanup = cleanup_temporary_directory(&self.record.temporary_directory);
        self.cleanup(cleanup.is_ok())?;
        cleanup
    }
}

pub(crate) enum InstallRestart {
    None,
    Pending {
        effect: InstallEffect,
        identity: Option<ProcessTreeIdentity>,
        windows_containment: Option<NativeWindowsContainmentIdentity>,
        cancellation_requested: bool,
    },
    Terminal { effect: InstallEffect, success: bool, code: Option<i32> },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct InstallRecord {
    version: u8,
    operation_id: String,
    provider: String,
    scope: String,
    url: String,
    script_sha256: String,
    temporary_directory: PathBuf,
    phase: InstallPhase,
    tree_root_pid: Option<u32>,
    tree_start_token: Option<u64>,
    tree_process_group: Option<u32>,
    tree_complete_containment: Option<bool>,
    windows_job_identity: Option<[u8; 32]>,
    windows_job_name: Option<String>,
    terminal_success: Option<bool>,
    terminal_code: Option<i32>,
    child_cleanup_complete: Option<bool>,
    child_cleanup_detail: Option<String>,
    cleanup_complete: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
enum InstallPhase {
    Prepared,
    Running,
    CancelRequested,
    Terminal,
    CleanupFailed,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CredentialRecord {
    version: u8,
    credential_reference: String,
    phase: CredentialPhase,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
enum CredentialPhase {
    Prepared,
    Published,
    CleanupRequired,
}

fn validate_install_record(
    record: &InstallRecord,
    kind: ProviderKind,
) -> Result<(), OnboardingError> {
    if record.version != RECORD_VERSION
        || record.provider != provider_name(kind)?
        || record.scope != "current-user"
        || !record.url.starts_with("https://")
        || record.operation_id.len() != 32
        || record.script_sha256.len() != 64
        || !record.operation_id.bytes().all(|byte| byte.is_ascii_hexdigit())
        || !record.script_sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        || !record.temporary_directory.is_absolute()
    {
        return Err(journal_detail("install effect record is malformed"));
    }
    let identity_fields = [
        record.tree_root_pid.is_some(),
        record.tree_start_token.is_some(),
        record.tree_complete_containment.is_some(),
    ];
    if identity_fields.iter().any(|present| *present)
        && identity_fields.iter().any(|present| !*present)
    {
        return Err(journal_detail("install process identity is incomplete"));
    }
    if record.windows_job_identity.is_some() != record.windows_job_name.is_some() {
        return Err(journal_detail("install Windows Job identity is incomplete"));
    }
    if matches!(record.phase, InstallPhase::Running | InstallPhase::CancelRequested)
        && record.tree_root_pid.is_none()
    {
        return Err(journal_detail("active install record has no process identity"));
    }
    Ok(())
}

fn install_identity(record: &InstallRecord) -> Result<ProcessTreeIdentity, OnboardingError> {
    let root_pid = record
        .tree_root_pid
        .ok_or_else(|| journal_detail("install process root is absent"))?;
    let start_token = record
        .tree_start_token
        .ok_or_else(|| journal_detail("install process birth token is absent"))?;
    let complete_containment = record
        .tree_complete_containment
        .ok_or_else(|| journal_detail("install containment identity is absent"))?;
    Ok(ProcessTreeIdentity::new(
        root_pid,
        Some(start_token),
        record.tree_process_group,
        complete_containment,
    ))
}

fn install_windows_containment(
    record: &InstallRecord,
    target: ProcessTreeIdentity,
) -> Result<Option<NativeWindowsContainmentIdentity>, OnboardingError> {
    match (&record.windows_job_identity, &record.windows_job_name) {
        (None, None) => Ok(None),
        (Some(job_identity), Some(job_name)) => NativeWindowsContainmentIdentity::new(
            peritus_types::Sha256Digest::new(*job_identity),
            job_name.clone(),
            target,
        )
        .map(Some)
        .map_err(|error| journal_error("validate Windows install owner", &error)),
        _ => Err(journal_detail("install Windows Job identity is incomplete")),
    }
}

fn validate_credential_record(record: &CredentialRecord) -> Result<(), OnboardingError> {
    if record.version != RECORD_VERSION {
        return Err(journal_detail("credential effect version is unsupported"));
    }
    let _reference = parse_credential_reference(&record.credential_reference)?;
    Ok(())
}

fn provider_name(kind: ProviderKind) -> Result<&'static str, OnboardingError> {
    match kind {
        ProviderKind::CodexAccount => Ok("codex-account"),
        ProviderKind::ClaudeAccount => Ok("claude-account"),
        _ => Err(OnboardingError::UnsupportedProvider),
    }
}

#[cfg(windows)]
fn provider_kind(name: &str) -> Result<ProviderKind, OnboardingError> {
    match name {
        "codex-account" => Ok(ProviderKind::CodexAccount),
        "claude-account" => Ok(ProviderKind::ClaudeAccount),
        _ => Err(journal_detail("install effect provider is unsupported")),
    }
}

fn cleanup_temporary_directory(path: &Path) -> Result<(), OnboardingError> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(journal_error("clean install temporary directory", &error)),
    }
}

fn read_optional<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Option<T>, OnboardingError> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|error| journal_error("decode effect record", &error)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(journal_error("read effect record", &error)),
    }
}

fn read_required<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, OnboardingError> {
    read_optional(path)?.ok_or_else(|| journal_detail("required effect record is absent"))
}

fn write_new(path: &Path, value: &impl Serialize) -> Result<(), OnboardingError> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| journal_error("encode effect record", &error))?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|error| journal_error("create effect record", &error))?;
    protect_file(&file, path)?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| journal_error("publish effect record", &error))?;
    sync_parent(path)
}

fn write_existing(path: &Path, value: &impl Serialize) -> Result<(), OnboardingError> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| journal_error("encode effect record", &error))?;
    let parent = path.parent().ok_or_else(|| journal_detail("effect record has no parent"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| journal_error("stage effect record", &error))?;
    protect_file(temporary.as_file(), temporary.path())?;
    temporary
        .write_all(&bytes)
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|error| journal_error("synchronize effect record", &error))?;
    temporary
        .persist(path)
        .map_err(|error| journal_error("replace effect record", &error.error))?;
    sync_parent(path)
}

fn remove_record(path: &Path) -> Result<(), OnboardingError> {
    match fs::remove_file(path) {
        Ok(()) => sync_parent(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(journal_error("remove settled effect record", &error)),
    }
}

fn random_operation_id() -> Result<String, OnboardingError> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| OnboardingError::Random(error.to_string()))?;
    if bytes == [0; 16] {
        bytes[0] = 1;
    }
    Ok(hex(&bytes))
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        value.push(char::from(HEX[usize::from(byte >> 4)]));
        value.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    value
}

fn journal_error(operation: &'static str, detail: &dyn std::fmt::Display) -> OnboardingError {
    OnboardingError::EffectJournal { operation, detail: detail.to_string() }
}

fn journal_detail(detail: &'static str) -> OnboardingError {
    OnboardingError::EffectJournal { operation: "validate effect record", detail: detail.to_owned() }
}

#[cfg(unix)]
fn protect_directory(path: &Path) -> Result<(), OnboardingError> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| journal_error("protect journal directory", &error))
}

#[cfg(windows)]
const fn protect_directory(_path: &Path) -> Result<(), OnboardingError> {
    Ok(())
}

#[cfg(unix)]
fn protect_file(file: &File, path: &Path) -> Result<(), OnboardingError> {
    use std::os::unix::fs::PermissionsExt as _;
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|error| journal_error("protect effect record", &error))
}

#[cfg(windows)]
const fn protect_file(_file: &File, _path: &Path) -> Result<(), OnboardingError> {
    Ok(())
}

#[cfg(unix)]
fn sync_parent(path: &Path) -> Result<(), OnboardingError> {
    let parent = path.parent().ok_or_else(|| journal_detail("effect record has no parent"))?;
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| journal_error("synchronize effect journal", &error))
}

#[cfg(windows)]
const fn sync_parent(_path: &Path) -> Result<(), OnboardingError> {
    Ok(())
}
