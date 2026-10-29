# Local working memory

Product writer and fixer loops use local working memory by default. Reviewers have separate
archives and retain their own exact observations, but the existing role policy excludes derived
memory. Architect/design invocations retain their existing context path. No cloud memory service,
remote embedding call, or separate provider semantic-compaction request is used by the local path.
The primary task model still uses its configured provider and can propose memory updates during
ordinary work.

## Configuration

These settings are accepted by the daemon's strict TOML configuration:

```toml
[context]
fully_offline = false

[context.local]
enabled = true
engine = "deterministic"
semantic_backend = "disabled"
trigger_percent = 85
max_read_bytes = 16384
checkpoint_every_completed_batch = true
reuse_checkpoint_pages = true
```

Set `enabled = false` to select the previous developer-loop compaction path explicitly.
Local storage/validation failures stop progress; they never silently enable legacy or remote
compaction. Storage, schema, and process bounds are validated before opening the store.

Coding runs have no Peritus token budget. All role-visible working entries and their complete
source dependencies are rendered without a token allocation, including after reopening saved
memory. The legacy `working_state_max_tokens` and `retrieved_evidence_max_tokens` keys are
accepted and ignored so existing configuration files continue to load. Automatically retrieved
archive previews retain their explicit byte/read bounds and use available provider request space.
The legacy `retain_recent_messages` count is also accepted and ignored. A large raw archive
does not impose an eight-message ceiling: complete recent exchanges are packed against the
provider's actual input and transport capacity so successive retrieved pages can coexist.

Provider input estimates guide optional transcript compaction; exceeding an estimate does not
reject required working memory. Actual provider context windows and transport sizes still apply.
Current host instructions, literal requirements, and complete current tool exchanges remain
protected, with exact archived observations available through the memory tools. Coding, review,
and semantic-compaction responses request the negotiated provider output capacity, without
Peritus's former 32,768-, 4,096-, or 8,192-token ceilings. Token usage is still recorded for
observability; it does not exhaust a run allowance.

Fixer requests keep the complete literal conversation, approved design, and conserved review
ledger. Generated candidate diffs and check history use non-authoritative bounded previews after
reserving the retained required working closure. Their complete immutable sections remain
available through `developer_evidence_read`, with UTF-8 offsets, byte lengths, and SHA-256 digests.
Even when no preview fits, the initial request retains the exact section manifests. Projection
errors preserve the underlying context category, affected entry identity, and numeric bounds;
structural failures are not mislabeled as token exhaustion.

`fully_offline = true` additionally requires every configured route to be a compatible provider
with a literal loopback endpoint and disables automatic provider failover. DNS names (including
localhost), remote endpoints, and provider subprocess routes are rejected. Configure the local
model server itself for offline operation too; loopback admission cannot constrain that server's
own network behavior.

## Investigation state and exact evidence

`context_update` atomically proposes source-backed observations, assertions, hypotheses,
decisions, failed approaches, plans, or glossary entries. Updates use the announced
`base_revision`; archive/protocol bookkeeping does not itself invalidate that revision.
Unknown fields, forged authority, unavailable/cross-scope sources, dependency cycles, excessive
size, and stale evidence are rejected. File/candidate/conversation changes conservatively
invalidate dependent conclusions. Recognizable credential-shaped text is rejected as an
accidental-disclosure guard, not as a complete secret classifier. Do not put secrets in entries.

Live input revisions refresh this binding even while the same memory owner remains open.
Conversation-dependent conclusions become stale; their exact text and sources remain available.
Returning to earlier wording does not silently make them current again. Recovery first finishes
committed observations under their original epoch, then checks current input before new work.

`context_read` retrieves exact authorized archived bytes by `obs:NNNNNN` or fully scoped
handles. It also supports bounded literal search and revision-bound working-entry pages.
Follow `next_offset` for a byte range, `next_handle_index` for unreturned explicit handles,
or `cursor` for another search/state page. Binary or split-UTF-8 ranges use hex encoding;
source digests and total byte lengths identify the original artifact. Small response limits
that cannot fit metadata return a rejection rather than a non-progressing page.

`context_sources` pages complete readable source descriptors within a fixed archive snapshot.
Its cursor is scope-bound, and current permissions and source restrictions are rechecked on
every page. It returns source handles and original byte identities; use the named retrieval
tool for exact bytes. New observations do not restart an in-progress descriptor scan.

### Decision views

