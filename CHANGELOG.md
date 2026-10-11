# Changelog

## [0.0.6] - Unreleased

### Fixed

- Recover browser-created conversations and queued inputs in the native workbench without duplicating messages.
- Retain useful candidates when a task's command restrictions prevent automatic checks, and preserve actionable failure reasons.
- Restore remembered folder scopes with their original identity and trust; reject unavailable or undeclared scopes without losing the current draft.
- Refresh compaction and conversation state from current durable revisions, and retain multiline paste across reconnects.
- Reject changed file content between preview and confirmation, and describe file ranges and imported context accurately.
- Complete checkpoint restore receipts and make their recovery checkpoints available for lookup and undo.
- Bound selected-window capture failures and settle them durably so subsequent preview controls remain usable.
- Stop ordinary background jobs across their process groups, preserve terminal startup output, and improve native process ownership and permission restoration.
- Export managed changes into initialized Git submodules while preserving parent and child histories.
- Improve design-phase inspection feedback, Bash command completion, and canceled-goal guidance.

### Changed

- Use the real-testing skill for development, based on the research linked in the README. Acceptance focuses on real interfaces, independently defined outcomes, varied inputs, recovery, and reproducible defect replay.
- Update the Rust workspace, internal package requirements, WebUI package metadata, and visible WebUI version to 0.0.6.
- Restore current installation, source build, workspace, conversation, and recovery documentation.

### Removed

- Retire external unit-test suites, proof infrastructure, qualification harnesses, and repository CI automation. The application's design, checks, reviewer, fixer, and durable recovery features remain available.

### Real-testing evidence

The completed develop campaign covers the recorded defect replays through the actual CLI, native terminal, WebUI, process and capture controls, and generated applications. Extended testing in an independent copy of a complex repository passed 20 business workflow groups, including explicit CSV import, cancellation, replay idempotence, whole-batch rejection, stale-preview handling, pagination, authentication boundaries, and restart recovery.

Release preparation reuses that completed campaign. Its coverage is bounded: it does not establish complete application coverage, continuous unattended multi-hour execution, external delivery acceptance, or fresh acceptance on every supported platform. Task candidates remain unqualified where their requested restrictions prevent required checks or review.

[0.0.6]: https://github.com/Corvidae-Coding-Projects/Project-Peritus/compare/v0.0.5...develop
