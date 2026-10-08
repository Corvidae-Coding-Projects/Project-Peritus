//! Resumable exact-identity release download, durable staging, and native extraction.

use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    process::Command,
};

use futures_util::StreamExt as _;
use peritus_process::{
    NativeProcessProbe, NativeWindowsContainmentIdentity, ProbeObservation, ProcessProbe as _,
    ProcessTreeIdentity, ProcessTreeQuiescence,
};
#[cfg(windows)]
use peritus_process::NativeWindowsProcessOwner;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use tokio::io::{AsyncReadExt as _, AsyncSeekExt as _, AsyncWriteExt as _};

use crate::{AppLayout, LauncherError};

use super::release::{Release, ReleaseAsset};

const RECEIPT_VERSION: u8 = 1;
const MAX_RECEIPT_BYTES: u64 = 256 * 1024;
const MAX_CHECKSUM_BYTES: usize = 1024;
const ACKNOWLEDGEMENT_BYTES: u64 = 8 * 1024 * 1024;

pub(super) struct Package {
    record_path: PathBuf,
    stage_root: PathBuf,
    record: UpdateRecord,
    lock: Option<UpdateLock>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum OwnerProgress {
    AwaitingIdentity,
    ExactOwned,
    Installed,
}

pub(super) fn pending_release(layout: &AppLayout) -> Result<Option<Release>, LauncherError> {
    let path = active_record_path(layout);
    let Some(record) = read_record_optional(&path)? else {
        return Ok(None);
    };
    record.validate()?;
    Ok((record.phase != UpdatePhase::Installed).then_some(record.release))
}

pub(super) async fn package(
    layout: &AppLayout,
    release: &Release,
) -> Result<Package, LauncherError> {
    release.validate()?;
    let asset_name = asset_name()?;
    let checksum_name = format!("{asset_name}.sha256");
    if let Some(mut package) = Package::open_matching(
        layout,
        release,
        &asset_name,
        &checksum_name,
    )? {
        package.prepare().await?;
        return Ok(package);
    }
    let client = reqwest::Client::builder()
        .build()
        .map_err(|error| network("construct update download client", &error))?;
    let checksum_asset = release.asset(&checksum_name)?.clone();
    let expected = checksum(&client, &checksum_asset).await?;
    let mut package = Package::open_with_checksum(
        layout,
        release,
        &asset_name,
        &checksum_name,
        expected,
    )?;
    package.prepare_with_client(&client).await?;
    Ok(package)
}

impl Package {
    fn open_matching(
        layout: &AppLayout,
        release: &Release,
        asset_name: &str,
        checksum_name: &str,
    ) -> Result<Option<Self>, LauncherError> {
        let lock = UpdateLock::acquire(layout)?;
        let path = active_record_path(layout);
        let Some(record) = read_record_optional(&path)? else {
            return Ok(None);
        };
        record.validate()?;
        if record.release == *release
            && record.asset_name == asset_name
            && record.checksum_asset_name == checksum_name
        {
            let mut package = Self::from_record(layout, path, record, lock)?;
            package.reconcile_interrupted_owner()?;
            return Ok(Some(package));
        }
        if record.phase == UpdatePhase::Installed {
            return Ok(None);
        }
        Err(LauncherError::Update(format!(
            "an unfinished update for {} owns the durable staging receipt",
            record.release.tag()
        )))
    }

    fn open_with_checksum(
        layout: &AppLayout,
        release: &Release,
        asset_name: &str,
        checksum_name: &str,
        checksum: [u8; 32],
    ) -> Result<Self, LauncherError> {
        let lock = UpdateLock::acquire(layout)?;
        let path = active_record_path(layout);
        let existing = read_record_optional(&path)?;
        let record = if let Some(record) = existing {
            record.validate()?;
            if record.release == *release
                && record.asset_name == asset_name
                && record.checksum_asset_name == checksum_name
            {
                if record.archive_sha256 != hex(&checksum) {
                    return Err(LauncherError::Update(
                        "release checksum content changed after durable acknowledgement"
                            .to_owned(),
                    ));
                }
                record
            } else if record.phase == UpdatePhase::Installed {
                let replacement = UpdateRecord::new(
                    release.clone(),
                    asset_name,
                    checksum_name,
                    checksum,
                )?;
                replace_record(&path, &replacement)?;
                replacement
            } else {
                return Err(LauncherError::Update(format!(
                    "an unfinished update for {} owns the durable staging receipt",
                    record.release.tag()
                )));
            }
        } else {
            let created = UpdateRecord::new(
                release.clone(),
                asset_name,
                checksum_name,
                checksum,
            )?;
            publish_record(&path, &created)?;
            created
        };
        let mut package = Self::from_record(layout, path, record, lock)?;
        package.reconcile_interrupted_owner()?;
        Ok(package)
    }

