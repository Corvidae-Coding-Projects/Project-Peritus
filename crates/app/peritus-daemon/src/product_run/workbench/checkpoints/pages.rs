//! Frame-sized, stateless checkpoint coverage pages.

use super::{
    ActorId, CheckpointId, ControlError, ControlOperationId, ConversationId, Error,
    ProductRunService, UserCheckpoint, WorkbenchCheckpointCoveragePage,
    WorkbenchCheckpointPageRequest, WorkbenchCheckpointReceipt, WorkbenchCoverageCursor,
    WorkbenchCoverageSection, WorkbenchRewindConfirmation, WorkbenchRewindCoveragePage,
    WorkbenchRewindPageRequest, WorkbenchRewindPreview, WorkbenchRewindRequest, check_record,
    error_response, public_checkpoint,
};
use peritus_app_protocol::{
    AppErrorCode, AppMessage, AppProtocolError, AppProtocolLimits, AppRequestEnvelope,
    AppResponseEnvelope, AppResponsePayload,
};

impl ProductRunService {
    pub(super) fn checkpoint_receipt(
        &self,
        actor: ActorId,
        request: WorkbenchRewindRequest,
    ) -> Result<WorkbenchCheckpointReceipt, Error> {
        self.control_workspace(request.query())?;
        let conversation = ConversationId::new(request.query().conversation().into_bytes())?;
        self.with_controls(false, |store| {
            let record = store.load(conversation)?.ok_or(ControlError::NotFound)?;
            check_record(&record, actor, request.query(), Some(request.revision()))?;
            let checkpoint = load_checkpoint(store, conversation, request.checkpoint())?;
            let operation = store
                .operation(
                    conversation,
                    peritus_product_runner::control::OperationId::new(
                        request.checkpoint().into_bytes(),
                    )?,
                )?
                .ok_or(ControlError::NotFound)?;
            let receipt = store.resolve(&operation)?.ok_or(ControlError::NotFound)?;
            public_checkpoint(request.query(), receipt.accepted_revision(), &checkpoint)
        })
    }

    pub(crate) fn checkpoint_coverage_page(
        &self,
        actor: ActorId,
        page_request: WorkbenchCheckpointPageRequest,
        envelope: &AppRequestEnvelope,
        limits: AppProtocolLimits,
    ) -> AppResponsePayload {
        let query = page_request.query();
        let result = self.control_workspace(query).and_then(|()| {
            let conversation = ConversationId::new(query.conversation().into_bytes())?;
            self.with_controls(false, |store| {
                let record = store.load(conversation)?.ok_or(ControlError::NotFound)?;
                check_record(&record, actor, query, Some(page_request.revision()))?;
                let checkpoint = load_checkpoint(store, conversation, page_request.checkpoint())?;
                let operation = store
                    .operation(
                        conversation,
                        peritus_product_runner::control::OperationId::new(
                            page_request.checkpoint().into_bytes(),
                        )?,
                    )?
                    .ok_or(ControlError::NotFound)?;
                let receipt = store.resolve(&operation)?.ok_or(ControlError::NotFound)?;
                public_checkpoint(query, receipt.accepted_revision(), &checkpoint)
            })
        });
        result.map_or_else(error_response, |receipt| {
            build_checkpoint_page(&receipt, page_request, envelope, limits)
                .map_or_else(AppResponsePayload::Error, AppResponsePayload::WorkbenchCheckpointPage)
        })
    }

