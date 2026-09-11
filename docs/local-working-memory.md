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
retain_recent_messages = 8
working_state_max_tokens = 4096
retrieved_evidence_max_tokens = 4096
max_update_operations = 32
max_entry_bytes = 2048
max_read_bytes = 16384
checkpoint_every_completed_batch = true
```

Set `enabled = false` to select the previous developer-loop compaction path explicitly.
Local storage/validation failures stop progress; they never silently enable legacy or remote
compaction. Bounds are validated before opening the store. Working-state allocations adapt to
the selected provider's input capacity, including tool schemas and framing.
`working_state_max_tokens` caps optional working detail and is the preferred allocation for the
whole working view. Required roots and their complete dependencies may exceed it using only the
additional headroom they need. This also applies when reopening existing memory after a restart.
If protected instructions or required evidence closure cannot fit the provider's actual input
capacity, assembly reports a capacity error; it never drops those entries to continue.

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

`context_read` retrieves exact authorized archived bytes by `obs:NNNNNN` or fully scoped
handles. It also supports bounded literal search and revision-bound working-entry pages.
Follow `next_offset` for a byte range, `next_handle_index` for unreturned explicit handles,
or `cursor` for another search/state page. Binary or split-UTF-8 ranges use hex encoding;
source digests and total byte lengths identify the original artifact. Small response limits
that cannot fit metadata return a rejection rather than a non-progressing page.

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
uses a one-GiB store quota, 64-MiB artifact limit, and bounded event/observation counts. Capacity
exhaustion is explicit; entries and old sources are not silently deleted to make space.
Run retention/deletion must include this trace-adjacent directory; no cross-task sync is added.

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
