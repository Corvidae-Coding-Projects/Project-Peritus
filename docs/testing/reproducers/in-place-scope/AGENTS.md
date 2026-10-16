# docs/testing/reproducers/in-place-scope — navigation

## Responsibility

Real-interface replay resources for in place scope; start with the local README/driver and its documented disposable-state prerequisites.

## Files in this folder

| File | Role |
| --- | --- |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |
| [consumer.rs](consumer.rs) | A command-line host of the actual public scoped-baseline API. |
| [replay.py](replay.py) | Replay actual Peritus directory tracking on archived copies, without unit tests. Requires Cargo, Python 3, git, and GNU patch. Outputs stay in the supplied disposable directory. |
| [runtime_setup.py](runtime_setup.py) | Create an owned disposable application fixture and copy actual CLI/daemon/TUI binaries for a runtime-write replay. |
| [runtime_replay.py](runtime_replay.py) | Drive the actual terminal, daemon, and authenticated Codex through source edits and commands while a bounded fixture writer remains active. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
