//! Explicit selection of host-observed files; launch always rechecks these identities.
use super::{
    AppModel, WorkbenchLaunchSource, WorkbenchLaunchSourceKind, WorkbenchLaunchText,
    WorkbenchResultQuery,
};
use peritus_app_protocol::{AppProtocolError, WorkbenchBuildIdentity, WorkbenchQuery};
use peritus_types::Sha256Digest;

pub(super) struct Selection<'a> {
    pub(super) executable: &'a str,
    pub(super) arguments: Vec<&'a str>,
    build: Option<&'a str>,
    source: Option<&'a str>,
}

impl<'a> Selection<'a> {
    pub(super) fn parse(value: &'a str) -> Option<Self> {
        let mut words = value.split_whitespace();
        let mut build = None;
        let mut source = None;
        let executable = loop {
            match words.next()? {
                "--build" if build.is_none() => build = Some(words.next()?),
                "--source" if source.is_none() => source = Some(words.next()?),
                "--" => break words.next()?,
                word if !word.starts_with("--") => break word,
                _ => return None,
            }
        };
        Some(Self { executable, arguments: words.collect(), build, source })
    }
}

impl AppModel {
    pub(super) fn preview_source_identity(
        &self,
        query: WorkbenchResultQuery,
        selection: &Selection<'_>,
    ) -> Result<(WorkbenchLaunchSource, Option<WorkbenchBuildIdentity>), AppProtocolError> {
        let missing =
            || AppProtocolError::new(peritus_app_protocol::AppErrorCode::StaleRevision, None);
        let build = selection
            .build
            .map(|path| {
                Ok::<_, AppProtocolError>(WorkbenchBuildIdentity::new(
                    WorkbenchLaunchText::new(path.to_owned())?,
                    self.observed_preview_file(query.query(), path).ok_or_else(missing)?,
                ))
            })
            .transpose()?;
        let source = if self.direct_folder_chat() == Some(true) {
            let path = selection.source.ok_or_else(missing)?;
            WorkbenchLaunchSource::new(
                WorkbenchLaunchSourceKind::PlainFolderFile,
                WorkbenchLaunchText::new(path.to_owned())?,
                self.observed_preview_file(query.query(), path).ok_or_else(missing)?,
            )
        } else {
            if selection.source.is_some() {
                return Err(missing());
            }
            let digest = self
                .product
                .as_ref()
                .and_then(super::super::ProductUi::selected_settlement)
                .and_then(peritus_run_settlement::RunSettlement::checkpoint)
                .map(|checkpoint| checkpoint.identity().candidate_digest())
                .ok_or_else(missing)?;
            WorkbenchLaunchSource::new(
                WorkbenchLaunchSourceKind::ManagedCandidate,
                WorkbenchLaunchText::new(".".to_owned())?,
                digest,
            )
        };
        Ok((source, build))
    }

    fn observed_preview_file(&self, query: WorkbenchQuery, path: &str) -> Option<Sha256Digest> {
        let files = &self.chat.workbench.files;
        if let Some(preview) = files.preview.as_ref().filter(|preview| {
            preview.request().query() == query && preview.request().path() == path
        }) {
            return Some(preview.file().source_digest());
        }
        files
            .page
            .as_ref()
            .filter(|page| page.query().query() == query)?
            .rows()
            .iter()
            .find(|row| row.label() == path)
            .map(|row| row.file().source_digest())
    }
}
