//! Durable explicit discard recovery; inspection never publishes restore effects.

#[cfg(test)]
pub(in crate::candidate::managed) mod fault;
mod owned;
mod state;
#[cfg(test)]
mod tests;
mod validation;

use super::{ManagedBaseline, capture, failure, recovery};
use crate::{DiscardTransactionState, ProductRunnerError, progress::WorkspaceCheckpoint};
use state::{Phase, Plan, Recovery, State};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use owned::Asset;
pub(in crate::candidate::managed) use owned::{Kind, Lock, own_directory};
pub(in crate::candidate::managed) use validation::directory_digest as validation_directory_digest;
pub(in crate::candidate::managed) use validation::{RootFact, projected_fact};

pub fn prepare(
    root: &Path,
    baseline: ManagedBaseline,
    paths: Vec<PathBuf>,
    path: &Path,
    binding: [u8; 32],
    candidate: [u8; 32],
) -> Result<[u8; 32], ProductRunnerError> {
    let _owner = Owner::acquire(path)?;
    let root = root.canonicalize().map_err(failure)?;
    if let Some(state) = state::read(path)? {
        state.plan.validate(binding)?;
        if state.phase != Phase::Prepared
            || state.plan.root != root
            || state.plan.baseline != baseline
            || state.plan.paths != paths
            || state.plan.candidate != candidate
        {
            return Err(failure(
                "existing discard intent requires its original recovery authority",
            ));
        }
        return state.plan.digest();
    }
    let head = capture::nested_head(&root)?
        .ok_or_else(|| failure("discard workspace has no committed HEAD"))?;
    let before = ManagedBaseline::capture(&root, true)?;
    if validation::candidate_digest(&before, &head)? != candidate
        || WorkspaceCheckpoint::capture(&root)?.digest().into_bytes() != candidate
    {
        return Err(failure("workspace changed before discard intent could be retained"));
    }
    let roots = validation::roots(&root, &before, &baseline)?;
    let owners = validation::owners(&root, &roots, &baseline)?;
    let plan =
        Plan { version: 1, binding, root, candidate, head, baseline, before, roots, owners, paths };
    plan.validate(binding)?;
    let digest = plan.digest()?;
    state::save(
        path,
        &State {
            plan,
            phase: Phase::Prepared,
            assets: Vec::new(),
            heads: BTreeMap::new(),
            recovery: Vec::new(),
        },
    )?;
    Ok(digest)
}

pub fn inspect(
    path: &Path,
    binding: [u8; 32],
    digest: [u8; 32],
) -> Result<Option<DiscardTransactionState>, ProductRunnerError> {
    let Some(state) = state::read(path)? else { return Ok(None) };
    validate(&state, binding, digest)?;
    Ok(Some(match state.phase {
        Phase::Prepared => DiscardTransactionState::Prepared,
        Phase::Restoring => DiscardTransactionState::Restoring,
        Phase::Completed => DiscardTransactionState::Completed(recovery_paths(&state)),
    }))
}

pub fn execute(
    path: &Path,
    binding: [u8; 32],
    digest: [u8; 32],
) -> Result<Vec<PathBuf>, ProductRunnerError> {
    let owner = Owner::acquire(path)?;
    let state = state::read(path)?.ok_or_else(|| failure("discard intent was not retained"))?;
    validate(&state, binding, digest)?;
    if state.phase == Phase::Completed {
        return Ok(recovery_paths(&state));
    }
    let mut journal = Journal { path: path.to_path_buf(), state, _owner: owner };
    journal.cleanup_owned()?;
    validation::transition(&journal.state)?;
    journal.state.assets.clear();
    journal.save()?;
    let baseline = journal.state.plan.baseline.clone();
    let root = journal.state.plan.root.clone();
    let paths = journal.state.plan.paths.clone();
    baseline.discard_observed(&root, &paths, Some(&mut journal))?;
    validation::postimages(&journal.state)?;
    journal.state.phase = Phase::Completed;
    journal.save()?;
    #[cfg(test)]
    fault::pause(fault::Stage::Completed);
    Ok(recovery_paths(&journal.state))
}

fn validate(state: &State, binding: [u8; 32], digest: [u8; 32]) -> Result<(), ProductRunnerError> {
    state.plan.validate(binding)?;
    if state.plan.digest()? != digest {
        return Err(failure("discard intent digest does not match its receipt"));
    }
    for asset in &state.assets {
        asset.validate(&state.plan)?;
    }
    validation::progress(state)
}

fn recovery_paths(state: &State) -> Vec<PathBuf> {
    state
        .recovery
        .iter()
        .filter(|entry| entry.source.is_none() || entry.path.join("repository").is_dir())
        .map(|entry| entry.path.clone())
        .collect()
}

pub(in crate::candidate::managed) struct Journal {
    path: PathBuf,
    state: State,
    _owner: Owner,
}

impl Journal {
    fn save(&self) -> Result<(), ProductRunnerError> {
        state::save(&self.path, &self.state)
    }

    pub fn register(&mut self, asset: Asset) -> Result<(), ProductRunnerError> {
        asset.validate(&self.state.plan)?;
        self.state.assets.push(asset);
        self.save()
    }

    pub fn seal_directory(&mut self, path: &Path) -> Result<(), ProductRunnerError> {
        let asset = self
            .state
            .assets
            .iter_mut()
            .find(|asset| asset.path == path)
            .ok_or_else(|| failure("discard staging has no ownership record"))?;
        asset.seal()?;
        self.save()
    }

    pub fn cleanup_directory(&self, path: &Path) -> Result<(), ProductRunnerError> {
        let asset = self
            .state
            .assets
            .iter()
            .find(|asset| asset.path == path)
            .ok_or_else(|| failure("discard staging has no ownership record"))?;
        asset.cleanup(&self.state.plan)
    }

    pub fn restoring(&mut self) -> Result<(), ProductRunnerError> {
        self.state.phase = Phase::Restoring;
        self.save()
    }

    pub fn head(&mut self, root: PathBuf, fact: RootFact) -> Result<(), ProductRunnerError> {
        validation::head_target(&self.state.plan, &root, &fact)?;
        self.state.heads.insert(root, fact);
        self.save()
    }

    pub fn recovery(
        &mut self,
        path: PathBuf,
        source: Option<PathBuf>,
        directory_digest: Option<[u8; 32]>,
    ) -> Result<(), ProductRunnerError> {
        validation::recovery_path(&self.state.plan, &path)?;
        self.state.recovery.push(Recovery { path, source, directory_digest });
        self.save()
    }

    fn cleanup_owned(&self) -> Result<(), ProductRunnerError> {
        for asset in &self.state.assets {
            asset.check(&self.state.plan)?;
        }
        for asset in &self.state.assets {
            asset.cleanup(&self.state.plan)?;
        }
        Ok(())
    }
}

struct Owner(fs::File);

impl Owner {
    fn acquire(path: &Path) -> Result<Self, ProductRunnerError> {
        let path = path.with_extension("discard-owner");
        if let Some(metadata) = state::ordinary_metadata(&path)?
            && metadata.len() != 0
        {
            return Err(failure("discard ownership file contains foreign data"));
        }
        let owner = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(failure)?;
        if !owner.metadata().map_err(failure)?.is_file() {
            return Err(failure("discard ownership is not an ordinary file"));
        }
        owner.try_lock().map_err(failure)?;
        Ok(Self(owner))
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        if let Err(error) = self.0.unlock() {
            use std::io::Write as _;
            let _ = std::io::stderr()
                .lock()
                .write_all(format!("discard ownership unlock failed: {error}\n").as_bytes());
        }
    }
}
