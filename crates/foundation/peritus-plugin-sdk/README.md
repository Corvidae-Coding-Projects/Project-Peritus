# peritus-plugin-sdk

## Folder guide

Versioned wire contracts and canonical manifests for isolated Peritus plugins.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-plugin-sdk`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Stable contracts for isolated Peritus plugins. |

Stable, serde-based contracts for Peritus plugins that execute outside the daemon address space.
The crate validates canonical manifests, bounded JSON payloads, protocol version negotiation, and
length-delimited request/result frames. Decoding a frame never grants authority; the plugin host
must bind every invocation to current daemon policy.

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.

## Native code module service

`module.rs` defines generic `ModuleAction`, agent/function requests, user menu actions,
and explicit artifact attachments. Independently compiled Rust executables use the
existing framed protocol with `module.workflow` invocation payloads: `{method,
params_json}` requests and `{json}` responses. Inner JSON strings preserve arbitrary
source metadata without weakening outer canonical rules. Command/tool/observation/
model-result services let the plugin own feature logic while Peritus supplies current
models and permitted host services. See the market's `CATALOG.md` and native Rust template:
https://github.com/Corvidae-Coding-Projects/Peritus-Module-Market.

`ModuleAction::Checkpoint { key, text, actions }` is an additive continuing-publication
service. Updated hosts persist the visible text and publish current controls before invoking
`checkpoint_ack` with that exact key. The plugin owns its saved checkpoint and continuation.
New user input or cancellation prevents acknowledgement. A crash in between may replay the
publication, so this is at-least-once delivery. Existing Reply/Model actions are unchanged;
plugins using Checkpoint require a host with this service, rather than assuming older hosts
will understand a new enum variant.

Updated module calls also carry a version-one `host_profile`, and native coordinators may return
`ModuleAction::HostService`. Peritus validates the request and returns the exact result through
`host_result`. The SDK includes capability and identity contracts for scoped source, owned jobs,
preparation receipts, and source-built packages; those contracts convey no authority. Linux hosts
advertise owned jobs only when their native isolation backend is configured, and source build
remains unadvertised until implemented. Existing Reply/Model/Checkpoint modules remain compatible.
