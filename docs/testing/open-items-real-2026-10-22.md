# Open issue and pull-request campaign

Requested scope: process the open issues and PRs sequentially, land fixes on develop, and leave
macOS real-testing issue #125 untouched. Tests use actual interfaces; no unit, mock, formal
verification or repository qualification suite is run.

## PR #148: hosted configuration migration

Closed as superseded by develop `4ad9153dc`. Compared both production diffs: the existing
shared migration handles all hosted providers, discovery-flag and old OpenCode capability
combinations, and immutable generation publication. It also handles retired timeout and
folder-catalog combinations. Its final 43-scenario real-interface evidence already covers
the PR's Go/OpenRouter upgrade behavior and rejection of unrelated changes; see
[the original campaign](configuration-upgrade-real-2026-10-21.md). No duplicate implementation
or unit-only fixture API was imported.

## PR #149: provider errors and terminal recovery

Reviewed and integrated the contributor's provider/workspace `not-ready` classifications,
actionable TUI rejection details, duplicate-invocation reply guard, and preservation of an
original interruption when terminal deliverable projection genuinely fails.

Built the actual CLI, daemon and WebUI gateway using `cargo build --offline --locked -p
peritus-cli -p peritus-daemon -p peritus-web --message-format=json`, with one Cargo job and
`CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_DEV_OPT_LEVEL=0`. The output target is
`/home/doll/.cache/peritus-read-loop-audit-20261020/target`.

The public IPC consumer reproduced four incorrect `invalid-identifier` responses against the
published v0.1.9 daemon. The integrated daemon returns `not-ready`, identifies the provider
or failed model discovery, and offers `after-recovery`. Each rejected admission leaves the
conversation editable: an independent rename at revision 2 succeeds and fresh readback is
exactly revision 3. The initial driver omitted the execution feature from negotiation;
correctly adding that feature reached the intended admission path without weakening checks.

The actual native TUI was then operated against a real daemon with an unavailable OS
credential store. Sending a message and invoking `/retry` retained the credential failure
instead of replacing it with a deliverable-handoff failure. `/model` remained usable afterward,
and the client exited cleanly. This directly exercises provider failure/retry presentation;
the duplicate-prior-reply branch itself is source-reviewed rather than independently fault-injected.

Replays and commands are in [the reproducer guide](reproducers/open-issues/README.md).
Raw observations are outside the checkout at
`/home/doll/.cache/peritus-open-items-20261022/pr149-before` and `pr149-after`.
The replay's actual daemons were shut down; no installed binaries or user sessions were changed.
