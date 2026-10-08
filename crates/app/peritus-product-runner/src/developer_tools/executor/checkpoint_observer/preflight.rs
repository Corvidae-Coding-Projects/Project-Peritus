//! Awaitable checkpoint admission retains exact preimages across resource waits.

use super::{
    CheckpointFileVersion, DeveloperLoopError, ToolCheckpointBoundary, WorkspaceDeveloperTools,
    WorkspaceMutationKind, checked, postimage::exact_file_receipt, tool,
};
use peritus_workspace::FolderIdentity;
use serde_json::Value;
use std::path::PathBuf;

#[derive(Eq, PartialEq)]
struct TargetWitness {
    root: FolderIdentity,
    targets: Vec<TargetVersion>,
}

#[derive(Eq, PartialEq)]
enum TargetVersion {
    File(CheckpointFileVersion),
    EmptyDirectory(FolderIdentity),
}

impl WorkspaceDeveloperTools {
    pub(in crate::developer_tools::executor) async fn prepare_effect_checkpoint_async(
        &mut self,
        name: &str,
        arguments: &Value,
    ) -> Result<(), DeveloperLoopError> {
        self.prepare_checkpoint_targets(name, arguments)?;
        let targets = self.checkpoint_targets.clone();
        let mutations = self.prepared_mutations.clone();
        let removal = self.prepared_removal.clone();
        let witness = if self.checkpoint_observer.is_some() && !targets.is_empty() {
            Some(inspect_targets(self.root.clone(), targets.clone()).await?)
        } else {
            None
        };
        if let Some(observer) = &self.checkpoint_observer {
            for (path, kind) in &targets {
                if let Some(view) = &self.checkpoint_view {
                    view.checkpoint_before_workspace_mutation_async(
                        std::path::Path::new(path),
                        *kind,
                    )
                    .await
                    .map_err(tool)?;
                } else {
                    observer(ToolCheckpointBoundary::BeforeMutation {
                        path: path.clone(),
                        kind: *kind,
                    })
                    .map_err(tool)?;
                }
            }
        }
        self.prepare_checkpoint_targets(name, arguments)?;
        if self.checkpoint_targets != targets
            || self.prepared_mutations != mutations
            || self.prepared_removal != removal
            || match witness {
                Some(before) => inspect_targets(self.root.clone(), targets).await? != before,
                None => false,
            }
        {
            return Err(tool(
                "workspace target or command scope changed during checkpoint preflight; reobserve before mutation",
            ));
        }
        Ok(())
    }
}

async fn inspect_targets(
    root: PathBuf,
    targets: Vec<(String, WorkspaceMutationKind)>,
) -> Result<TargetWitness, DeveloperLoopError> {
    // Join the owned read-only job so hashing large targets leaves the async host responsive.
    tokio::task::spawn_blocking(move || {
        let identity = FolderIdentity::observe(&root).map_err(|error| tool(error.to_string()))?;
        let mut versions = Vec::with_capacity(targets.len());
        for (path, kind) in targets {
            versions.push(match kind {
                WorkspaceMutationKind::File => {
                    TargetVersion::File(exact_file_receipt(&root, &path)?.owned_postchange)
                }
                WorkspaceMutationKind::EmptyDirectory => TargetVersion::EmptyDirectory(
                    FolderIdentity::observe(&checked(&root, &path, false)?)
                        .map_err(|error| tool(error.to_string()))?,
                ),
            });
        }
        if FolderIdentity::observe(&root).map_err(|error| tool(error.to_string()))? != identity {
            return Err(tool("workspace root changed during checkpoint target inspection"));
        }
        Ok(TargetWitness { root: identity, targets: versions })
    })
    .await
    .map_err(|_| tool("checkpoint target inspection worker failed"))?
}
