# G2 Interactive TUI

G2 provides `peritus-tui`, an interactive projection of the protected local G0 daemon. It is a
client, not a second application authority: durable state, command acceptance, authorization,
process ownership, review truth, and production activation remain in G0 and the lower layers.

## Start and session continuity

```text
peritus-tui --endpoint <unix-socket-or-windows-pipe> [--session <32-hex-id>]
```

The TUI negotiates A3, subscribes to the bounded live event surface, obtains daemon status, and
builds its presentation model from observations. A supplied session resumes the exact durable A3
session. On a recoverable disconnect it reconnects with bounded delay and resumes after the last
observed authoritative cursor. Retained-data gaps remain visible and require the protocol's
snapshot-recovery path; they are not concealed by resetting the cursor.

## State and effect architecture

### Chat and the production pipeline

Chat answers questions and supports read-only inspection directly. For explicitly requested
implementation or effects, its host handoff enters the same design, writer, exact-target gates,
independent reviewer, and fixer loop used by Build. The handoff preserves the conversation,
selected role models, cancellation, and elapsed-time accounting; remaining calls in that tool
batch cannot execute. Plan, Review, and untrusted folders cannot request this handoff.

In-place folders adapt file comparison and checkpoints, not the workflow. A durable journal records
only explicitly enrolled task files before effects; it never initializes Git or inventories the
whole folder. Retry retains the original task baseline, while a follow-up after completion starts
a new scope. Checks and review use the same qualification machinery. Missing checks or interrupted
work remain unqualified, with effects retained. The public snapshot shows status, scoped diff,
gates, and review, but no managed deliverable or accept/discard controls. The legacy candidate
settlement wire format remains managed-delivery-only; in-place checkpoints stay daemon-owned.

### Conversational progress

The conversation view displays public assistant prose during work, not only the terminal answer.
Interactive invocations ask the model to include concise next-step explanations alongside host
tool calls and to report meaningful discoveries, edits, and verification. Intermediate updates
do not change the role's final response format or grant tool authority.

G0 persists a short acknowledgement when a new input revision reaches a model request, plus
truthful build-stage transitions. D0 emits a waiting observation every 20 seconds while opening
a provider request or waiting for its first public text. It awaits the same owned future: no
duplicate provider request or detached task is created. Once text arrives, waiting observations
stop so they cannot split streamed prose. A failed observation cancels the pending request.
Repeated waiting observations update one durable status entry instead of filling the transcript.
These host observations are `Status` activities, not fabricated assistant text or progress evidence.

Normal conversation hides host `Status` and `Tool` activities. `/details` reveals them with distinct
Status/Tool labels and expanded metadata; user messages, model replies, and errors remain visible.
One `*working (40s)` footer indicator replaces repeated harness chatter. It advances from monotonic
runtime timestamps, not tick counts or provider heartbeat messages, and resets when work becomes
idle, the selected conversation changes, or the connection is lost. Reopening active work starts a
new client-observed busy period; the timer does not claim historical task duration or model progress.
Arguments, tool output, credentials, and private reasoning are not copied into progress narration. Existing
public text streaming remains unchanged. In particular, the current Codex account adapter
validates and delivers one complete provider response at a time: it does not expose token deltas.
The underlying [Codex JSONL interface](https://learn.chatgpt.com/docs/non-interactive-mode#make-output-machine-readable)
separates agent messages from reasoning and tool events; Peritus retains its strict normalization.

### Model selection

`/model [writer|reviewer|fixer]` selects the exact model for that role. For an existing
conversation, selection sends a dedicated daemon request immediately, including while work is
active. The client shows a pending notice until the daemon validates the adapters and durably
acknowledges the choices; rejection or disconnection must not appear as success. Input submission
waits for this acknowledgement, so it cannot accidentally send the old choices back to the daemon.

Each subsequent logical model turn resolves the selected immutable adapter and renegotiates its
capabilities and context limits. An in-flight turn (including its retries) finishes on its original
adapter; selection neither restarts the run nor adds synthetic user input. Activity records name
the model requested at each turn. Idle selection does not start work. New conversations retain
the client's role choices; reopening a saved conversation restores that conversation's choices.

The additive `UpdateModels` request uses A3 payload tag 25. Older daemons reject the unknown tag;
they cannot silently acknowledge a model change they do not implement. Deploy matching daemon
and TUI builds to use this behavior.

### State ownership

`AppModel` is a deterministic bounded reducer. Terminal events, A3 messages, ticks, and keyboard
events produce a new presentation state plus explicit effects. The runtime alone performs effects:
connect, subscribe, acknowledge, send prompt input, attach/control a terminal, or shut down. This
keeps rendering and navigation replayable and prevents a view transition from becoming an implied
daemon transition.

The primary projections are Runs, Diff, Review, Trace, Evolution, Terminal, and Approvals. They
retain bounded recent data and show daemon readiness, connection state, session, cursor, current
selection, and typed notices. The Help view describes the live key map.

## Key controls

- `1` through `7` select Runs, Diff, Review, Trace, Evolution, Terminal, or Approvals.
- `Tab` and `Shift-Tab` cycle views; `j`/`k` or arrows move the selection.
- `PageUp` and `PageDown` move by a page; `r` requests a refresh.
- `p` and `u` pause and resume the active event subscription.
- `Enter` opens the selected approval/input editor; `c` cancels a selected prompt.
- In Terminal, `a` begins attach entry, `i` captures keys for the PTY, `Ctrl-]` releases capture,
  `d` detaches, and `x` requests process cancellation.
- `Ctrl-Q` or `Ctrl-C` performs orderly client shutdown.

While PTY capture is active, Unicode text, control-letter bytes, arrows, editing/navigation keys,
and function keys are mapped to terminal bytes. Client-global shortcuts do not intercept captured
process input except the explicit `Ctrl-]` escape.

## Approval and input boundary

Prompt bindings carry the originating request, actor/session, exact revision, freshness digest,
and cancellation generation. The TUI can collect prompt input, but it cannot turn an ordinary text
acknowledgement into approval. Approval decisions must be supplied as externally signed canonical
B1 data and are revalidated by G0 against current authority and freshness.

Cancellation and revision changes invalidate stale editors. A rendered success message reflects a
typed daemon response, never mere keyboard submission.

## Terminal safety and restoration

Terminal output is untrusted data. The streaming sanitizer removes CSI, OSC, DCS/SOS/PM/APC string
controls and non-display controls before rendering. It preserves valid UTF-8 even when a code point
is split across transport reads and replaces malformed or incomplete sequences. Transcript and
event retention are bounded.

The runtime owns raw mode, alternate-screen entry, paste/focus/mouse modes, and cursor state through
an RAII guard. Normal completion and every propagated error restore terminal modes. Panic-time
restoration is best effort and does not claim to repair an externally corrupted terminal.

Detach only releases this UI attachment; it does not cancel the C2 process. Cancellation is a
separate explicit effect. Output ordering follows A3 attachment sequence and byte offsets, and one
terminal exit fence closes the stream.

## Verification

```text
CARGO_BUILD_JOBS=1 cargo test --locked --package peritus-tui --all-targets --all-features
CARGO_BUILD_JOBS=1 cargo clippy --locked --package peritus-tui \
  --all-targets --all-features -- -D warnings
```

The focused suite covers strict entry parsing, UTF-8 editor behavior, terminal key mapping,
streaming sanitation, deterministic navigation/effects, protocol projection, reconnect planning,
and shutdown behavior. A3/G0 independently qualify authenticated transport, event, prompt, and PTY
semantics.
