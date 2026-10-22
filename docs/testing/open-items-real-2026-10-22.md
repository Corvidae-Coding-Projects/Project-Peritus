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

## Issue #145: saved-context text and null-query labels

The shared human projection treated every `entries` array as directory paths. It now dispatches
actual working entries by their id/kind/status/text fields, preserves unfamiliar records as
JSON, and labels an empty state correctly. JSON-fragment pages retain their data and continuation
notice. Ordinary directory paths and trailing directory slashes keep their existing rendering.
This changes the human projection only; the tool result supplied to the model remains intact.

The actual tool-label emitter omits null arguments and quotes literal search strings, so a
null state/source query differs from a literal search for `"null"`. The TUI recognizes bare
context-read calls and cleans up the earlier null-query label spelling. Decoded tool strings
are sanitized after JSON decoding as well as before it reaches display formatting.

The native public-projection consumer crosses seven entry kinds, five statuses and four body
shapes (140 cases). The pre-fix renderer preserved none of those bodies, mislabeled empty state
as an empty directory, and hid fragment/unknown entry data. The changed renderer passes all
140 body/id/kind/status comparisons plus the empty-state, JSON-fragment, unknown-record and
unchanged-directory cases.

A real official-account/native TUI journey read a 301-line file and retrieved its exact archived
source with context_read/query=null. The resulting context-read card and source body were
observed in the terminal, the null-query label was absent, and the file digest was unchanged.
The first proposed UI exercise asked the read-only conversation role to modify/browse derived
working memory; that role correctly refused. The valid UI scenario uses its permitted exact
source retrieval path. Working-entry bodies are covered separately through the real shared
projection API rather than bypassing that role policy or presenting a fabricated UI result.
Already-recorded historical `?` detail strings are not reconstructed by this change.

Artifacts: `issue145-before.json`, `issue145-after.json`, `issue145-native` (the invalid-role
attempt) and `issue145-native-valid` under the campaign cache. The valid fixture metadata is in
`issue145-native-valid/fixture.json`; its actual terminal/daemon were closed and its owned source
and state are retained temporarily for the next display replay. The source fixture's optional
`PERITUS_FIXTURE_WORKSPACE` puts it outside the application's protected cache/state directories.

## Issue #147: compact long reads with explicit expansion

Completed workspace, skill and context reads now show an eight-line preview when their body
exceeds 16 lines. Very long individual lines also receive a width-dependent character preview.
A visible footer states the omitted line count, or total line count for a clipped long line,
and names `/details` to expand. That existing command expands the complete literal result;
repeating it or pressing Esc collapses it. Toggling resets the transcript scroll offset so an
expanded view's offset cannot leave the compact transcript blank. The command catalog describes
this behavior. The existing Ctrl-O filename-picker shortcut retains its original purpose.

The preview is applied after continuation chunks join and before syntax/Markdown formatting.
The footer is outside the Markdown body, including when a preview ends inside a code fence.
Failed/pending calls, commands, changes and assistant messages are not folded. No tool result,
model evidence, durable activity or source byte is removed by presentation folding.

The native replay reopened the actual 301-line source/context run from #145 at 80, 120 and
160 columns. The earlier TUI filled the default viewport with file tails at all three widths.
The changed TUI showed initial text and expansion guidance, exposed the complete tail under
`/details`, collapsed again, and recovered the compact viewport with Esc after scrolling an
expanded result. Public IPC snapshots before/after those display actions had identical durable
activities and phases; source hashes also matched. No inference was requested by those actions.

The first newline-only preview implementation failed on the actual archived context's very
long JSON line. Its native failure artifacts are retained; the final implementation also bounds
that preview by terminal width. A further real official-account request read Markdown with a
code fence spanning the preview boundary, a long Unicode line, a complete three-line file,
and an intentionally missing file. Native compact/expanded paging verified the Markdown/code
preview and footer, full Unicode/document tails on expansion, every short-file line by default,
and the missing-file failure. The replay's pyte observer initially crashed on an orphaned
wide-character stub; its screen reader now handles that observer condition. Reopening the
already-recorded results completed the checks without repeating provider inference.

Artifacts: `issue147-before`, `issue147-initial-long-line-failure`, `issue147-after`,
`issue147-variants` and `issue147-variants-final` under the campaign cache. Build with the same
bounded production command above and link `ui.rs` from `issue147-build.jsonl`. Run
`read-preview.py` using the guide's arguments and the retained `fixture.json`; the before case
uses `ui145`, and the final case uses `ui147 --fixed`. The optional `--variants` performs the
four real-provider reads; `--variants --variants-only` reuses completed evidence. All owned
fixture clients/daemons were closed. This does not claim Windows/macOS or live paid Zen/Go
inference. Issue #125 remains outside this campaign.
