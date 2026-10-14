# docs/testing/reproducers/context-resume — navigation

## Responsibility

Real-interface replay resources for context resume; start with the local README/driver and its documented disposable-state prerequisites.

## Files in this folder

| File | Role |
| --- | --- |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |
| [build_consumer.py](build_consumer.py) | Build the public context consumer against this checkout, without test suites. |
| [consumer.rs](consumer.rs) | Local implementation for docs / testing / reproducers / context-resume. Entry names: `id`, `hex`, `artifact`, `main`, `message`. |
| [host_consumer.rs](host_consumer.rs) | Local implementation for docs / testing / reproducers / context-resume. Entry names: `text`, `bytes`, `Conversation`, `revision`, `render`. |
| [prepare_host.py](prepare_host.py) | Prepare an isolated host replay from the already copied context and non-secret profile. |
| [prepare_snapshot.py](prepare_snapshot.py) | Export a copied context checkpoint and uncovered events for public-API replay. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
