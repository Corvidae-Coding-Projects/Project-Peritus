# peritus-tui

G2 owns the interactive terminal client. Its deterministic reducer projects A3/G0 observations
into bounded run, diff, review, trace, evolution, approval, and terminal views. Effects are emitted
as typed requests; presentation state is never authoritative.

The crate owns terminal-mode restoration, input mapping, reconnect and resumable-session behavior,
bounded transcript sanitization, and orderly connection shutdown. It depends on the A3 application
protocol and foundation contracts plus Crossterm and Ratatui for presentation. All authorization,
durable state, and acceptance decisions remain in G0 and the verified lower layers.

The Runs dashboard presents accepted, candidate-available, waiting, cancelled, stopped, and
recovery-required states directly. Its handoff panel exposes exact paths, checks, review evidence,
remaining work, and run instructions. Users can inspect, continue, run, export, accept, commit, or
discard a candidate without finding an internal worktree or log. Foreground run commands temporarily
return terminal ownership to the candidate and restore the full-screen interface afterward.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-tui
```
