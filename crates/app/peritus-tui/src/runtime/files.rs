//! One owned bounded external-text read at a time; the event loop stays interactive.

use crate::{
    action::Action,
    file_import::{self, FileBytes},
};
use peritus_app_protocol::{ControlOperationId, WorkbenchFileRange};
use std::path::PathBuf;
use tokio::task::JoinSet;

#[derive(Default)]
pub(super) struct FileReads {
    jobs: JoinSet<(ControlOperationId, Result<FileBytes, &'static str>)>,
}

impl FileReads {
    pub(super) fn active(&self) -> bool {
        !self.jobs.is_empty()
    }

    pub(super) fn start(
        &mut self,
        operation: ControlOperationId,
        path: PathBuf,
        range: WorkbenchFileRange,
    ) -> bool {
        if self.active() {
            return false;
        }
        self.jobs.spawn_blocking(move || (operation, file_import::read(&path, range)));
        true
    }

    pub(super) async fn next(&mut self) -> Action {
        match self.jobs.join_next().await {
            Some(Ok((operation, result))) => Action::FileRead { operation, result },
            Some(Err(_)) | None => Action::FileReadFailed,
        }
    }
}
