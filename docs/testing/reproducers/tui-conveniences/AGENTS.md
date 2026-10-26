# TUI convenience replay

## Responsibility

This folder contains an actual PTY driver for the launcher, TUI, and local daemon.
[replay.py](replay.py) creates disposable source/XDG state, uses an isolated X11 clipboard, operates
search/edit/queue/theme/copy controls, decodes real terminal output, and preserves results.
Its optional provider journey uses the configured real account for one read-only response.
It does not replace the product, daemon, clipboard service, or provider with a mock.
See README.md for prerequisites and replay commands. Keep evidence separate from fixture state.
[archive-consumer.rs](archive-consumer.rs) operates the actual production transcript cache and checks disk/readback
contracts. [archive-replay.py](archive-replay.py) links it against the built TUI dependency cohort and runs fresh
processes with ripgrep present and absent. These are real-interface replays, not unit-test targets.
[attachments.py](attachments.py) reopens a retained disposable run and exercises actual image/file confirmation,
retained attachment readback, composer preservation, and unchanged inference accounting.
## Working in this area

Start with [`README.md`](README.md) to recover prerequisites, disposable-state rules, and the independent oracle before running anything. Keep generated binaries and mutable run state outside the repository, and treat a replay as evidence for the exact observed build and scenario rather than universal product coverage.