    #[cfg(windows)]
    pub(super) fn reopen_owner(path: &Path) -> Result<Self, LauncherError> {
        let layout = AppLayout::discover()?.prepare()?;
        let expected = active_record_path(&layout);
        let actual = fs::canonicalize(path)
            .map_err(|error| LauncherError::filesystem("canonicalize update owner receipt", path, error))?;
        let expected = fs::canonicalize(&expected).map_err(|error| {
            LauncherError::filesystem("canonicalize active update receipt", &expected, error)
        })?;
        if actual != expected {
            return Err(LauncherError::Update(
                "update owner receipt is outside the protected update journal".to_owned(),
            ));
        }
        let lock = UpdateLock::acquire(&layout)?;
        let record = read_record_required(&expected)?;
        record.validate()?;
        if record.phase != UpdatePhase::OwnerPrepared {
            return Err(LauncherError::Update(
                "Windows update owner requires an exact prepared receipt".to_owned(),
            ));
        }
        Self::from_record(&layout, expected, record, lock)
    }

    fn from_record(
        layout: &AppLayout,
        record_path: PathBuf,
        record: UpdateRecord,
        lock: UpdateLock,
    ) -> Result<Self, LauncherError> {
        let asset = record.asset()?;
        let stage_root = layout
            .cache_root()
            .join("updates")
            .join(format!("{:020}-{:020}", record.release.id(), asset.id()));
        fs::create_dir_all(&stage_root).map_err(|error| {
            LauncherError::filesystem("create update staging directory", &stage_root, error)
        })?;
        Ok(Self { record_path, stage_root, record, lock: Some(lock) })
    }

    async fn prepare(&mut self) -> Result<(), LauncherError> {
        if self.record.phase == UpdatePhase::Installed {
            return Ok(());
        }
        let client = reqwest::Client::builder()
            .build()
            .map_err(|error| network("construct update download client", &error))?;
        self.prepare_with_client(&client).await
    }

    async fn prepare_with_client(
        &mut self,
        client: &reqwest::Client,
    ) -> Result<(), LauncherError> {
        self.ensure_downloaded(client).await?;
        #[cfg(not(windows))]
        self.ensure_extracted()?;
        Ok(())
    }

    async fn ensure_downloaded(
        &mut self,
        client: &reqwest::Client,
    ) -> Result<(), LauncherError> {
        match self.record.phase {
            UpdatePhase::Prepared | UpdatePhase::Downloading => receive(client, self).await,
            UpdatePhase::Downloaded => self.verify_complete_archive().await,
            UpdatePhase::OwnerPrepared
            | UpdatePhase::OwnerActive
            | UpdatePhase::Extracting
            | UpdatePhase::Extracted
            | UpdatePhase::Installing
            | UpdatePhase::InstallFailed
            | UpdatePhase::Installed => Ok(()),
        }
    }

    async fn verify_complete_archive(&self) -> Result<(), LauncherError> {
        let asset = self.record.asset()?;
        let actual = hash_file_exact(&self.archive_path(), asset.size()).await?;
        if hex(&actual) != self.record.archive_sha256 {
            return Err(LauncherError::Update(
                "durable release archive no longer matches its exact checksum".to_owned(),
            ));
        }
        Ok(())
    }

    pub(super) fn ensure_extracted(&mut self) -> Result<(), LauncherError> {
        if matches!(
            self.record.phase,
            UpdatePhase::Extracted
                | UpdatePhase::Installing
                | UpdatePhase::InstallFailed
                | UpdatePhase::Installed
        ) {
            return validate_bundle(&self.bundle_path());
        }
        if !matches!(
            self.record.phase,
            UpdatePhase::Downloaded | UpdatePhase::OwnerActive | UpdatePhase::OwnerPrepared
        ) {
            return Err(LauncherError::Update(
                "release extraction was requested before the archive was acknowledged".to_owned(),
            ));
        }
        let extracted = self.extracted_root();
        if extracted.is_dir() {
            validate_bundle(&self.bundle_path())?;
            self.record.phase = UpdatePhase::Extracted;
            self.record.clear_process();
            self.save()?;
            return Ok(());
        }
        let pending = self.extraction_pending();
        if pending.exists() {
            fs::remove_dir_all(&pending).map_err(|error| {
                LauncherError::filesystem("clear incomplete extraction stage", &pending, error)
            })?;
        }
        fs::create_dir_all(&pending).map_err(|error| {
            LauncherError::filesystem("create extraction stage", &pending, error)
        })?;
        let archive = self.archive_path();
        let mut command = extraction_command(&archive, &pending);
        let result = super::process::status(
            &mut command,
            "extract release archive",
            |identity| self.record_process(UpdatePhase::Extracting, identity),
        );
        let status = match result {
            Ok(status) => status,
            Err(error) => {
                self.return_to_downloaded()?;
                return Err(error);
            }
        };
        if !status.success() {
            self.return_to_downloaded()?;
            return Err(LauncherError::Update(format!(
                "release extraction failed with status {status}"
            )));
        }
        let pending_bundle = pending.join(self.bundle_directory_name()?);
        validate_bundle(&pending_bundle)?;
        fs::rename(&pending, &extracted).map_err(|error| {
            LauncherError::filesystem("publish extracted release", &extracted, error)
        })?;
        sync_directory(&self.stage_root)?;
        self.record.phase = UpdatePhase::Extracted;
        self.record.clear_process();
        self.record.failure = None;
        self.save()
    }

