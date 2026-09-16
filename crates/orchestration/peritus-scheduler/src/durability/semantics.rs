//! Live creation policy and immutable aggregate schema at the journal owner.

use peritus_codec::{CodecLimits, decode_frame};
use peritus_journal::DurableStateRecord;

use crate::{
    SchedulerCommand, SchedulerCommandKind, SchedulerError, SchedulerErrorKind, SchedulerSemantics,
};

pub(super) fn validate_append(
    command: &SchedulerCommand,
    current: Option<&DurableStateRecord>,
) -> Result<(), SchedulerError> {
    match current {
        None => {
            // Exact committed-command resolution runs before this check, so a
            // historical genesis retry keeps its existing durable result.
            if command.semantics() != SchedulerSemantics::StrictRecoveryQueueV2
                || !matches!(command.kind(), SchedulerCommandKind::StartScheduler { .. })
            {
                return Err(crate::error::reject(
                    SchedulerErrorKind::BindingMismatch,
                    "new durable scheduler aggregates require current-schema StartScheduler",
                ));
            }
        }
        Some(record) => {
            let frame = decode_frame(record.bytes(), CodecLimits::PRODUCTION)
                .map_err(super::codec_error)?;
            if frame.header().family() != 72 {
                return Err(super::binding_error(
                    "scheduler checkpoint has a different frame family",
                ));
            }
            let semantics =
                SchedulerSemantics::from_schema_version(frame.header().schema_version())
                    .ok_or_else(|| {
                        super::binding_error("scheduler checkpoint schema is unsupported")
                    })?;
            if !semantics.same(command.semantics()) {
                return Err(crate::error::reject(
                    SchedulerErrorKind::BindingMismatch,
                    "scheduler command schema differs from its durable checkpoint",
                ));
            }
        }
    }
    Ok(())
}
