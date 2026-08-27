# peritus-tui

G2 owns the interactive terminal client. Its deterministic reducer projects A3/G0 observations
into bounded run, diff, review, trace, evolution, approval, and terminal views. Effects are emitted
as typed requests; presentation state is never authoritative.

The crate owns terminal-mode restoration, input mapping, reconnect and resumable-session behavior,
bounded transcript sanitization, and orderly connection shutdown. It depends on the A3 application
protocol and foundation contracts plus Crossterm and Ratatui for presentation. All authorization,
durable state, and acceptance decisions remain in G0 and the verified lower layers.
