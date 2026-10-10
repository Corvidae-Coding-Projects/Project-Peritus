//! Scoped, bounded, restart-retained preview output projection.

use super::super::super::PreviewAggregate;
use super::{
    ActorId, AppProtocolError, AppResponsePayload, Code, ControlOperationId, PreviewObservation,
    ProductRunService, WorkbenchResultQuery, app_error,
};
use peritus_app_protocol::{
    MAX_WORKBENCH_PREVIEW_OUTPUT_BYTES, WorkbenchPreviewOutput, WorkbenchPreviewOutputQuery,
    WorkbenchPreviewOutputRange, WorkbenchPreviewOutputStream, WorkbenchPreviewSnapshot,
};
use peritus_types::RunId;

use super::hex;

// Sixteen launches, two streams each, fit the four MiB durable aggregate bound.
const RETAINED_STREAM_BYTES: usize = 128 * 1024;

impl ProductRunService {
    pub(crate) fn workbench_preview_output_range(
        &self,
        actor: ActorId,
        query: WorkbenchPreviewOutputQuery,
    ) -> AppResponsePayload {
        self.preview_output_range(actor, query)
            .map_or_else(AppResponsePayload::Error, AppResponsePayload::WorkbenchPreviewOutput)
    }

    fn preview_output_range(
        &self,
        actor: ActorId,
        query: WorkbenchPreviewOutputQuery,
    ) -> Result<WorkbenchPreviewOutputRange, AppProtocolError> {
        let result_query = WorkbenchResultQuery::new(query.query(), query.run());
        let result = self.result_page(actor, result_query)?;
        let launch = result
            .launches()
            .iter()
            .find(|row| row.launch() == query.launch())
            .ok_or_else(|| app_error(Code::InvalidIdentifier))?;
        let process_id = launch.process().ok_or_else(|| app_error(Code::StaleRevision))?;
        let (workspace, direct) = self.verify_profile(query.query(), launch.profile())?;
        let active = self
            .inner
            .preview_processes
            .lock()
            .map_err(|_| app_error(Code::Backpressure))?
            .get(&query.launch())
            .cloned();
        let runtime =
            if let Some(active) = active.filter(|item| item.launch.process_id() == process_id) {
                active.runtime
            } else {
                let run = RunId::new(query.launch().into_bytes())
                    .map_err(|_| app_error(Code::Internal))?;
                let state =
                    self.preview_state_root().join("commands").join(hex(query.launch().as_bytes()));
                if direct {
                    peritus_product_runner::CommandRuntime::open_direct(
                        state,
                        workspace,
                        run,
                        self.inner.processes.clone(),
                    )
                } else {
                    peritus_product_runner::CommandRuntime::open(
                        state,
                        workspace,
                        run,
                        self.inner.processes.clone(),
                    )
                }
                .map_err(|_| app_error(Code::Backpressure))?
            };
        let stream = match query.stream() {
            WorkbenchPreviewOutputStream::Stdout => peritus_process::OutputStream::Stdout,
            WorkbenchPreviewOutputStream::Stderr => peritus_process::OutputStream::Stderr,
            WorkbenchPreviewOutputStream::Terminal => peritus_process::OutputStream::Terminal,
        };
        let range = runtime
            .preview_output_range(process_id, stream, query.offset(), query.maximum_bytes())
            .map_err(|_| app_error(Code::Backpressure))?;
        WorkbenchPreviewOutputRange::new(
            query.launch(),
            query.stream(),
            query.offset(),
            range.total_bytes(),
            range.digest(),
            range.bytes().to_vec(),
        )
    }

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