    fn return_to_downloaded(&mut self) -> Result<(), LauncherError> {
        self.record.phase = if self.record.windows_job_name.is_some() {
            UpdatePhase::OwnerActive
        } else {
            UpdatePhase::Downloaded
        };
        self.record.clear_process();
        self.save()
    }

    pub(super) fn release(&self) -> &Release { &self.record.release }
    pub(super) fn is_installed(&self) -> bool { self.record.phase == UpdatePhase::Installed }
    pub(super) fn receipt_path(&self) -> &Path { &self.record_path }
    pub(super) fn operation_id(&self) -> &str { &self.record.operation_id }
    pub(super) fn bundle_path(&self) -> PathBuf {
        self.extracted_root().join(
            self.bundle_directory_name().unwrap_or_else(|_| "invalid-package".to_owned()),
        )
    }
    pub(super) fn capture_path(&self, name: &str) -> PathBuf { self.stage_root.join(name) }

    pub(super) fn record_installing(
        &mut self,
        identity: ProcessTreeIdentity,
    ) -> Result<(), LauncherError> {
        self.record_process(UpdatePhase::Installing, identity)
    }

    pub(super) fn record_install_failed(
        &mut self,
        detail: impl Into<String>,
    ) -> Result<(), LauncherError> {
        let mut detail = detail.into();
        detail.truncate(4096);
        self.record.phase = UpdatePhase::InstallFailed;
        self.record.failure = Some(detail);
        self.record.clear_process();
        self.save()
    }

    pub(super) fn record_installed(&mut self) -> Result<(), LauncherError> {
        self.record.phase = UpdatePhase::Installed;
        self.record.failure = None;
        self.record.clear_process();
        self.save()
    }

    #[cfg(windows)]
    pub(super) fn prepare_windows_owner(&mut self) -> Result<(), LauncherError> {
        if !matches!(
            self.record.phase,
            UpdatePhase::Downloaded
                | UpdatePhase::Extracted
                | UpdatePhase::InstallFailed
                | UpdatePhase::OwnerPrepared
        ) {
            return Err(LauncherError::Update(
                "Windows update owner cannot be dispatched from the current durable phase"
                    .to_owned(),
            ));
        }
        self.record.phase = UpdatePhase::OwnerPrepared;
        self.record.failure = None;
        self.record.clear_process();
        self.record.clear_owner();
        self.save()
    }

    #[cfg(windows)]
    pub(super) fn record_windows_owner(
        &mut self,
        containment: &NativeWindowsContainmentIdentity,
    ) -> Result<(), LauncherError> {
        let identity = containment.target_identity();
        self.record.phase = UpdatePhase::OwnerActive;
        self.record.owner_root_pid = Some(identity.root_pid());
        self.record.owner_start_token = identity.start_token();
        self.record.windows_job_identity = Some(hex(containment.job_identity().as_bytes()));
        self.record.windows_job_name = Some(containment.object_name().to_owned());
        self.save()
    }

    #[cfg(windows)]
    pub(super) fn windows_job_identity(&self) -> peritus_types::Sha256Digest {
        let mut digest = Sha256::new();
        digest.update(b"peritus-product-update-owner-v1\0");
        digest.update(self.record.operation_id.as_bytes());
        digest.update(b"\0");
        digest.update(self.record.archive_sha256.as_bytes());
        peritus_types::Sha256Digest::new(digest.finalize().into())
    }

    #[cfg(windows)]
    pub(super) fn windows_job_name(&self) -> String {
        format!("Local\\PeritusJob-Update-{}", self.record.operation_id)
    }

    pub(super) fn release_lock(&mut self) { self.lock.take(); }

    fn archive_path(&self) -> PathBuf { self.stage_root.join(&self.record.asset_name) }
    fn extracted_root(&self) -> PathBuf { self.stage_root.join("extracted") }
    fn extraction_pending(&self) -> PathBuf { self.stage_root.join("extraction.pending") }

    fn bundle_directory_name(&self) -> Result<String, LauncherError> {
        self.record.asset_name.strip_suffix(archive_suffix()).map(str::to_owned).ok_or_else(|| {
            LauncherError::Update("release asset has an unexpected archive suffix".to_owned())
        })
    }

    fn record_process(
        &mut self,
        phase: UpdatePhase,
        identity: ProcessTreeIdentity,
    ) -> Result<(), LauncherError> {
        self.record.phase = phase;
        self.record.process_root_pid = Some(identity.root_pid());
        self.record.process_start_token = identity.start_token();
        self.record.process_group = identity.process_group();
        self.record.process_complete_containment = Some(identity.complete_containment());
        self.save()
    }

    fn acknowledge(
        &mut self,
        bytes: u64,
        digest: [u8; 32],
    ) -> Result<(), LauncherError> {
        self.record.phase = UpdatePhase::Downloading;
        self.record.acknowledged_bytes = bytes;
        self.record.acknowledged_sha256 = hex(&digest);
        self.save()
    }

    fn downloaded(&mut self) -> Result<(), LauncherError> {
        let size = self.record.asset()?.size();
        self.record.phase = UpdatePhase::Downloaded;
        self.record.acknowledged_bytes = size;
        self.record.acknowledged_sha256.clone_from(&self.record.archive_sha256);
        self.record.clear_process();
        self.save()
    }