    pub(crate) fn rewind_coverage_page(
        &self,
        actor: ActorId,
        page_request: WorkbenchRewindPageRequest,
        envelope: &AppRequestEnvelope,
        limits: AppProtocolLimits,
    ) -> AppResponsePayload {
        let request = page_request.request();
        let result = self.control_workspace(request.query()).and_then(|()| {
            let conversation = ConversationId::new(request.query().conversation().into_bytes())?;
            self.with_controls(false, |store| {
                let record = store.load(conversation)?.ok_or(ControlError::NotFound)?;
                check_record(&record, actor, request.query(), Some(request.revision()))?;
                if record.restores().iter().any(|restore| {
                    matches!(
                        restore.status(),
                        peritus_product_runner::control::RestoreStatus::Prepared
                            | peritus_product_runner::control::RestoreStatus::RecoveryRequired
                    )
                }) {
                    return Err(Error::Corrupt(
                        "a prepared rewind requires recovery before another preview",
                    ));
                }
                let checkpoint = load_checkpoint(store, conversation, request.checkpoint())?;
                let operation = store
                    .operation(
                        conversation,
                        peritus_product_runner::control::OperationId::new(
                            request.checkpoint().into_bytes(),
                        )?,
                    )?
                    .ok_or(ControlError::NotFound)?;
                let receipt = store.resolve(&operation)?.ok_or(ControlError::NotFound)?;
                public_checkpoint(request.query(), receipt.accepted_revision(), &checkpoint)
            })
        });
        let result = result.and_then(|receipt| {
            self.rewind_preview(actor, request).map(|preview| (receipt, preview))
        });
        result.map_or_else(error_response, |(receipt, preview)| {
            WorkbenchRewindConfirmation::for_preview(request, &receipt, &preview)
                .map_err(|_| AppProtocolError::new(AppErrorCode::StaleRevision, None))
                .and_then(|confirmation| {
                    build_rewind_page(
                        &preview,
                        confirmation,
                        page_request.cursor(),
                        envelope,
                        limits,
                    )
                })
                .map_or_else(AppResponsePayload::Error, AppResponsePayload::WorkbenchRewindPage)
        })
    }

    pub(crate) fn first_checkpoint_coverage_page(
        &self,
        receipt: &WorkbenchCheckpointReceipt,
        envelope: &AppRequestEnvelope,
        limits: AppProtocolLimits,
    ) -> AppResponsePayload {
        let request = match WorkbenchCheckpointPageRequest::new(
            receipt.query(),
            receipt.accepted_revision(),
            receipt.checkpoint(),
            None,
        ) {
            Ok(request) => request,
            Err(error) => return AppResponsePayload::Error(error),
        };
        build_checkpoint_page(receipt, request, envelope, limits)
            .map_or_else(AppResponsePayload::Error, AppResponsePayload::WorkbenchCheckpointPage)
    }
}

fn load_checkpoint(
    store: &super::ControlStore,
    conversation: ConversationId,
    checkpoint: ControlOperationId,
) -> Result<UserCheckpoint, Error> {
    store
        .load_checkpoint(conversation, CheckpointId::new(checkpoint.into_bytes())?)?
        .ok_or_else(|| ControlError::NotFound.into())
}

