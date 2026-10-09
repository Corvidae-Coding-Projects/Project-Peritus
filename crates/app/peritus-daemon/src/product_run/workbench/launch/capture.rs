//! Selected-window capture, artifact publication, and host capability discovery.

use super::*;

impl ProductRunService {
    #[allow(clippy::too_many_arguments, reason = "authenticated transfer bindings remain explicit")]
    pub(super) async fn capture_preview(
        &self,
        authority: &AuthorityHandle,
        actor: ActorId,
        session: SessionId,
        correlation: CorrelationId,
        maximum_chunk_bytes: usize,
        scope: ArtifactScope,
        run: RunId,
        command: &WorkbenchCommand,
        request: peritus_app_protocol::WorkbenchCaptureRequest,
    ) -> Result<WorkbenchReceipt, AppProtocolError> {
        let state = if request.consent() == WorkbenchCaptureConsent::Denied {
            WorkbenchCaptureState::Denied
        } else {
            WorkbenchCaptureState::Failed
        };
        let detail = if state == WorkbenchCaptureState::Denied {
            "capture denied; no backend action taken"
        } else {
            "capture admitted; completion not observed"
        };
        let (receipt, admitted) = self.admit_preview(command, run, |preview| {
            mutate_launch(preview, request.launch(), |current| {
                let mut captures = current.captures().to_vec();
                captures.push(capture_receipt(
                    command.operation(),
                    state,
                    request.target(),
                    detail,
                )?);
                rebuild_launch_with(
                    current,
                    current.interactions().to_vec(),
                    captures,
                    current.feedback().to_vec(),
                    current.behavior_checks(),
                )
            })
        })?;
        if !admitted {
            self.qualify_graphical_goal(run, command, request.launch())?;
            return Ok(receipt);
        }
        if state == WorkbenchCaptureState::Denied {
            return Ok(receipt);
        }
        let capture = self.capture_window(run, command.operation(), request)?;
        let published = self
            .publish_capture(
                authority,
                actor,
                session,
                correlation,
                maximum_chunk_bytes,
                scope,
                command.operation(),
                capture,
            )
            .await;
        if let Ok(completed) = published {
            self.update_capture(run, request.launch(), command.operation(), completed)?;
            self.qualify_graphical_goal(run, command, request.launch())?;
        }
        Ok(receipt)
    }

    pub(super) fn capture_window(
        &self,
        run: RunId,
        operation: ControlOperationId,
        request: peritus_app_protocol::WorkbenchCaptureRequest,
    ) -> Result<CapturedImage, AppProtocolError> {
        let import = self
            .inner
            .preview_capture
            .program
            .as_ref()
            .ok_or_else(|| app_error(Code::MissingRequiredFeature))?;
        let display = self
            .inner
            .preview_capture
            .display
            .clone()
            .ok_or_else(|| app_error(Code::MissingRequiredFeature))?;
        let WorkbenchCaptureTarget::X11Window(window) = request.target();
        let active = self.preview_process(request.launch())?;
        let directory = self.preview_state_root().join("captures");
        fs::create_dir_all(&directory).map_err(|_| app_error(Code::Backpressure))?;
        let path = directory.join(format!("{}.png", hex(operation.as_bytes())));
        let helper = PreviewCommand::new(
            import.to_string_lossy().into_owned(),
            vec![
                "-display".to_owned(),
                display,
                "-window".to_owned(),
                format!("0x{window:x}"),
                format!("png:{}", path.display()),
            ],
            self.inner
                .workspaces
                .get(&active_launch_run(self, run, request.launch())?)
                .cloned()
                .ok_or_else(|| app_error(Code::InvalidIdentifier))?,
            Duration::from_secs(10),
            false,
            24,
            80,
            format!("capture-{}", hex(operation.as_bytes())),
            Vec::new(),
        )
        .map_err(|_| app_error(Code::MalformedFrame))?;
        let observation = active
            .runtime
            .run_preview_helper(&helper)
            .map_err(|_| app_error(Code::Backpressure))?;
        if observation.state() != PreviewProcessState::Succeeded {
            return Err(app_error(Code::Backpressure));
        }
        let metadata = fs::metadata(&path).map_err(|_| app_error(Code::Backpressure))?;
        if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_CAPTURE_BYTES {
            return Err(app_error(Code::LimitExceeded));
        }
        let bytes = fs::read(&path).map_err(|_| app_error(Code::Backpressure))?;
        let _ = fs::remove_file(&path);
        let image = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
            .map_err(|_| app_error(Code::MalformedFrame))?;
        Ok(CapturedImage {
            digest: peritus_codec::sha256(&bytes),
            dimensions: image.dimensions(),
            bytes,
            captured_unix_millis: unix_millis()?,
        })
    }

