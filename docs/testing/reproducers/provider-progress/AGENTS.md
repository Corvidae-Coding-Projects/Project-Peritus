# Provider progress replay navigation

This folder preserves a real terminal/provider feedback regression. It is outside Cargo test
membership. Use disposable state and a signed-in official Codex client; commands consume inference.

| File | Purpose |
| --- | --- |
| [README.md](README.md) | Independent expectations, prerequisites, commands and evidence limits. |
| [provider-consumer.rs](provider-consumer.rs) | Ordinary production-provider consumer for text, Unicode, tool-proposal and cancellation cases. |
| [link.py](link.py) | Links that consumer against an already built daemon dependency cohort. |
| [live-progress.py](live-progress.py) | Actual PTY client/daemon conversation, optional owned-child pause, and cleanup. |
| [render.py](render.py) | Independent readback of the actual emitted ANSI stream. |
| [observed-results.json](observed-results.json) | Small measured original/current outcomes; no credentials or private user source. |