`[context.local] decision_view = "conservative"` is the default decision compiler. Set
`decision_view = "history"` to opt out. Host-bound entries require a committed
checkpoint-v6 reader guard; older supported state remains readable and migrates conservatively.
Setting it back to `"history"` retains that guard, current committed progress and exact sources.

The compiler retains current host policy, literal input, control and pending outcomes. It
prioritizes active working dependencies within the negotiated provider envelope, separates
exact original tool bytes from complete current native exchanges, and coalesces only proven
identical successful inspections. Errors and unknown/effect outcomes remain distinct. Partial
raw JSON cards announce their original byte offset and retrieval path; they are not complete
file pages. The full archive remains retrievable when it exceeds model capacity.

Model-authored scope cannot establish independence. The compiler annotates derived projections
with conservative whole-view/candidate dependencies, and every supplied file read contributes a current binding
even when a model omits that path from its declaration. If a supplied source is excluded by
current restrictions, unscoped derived/native history is withheld rather than quoting it into
the new view. Reviewers still receive only their own exact observations and no writer scratch.

Unproven opaque/native replay dependencies and incompatible adapter envelopes retain the
existing history selector when current restrictions permit it. Restricted views instead page
readable exact sources. Neither path replaces fresh grounding, full candidate checks, independent
review or settlement evidence. Reconstruction identities are diagnostics, never cached verdicts;
the complete final request fingerprint includes options, reasoning, tool choice and schema order.
Current input admission still governs submission.

Durable host bindings come from the exact admitted originating request. Changed or unknown
inputs make dependent conclusions stale without deleting their exact text or declared progress.
Model declarations cannot narrow those constraints or restore freshness. Ordinary events keep
their existing encoding; host events use version 3 and bound-entry pages use version 4. Actual
continuation, migration, recovery and rollback evidence is recorded in the
[I4 report](testing/execution-efficiency-i4-decision-views.md). Measured benefit varies by workload.

Sequential read-only plans are also enabled by default with `read_only_plans = "sequential"`.
Set `read_only_plans = "disabled"` to opt out while retaining direct tools and exact retrieval
of prior plan results. Omitted policy fields use these defaults; explicit settings take
precedence. Existing automatic decision-view fallback and all authority checks remain.

### Experimental shared candidate observations

Set `[context.local] candidate_observation = "shared"` on Linux to derive managed repository and source
identities from the same complete captured preimages. A second fresh full capture must match
both identities before a candidate checkpoint is observed. Each material boundary still scans
current contents, kinds, modes, HEAD and nested repositories; no prior-boundary candidate digest,
filesystem notification or metadata-only token substitutes for that scan. In-place task scopes
retain their existing observation path.

The default `"separate"` preserves the original repository/source/repository capture sequence.
Returning to it requires no state migration and retains current history. Other platforms always
use Separate even if Shared is requested. This changes local capture work, not qualification
requirements or digest encodings. The [I6 report](testing/execution-efficiency-i6-freshness.md)
records local savings, current acceptance/restart/rollback and explicit platform coverage.

Tool outputs remain untrusted evidence. Host-owned source metadata overwrites any forged
`local_context` annotation only after the original output is archived. Neither memory updates
nor reads confer workspace-grounding credit, permission to execute effects, or acceptance.
Every fresh invocation reacquires required workspace grounding. Reviewers cannot read writer
archives or create/read derived entries.

Assembly renders structured records and source-backed evidence, never just the previous summary.
Current instructions and corrections, pending operation state, unresolved contradictions, failed
approaches, and open plans receive protected treatment. Recent exchanges remain complete.
Older evidence has explicit truncation and exact-source retrieval. Original authorized tool
outputs are archived before model-facing output limits; they are not a credential-store scan.

## Durability, inspection, and recovery

State is stored beside the configured trace as `TRACE.context/{writer,fixer,reviewer}`, outside
the editable workspace. Each lineage has one owner, a C0 journal (namespace 3401), immutable
artifact bytes, canonical C6 state/events, and a transactionally published checkpoint manifest.
Artifacts include the exact C5 message archive, transcript/source indexes, and validation.
Checkpoint roots retain source artifacts through normal C0 garbage collection. The archive
uses the artifact size representation bound imposed by SQLite's signed 64-bit sizes; this
local store imposes no separate byte quota. Capacity/IO failures remain explicit; entries and
old sources are not silently deleted to make space.
Run retention/deletion must include this trace-adjacent directory; no cross-task sync is added.

