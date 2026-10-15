# crates/runtime/peritus-workspace/src — navigation

## Responsibility

Target-owned authorization and lifecycle boundary for isolated Peritus workspaces.

## Files in this folder

| File | Role |
| --- | --- |
| [authorization.rs](authorization.rs) | Complete borrowed request for one target-owned authorization decision. |
| [caller.rs](caller.rs) | Dependency-neutral C4 caller and immutable target bindings. |
| [candidate.rs](candidate.rs) | Authorized candidate creation and content-addressed manifest finalization. |
| [consumption.rs](consumption.rs) | Durable per-revision action-consumption markers owned by the writable target. |
| [error.rs](error.rs) | Stable workspace failures and recovery guidance. |
| [filesystem.rs](filesystem.rs) | Platform-specific durable filesystem operations. |
| [folder.rs](folder.rs) | Canonical direct-directory identity, without Git discovery or recursive content inspection. |
| [folder_mutation.rs](folder_mutation.rs) | Exact-authority patch application for one registered ordinary folder. |
| [gateway.rs](gateway.rs) | Target-owned authorization gateway and private permit construction. |
| [git_inspection.rs](git_inspection.rs) | Structured Git observations anchored to an immutable workspace snapshot. |
| [git_mutation_recovery.rs](git_mutation_recovery.rs) | Durable replay of completed candidate and rollback actions. |
| [identity.rs](identity.rs) | Exact workspace, resource, root, lineage, and snapshot identity values. |
| [inspection.rs](inspection.rs) | Structured no-follow inspection of an immutable workspace snapshot. |
| [lib.rs](lib.rs) | Target-owned authorization and lifecycle boundary for isolated Peritus workspaces. |
| [manifest.rs](manifest.rs) | Canonical content-addressed workspace outcome manifests. |
| [mutation.rs](mutation.rs) | Authorized atomic patch application. |
| [open.rs](open.rs) | Ownership-bearing open requests for writable and read-only workspace handles. |
| [publication.rs](publication.rs) | Atomic ownership of retained Git snapshots and their durable workspace manifests. |
| [read_only.rs](read_only.rs) | Immutable snapshot worktree with inspection-only methods. |
| [reconcile.rs](reconcile.rs) | Restart and post-fence workspace reconciliation. |
| [refinement.rs](refinement.rs) | Named C1/B1 refinement obligations. |
| [registration.rs](registration.rs) | Canonical stable inputs for reopening one configured writable workspace. |
| [rollback.rs](rollback.rs) | Authorized history-preserving rollback. |
| [scoped_inspection.rs](scoped_inspection.rs) | Handle-relative, bounded reads for explicit live-folder references, not mutation authority. |
| [state.rs](state.rs) | Explicit writable workspace state. |
| [transaction_namespace.rs](transaction_namespace.rs) | Canonical target-owned transaction namespaces and durable identity binding. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `ResourceFacts`, `AuthorityFacts`, `identifier_values_equal_from`, `resource_identity_exact`, `authority_complete`. |
| [writable.rs](writable.rs) | Move-only writable workspace handle hidden behind the target gateway. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [authorization/](authorization/AGENTS.md) | Implementation pieces for `authorization.rs`: Complete borrowed request for one target-owned authorization decision. |
| [consumption/](consumption/AGENTS.md) | Implementation pieces for `consumption.rs`: Durable per-revision action-consumption markers owned by the writable target. |
| [folder_mutation/](folder_mutation/AGENTS.md) | Implementation pieces for `folder_mutation.rs`: Exact-authority patch application for one registered ordinary folder. |
| [git_mutation_recovery/](git_mutation_recovery/AGENTS.md) | Implementation pieces for `git_mutation_recovery.rs`: Durable replay of completed candidate and rollback actions. |
| [inspection/](inspection/AGENTS.md) | Implementation pieces for `inspection.rs`: Structured no-follow inspection of an immutable workspace snapshot. |
| [mutation/](mutation/AGENTS.md) | Implementation pieces for `mutation.rs`: Authorized atomic patch application. |
| [scoped_inspection/](scoped_inspection/AGENTS.md) | Implementation pieces for `scoped_inspection.rs`: Handle-relative, bounded reads for explicit live-folder references, not mutation authority. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
