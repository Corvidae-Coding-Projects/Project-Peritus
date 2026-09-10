//! Authenticated retained-file inspection, without opening source paths.
use super::{
    ActorId, AppResponsePayload, Code, ControlError, ProductRunService, WorkbenchFileMetadata,
    app_error, error_response,
};
use peritus_app_protocol::{
    ControlOperationId, WorkbenchFileMode, WorkbenchFilePage, WorkbenchFileQuery, WorkbenchFileRow,
};

impl ProductRunService {
    pub(crate) fn workbench_files(
        &self,
        actor: ActorId,
        query: WorkbenchFileQuery,
    ) -> AppResponsePayload {
        let result = (|| {
            let record = self.file_record(actor, query.query(), query.revision())?;
            let included = record.inputs().capture()?;
            let rows = record
                .files()
                .entries()
                .iter()
                .skip(query.offset() as usize)
                .take(32)
                .map(|entry| {
                    let source = entry.file();
                    let version = entry.current();
                    let metadata = version.observation();
                    let mode = match entry.mode() {
                        peritus_product_runner::control::FileMode::Snapshot => {
                            WorkbenchFileMode::Snapshot
                        }
                        peritus_product_runner::control::FileMode::RefreshOnRequest => {
                            WorkbenchFileMode::RefreshOnRequest
                        }
                    };
                    WorkbenchFileRow::new(
                        ControlOperationId::new(*source.operation().as_bytes())
                            .map_err(|_| app_error(Code::MalformedFrame))?,
                        ControlOperationId::new(*version.operation().as_bytes())
                            .map_err(|_| app_error(Code::MalformedFrame))?,
                        source.source().label().to_owned(),
                        mode,
                        WorkbenchFileMetadata::new(
                            metadata.source_digest(),
                            metadata.source_bytes(),
                            metadata.range(),
                            metadata.digest(),
                        )?,
                        entry.selected(),
                        entry.selected()
                            && included.included().iter().any(|input| input.id() == source.input()),
                    )
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| ControlError::InvalidInput)?;
            WorkbenchFilePage::new(
                query,
                rows,
                u32::try_from(record.files().entries().len())
                    .map_err(|_| ControlError::Capacity)?,
            )
            .map_err(|_| ControlError::InvalidInput.into())
        })();
        result.map_or_else(error_response, AppResponsePayload::WorkbenchFiles)
    }
}