    #[allow(clippy::too_many_arguments, reason = "artifact authority bindings remain explicit")]
    pub(super) async fn publish_capture(
        &self,
        authority: &AuthorityHandle,
        actor: ActorId,
        session: SessionId,
        correlation: CorrelationId,
        maximum_chunk_bytes: usize,
        scope: ArtifactScope,
        operation: ControlOperationId,
        capture: CapturedImage,
    ) -> Result<CompletedCapture, AppProtocolError> {
        let artifact = ArtifactId::new(derived_id(b"preview-capture", operation, capture.digest))
            .map_err(|_| app_error(Code::Internal))?;
        let transfer = TransferId::new(derived_id(b"preview-transfer", operation, capture.digest))
            .map_err(|_| app_error(Code::Internal))?;
        let size =
            u64::try_from(capture.bytes.len()).map_err(|_| app_error(Code::LimitExceeded))?;
        let preferred = u32::try_from(maximum_chunk_bytes.min(64 * 1_024))
            .map_err(|_| app_error(Code::LimitExceeded))?;
        let metadata = ArtifactMetadata::new(
            transfer,
            artifact,
            size,
            CanonicalMediaType::new("image/png".to_owned(), 255)
                .map_err(|_| app_error(Code::Internal))?,
            capture.digest,
            preferred,
            maximum_chunk_bytes,
        )
        .map_err(|_| app_error(Code::LimitExceeded))?;
        authority
            .begin_scoped_artifact_upload(actor, session, metadata, maximum_chunk_bytes, scope)
            .await
            .map_err(super::super::images::daemon_error)?;
        let upload = upload_capture_chunks(
            authority,
            actor,
            session,
            transfer,
            artifact,
            maximum_chunk_bytes,
            &capture.bytes,
        )
        .await;
        let upload = if upload.is_ok() {
            authority
                .complete_artifact_upload(
                    actor,
                    session,
                    ArtifactCompletion::new(transfer, artifact, size, capture.digest),
                )
                .await
                .map_err(super::super::images::daemon_error)
        } else {
            upload
        };
        if let Err(error) = upload {
            let _ = authority
                .cancel_artifact_transfer(
                    actor,
                    session,
                    ArtifactCancellation::new(transfer, artifact, correlation),
                )
                .await;
            return Err(error);
        }
        Ok(CompletedCapture {
            artifact,
            digest: capture.digest,
            dimensions: capture.dimensions,
            captured_unix_millis: capture.captured_unix_millis,
        })
    }

    pub(super) fn update_capture(
        &self,
        run: RunId,
        launch: ControlOperationId,
        operation: ControlOperationId,
        completed: CompletedCapture,
    ) -> Result<(), AppProtocolError> {
        self.update_launch(run, launch, |current| {
            let captures = current
                .captures()
                .iter()
                .map(|value| {
                    if value.operation() == operation {
                        WorkbenchCaptureReceipt::new(
                            operation,
                            WorkbenchCaptureState::Captured,
                            value.target(),
                            Some(completed.artifact),
                            Some(completed.digest),
                            Some(completed.dimensions),
                            Some(completed.captured_unix_millis),
                            text("selected window captured and published")?,
                        )
                    } else {
                        Ok(value.clone())
                    }
                })
                .collect::<Result<Vec<_>, AppProtocolError>>()?;
            rebuild_launch_with(
                current,
                current.interactions().to_vec(),
                captures,
                current.feedback().to_vec(),
                current.behavior_checks(),
            )
        })
    }
}

