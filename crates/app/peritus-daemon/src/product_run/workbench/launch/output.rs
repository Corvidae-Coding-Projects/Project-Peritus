//! Scoped, bounded, restart-retained preview output projection.

use super::super::super::PreviewAggregate;
use super::{
    ActorId, AppProtocolError, AppResponsePayload, Code, ControlOperationId, PreviewObservation,
    ProductRunService, WorkbenchResultQuery, app_error,
};
use peritus_app_protocol::{
    MAX_WORKBENCH_PREVIEW_OUTPUT_BYTES, WorkbenchPreviewOutput, WorkbenchPreviewSnapshot,
};

// Sixteen launches, two streams each, fit the four MiB durable aggregate bound.
const RETAINED_STREAM_BYTES: usize = 128 * 1024;

impl ProductRunService {
    pub(crate) fn workbench_preview(
        &self,
        actor: ActorId,
        query: WorkbenchResultQuery,
    ) -> AppResponsePayload {
        self.preview_snapshot(actor, query)
            .map_or_else(AppResponsePayload::Error, AppResponsePayload::WorkbenchPreview)
    }
    fn preview_snapshot(
        &self,
        actor: ActorId,
        query: WorkbenchResultQuery,
    ) -> Result<WorkbenchPreviewSnapshot, AppProtocolError> {
        let result = self.result_page(actor, query)?;
        let records = self.inner.records.read().map_err(|_| app_error(Code::Backpressure))?;
        let record = records.get(&query.run()).ok_or_else(|| app_error(Code::InvalidIdentifier))?;
        let mut outputs = Vec::new();
        for launch in result.launches() {
            let id = launch.launch();
            let stdout = record.preview.outputs.get(&id).map_or("", String::as_str);
            let stderr = record.preview.errors.get(&id).map_or("", String::as_str);
            outputs.push(WorkbenchPreviewOutput::new(
                id,
                tail(stdout, MAX_WORKBENCH_PREVIEW_OUTPUT_BYTES).to_owned(),
                tail(stderr, MAX_WORKBENCH_PREVIEW_OUTPUT_BYTES).to_owned(),
                record.preview.truncated.contains(&id)
                    || stdout.len() > MAX_WORKBENCH_PREVIEW_OUTPUT_BYTES
                    || stderr.len() > MAX_WORKBENCH_PREVIEW_OUTPUT_BYTES,
            )?);
        }
        WorkbenchPreviewSnapshot::new(result, outputs)
    }
}

pub(super) fn retain_output(
    preview: &mut PreviewAggregate,
    launch: ControlOperationId,
    observed: &PreviewObservation,
) {
    preview.outputs.insert(launch, tail(observed.stdout(), RETAINED_STREAM_BYTES).to_owned());
    preview.errors.insert(launch, tail(observed.stderr(), RETAINED_STREAM_BYTES).to_owned());
    if observed.stdout().len() > RETAINED_STREAM_BYTES
        || observed.stderr().len() > RETAINED_STREAM_BYTES
    {
        preview.truncated.insert(launch);
    }
}
pub(super) fn output_matches(
    preview: &PreviewAggregate,
    launch: ControlOperationId,
    observed: &PreviewObservation,
) -> bool {
    preview
        .outputs
        .get(&launch)
        .is_some_and(|value| value == tail(observed.stdout(), RETAINED_STREAM_BYTES))
        && preview
            .errors
            .get(&launch)
            .is_some_and(|value| value == tail(observed.stderr(), RETAINED_STREAM_BYTES))
}
fn tail(value: &str, bound: usize) -> &str {
    let mut start = value.len().saturating_sub(bound);
    while !value.is_char_boundary(start) {
        start += 1;
    }
    &value[start..]
}
