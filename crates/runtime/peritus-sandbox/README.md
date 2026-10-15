# peritus-sandbox

## Folder guide

Checked platform-neutral sandbox contracts and executable reference semantics.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-sandbox`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Checked platform-neutral sandbox contracts and executable reference semantics. |

`peritus-sandbox` is Project Peritus's platform-neutral execution-isolation contract. It validates
default-deny filesystem, process, environment, network, secret, resource, and terminal policy;
compiles deterministic inert plans; admits only complete backend support; and supplies an
executable reference backend for conformance.

The crate never spawns an operating-system process and never grants authority. Real launch is
owned by `peritus-process`, whose target gateway binds a checked plan and backend admission to an
opaque authorized launch. Later C3 native backends implement that process-owned boundary without
changing or weakening the values defined here.

Canonical plan and backend bytes are versioned and content addressed. Secret references and
delivery destinations are represented, but secret values are deliberately absent from every plan,
error, observation, and debug representation.

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
