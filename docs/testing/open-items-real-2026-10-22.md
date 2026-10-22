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

## PR #150: long OpenCode planning replies

Reviewed and integrated the contributor's C0 string escaping, optional empty `tool_calls`,
exact duplicated empty trailing-fragment reconstruction, and inert plain-prose fallback.
Tool choice/selection validation and canonical duplicate-key rejection remain in the actual
production path; the fallback never executes tools and records `plain-text-v1` provenance.

The native [public parser consumer](reproducers/open-issues/healing.rs) generated 96 actual
inputs: all 32 literal C0 characters crossed with ASCII, Unicode and quoted/backslash-bearing
content. The pre-PR library rejected all 96 and the captured trailing fragment. The changed
library returns every content value byte-for-byte and repairs that exact fragment. Six strict
boundary cases remain rejected: ordinary tool-argument use of the fragment, duplicate content,
duplicate call fields, nonempty trailing calls, a second object and truncation. These are
actual public library calls, without a provider stand-in or copied repair implementation.

Four real requests through the signed-in official Codex also passed after the shared-core
change: exact ASCII, exact Unicode, one inert named tool proposal and cancellation immediately
after runtime acceptance. No host tool was executed. This checks the adjacent provider path;
it does not establish live Zen/Go inference. The omitted-array and plain-prose adapter branches
were source-reviewed; the contributor's fifteen scripted wire scenarios are retained with
their explicit scripted-vendor limitation and were not run as a substitute for live inference.

Artifacts: `pr150-before.json`, `pr150-after.json` and `pr150-codex/result.json` under
`/home/doll/.cache/peritus-open-items-20261022`. The production build uses the same bounded
command above; `link.py --source .../healing.rs` produces the public parser consumer and
`--source docs/testing/reproducers/provider-progress/provider-consumer.rs` produces the real
Codex consumer. The latter takes one fresh output directory argument.

## Issue #151: provider replacement and saved-key model edits

Added `/provider`, with arrow/Enter selection, Tab role selection, and explicit numeric forms
for all roles or writer/reviewer/fixer. Existing runs retain authoritative old profile IDs;
the new negotiated `app.provider-selection` request (additive tag 61) atomically persists
replacement providers and models. Changed roles reset to the replacement route's configured
model; unchanged roles keep their choices. Legacy model-only tags 25/27 retain their exact
encoding. The server authenticates ownership, resolves the requested choices before mutation,
fences concurrent selection changes, and restores request/snapshot/interaction on save failure.
Live selection uses the existing per-turn provider lookup. Choosing does not resume a run.

Actual native TUI replay first created a failed Go run with an unavailable credential store,
then removed Go from the configured providers. The old TUI reproduced the stranded header and
had no `/provider` command. The changed TUI selected a replacement on that same run. Three
fresh campaigns retained the original activities, saved the new selection across daemon restart,
and successfully retried the original instruction through the signed-in official Codex; each
returned exactly `RETAINED-RUN-PROVIDER-RECOVERY-153791` on its original run ID.

The final campaign additionally configured Zen and changed reviewer and fixer independently
through actual `/provider reviewer 2` and `/provider fixer 2`, then restored all roles. Native
IPC checks exercised three distinct role effort values in the additive request, rejected an
unknown provider without changing the snapshot, and accepted a legacy model-only update
without changing providers. This establishes actual cross-provider selection and account retry;
it does not claim paid Zen/Go inference or an in-flight switch during an outstanding request.

Provider settings now offer `m` to discover/change a model using the existing credential
reference, with Enter-to-keep and explicit manual-ID fallback. The first real settings replay
also exposed an unnecessary runtime installation check before any existing-route action.
That check now belongs to connection testing; replacing a key still uses normal setup.
Six actual settings runs (Go, Zen, OpenRouter, compatible endpoint, Anthropic, Gemini) changed
the model with an unavailable credential store. No key prompt appeared; independent persisted
profile comparisons found only the expected model change. All credential references, endpoints,
headers and protocols were retained. No secret was created, copied or read by these fixtures.

Replay commands use the consumers in [the guide](reproducers/open-issues/README.md): link
`selection.rs` and `ui.rs` from `issue151-build.jsonl`, and rebuild the configuration-upgrade
seed consumer with that same log. Then run `provider-switch.py` with `--binary-dir`,
`--seed-consumer`, `--ui`, `--old-ui`, `--probe` and `--output`. The old UI is the actual
pre-issue public TUI consumer linked for PR #149. `model-key-reuse.py` takes `--binary`,
`--seed-consumer` and `--output`. Both are operated with `uv run --with pyte python3`.
Artifacts are in `issue151-switch`, `issue151-switch-final`, `issue151-roles-final` and
`issue151-keys-final` under the campaign cache. All fixture daemons/terminals were closed.
