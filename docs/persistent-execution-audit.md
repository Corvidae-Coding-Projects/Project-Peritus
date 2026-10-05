# Persistent execution and session recovery

This repair starts from signed v0.0.5, commit
`90925794845121dd9278d06c5eec92a5bbfd85e9`, on `fix/persistent-execution`.
It removes default work-duration deadlines and preserves native and host context
under the logical run and role. No new release version or signed release is claimed.

## Incident diagnosis

The durable evidence for run `372c08a4ee81696b578943325d2738ac` is its
length-framed `product-runs/<run>.trace` and `workbench-v1/runs/<run>.json`
under the user's daemon state directory. The run lasted 2,423.25 seconds,
made 37 model requests and 189 application tool calls, and recorded two role
recoveries. It consumed 1,587,998 normalized tokens. It performed 26 conversational
inspection calls, 162 design inspection calls and one pipeline handoff; no design
was published and the implementation role never began. The eventual terminal
state records user cancellation, rather than an agent-issued cancellation.

The failure chain is grounded in both the trace and the v0.0.5 callers:

1. `peritus-provider-core/src/process.rs` imposed a 600-second production native
   subprocess deadline. Two Codex responses, 19 and 30, carry normalized timeout
   failures with `SendingBody`, `MaybeAccepted`, `CallerDecision`, and diagnostic
   `openai.codex_runtime.transport`. The adapter maps the subprocess deadline to
   that failure. The top-level run had no ordinary interactive horizon.
2. `peritus-agent/src/developer/retry.rs` supplied a 120-second elapsed retry
   envelope. A 600-second failed call exhausted that envelope before a retry
   could use it. Outer role recovery then started another designer invocation.
3. `peritus-product-runner/src/design.rs` supplied no durable local context port
   to the designer. Recovery constructed a new executor and prompt without its
   completed inspection history. The earlier writer context directory did not
   contain the designer's reads.
4. The Codex adapter explicitly passed `--ephemeral`, used temporary invocation
   storage, and did not resume an exact native thread. Process failure erased
   the native continuation and partial diagnostic output.
5. The local-memory selector also surfaced older failed tool outputs but not
   recent successful reads unless a working entry cited them. That omission
   would still cause needless rediscovery after adding a durable designer port.

The prior top-level removal was incomplete: lower transport, subprocess, tool,
setup and UI defaults remained. This was a lifecycle and identity design error,
not a user-selected goal budget. Lost ephemeral output cannot establish why the
underlying provider had not finished when the old 600-second timer fired.

Inspection also exposed a possible pipe deadlock: a parent could finish writing
stdin before beginning to drain stdout while the child did the opposite. The
transport now pumps stdin, stdout, stderr and child completion concurrently.
That defect was reproduced independently; it is not established as this
incident's trigger.

## Ownership and persistence

The host binds every ordinary developer request to
`<trace>.context/<role>/provider-sessions`, including designer, chat, planning,
review, writer and fixer calls. This binding is independent of whether local
working-memory assembly is enabled. A provider namespace incorporates exact
profile identity, profile revision and model; a file lock excludes simultaneous
owners. Role and task separation prevents a reviewer from inheriting writer
history. Semantic compaction requests do not inherit the developer session.

Codex retains each native JSONL invocation as it is read, synchronizes it before
continuing, recovers its exact `thread.started` identity, and uses `exec resume`
with that identity. Claude synchronizes a generated session UUID before inference,
uses `--session-id` initially and `--resume` thereafter, and validates the returned
session ID. Neither adapter disables native session persistence. Missing or
conflicting identities fail explicitly; neither uses a global most-recent session
nor silently substitutes a fresh context after an ambiguous launch.

Native history is historical evidence. Each request still supplies the complete
current host policy, task, tools and authorized message projection. Native
continuation is not a provider event-cursor resume capability and does not grant
tool authority. Completed tools remain completed; pending effects require host
reconciliation rather than automatic redispatch.

The designer holds one durable local-memory owner across recovery invocations.
Recent successful tool outputs from prior invocations are selected as bounded,
non-authoritative evidence with exact retrieval handles. They are not replayed as
fresh tool executions or accepted as new repository-grounding credit.

Resource accounting is separate from execution ownership. An attempt can be
charged with no elapsed-time capacity reservation. The tool and process gates
still require exact committed intent, revision, capability, accounting and
single-use dispatch. They no longer require a fictional positive time reservation
just to keep an otherwise empty accounting reservation in its Held phase.

