# Process manifest replacement recovery

- Source base: `f75f5dee3976b1c72b85e78fa1d6c50475d5d8fa`
- Host: Fedora Linux, x86_64
- Boundary: `peritus-process` manifest store reopen
- Execution: owner-crate tests through `ProcessStore::open`

Two deterministic schedules exercise real filesystem state under a temporary registry. The interrupted schedule moves the valid current manifest to its `.previous` name and writes invalid `.staging` bytes. Reopen must restore the exact prior bytes, remove both transient files, and expose the same process identity through reconciliation. The completed schedule leaves valid current and previous files; reopen must retain current and discard stale previous.

Focused commands for both schedules passed. These tests intentionally remain inside the owner crate because the manifest codec is private; no public production API was added solely for testing.

An owner-internal, test-only thread-local seam schedules a named point and occurrence. Six cases inject permission or storage-full errors at staging write, staging sync, prior rename, publish rename, directory sync, and backup delete. Every schedule verifies its exact hit, returns a persistence error, reopens from actual filesystem state, and reconciles the original process identity. Each case ran three times from a fresh registry. A second-occurrence negative control verifies that a missed schedule does not inject and is reported as missed by the harness.

These are deterministic adapter-level errors, including quota classification through `StorageFull`; no host filesystem was exhausted. Native kernel ENOSPC, permission, and power-loss behavior remain unsupported in this local campaign.
