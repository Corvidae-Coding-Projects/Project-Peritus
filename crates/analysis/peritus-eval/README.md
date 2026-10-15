# peritus-eval

## Folder guide

Durable reproducible harness evaluation and statistical evidence for Peritus.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-eval`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Durable reproducible harness evaluation for Peritus. |

`peritus-eval` is the durable E3 boundary for reproducible harness evaluation. It freezes exact
dataset, E1 harness, C5 provider, execution, resource, seed, retry, metric, and infrastructure
policies; expands them into a deterministic D3 work ledger; retains every rollout outcome; and
publishes replayable statistical evidence through C0.

Candidate-visible task inputs and sealed evaluator inputs are separate types and effect
directives. The crate is runtime-neutral: later application composition maps its checked inert
execution requests to C2/C3 and returns exact observations. E3 cannot mutate or promote a harness,
accept a run, waive a finding, or grant authority.

The complete frozen contract is in [the E3 design](../../../.design/e3-evaluation.md).

Provider snapshots retain the v1 capability inventory and bytes when every later capability is
unsupported. Profiles with supported or unknown later capabilities use the
`peritus.evaluation.provider-snapshot.v2` digest domain: after the original capability rows, a
u32 count precedes the additional name/state rows sorted by name. Omitted later capabilities
mean unsupported. Adding an unused capability therefore preserves existing campaign identities,
while reasoning-replay support and unknown support produce distinct fingerprints.

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
