//! Run-owned sidecar: atomic publication, identity checks, and conservative scope rebinding.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read as _, Write as _},
    path::PathBuf,
};

use serde::{Deserialize, Serialize};

use super::{Capability, DeliveryReview, digest, invalid};
use crate::{ProductRunnerError, ProductRunnerErrorKind, execution::ProductRunInput};

const MAXIMUM_BYTES: u64 = 4 * 1024 * 1024;

pub struct Delivery {
    path: PathBuf,
    pub(super) state: State,
    pub(super) source: String,
    pub(super) requirements: Vec<String>,
    pub(super) revision: u64,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct State {
    version: u32,
    run: String,
    workspace: String,
    source: String,
    pub plan: Vec<Capability>,
    #[serde(default)]
    pub admitted: Vec<Capability>,
    pub scope_reviewed: bool,
    pub drafted: BTreeSet<String>,
    #[serde(default)]
    pub draft_evidence: BTreeMap<String, String>,
    pub cursor: usize,
    #[serde(default)]
    pub segments: BTreeMap<String, u64>,
    #[serde(default)]
    pub decomposition: Option<String>,
    pub debt: Vec<String>,
    pub qualification: Option<(String, DeliveryReview)>,
    #[serde(default)]
    pub qualification_revision: Option<u64>,
    /// Retained amendments never silently destroy the previous contract.
    previous: Vec<Capability>,
}

impl Delivery {
    pub(crate) fn recover_design(
        input: &ProductRunInput,
    ) -> Result<Option<crate::design::DesignDocument>, ProductRunnerError> {
        let path = input.trace_path.with_extension("delivery.json");
        let Some(state) = read_state(
            &path,
            &format!("{:?}", input.run_id),
            &format!("{:?}", input.workspace_id),
        )?
        else {
            return Ok(None);
        };
        if state.admitted.is_empty() {
            return Ok(None);
        }
        let Some(design) = crate::design::restore_admitted_delivery(input, &state.source) else {
            return Ok(None);
        };
        // Validate mappings and current authority before handing the frozen contract to a writer.
        let _ = Self::load(input, &design)?;
        Ok(Some(design))
    }

    pub(crate) fn load(
        input: &ProductRunInput,
        design: &crate::design::DesignDocument,
    ) -> Result<Self, ProductRunnerError> {
        let source = format!(
            "{}\n{}\n{}",
            input.task,
            input.conversation.reference_authority_context(),
            design.markdown()
        );
        let requirements = crate::execution::delivery_requirements(input)?;
        // Questions and status requests are not a scope reset. The accepted design carries
        // material amendments; every actual review still observes the live conversation.
        let source_digest = digest(format!("{}\n{}", input.task, design.markdown()).as_bytes());
        let run = format!("{:?}", input.run_id);
        let workspace = format!("{:?}", input.workspace_id);
        let path = input.trace_path.with_extension("delivery.json");
        let mut delivery = Self::open(path, run, workspace, source, source_digest, requirements)?;
        delivery.revision = input.conversation.revision();
        Ok(delivery)
    }

    pub(super) fn open(
        path: PathBuf,
        run: String,
        workspace: String,
        source: String,
        source_digest: String,
        requirements: Vec<String>,
    ) -> Result<Self, ProductRunnerError> {
        let mut state = read_state(&path, &run, &workspace)?.unwrap_or_else(|| State {
            version: 1,
            run,
            workspace,
            source: source_digest.clone(),
            plan: Vec::new(),
            admitted: Vec::new(),
            scope_reviewed: false,
            drafted: BTreeSet::new(),
            draft_evidence: BTreeMap::new(),
            cursor: 0,
            segments: BTreeMap::new(),
            decomposition: None,
            debt: Vec::new(),
            qualification: None,
            qualification_revision: None,
            previous: Vec::new(),
        });
        if state.source != source_digest {
            state.previous = std::mem::take(&mut state.plan);
            state.admitted.clear();
            state.source = source_digest;
            state.scope_reviewed = false;
            state.drafted.clear();
            state.draft_evidence.clear();
            state.cursor = 0;
            state.segments.clear();
            state.decomposition = None;
            state.qualification = None;
            state.debt = vec!["Scope changed: reconcile the preserved earlier plan with current user instructions; credit existing implementations by inspection instead of rewriting them.".to_owned()];
        }
        if !state.plan.is_empty() {
            super::contract::validate_plan(&state.plan, &source, &requirements)?;
        }
        if state.drafted.iter().any(|id| !state.plan.iter().any(|item| &item.id == id)) {
            return Err(storage("drafted capability is absent from its plan"));
        }
        if state.cursor >= state.plan.len().max(1)
            || (state.scope_reviewed && state.plan.is_empty())
        {
            return Err(storage("delivery queue or admitted scope is invalid"));
        }
        Ok(Self { path, state, source, requirements, revision: 1 })
    }

    pub(crate) fn save(&self) -> Result<(), ProductRunnerError> {
        let bytes = serde_json::to_vec(&self.state).map_err(storage)?;
        if bytes.len() as u64 > MAXIMUM_BYTES {
            return Err(storage("delivery state exceeds its storage bound"));
        }
        let parent = self.path.parent().ok_or_else(|| invalid("delivery state has no parent"))?;
        fs::create_dir_all(parent).map_err(storage)?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(storage)?;
        temporary.write_all(&bytes).map_err(storage)?;
        temporary.as_file().sync_all().map_err(storage)?;
        temporary.persist(&self.path).map_err(|error| storage(error.error))?;
        #[cfg(unix)]
        fs::File::open(parent).and_then(|file| file.sync_all()).map_err(storage)?;
        Ok(())
    }

    pub(super) fn previous(&self) -> &[Capability] {
        &self.state.previous
    }
}

fn read_state(
    path: &std::path::Path,
    run: &str,
    workspace: &str,
) -> Result<Option<State>, ProductRunnerError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.is_file()
                || metadata.file_type().is_symlink()
                || metadata.len() > MAXIMUM_BYTES
            {
                return Err(storage("delivery state is not a bounded regular file"));
            }
            let mut bytes = Vec::new();
            let mut file = fs::File::open(path).map_err(storage)?;
            let opened = file.metadata().map_err(storage)?;
            if !same_entry(&metadata, &opened) {
                return Err(storage("delivery state changed while opening"));
            }
            std::io::Read::by_ref(&mut file)
                .take(MAXIMUM_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(storage)?;
            if !same_entry(&opened, &file.metadata().map_err(storage)?)
                || !same_entry(&opened, &fs::symlink_metadata(path).map_err(storage)?)
            {
                return Err(storage("delivery state changed while reading"));
            }
            if bytes.len() as u64 > MAXIMUM_BYTES {
                return Err(storage("delivery state exceeds its storage bound"));
            }
            let state: State = serde_json::from_slice(&bytes).map_err(storage)?;
            if state.version != 1 || state.run != run || state.workspace != workspace {
                return Err(storage(
                    "delivery state belongs to another version, run, or workspace",
                ));
            }
            Ok(Some(state))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(storage(error)),
    }
}

fn storage(error: impl std::fmt::Display) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::Repository,
        "persist capability delivery",
        error.to_string(),
    )
}

fn same_entry(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    let regular = left.is_file()
        && right.is_file()
        && !right.file_type().is_symlink()
        && left.len() == right.len()
        && left.modified().ok() == right.modified().ok();
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        regular
            && left.dev() == right.dev()
            && left.ino() == right.ino()
            && left.ctime() == right.ctime()
            && left.ctime_nsec() == right.ctime_nsec()
    }
    #[cfg(not(unix))]
    regular
}
