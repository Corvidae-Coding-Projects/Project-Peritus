# Owned process output-limit boundary

- Source base: `f75f5dee3976b1c72b85e78fa1d6c50475d5d8fa`
- Host: Fedora Linux, x86_64
- Boundary: production pipe supervisor and durable terminal store
- Command: `CARGO_BUILD_JOBS=2 cargo test -p peritus-process --test process_integration`

The fixture writes exactly eight known stdout bytes against a four-byte output ceiling and then sleeps. The independent oracle checks eight observed bytes, four retained bytes, four dropped bytes, `OutputLimit` as both first stop reason and terminal disposition, complete process-tree cleanup, joined support tasks, and equality with the terminal value reopened from the process store.

The expanded integration target adds an output-limit plus descendant case and an output-barrier plus explicit cancellation case. The former proves a descendant holding inherited streams is reaped within ten seconds after the exact output trigger. The latter waits for exact `ready` bytes before cancellation, then independently verifies five observed/retained bytes, zero dropped bytes, user cancellation as the first reason, durable terminal equality, and complete cleanup. All roots and executable identities are owned by `TestRoot`.

This is real native Linux process execution. Windows job-object and macOS process-group execution were not run in this lane.
