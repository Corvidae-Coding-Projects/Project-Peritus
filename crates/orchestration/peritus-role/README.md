# peritus-role

## Folder guide

role-aware context and capability views for Peritus.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-role`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Role-aware context policy for Peritus. |

`peritus-role` projects the stable B1 security roles into deterministic C6 context policies and
read-only capability views. It does not define security identities, issue capabilities, evaluate
authority, or perform effects.

The writer, reviewer, fixer, evaluator, and evolution-agent profiles are explicit. Other B1 roles
receive restricted profiles. Each profile defines canonical visible, required, and contributable
context-class sets; memory and hidden-reasoning visibility; fresh-context and producer-ancestry
rules; and provider-neutral presentation policy.

Reviewer context is always fresh, excludes producer ancestry, memory, and hidden reasoning, and
exposes inspection only. Writer and fixer views exclude acceptance, waiver, policy amendment, and
harness promotion. Evaluator and reviewer views exclude mutation. The public checked
`CapabilityView` constructor rejects any operation denied by the underlying B1 role, and its
executable `is_narrow` result is proved equivalent to the formal B1 subset predicate.

`ReviewIndependenceView` copies every immutable B2 reviewer-independence requirement and adds the
C6 fresh-context requirement. It requests evidence from the future review engine; it never claims
that evidence already exists.

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