## Deadline coverage

The audit searched Rust source, product configuration, native launchers and
packaging, then followed the matching production constructors and callers.
The inspected owners are listed below; this is not an exhaustive functional
certification of unrelated repository code.

| Owner and callers | Changed production behavior |
| --- | --- |
| Provider core subprocess and HTTP transports | No default process, connect, response-header, body-idle or total HTTP deadline; bounded bytes and cancellation remain. |
| Model retry planner and native/API provider configurations | No elapsed retry envelope; explicit attempt and backoff contracts remain. |
| Account/hosted catalogs, authentication checks, onboarding, launcher setup and updates | Removed inferred readiness, discovery, installer, verification, fetch, extraction and deferred-launch time limits. |
| App client, CLI, TUI and web daemon/Git bridge | Pending work stays pending until completion, disconnection or cancellation. CLI timeouts are explicit opt-in. |
| Built-in shell, filesystem, Git and quality catalogs | No default tool duration bound. An omitted command deadline stays absent through routing, process policy, sandbox admission and result receipts. |
| Quality discovery and gate plans | No discovered-check default duration. Explicit caller-selected check deadlines remain representable. |
| Terminal and workbench preview launchers | Removed implicit startup, preview lifetime and screenshot process deadlines. |
| Linux/macOS launch acknowledgments and Linux capability probes | Removed imposed readiness waits that converted slow launch or probing into failure. |
| Linux/macOS/Windows resource adapters | Absent wall/CPU limits stay absent; no synthetic maximum or CPU rlimit/job timer substitutes for absence. |
| Network proxy and local compactor adapters | Removed hidden clamps on caller-declared connection and compactor durations. |

Retained time-related operations have different owners and purposes:

- Capability, credential, lease and authority validity are admission/security
  facts, not an elapsed-work kill switch.
- Shutdown and cleanup grace periods start after explicit stop, owner loss or
  termination. UI close, worker teardown, process-tree cleanup and cgroup cleanup
  can escalate to finish cancellation; they do not limit active work.
- SQLite busy waits, exclusive-owner contention, readiness observation windows,
  polling, redelivery, progress notices, update cadence and retry backoff govern
  coordination or observation rather than run lifetime.
- Explicit embedding/benchmark horizons, command/check deadlines, network bounds,
  local-compactor settings and plugin quotas remain caller-declared contracts.
  Ordinary daemon launch supplies no run horizon. No interactive goal-budget UI
  or numeric maximum masquerades as unlimited execution.
- Qualification fixtures, hosted CI jobs and test watchdogs remain finite so failed test subjects
  can be diagnosed and reaped. They do not supply product execution defaults.

## Verification and operational evidence

Required checks cover elapsed virtual hours without default provider/HTTP expiry,
explicit finite deadlines, cancellation, concurrent pipe drainage, native journal
retention, exact-ID recovery and scope rejection, designer recovery, authority
single-use enforcement, and absence of command wall/CPU ceilings through receipts.
The final workspace, formal, native-account and installed-binary evidence is
recorded in the local repair evidence directory beside the reinstall backup.

Validation on 2026-10-05:

| Check | Result |
| --- | --- |
| `cargo test --workspace --locked` | 3,789 passed; 21 explicitly ignored; no failures. |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed. |
| `cargo xtask all` | Passed: 86 packages, 4,867 source/trust files, 14,948 ordinary-safe entry points, 273 documentation files and 147 pinned actions. |
| Strict `cargo verus verify` for all 25 changed verification-enabled packages | 4,041 verified obligations across root/dependency invocations; no errors; `--no-cheating --rlimit 20`. |
| Native-provider `--all-features` suites | 83 passed; two account-dependent live tests excluded from the default suite. |
| Authenticated installed Codex and Claude runtimes | Each completed two calls with the same exact native ID after provider reconstruction; both live tests passed separately. |

The original user configuration and daemon state were backed up and verified
before reinstalling v0.0.5. The repair installer replaces product binaries without
deleting conversations or state. Native sessions erased by the old ephemeral
mode cannot be recovered retroactively. Native daemon crash, power-loss and
cross-platform production qualification remain distinct from the focused tests
and reconstructed-provider live checks performed for this repair.
