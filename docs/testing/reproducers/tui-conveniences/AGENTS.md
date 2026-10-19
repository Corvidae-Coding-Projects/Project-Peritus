# TUI convenience replay

This folder contains an actual PTY driver for the launcher, TUI, and local daemon.
`replay.py` creates disposable source/XDG state, uses an isolated X11 clipboard, operates
search/edit/queue/theme/copy controls, decodes real terminal output, and preserves results.
Its optional provider journey uses the configured real account for one read-only response.
It does not replace the product, daemon, clipboard service, or provider with a mock.
See README.md for prerequisites and replay commands. Keep evidence separate from fixture state.
`archive-consumer.rs` operates the actual production transcript cache and checks disk/readback
contracts. `archive-replay.py` links it against the built TUI dependency cohort and runs fresh
processes with ripgrep present and absent. These are real-interface replays, not unit-test targets.
`attachments.py` reopens a retained disposable run and exercises actual image/file confirmation,
retained attachment readback, composer preservation, and unchanged inference accounting.
