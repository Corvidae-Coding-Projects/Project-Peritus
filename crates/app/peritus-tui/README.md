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

Conversation view pins the active elapsed working indicator directly above the message-entry box in
bold white. Tool calls remain visible without enabling diagnostics: one entry shows the command
or operation, then its observed result, exit status and bounded output preview. Long entries keep
their opening and final lines; `/details` expands retained details and host status diagnostics.
The timer stops on disconnect or idle/terminal phases; it never claims provider progress.

`/effort` opens the reasoning-effort picker without requiring model discovery. Use arrows and
Enter, Tab to change roles, or `/effort [writer|reviewer|fixer] LEVEL`. Levels are `default`,
`minimal`, `low`, `medium`, `high`, `xhigh`, `max`, and `ultra`; provider/model support varies.
The model picker also opens this control with `e`. Each role keeps its effort when its model
changes. Active conversations show success only after durable daemon acknowledgement; the next
model turn uses the selection while an in-flight turn remains unchanged. `default` retains the
existing policy: high when reasoning controls are negotiated, otherwise no reasoning control.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-tui
```