    fn save(&self) -> Result<(), LauncherError> { replace_record(&self.record_path, &self.record) }

    fn reconcile_interrupted_owner(&mut self) -> Result<(), LauncherError> {
        if !matches!(
            self.record.phase,
            UpdatePhase::OwnerActive
                | UpdatePhase::Extracting
                | UpdatePhase::Installing
                | UpdatePhase::InstallFailed
        ) {
            return Ok(());
        }
        let mut had_binding = false;
        if let Some(containment) = self.record.windows_containment()? {
            had_binding = true;
            #[cfg(windows)]
            reconcile_windows_owner(&containment)?;
            #[cfg(not(windows))]
            return Err(LauncherError::Update(
                "Windows update ownership was recorded on a non-Windows host".to_owned(),
            ));
        } else if let Some(identity) = self.record.process_identity()? {
            had_binding = true;
            reconcile_native_process(identity)?;
        }
        if !had_binding && self.record.phase != UpdatePhase::InstallFailed {
            return Err(LauncherError::Update(
                "interrupted update phase has no exact process ownership identity".to_owned(),
            ));
        }
        if self.record.phase == UpdatePhase::Extracting {
            let pending = self.extraction_pending();
            if pending.exists() {
                fs::remove_dir_all(&pending).map_err(|error| {
                    LauncherError::filesystem("clear interrupted extraction stage", &pending, error)
                })?;
            }
        }
        self.record.phase = if self.extracted_root().is_dir() {
            UpdatePhase::Extracted
        } else {
            UpdatePhase::Downloaded
        };
        self.record.failure = None;
        self.record.clear_process();
        self.record.clear_owner();
        self.save()
    }
}

#[cfg(windows)]
pub(super) fn observe_owner_progress(
    path: &Path,
    operation_id: &str,
) -> Result<OwnerProgress, LauncherError> {
    let record = read_record_required(path)?;
    record.validate()?;
    if record.operation_id != operation_id {
        return Err(LauncherError::Update(
            "Windows update owner changed operation identity".to_owned(),
        ));
    }
    if record.phase == UpdatePhase::Installed {
        return Ok(OwnerProgress::Installed);
    }
    if record.phase == UpdatePhase::InstallFailed {
        return Err(LauncherError::Update(record.failure.unwrap_or_else(|| {
            "Windows update owner failed without a durable detail".to_owned()
        })));
    }
    let Some(containment) = record.windows_containment()? else {
        return Ok(OwnerProgress::AwaitingIdentity);
    };
    match NativeWindowsProcessOwner::observe_durable(&containment)
        .map_err(|error| LauncherError::Update(format!("observe Windows update owner: {error}")))?
    {
        ProbeObservation::ExactLive => Ok(OwnerProgress::ExactOwned),
        ProbeObservation::ExactAbsent => Err(LauncherError::Update(
            "Windows update owner exited before publishing a terminal receipt".to_owned(),
        )),
        ProbeObservation::Mismatched => Err(LauncherError::Update(
            "Windows update owner PID now names a different process".to_owned(),
        )),
        ProbeObservation::Unverifiable => Err(LauncherError::Update(
            "Windows update owner exact birth identity is unverifiable".to_owned(),
        )),
    }
}

async fn checksum(
    client: &reqwest::Client,
    asset: &ReleaseAsset,
) -> Result<[u8; 32], LauncherError> {
    if asset.size() > u64::try_from(MAX_CHECKSUM_BYTES).unwrap_or(u64::MAX) {
        return Err(LauncherError::Update(format!(
            "release checksum asset exceeds the {MAX_CHECKSUM_BYTES}-byte package format"
        )));
    }
    let response = client
        .get(asset.url())
        .header(reqwest::header::USER_AGENT, "peritus-updater")
        .send()
        .await
        .map_err(|error| network("download release checksum", &error))?
        .error_for_status()
        .map_err(|error| network("download release checksum", &error))?;
    let bytes = super::release::bounded_body(
        response,
        MAX_CHECKSUM_BYTES,
        "read release checksum",
    )
    .await?;
    if u64::try_from(bytes.len()).ok() != Some(asset.size()) {
        return Err(LauncherError::Update(
            "release checksum length differs from its asset identity".to_owned(),
        ));
    }
    parse_checksum(&bytes)
}

async fn receive(client: &reqwest::Client, package: &mut Package) -> Result<(), LauncherError> {
    let asset = package.record.asset()?.clone();
    let expected = decode_hex(&package.record.archive_sha256)?;
    let path = package.archive_path();
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(&path)
        .await
        .map_err(|error| LauncherError::filesystem("open update archive", &path, error))?;
    let metadata = file.metadata().await
        .map_err(|error| LauncherError::filesystem("inspect update archive", &path, error))?;
    if !metadata.is_file() || metadata.len() < package.record.acknowledged_bytes {
        return Err(LauncherError::Update(
            "release archive is shorter than its durable acknowledged progress".to_owned(),
        ));
    }
    let mut hasher = Sha256::new();
    let mut remaining = package.record.acknowledged_bytes;
    file.seek(std::io::SeekFrom::Start(0)).await
        .map_err(|error| LauncherError::filesystem("seek update archive", &path, error))?;
    let mut buffer = [0_u8; 64 * 1024];
    while remaining > 0 {
        let wanted = usize::try_from(remaining.min(buffer.len() as u64)).unwrap_or(buffer.len());
        let count = file.read(&mut buffer[..wanted]).await
            .map_err(|error| LauncherError::filesystem("read update archive prefix", &path, error))?;
        if count == 0 {
            return Err(LauncherError::Update(
                "release archive ended within acknowledged progress".to_owned(),
            ));
        }
        hasher.update(&buffer[..count]);
        remaining -= u64::try_from(count).unwrap_or(u64::MAX);
    }
    let prefix: [u8; 32] = hasher.clone().finalize().into();
    if hex(&prefix) != package.record.acknowledged_sha256 {
        return Err(LauncherError::Update(
            "release archive acknowledged prefix changed on disk".to_owned(),
        ));
    }
    file.set_len(package.record.acknowledged_bytes).await
        .map_err(|error| LauncherError::filesystem("truncate unacknowledged archive tail", &path, error))?;
    file.seek(std::io::SeekFrom::Start(package.record.acknowledged_bytes)).await
        .map_err(|error| LauncherError::filesystem("seek update archive resume point", &path, error))?;

    let offset = package.record.acknowledged_bytes;
    let mut request = client
        .get(asset.url())
        .header(reqwest::header::USER_AGENT, "peritus-updater");
    if offset > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={offset}-"));
    }
    let response = request.send().await
        .map_err(|error| network("download release archive", &error))?;
    validate_archive_response(&response, offset, asset.size())?;
    let mut stream = response.bytes_stream();
    let mut size = offset;
    let mut next_ack = offset.saturating_add(ACKNOWLEDGEMENT_BYTES).min(asset.size());
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| network("stream release archive", &error))?;
        size = size.checked_add(u64::try_from(chunk.len()).unwrap_or(u64::MAX))
            .ok_or_else(|| LauncherError::Update("release archive size overflowed".to_owned()))?;
        if size > asset.size() {
            return Err(LauncherError::Update(
                "release archive exceeds its declared asset size".to_owned(),
            ));
        }
        hasher.update(&chunk);
        file.write_all(&chunk).await
            .map_err(|error| LauncherError::filesystem("write update archive", &path, error))?;
        if size >= next_ack && size < asset.size() {
            file.sync_all().await
                .map_err(|error| LauncherError::filesystem("sync update archive", &path, error))?;
            let digest: [u8; 32] = hasher.clone().finalize().into();
            package.acknowledge(size, digest)?;
            next_ack = size.saturating_add(ACKNOWLEDGEMENT_BYTES).min(asset.size());
        }
    }
    if size != asset.size() {
        return Err(LauncherError::Update(format!(
            "release archive ended at {size} bytes; expected {}",
            asset.size()
        )));
    }
    file.sync_all().await
        .map_err(|error| LauncherError::filesystem("sync update archive", &path, error))?;
    let actual: [u8; 32] = hasher.finalize().into();
    if actual != expected {
        return Err(LauncherError::Update(
            "release archive checksum did not match its exact content identity".to_owned(),
        ));
    }
    package.downloaded()
}

