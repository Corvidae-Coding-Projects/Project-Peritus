# TUI reconnect replay

## Responsibility

[replay.py](replay.py) operates the real terminal and daemon with disposable state, injecting actual process loss. `README.md` gives its command and oracles. Keep draft preservation, automatic recovery, no inferred control retries, and private diagnostics as observable requirements. Never direct the signal operations at personal daemon processes.

[context_load.rs](context_load.rs) is an actual public-library consumer for generated context graphs and private
captured working-state snapshots. It checks selection, ordering, budget/error behavior and
reports full typed-render digests and timings without printing private source text.
## Working in this area

Start with [`README.md`](README.md) to recover prerequisites, disposable-state rules, and the independent oracle before running anything. Keep generated binaries and mutable run state outside the repository, and treat a replay as evidence for the exact observed build and scenario rather than universal product coverage.
