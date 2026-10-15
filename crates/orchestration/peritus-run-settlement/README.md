# peritus-run-settlement

## Folder guide

candidate-checkpoint and run-settlement domain for Peritus.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-run-settlement`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Candidate-checkpoint and terminal-settlement domain for Peritus. |

`peritus-run-settlement` is the pure V-class domain for recording the latest candidate observed
during a coding run and deriving one honest terminal disposition. It separates candidate delivery
from automated acceptance: a candidate may be available for inspection or continuation without
claiming that its gates, public obligations, and independent review all passed.

The crate performs no filesystem, provider, process, clock, persistence, protocol, or user-interface
effects. Effectful callers supply checked observations. The reducer validates candidate lineage,
monotonic checkpoint order, evidence provenance, stage consistency, and exactly-once settlement.

`CandidateCheckpoint::observe` owns evidence reconciliation and bounds requested work progress by
the evidence that currently applies. `reobserve` uses those same rules for restart and handoff.
Observation sequences advance; qualification may decrease when a newer check fails or evidence is
revoked. An unchanged workspace must remain accessible in that case, without retaining a historical
success claim. Snapshot bindings currently cover source and Git history together; callers must not
interpret this digest as a content-only identity or assume it covers an unobserved toolchain.

Acceptance is fail closed. `RunDisposition::Accepted` is produced only for a qualified checkpoint
whose gate, obligation, and review evidence are current and satisfied. Provider, context, gate,
review, repository, adapter, deadline, cancellation, and recovery causes remain distinct.

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