fn validate_archive_response(
    response: &reqwest::Response,
    offset: u64,
    total: u64,
) -> Result<(), LauncherError> {
    let remaining = total.checked_sub(offset).ok_or_else(|| {
        LauncherError::Update("release resume offset exceeds asset size".to_owned())
    })?;
    let expected_status = if offset == 0 {
        reqwest::StatusCode::OK
    } else {
        reqwest::StatusCode::PARTIAL_CONTENT
    };
    if response.status() != expected_status {
        return Err(LauncherError::Update(format!(
            "release archive server returned {} for resume offset {offset}",
            response.status()
        )));
    }
    if response.content_length().is_some_and(|length| length != remaining) {
        return Err(LauncherError::Update(
            "release archive response length differs from its exact asset identity".to_owned(),
        ));
    }
    if offset > 0 {
        let value = response.headers().get(reqwest::header::CONTENT_RANGE)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| LauncherError::Update(
                "resumed release archive omitted Content-Range".to_owned(),
            ))?;
        let expected = format!("bytes {offset}-{}/{total}", total - 1);
        if value != expected {
            return Err(LauncherError::Update(
                "resumed release archive returned a different content range".to_owned(),
            ));
        }
    }
    Ok(())
}

async fn hash_file_exact(path: &Path, expected_size: u64) -> Result<[u8; 32], LauncherError> {
    let metadata = tokio::fs::metadata(path).await
        .map_err(|error| LauncherError::filesystem("inspect update archive", path, error))?;
    if !metadata.is_file() || metadata.len() != expected_size {
        return Err(LauncherError::Update(
            "release archive length differs from its exact asset identity".to_owned(),
        ));
    }
    let mut file = tokio::fs::File::open(path).await
        .map_err(|error| LauncherError::filesystem("open update archive", path, error))?;
    let mut hasher = Sha256::new();
    let mut observed = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).await
            .map_err(|error| LauncherError::filesystem("read update archive", path, error))?;
        if count == 0 { break; }
        observed = observed.checked_add(u64::try_from(count).unwrap_or(u64::MAX))
            .ok_or_else(|| LauncherError::Update("release archive size overflowed".to_owned()))?;
        hasher.update(&buffer[..count]);
    }
    if observed != expected_size {
        return Err(LauncherError::Update(
            "release archive changed while its checksum was inspected".to_owned(),
        ));
    }
    Ok(hasher.finalize().into())
}

