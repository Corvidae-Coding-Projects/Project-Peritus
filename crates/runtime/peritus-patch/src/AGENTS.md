# crates/runtime/peritus-patch/src — navigation

## Responsibility

Checked typed patches and recoverable filesystem transactions for Peritus.

## Files in this folder

| File | Role |
| --- | --- |
| [codec.rs](codec.rs) | Native representation capacities for local canonical records. |
| [content.rs](content.rs) | Final file content with exact computed identity. |
| [error.rs](error.rs) | Stable patch failures and recovery guidance. |
| [lib.rs](lib.rs) | Checked typed patches and recoverable filesystem transactions for Peritus. |
| [line_endings.rs](line_endings.rs) | Explicit final-content line-ending policies. |
| [operation.rs](operation.rs) | Closed patch operation vocabulary. |
| [path.rs](path.rs) | Bounded canonical workspace-relative paths. |
| [plan.rs](plan.rs) | Exact workspace-bound patch plans and stable identities. |
| [preimage.rs](preimage.rs) | Exact file preimages and portable mode intent. |
| [set.rs](set.rs) | Canonical patch sets with native representation bounds. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `path_bounds_valid`, `patch_bounds_valid`, `workspace_version_matches`, `file_identity_matches`, `target_is_recoverable`. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [transaction/](transaction/AGENTS.md) | Recoverable staged-final and backup transaction adapter. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
