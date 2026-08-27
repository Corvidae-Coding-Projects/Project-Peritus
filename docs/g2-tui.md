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