fn parse_checksum(bytes: &[u8]) -> Result<[u8; 32], LauncherError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| LauncherError::Update("release checksum is not UTF-8".to_owned()))?
        .trim();
    decode_hex(text)
}

fn decode_hex(value: &str) -> Result<[u8; 32], LauncherError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(LauncherError::Update("release checksum is malformed".to_owned()));
    }
    let mut result = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        result[index] = u8::from_str_radix(std::str::from_utf8(pair).unwrap_or(""), 16)
            .map_err(|_| LauncherError::Update("release checksum is malformed".to_owned()))?;
    }
    Ok(result)
}

#[cfg(not(windows))]
fn extraction_command(archive: &Path, root: &Path) -> Command {
    let mut command = Command::new("tar");
    command.args(["-xzf"]).arg(archive).arg("-C").arg(root);
    command
}

#[cfg(windows)]
fn extraction_command(archive: &Path, root: &Path) -> Command {
    let mut command = Command::new("powershell");
    command
        .args([
            "-NoProfile",
            "-Command",
            "$ErrorActionPreference='Stop'; Expand-Archive -LiteralPath $env:PERITUS_ARCHIVE_SOURCE -DestinationPath $env:PERITUS_ARCHIVE_DESTINATION -Force",
        ])
        .env("PERITUS_ARCHIVE_SOURCE", archive)
        .env("PERITUS_ARCHIVE_DESTINATION", root);
    command
}

fn validate_bundle(bundle: &Path) -> Result<(), LauncherError> {
    if !bundle.is_dir() {
        return Err(LauncherError::Update(format!(
            "release archive omitted package directory {}",
            bundle.display()
        )));
    }
    let executable_suffix = if cfg!(windows) { ".exe" } else { "" };
    let installer_suffix = if cfg!(windows) { ".ps1" } else { ".sh" };
    for relative in [
        "manifest.toml".to_owned(),
        "SHA256SUMS".to_owned(),
        format!("bin/peritus{executable_suffix}"),
        format!("bin/peritusd{executable_suffix}"),
        format!("Install-Peritus{installer_suffix}"),
        format!("Upgrade-Peritus{installer_suffix}"),
    ] {
        if !bundle.join(&relative).is_file() {
            return Err(LauncherError::Update(format!(
                "release package omitted {relative}"
            )));
        }
    }
    Ok(())
}

fn asset_name() -> Result<String, LauncherError> {
    let platform = if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(windows) {
        "windows"
    } else {
        return Err(LauncherError::Update(
            "self-update is unsupported on this platform".to_owned(),
        ));
    };
    let architecture = match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        other => return Err(LauncherError::Update(format!(
            "self-update is unsupported on {other}"
        ))),
    };
    Ok(format!("peritus-{platform}-{architecture}{}", archive_suffix()))
}

