# crates/analysis/peritus-eval/src/profile — navigation

## Responsibility

Frozen evaluation bindings and metric/failure policies.

## Files in this folder

| File | Role |
| --- | --- |
| [binding.rs](binding.rs) | Complete immutable campaign profile and arm/execution bindings. |
| [mod.rs](mod.rs) | Frozen evaluation bindings and metric/failure policies. |
| [policy.rs](policy.rs) | Versioned retry, metric, seed, and infrastructure policies. |
| [provider.rs](provider.rs) | Canonical E3 snapshots of public C5 provider and model-control contracts. |
| [storage.rs](storage.rs) | Versioned complete recovery data. Decoded bindings remain inert until runtime admission. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
