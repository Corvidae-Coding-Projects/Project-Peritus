# peritus-product-state

## Folder guide

Pure resumable product-state model for the Peritus application experience.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-product-state`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Pure, resumable product state for the single-command Peritus experience. |

`peritus-product-state` is the H-class G4 owner of pure, versioned, resumable product state. It
retains stable non-secret installation identities, the exact durable local-bootstrap phase,
provider/default/offline selection, and non-secret direct-route profiles containing only opaque
credential references. It also retains explicit, default-off automatic-failover consent and
requires at least two selected routes before that choice is valid. The executable transition
predicate is mirrored by a Verus refinement.

Selected account models and resolved executable paths are retained as non-secret provider facts.
Legacy state may omit executable pins; launcher reconciliation publishes them in a new generation.

The crate performs no filesystem, process, terminal, network, credential, workspace, or daemon
effect. Effectful composition belongs to `peritus-launcher`; durable runtime authority remains in
the existing G0/C0/C1/B1 boundaries. State generations never persist live daemon readiness.

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