const fn archive_suffix() -> &'static str { if cfg!(windows) { ".zip" } else { ".tar.gz" } }

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum UpdatePhase {
    Prepared,
    Downloading,
    Downloaded,
    OwnerPrepared,
    OwnerActive,
    Extracting,
    Extracted,
    Installing,
    InstallFailed,
    Installed,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct UpdateRecord {
    version: u8,
    operation_id: String,
    release: Release,
    asset_name: String,
    checksum_asset_name: String,
    archive_sha256: String,
    phase: UpdatePhase,
    acknowledged_bytes: u64,
    acknowledged_sha256: String,
    process_root_pid: Option<u32>,
    process_start_token: Option<u64>,
    process_group: Option<u32>,
    process_complete_containment: Option<bool>,
    owner_root_pid: Option<u32>,
    owner_start_token: Option<u64>,
    windows_job_identity: Option<String>,
    windows_job_name: Option<String>,
    failure: Option<String>,
}

impl UpdateRecord {
    fn new(
        release: Release,
        asset_name: &str,
        checksum_asset_name: &str,
        checksum: [u8; 32],
    ) -> Result<Self, LauncherError> {
        let mut operation = [0_u8; 16];
        getrandom::fill(&mut operation)
            .map_err(|error| LauncherError::Random(error.to_string()))?;
        if operation == [0; 16] { operation[0] = 1; }
        Ok(Self {
            version: RECEIPT_VERSION,
            operation_id: hex(&operation),
            release,
            asset_name: asset_name.to_owned(),
            checksum_asset_name: checksum_asset_name.to_owned(),
            archive_sha256: hex(&checksum),
            phase: UpdatePhase::Prepared,
            acknowledged_bytes: 0,
            acknowledged_sha256:
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                    .to_owned(),
            process_root_pid: None,
            process_start_token: None,
            process_group: None,
            process_complete_containment: None,
            owner_root_pid: None,
            owner_start_token: None,
            windows_job_identity: None,
            windows_job_name: None,
            failure: None,
        })
    }

    fn validate(&self) -> Result<(), LauncherError> {
        self.release.validate()?;
        let asset = self.asset()?;
        let checksum = self.checksum_asset()?;
        if self.version != RECEIPT_VERSION
            || self.operation_id.len() != 32
            || !self.operation_id.bytes().all(|byte| byte.is_ascii_hexdigit())
            || decode_hex(&self.archive_sha256).is_err()
            || decode_hex(&self.acknowledged_sha256).is_err()
            || checksum.size() > u64::try_from(MAX_CHECKSUM_BYTES).unwrap_or(u64::MAX)
            || self.acknowledged_bytes > asset.size()
        {
            return Err(LauncherError::Update("update receipt is malformed".to_owned()));
        }
        if matches!(
            self.phase,
            UpdatePhase::Downloaded
                | UpdatePhase::OwnerPrepared
                | UpdatePhase::OwnerActive
                | UpdatePhase::Extracting
                | UpdatePhase::Extracted
                | UpdatePhase::Installing
                | UpdatePhase::InstallFailed
                | UpdatePhase::Installed
        ) && (self.acknowledged_bytes != asset.size()
            || self.acknowledged_sha256 != self.archive_sha256)
        {
            return Err(LauncherError::Update(
                "completed update receipt has incomplete content identity".to_owned(),
            ));
        }
        let process_fields = [
            self.process_root_pid.is_some(),
            self.process_start_token.is_some(),
            self.process_complete_containment.is_some(),
        ];
        if process_fields.iter().any(|field| *field)
            && process_fields.iter().any(|field| !*field)
        {
            return Err(LauncherError::Update(
                "update helper process identity is incomplete".to_owned(),
            ));
        }
        let owner_fields = [
            self.owner_root_pid.is_some(),
            self.owner_start_token.is_some(),
            self.windows_job_identity.is_some(),
            self.windows_job_name.is_some(),
        ];
        if owner_fields.iter().any(|field| *field)
            && owner_fields.iter().any(|field| !*field)
        {
            return Err(LauncherError::Update(
                "Windows update owner identity is incomplete".to_owned(),
            ));
        }
        if let Some(identity) = &self.windows_job_identity {
            decode_hex(identity)?;
        }
        Ok(())
    }

    fn asset(&self) -> Result<&ReleaseAsset, LauncherError> { self.release.asset(&self.asset_name) }
    fn checksum_asset(&self) -> Result<&ReleaseAsset, LauncherError> {
        self.release.asset(&self.checksum_asset_name)
    }

    fn process_identity(&self) -> Result<Option<ProcessTreeIdentity>, LauncherError> {
        match (
            self.process_root_pid,
            self.process_start_token,
            self.process_complete_containment,
        ) {
            (None, None, None) => Ok(None),
            (Some(pid), Some(start), Some(complete)) => Ok(Some(ProcessTreeIdentity::new(
                pid,
                Some(start),
                self.process_group,
                complete,
            ))),
            _ => Err(LauncherError::Update(
                "update helper process identity is incomplete".to_owned(),
            )),
        }
    }

    fn windows_containment(
        &self,
    ) -> Result<Option<NativeWindowsContainmentIdentity>, LauncherError> {
        match (
            self.owner_root_pid,
            self.owner_start_token,
            &self.windows_job_identity,
            &self.windows_job_name,
        ) {
            (None, None, None, None) => Ok(None),
            (Some(pid), Some(start), Some(job), Some(name)) => {
                NativeWindowsContainmentIdentity::new(
                    peritus_types::Sha256Digest::new(decode_hex(job)?),
                    name.clone(),
                    ProcessTreeIdentity::new(pid, Some(start), None, true),
                )
                .map(Some)
                .map_err(|error| LauncherError::Update(format!(
                    "validate Windows update owner: {error}"
                )))
            }
            _ => Err(LauncherError::Update(
                "Windows update owner identity is incomplete".to_owned(),
            )),
        }
    }

    fn clear_process(&mut self) {
        self.process_root_pid = None;
        self.process_start_token = None;
        self.process_group = None;
        self.process_complete_containment = None;
    }

    fn clear_owner(&mut self) {
        self.owner_root_pid = None;
        self.owner_start_token = None;
        self.windows_job_identity = None;
        self.windows_job_name = None;
    }
}

fn reconcile_native_process(identity: ProcessTreeIdentity) -> Result<(), LauncherError> {
    let mut probe = NativeProcessProbe::new();
    match probe.observe(identity)
        .map_err(|error| LauncherError::Update(format!("observe interrupted update helper: {error}")))?
    {
        ProbeObservation::ExactLive => Err(LauncherError::Update(
            "the exact interrupted update helper is still running".to_owned(),
        )),
        ProbeObservation::Mismatched => Err(LauncherError::Update(
            "the interrupted update helper PID now names a different process".to_owned(),
        )),
        ProbeObservation::Unverifiable => Err(LauncherError::Update(
            "the interrupted update helper exact birth identity is unverifiable".to_owned(),
        )),
        ProbeObservation::ExactAbsent => {
            if identity.complete_containment()
                && probe.observe_quiescence(identity).map_err(|error| {
                    LauncherError::Update(format!(
                        "observe interrupted update helper containment: {error}"
                    ))
                })? != ProcessTreeQuiescence::Quiescent
            {
                return Err(LauncherError::Update(
                    "the interrupted update helper has unverified descendants".to_owned(),
                ));
            }
            Ok(())
        }
    }
}

#[cfg(windows)]
fn reconcile_windows_owner(
    containment: &NativeWindowsContainmentIdentity,
) -> Result<(), LauncherError> {
    match NativeWindowsProcessOwner::observe_durable(containment)
        .map_err(|error| LauncherError::Update(format!("observe Windows update owner: {error}")))?
    {
        ProbeObservation::ExactLive => Err(LauncherError::Update(
            "the exact Windows update owner is still running".to_owned(),
        )),
        ProbeObservation::Mismatched => Err(LauncherError::Update(
            "the Windows update owner PID now names a different process".to_owned(),
        )),
        ProbeObservation::Unverifiable => Err(LauncherError::Update(
            "the Windows update owner exact birth identity is unverifiable".to_owned(),
        )),
        ProbeObservation::ExactAbsent => {
            if NativeWindowsProcessOwner::observe_durable_quiescence(containment)
                .map_err(|error| LauncherError::Update(format!(
                    "observe Windows update owner containment: {error}"
                )))? != ProcessTreeQuiescence::Quiescent
            {
                return Err(LauncherError::Update(
                    "the Windows update owner Job is not verifiably quiescent".to_owned(),
                ));
            }
            Ok(())
        }
    }
}

struct UpdateLock { file: File }

impl UpdateLock {
    fn acquire(layout: &AppLayout) -> Result<Self, LauncherError> {
        let path = layout.update_effects_root().join("active.lock");
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&path)
            .map_err(|error| LauncherError::filesystem("open update operation lock", &path, error))?;
        crate::persistence::protect_file(&file, &path)?;
        fs4::FileExt::lock(&file)
            .map_err(|error| LauncherError::filesystem("lock update operation", &path, error))?;
        Ok(Self { file })
    }
}