fn build_checkpoint_page(
    receipt: &WorkbenchCheckpointReceipt,
    request: WorkbenchCheckpointPageRequest,
    envelope: &AppRequestEnvelope,
    limits: AppProtocolLimits,
) -> Result<WorkbenchCheckpointCoveragePage, AppProtocolError> {
    let fingerprint =
        receipt.page_fingerprint(request.revision()).map_err(AppProtocolError::from_codec)?;
    let totals = [
        u64::try_from(receipt.paths().len())
            .map_err(|_| AppProtocolError::new(AppErrorCode::LimitExceeded, None))?,
        u64::try_from(receipt.exclusions().len())
            .map_err(|_| AppProtocolError::new(AppErrorCode::LimitExceeded, None))?,
        u64::try_from(receipt.external_effects().len())
            .map_err(|_| AppProtocolError::new(AppErrorCode::LimitExceeded, None))?,
    ];
    let (section, offset) = start_position(request.cursor(), fingerprint, totals)?;
    let available = usize::try_from(section_total(section, totals).saturating_sub(offset))
        .unwrap_or(usize::MAX)
        .min(limits.codec().max_collection_items);
    sized_page(
        available,
        |count| {
            let end = offset
                + u64::try_from(count)
                    .map_err(|_| AppProtocolError::new(AppErrorCode::LimitExceeded, None))?;
            let next = next_cursor(section, end, totals, fingerprint);
            let range = usize::try_from(offset)
                .map_err(|_| AppProtocolError::new(AppErrorCode::LimitExceeded, None))?
                ..usize::try_from(end)
                    .map_err(|_| AppProtocolError::new(AppErrorCode::LimitExceeded, None))?;
            let (paths, exclusions, effects) = match section {
                WorkbenchCoverageSection::Paths => {
                    (receipt.paths()[range].to_vec(), Vec::new(), Vec::new())
                }
                WorkbenchCoverageSection::Exclusions => {
                    (Vec::new(), receipt.exclusions()[range].to_vec(), Vec::new())
                }
                WorkbenchCoverageSection::ExternalEffects => {
                    (Vec::new(), Vec::new(), receipt.external_effects()[range].to_vec())
                }
            };
            WorkbenchCheckpointCoveragePage::new(
                receipt,
                request.revision(),
                fingerprint,
                totals[0],
                totals[1],
                totals[2],
                section,
                offset,
                paths,
                exclusions,
                effects,
                next,
            )
        },
        |page| {
            response_fits(
                envelope,
                limits,
                AppResponsePayload::WorkbenchCheckpointPage(page.clone()),
            )
        },
    )
}

fn build_rewind_page(
    preview: &WorkbenchRewindPreview,
    confirmation: WorkbenchRewindConfirmation,
    cursor: Option<WorkbenchCoverageCursor>,
    envelope: &AppRequestEnvelope,
    limits: AppProtocolLimits,
) -> Result<WorkbenchRewindCoveragePage, AppProtocolError> {
    let fingerprint = confirmation.preview_digest();
    let totals = [
        u64::try_from(preview.paths().len())
            .map_err(|_| AppProtocolError::new(AppErrorCode::LimitExceeded, None))?,
        u64::try_from(preview.exclusions().len())
            .map_err(|_| AppProtocolError::new(AppErrorCode::LimitExceeded, None))?,
        u64::try_from(preview.external_effects().len())
            .map_err(|_| AppProtocolError::new(AppErrorCode::LimitExceeded, None))?,
    ];
    let (section, offset) = start_position(cursor, fingerprint, totals)?;
    let available = usize::try_from(section_total(section, totals).saturating_sub(offset))
        .unwrap_or(usize::MAX)
        .min(limits.codec().max_collection_items);
    sized_page(
        available,
        |count| {
            let end = offset
                + u64::try_from(count)
                    .map_err(|_| AppProtocolError::new(AppErrorCode::LimitExceeded, None))?;
            let next = next_cursor(section, end, totals, fingerprint);
            let range = usize::try_from(offset)
                .map_err(|_| AppProtocolError::new(AppErrorCode::LimitExceeded, None))?
                ..usize::try_from(end)
                    .map_err(|_| AppProtocolError::new(AppErrorCode::LimitExceeded, None))?;
            let (paths, exclusions, effects) = match section {
                WorkbenchCoverageSection::Paths => {
                    (preview.paths()[range].to_vec(), Vec::new(), Vec::new())
                }
                WorkbenchCoverageSection::Exclusions => {
                    (Vec::new(), preview.exclusions()[range].to_vec(), Vec::new())
                }
                WorkbenchCoverageSection::ExternalEffects => {
                    (Vec::new(), Vec::new(), preview.external_effects()[range].to_vec())
                }
            };
            WorkbenchRewindCoveragePage::new(
                confirmation,
                totals[0],
                totals[1],
                totals[2],
                section,
                offset,
                paths,
                exclusions,
                effects,
                next,
            )
        },
        |page| {
            response_fits(envelope, limits, AppResponsePayload::WorkbenchRewindPage(page.clone()))
        },
    )
}

