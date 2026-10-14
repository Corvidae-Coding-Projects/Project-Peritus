# peritus-obligations

## Folder guide

public requirement obligations, evidence qualification, and failure ownership.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-obligations`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Public requirement obligations, qualification evidence, and failure ownership. |

`peritus-obligations` is the pure S2 contract for turning public task clauses into checked
requirements. A ledger retains each exact clause and its source span, distinguishes mandatory
outputs from inputs and examples, and binds every evidence observation to the current ledger and
candidate.

Qualification is fail-closed. Required direct, performance, lifecycle, schema, browser, and
external-effect obligations need matching current evidence. Conditional obligations activate only
after their public condition is observed, and an alternative group succeeds only when every member
of at least one branch succeeds. Internal lifecycle simulation and hand-written HTML parsing remain
supplementary evidence and cannot satisfy claims about real ingress or browser behavior.

The crate also owns a closed failure taxonomy. Only `CandidateDefect` authorizes another fixer
cycle; ambiguity gets at most one material question, provider and harness failures recover when a
recovery route exists, and external-evaluator failures settle without changing candidate quality.

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
