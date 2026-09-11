# Owned process output-limit boundary

- Source base: `f75f5dee3976b1c72b85e78fa1d6c50475d5d8fa`
- Host: Fedora Linux, x86_64
- Boundary: production pipe supervisor and durable terminal store
- Command: `CARGO_BUILD_JOBS=2 cargo test -p peritus-process --test process_integration`

The fixture writes exactly eight known stdout bytes against a four-byte output ceiling and then sleeps. The independent oracle checks eight observed bytes, four retained bytes, four dropped bytes, `OutputLimit` as both first stop reason and terminal disposition, complete process-tree cleanup, joined support tasks, and equality with the terminal value reopened from the process store.

The full integration target passed 23 tests, including descendant pipe-holder cleanup, process-count enforcement, cancellation, signal ordering, partial artifact publication retry, and the new output-limit case. All roots and executable identities are owned by `TestRoot`; the suite reported no surviving child or cleanup failure.

This is real native Linux process execution. Windows job-object and macOS process-group execution were not run in this lane.
