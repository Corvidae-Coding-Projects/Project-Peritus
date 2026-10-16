# Binary evidence replay waypoint

This folder preserves real library and full-product reproduction of NUL-bearing binary previews entering model text. Start with [README.md](README.md) for the independent contract and bounded replay commands. The production owners are scoped comparison in `workspace_delivery/scope.rs`, repository diff assembly in `bundle.rs`, and role evidence projection in `turn/evidence.rs` under `peritus-product-runner`.

| File | Responsibility |
| --- | --- |
| [consumer.rs](consumer.rs) | Actual public API caller: scoped file comparisons, developer evidence projection, exact historical source paging, Unicode, raw protocol guard, and binary changes beyond the preview prefix. |
| [build_consumer.py](build_consumer.py) | Builds that caller in a new external directory using the current source and two Cargo workers; runs no unit or qualification suites. |
| [setup.py](setup.py) | Creates an owned source/binary fixture, a dependency-free Python project descriptor, and copies actual sibling Peritus executables. |
| [tui_replay.py](tui_replay.py) | Uses the real TUI/daemon and authenticated Codex to edit and operate the fixture, then observes past writer completion into review or the final outcome. |
| [README.md](README.md) | Contract, original failure, replay steps, provenance requirements, and limits. |

Inputs are ordinary fixture data, never substitute implementations of Peritus or its provider. Preserve exact retained source identities and readback when changing presentation. Keep private live diffs, credentials, and raw user project data outside this folder. Always start with a fresh fixture path.
