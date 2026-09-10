//! One owned bounded file read at a time; the event loop stays interactive.

use crate::{
    action::Action,
    image_import::{self, ImageBytes},
};
use peritus_app_protocol::ControlOperationId;
use std::path::PathBuf;
use tokio::task::JoinSet;

#[derive(Default)]
pub(super) struct ImageReads {
    jobs: JoinSet<(ControlOperationId, Result<ImageBytes, &'static str>)>,
}
impl ImageReads {
    pub(super) fn active(&self) -> bool {
        !self.jobs.is_empty()
    }
    pub(super) fn start(&mut self, operation: ControlOperationId, path: PathBuf) -> bool {
        if self.active() {
            return false;
        }
        self.jobs.spawn_blocking(move || (operation, image_import::read(&path)));
        true
    }
    pub(super) async fn next(&mut self) -> Action {
        match self.jobs.join_next().await {
            Some(Ok((operation, result))) => Action::ImageRead { operation, result },
            Some(Err(_)) | None => Action::ImageReadFailed,
        }
    }
}