impl Drop for UpdateLock {
    fn drop(&mut self) { let _ = fs4::FileExt::unlock(&self.file); }
}

fn active_record_path(layout: &AppLayout) -> PathBuf {
    layout.update_effects_root().join("active.json")
}

fn read_record_optional(path: &Path) -> Result<Option<UpdateRecord>, LauncherError> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(LauncherError::filesystem("inspect update receipt", path, error)),
    };
    if !metadata.is_file() || metadata.len() > MAX_RECEIPT_BYTES {
        return Err(LauncherError::Update(
            "durable update receipt is not a bounded regular file".to_owned(),
        ));
    }
    let bytes = fs::read(path)
        .map_err(|error| LauncherError::filesystem("read update receipt", path, error))?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| LauncherError::Update(format!("decode update receipt: {error}")))
}

fn read_record_required(path: &Path) -> Result<UpdateRecord, LauncherError> {
    read_record_optional(path)?.ok_or_else(|| {
        LauncherError::Update("required durable update receipt is absent".to_owned())
    })
}

fn encode_record(record: &UpdateRecord) -> Result<Vec<u8>, LauncherError> {
    serde_json::to_vec(record)
        .map_err(|error| LauncherError::Update(format!("encode update receipt: {error}")))
}

fn publish_record(path: &Path, record: &UpdateRecord) -> Result<(), LauncherError> {
    let bytes = encode_record(record)?;
    let actual = crate::persistence::read_exact_or_publish(path, &bytes)?;
    if actual == bytes {
        Ok(())
    } else {
        Err(LauncherError::Update(
            "durable update receipt was concurrently published with different content".to_owned(),
        ))
    }
}

fn replace_record(path: &Path, record: &UpdateRecord) -> Result<(), LauncherError> {
    crate::persistence::replace_recovery_file(path, &encode_record(record)?)
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

fn sync_directory(path: &Path) -> Result<(), LauncherError> {
    #[cfg(unix)]
    {
        File::open(path)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| LauncherError::filesystem("synchronize update staging", path, error))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

fn network(operation: &'static str, error: &reqwest::Error) -> LauncherError {
    LauncherError::Update(format!("{operation}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksums_are_exact_hex() {
        let digest = parse_checksum(
            b"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n",
        )
        .expect("checksum");
        assert_eq!(digest[0], 0x01);
        assert_eq!(digest[31], 0xef);
        for invalid in [
            b"abc".as_slice(),
            b"g123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        ] {
            assert!(parse_checksum(invalid).is_err());
        }
    }

    #[test]
    fn host_asset_matches_release_naming() {
        let asset = asset_name().expect("supported qualification platform");
        assert!(asset.starts_with("peritus-"));
        assert!(asset.ends_with(archive_suffix()));
    }
}
