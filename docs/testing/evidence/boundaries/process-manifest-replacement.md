# Process manifest replacement recovery

- Source base: `f75f5dee3976b1c72b85e78fa1d6c50475d5d8fa`
- Host: Fedora Linux, x86_64
- Boundary: `peritus-process` manifest store reopen
- Execution: owner-crate tests through `ProcessStore::open`

Two deterministic schedules exercise real filesystem state under a temporary registry. The interrupted schedule moves the valid current manifest to its `.previous` name and writes invalid `.staging` bytes. Reopen must restore the exact prior bytes, remove both transient files, and expose the same process identity through reconciliation. The completed schedule leaves valid current and previous files; reopen must retain current and discard stale previous.

Focused commands for both schedules passed. These tests intentionally remain inside the owner crate because the manifest codec is private; no public production API was added solely for testing.

Injected write, sync, rename, delete, permission, and quota failures remain uncovered. They require a narrow owner-internal filesystem fault seam or a disposable filesystem with controllable faults; host-wide quota or disk exhaustion is prohibited.
