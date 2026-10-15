# peritus-policy

## Folder guide

policy, capability, and authority-time core for Project Peritus.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-policy`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Verified, effect-free policy, capability, and authority-time decisions for Peritus. Public constructors validate ordinary-Rust inputs; outputs are only logical transitions. |

`peritus-policy` is the verified, effect-free authority core for Peritus. It evaluates immutable
policy definitions, enforces compiled role separation, preserves exact resource/capability pairs,
tracks monotonic authority time, and produces move-only logical capability transitions.

## Invariants

- Policy evaluation is total for every checked policy and request.
- Permission sets contain exact `(ResourceId, CapabilityName)` pairs and are canonical.
- A matching immutable or restriction denial dominates grants and approval requirements.
- A restriction layer is neutral when no rule matches and can never grant authority.
- Reviewer/evaluator roles cannot mutate workspaces; writer/fixer roles cannot accept, waive,
  amend policy, or promote harnesses.
- Capabilities remain bound to one actor, role, environment, revision tuple, validity window, and
  exact permission set. Limited use counts decrease exactly once per accepted logical transition.
- Authority-clock observations never regress or cross epochs implicitly.
- An amendment preview retains the original ceiling and immutable denials and changes exactly one
  declared restriction tier. It is not an active-policy fact.

## Boundary

This crate performs no hashing, serialization, storage, clock I/O, target resolution, tool
dispatch, or effect execution. Transition digests and action digests are exact correlation bytes,
not authenticity claims. C0 and later target gateways own durable commit and effect permits.

The production dependency set is limited to `peritus-types` and the workspace-pinned `vstd`.

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