Immutable observation/source pages are reused by default. Reuse requires the same lineage,
record range and immutable page identity, followed by full artifact size/digest verification.
Changed tails are encoded normally; every checkpoint retains its complete dependency roots.
Both the completed-batch and next-request checkpoints remain, with their existing fsync and CAS
boundaries. Set `reuse_checkpoint_pages = false` under `[context.local]` to use the original
encoding/publication path against current committed history. No new storage format is needed.
Non-regular artifact objects, including symlinks to matching bytes, are rejected by verification.

```sh
peritusd context-inspect --trace /absolute/path/run.trace \
  --run 01010101010101010101010101010101 \
  --workspace 02020202020202020202020202020202 --role writer
```

Use the actual run/workspace identifiers; role is writer, fixer, or reviewer. Inspection is
read-only and works while the lineage owner is active. It validates the published artifacts and
returns readable messages plus `canonical_view_archive_hex`, source references, and validation.
It shows the **last published view**, not a speculative future view. `uncovered_tail` explicitly
reports newer events that require recovery/assembly before the next request.

Trace tag 7 is a versioned exact tool observation identified by scope, invocation, and monotonic
tool sequence (provider call IDs may repeat). Recovery ingests committed trace observations
idempotently. Trace tag 6 records exact checkpoint references and validation; legacy tag 3 remains
unchanged. Old traces without checkpoint artifacts cannot promise exact restoration.

There is no cross-store exactly-once transaction. Tool trace persistence precedes memory
ingestion; checkpoint publication precedes installing the next model-visible view. A crash in
the trace-to-memory gap replays one observation. Failure before publication retains the prior
checkpoint; failure writing its trace reference after publication stops the loop even though
the newer checkpoint exists. Torn trace tails recover the valid prefix and stop with a named
repair requirement; they are not automatically truncated.

Pending or unknown operations remain unresolved. Memory recovery never redispatches effects;
use existing process/effect recovery. Back up the complete run state before any operator repair.
Inspection never creates missing stores, recovers writers, or changes checkpoint heads.

Validation reports generation, source/archive bytes, estimated input tokens and savings,
stale/omitted entries, pending operations, retrieval calls, and local-compactor failures.
Archive bytes count logical source references, not deduplicated physical disk usage.

## Optional local subprocess

`semantic_backend = "local_process"` requires a complete `local_process` table containing
`executable`, `model_path`, `timeout_millis`, `max_input_bytes`, `max_output_bytes`,
`memory_bytes`, and a platform-tagged `sandbox` table. Executable and weights must be
preinstalled absolute regular files outside both editable workspace and run storage.
Weights directories and automatic downloads are not supported.

The executable is invoked as `EXECUTABLE --model MODEL_FILE`. It receives one bounded JSON
document on stdin (closed after writing), containing entries, selected observations, and an
`output_schema`. It must return exactly one JSON `context_update` proposal on stdout and exit
successfully. Use a self-contained executable using only admitted system libraries; arbitrary
interpreter installations, GPU device access, and auxiliary model-directory files are not
implicitly granted.

Native prerequisites are explicit:

- Linux: `platform = "linux"`, `bubblewrap`, `helper`, and an exact delegated cgroup-v2
  `cgroup_root`; the host-wide cgroup root is rejected. Namespaces, seccomp/Landlock and the
  existing Peritus Linux helper must pass native admission. A private filesystem contains only
  declared inference inputs, read-only runtime libraries, and bounded scratch space.
- macOS: `platform = "macos"`, installed `helper` and `seatbelt` (sandbox-exec) paths.
  Existing Seatbelt/resource admission must succeed.
- Windows: `platform = "windows"` and installed `helper`; AppContainer/Job Object admission
  is required. Explicit external input ACLs allow reading/execution, never workspace-style writes.

The existing C2/C3 gateway enforces cleared environment, no network/secrets, bounded stdin/output,
deadline, process-tree ownership, and resources. Missing prerequisites/weights, rejected native
admission, timeout, malformed output, or invalid deltas retain deterministic local assembly.
No raw subprocess or remote fallback exists. Input/output artifacts and failure counters support
audit. Local unit tests cover malformed proposals and injected failures; native filesystem tests
exercise isolation. These checks are not a real-model quality or performance benchmark.

## Qualification boundary

Regression fixtures cover source retrieval beyond old previews, repeated compaction retention,
late corrections, invalidation, trace-gap replay, publication failure, role isolation, provider
recovery, pending operations, and zero auxiliary provider calls on the local D0 path.
Cross-platform runners qualify platform compilation and native fixtures where available.
A paired task-quality/cost comparison requires a separately declared campaign with fixed models,
budgets, and repeated runs. This feature does not change or claim results from frozen S9.