fn sized_page<T>(
    available: usize,
    mut create: impl FnMut(usize) -> Result<T, AppProtocolError>,
    fits: impl Fn(&T) -> bool,
) -> Result<T, AppProtocolError> {
    if available == 0 {
        let page =
            create(0).map_err(|_| AppProtocolError::new(AppErrorCode::LimitExceeded, None))?;
        return if fits(&page) {
            Ok(page)
        } else {
            Err(AppProtocolError::new(AppErrorCode::LimitExceeded, None))
        };
    }
    let (mut low, mut high, mut best) = (1, available, None);
    while low <= high {
        let count = low + (high - low) / 2;
        let page = create(count)?;
        if fits(&page) {
            best = Some(page);
            low = count.saturating_add(1);
        } else {
            high = count.saturating_sub(1);
        }
    }
    best.ok_or_else(|| AppProtocolError::new(AppErrorCode::LimitExceeded, None))
}

fn response_fits(
    request: &AppRequestEnvelope,
    limits: AppProtocolLimits,
    payload: AppResponsePayload,
) -> bool {
    let response = AppResponseEnvelope::new(
        request.context(),
        request.request_id(),
        request.correlation_id(),
        payload,
    );
    peritus_app_protocol::encode_app_message(&AppMessage::Response(response), limits).is_ok()
}

pub(super) fn restore_summary_fits(
    request: &AppRequestEnvelope,
    limits: AppProtocolLimits,
    summary: &peritus_app_protocol::WorkbenchRestoreSummary,
) -> bool {
    response_fits(request, limits, AppResponsePayload::WorkbenchRestoreSummary(summary.clone()))
}

fn start_position(
    cursor: Option<WorkbenchCoverageCursor>,
    fingerprint: peritus_types::Sha256Digest,
    totals: [u64; 3],
) -> Result<(WorkbenchCoverageSection, u64), AppProtocolError> {
    if let Some(cursor) = cursor {
        let offset = section_index(cursor.section());
        if cursor.fingerprint() != fingerprint
            || offset >= totals.len()
            || cursor.offset() >= totals[offset]
        {
            return Err(AppProtocolError::new(AppErrorCode::StaleRevision, None));
        }
        Ok((cursor.section(), cursor.offset()))
    } else {
        Ok(totals
            .iter()
            .position(|count| *count > 0)
            .map_or((WorkbenchCoverageSection::Paths, 0), |index| (section_at(index), 0)))
    }
}

#[cfg(test)]
#[path = "pages/tests.rs"]
mod tests;

fn next_cursor(
    section: WorkbenchCoverageSection,
    end: u64,
    totals: [u64; 3],
    fingerprint: peritus_types::Sha256Digest,
) -> Option<WorkbenchCoverageCursor> {
    let index = section_index(section);
    if end < totals[index] {
        return Some(WorkbenchCoverageCursor::new(section, end, fingerprint));
    }
    (index + 1..totals.len())
        .find(|next| totals[*next] > 0)
        .map(|next| WorkbenchCoverageCursor::new(section_at(next), 0, fingerprint))
}

const fn section_total(section: WorkbenchCoverageSection, totals: [u64; 3]) -> u64 {
    totals[section_index(section)]
}

const fn section_index(section: WorkbenchCoverageSection) -> usize {
    match section {
        WorkbenchCoverageSection::Paths => 0,
        WorkbenchCoverageSection::Exclusions => 1,
        WorkbenchCoverageSection::ExternalEffects => 2,
    }
}

const fn section_at(index: usize) -> WorkbenchCoverageSection {
    match index {
        0 => WorkbenchCoverageSection::Paths,
        1 => WorkbenchCoverageSection::Exclusions,
        _ => WorkbenchCoverageSection::ExternalEffects,
    }
}