pub(super) struct CapturedImage {
    bytes: Vec<u8>,
    digest: Sha256Digest,
    dimensions: (u32, u32),
    captured_unix_millis: u64,
}

#[derive(Clone, Copy)]
pub(super) struct CompletedCapture {
    artifact: ArtifactId,
    digest: Sha256Digest,
    dimensions: (u32, u32),
    captured_unix_millis: u64,
}

pub(super) async fn upload_capture_chunks(
    authority: &AuthorityHandle,
    actor: ActorId,
    session: SessionId,
    transfer: TransferId,
    artifact: ArtifactId,
    maximum_chunk_bytes: usize,
    bytes: &[u8],
) -> Result<(), AppProtocolError> {
    for (ordinal, chunk) in bytes.chunks(maximum_chunk_bytes).enumerate() {
        let ordinal = u64::try_from(ordinal).map_err(|_| app_error(Code::LimitExceeded))?;
        let offset = ordinal
            .checked_mul(
                u64::try_from(maximum_chunk_bytes).map_err(|_| app_error(Code::LimitExceeded))?,
            )
            .ok_or_else(|| app_error(Code::LimitExceeded))?;
        let chunk = ArtifactChunk::new(
            transfer,
            artifact,
            ordinal,
            offset,
            chunk.to_vec(),
            maximum_chunk_bytes,
        )
        .map_err(|_| app_error(Code::MalformedFrame))?;
        authority
            .upload_artifact_chunk(actor, session, chunk)
            .await
            .map_err(super::super::images::daemon_error)?;
    }
    Ok(())
}

pub(super) fn capture_receipt(
    operation: ControlOperationId,
    state: WorkbenchCaptureState,
    target: WorkbenchCaptureTarget,
    detail: &str,
) -> Result<WorkbenchCaptureReceipt, AppProtocolError> {
    WorkbenchCaptureReceipt::new(operation, state, target, None, None, None, None, text(detail)?)
}

pub(super) const fn receipt(
    command: &WorkbenchCommand,
    accepted_revision: u64,
    digest: Sha256Digest,
) -> Result<WorkbenchReceipt, AppProtocolError> {
    WorkbenchReceipt::new(command.operation(), command.query(), accepted_revision, digest)
}

pub(super) fn capture_capability_for(
    host: &super::super::super::PreviewCaptureHost,
) -> WorkbenchCaptureCapability {
    if host.display.is_some() && host.program.is_some() {
        WorkbenchCaptureCapability::X11SelectedWindow
    } else {
        WorkbenchCaptureCapability::Unavailable(
            text("selected-window capture requires DISPLAY and ImageMagick import")
                .expect("static capability detail"),
        )
    }
}

pub(super) fn capture_program() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join("import"))
        .find(|candidate| candidate.is_file())
}

impl super::super::super::PreviewCaptureHost {
    pub(in crate::product_run) fn discover() -> Self {
        Self { display: std::env::var("DISPLAY").ok(), program: capture_program() }
    }
}

pub(super) fn derived_id(
    label: &[u8],
    operation: ControlOperationId,
    digest: Sha256Digest,
) -> [u8; 16] {
    let mut material = Vec::with_capacity(label.len() + 48);
    material.extend_from_slice(label);
    material.extend_from_slice(operation.as_bytes());

    material.extend_from_slice(digest.as_bytes());
    let hash = peritus_codec::sha256(&material);
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&hash.as_bytes()[..16]);
    bytes
}

pub(super) fn region_within(region: WorkbenchArtifactRegion, dimensions: (u32, u32)) -> bool {
    let (x, y, width, height) = region.coordinates();
    x.checked_add(width).is_some_and(|right| right <= dimensions.0)
        && y.checked_add(height).is_some_and(|bottom| bottom <= dimensions.1)
}

pub(super) fn unix_millis() -> Result<u64, AppProtocolError> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| app_error(Code::Internal))?
        .as_millis();
    u64::try_from(millis).map_err(|_| app_error(Code::Internal))
}

pub(super) fn hex(bytes: &[u8; 16]) -> String {
    bytes.iter().fold(String::with_capacity(32), |mut text, byte| {
        use core::fmt::Write as _;
        let _ = write!(text, "{byte:02x}");
        text
    })
}
