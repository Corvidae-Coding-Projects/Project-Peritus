# Stage 5 provider boundary campaign

## Actual adapters and modes

| Target/mode | Actual constructor and owner | Controlled dependency |
| --- | --- | --- |
| Private `openai_http` | `OpenAiProvider::new`, existing private `OpenAiConfig::from_endpoint`, Responses request/stream decoder | Owned loopback TCP and ordinary hardened Reqwest |
| Public Anthropic | `AnthropicClient::new`, Messages request/stream decoder | Owned loopback TCP and ordinary hardened Reqwest |
| Public Google Generate Content | `GoogleClient::new`, exact `GeminiGenerateContentV1` profile, generate projector/decoder | Owned loopback TCP and ordinary hardened Reqwest |
| Public Google Interactions | `GoogleClient::new`, exact `GeminiInteractionsV1` profile, interactions projector/decoder | Owned loopback TCP and ordinary hardened Reqwest |
| Public Compatible Chat | `CompatibleClient::new`, explicit Chat contract | Owned exact loopback operation; explicitly mapped retry statuses |
| Public Compatible Responses | `CompatibleClient::new`, explicit Responses contract | Owned exact loopback operation; explicitly mapped retry statuses |
| Public Codex account runtime | Pinned `CodexExecutable`, `CodexRuntimeConfig`, `CodexRuntimeProvider` | Owned synthetic executable; real process transport, login-status, argv/stdin, JSONL/output-file decoder |
| Public Claude account runtime | Pinned `ClaudeExecutable`, `ClaudeRuntimeConfig`, `ClaudeRuntimeProvider` | Owned synthetic executable; real process transport, auth-status, argv/stdin, structured-result decoder |
| Public OpenCode runtime | `OpenCodeRuntimeProvider::with_executable_path` | Owned synthetic executable with version 1.19.0 reply; real private config/process/JSONL decoder; Zen/Go and legacy/once-only layouts |

The five active adapter packages and account-runtime variants are confirmed in Cargo membership
and daemon `component/providers.rs:instantiate`. No live service, official client installation,
account sign-in, user credential, external inference or user workspace is involved. Credentials
are clearly synthetic and are sent only to the owned loopback endpoint or private owned fixture
storage. Synthetic executable replies are external transport dependencies; they do not implement
the adapters or reducers under test. The Python child is embedded in fingerprinted Rust source.

## Private owner hook

Only a cfg(test) child module in OpenAI `config.rs` grants the discovery module access to the
existing private endpoint constructor. Production builds expose no new constructor, feature,
transport bypass or API. The owner uses ordinary production Reqwest and decoder implementations.
Dev dependencies are proptest, sha2 for shared fingerprinting, and tokio runtime support. The
shared owner runner is `support/stage5_owner.rs`; root owns controller/build-receipt integration.

## Observable promises and independent expectations

Selected models are changed through the actual `ModelProvider::select_model`; requests must
retain that exact selection in captured HTTP JSON/route or native argv. Supported explicit effort
must retain its exact spelling. Unsupported effort must reject before submission, rather than
silently selecting another effort. Initial model identity is deliberately different.

Completed text and completed inert tool-call JSON must match the independent generated payload.
The observer keeps its own item/call map, byte buffers, completion flags and exact expected JSON.
It rejects incomplete JSON becoming a completed call, repeated call/item identities and argument
deltas after item completion. No test tool is actually executed: a complete shape is still inert
and host execution authority remains external to C5.

The same envelopes also pass through the real `ResponseReducer`, the ordinary C5 downstream
consumer. Its usable terminal and completed text/tool results must reproduce the independent
payload; a merely successful adapter terminal cannot conceal missing downstream output.
Envelope identities/digests, original content bytes and downstream transitions are retained in
failure traces. Exact raw identity repeats are checked separately from conflicting identity bytes.

This separation follows `docs/c5-model-providers.md:77-85` and the public reducer contract: exact
identity/digest repeats are ignored; reused identity with different bytes fails. Rejection may
occur in an adapter or in the actual downstream reducer. An early expectation that every adapter
must itself produce a failure terminal for reused SSE IDs was too strong and is retained as a
historical harness finding. Repeated SSE cursor IDs are not assumed to be globally unique solely
because the current Anthropic dedup implementation pairs ID and digest. The explicit normalized
C5 identity and ordinary consumer determine acceptance. Semantically changed response/thread/
session identities are separate negative cases.

Known input/output/total usage must match scripted counts; unknown usage stays unknown, including
Claude runtime's unavailable total counter. Every represented cache, creation, reasoning, tool and
cost dimension is independently checked, and final counters must agree with the real reducer's
high-water observation. Compatible minimum-safe fixtures carry only their documented common
aggregate fields; nested provider-specific usage extensions are outside this fixture family,
rather than being injected and treated as known. OpenCode's cache input partitions and reasoning
output partitions are constructed separately. Its one supported format correction has
independently summed attempt usage. Claude tool replies carry independently verified nonempty
commentary as required by its current final-result decoder; all other pure tool replies carry
empty public content.

## Inputs, histories and controlled effects

Serde cases vary provider mode, text/tool output, Unicode/escaping, selected model, supported and
unsupported effort, optional usage, byte-write schedules, present/absent SSE IDs, malformed frames,
missing terminals, partial tool transport, exact duplicates, conflicting IDs, changed response
identity, truncation and extra terminals. HTTP retry uses an explicit non-accepting 500 followed by
the same request. Runtime format correction is limited to OpenCode's actual documented owner.
Native lifecycle duplicates are distinct from provider-event retransmission: final-result-only
account adapters do not gain provider-event deduplication claims.

Positive text generation is nonempty; empty argument strings remain representable separately.
Constructive mode profiles avoid spending most traces in unsupported-admission rejection. HTTP
write schedules split actual network bytes, including UTF-8 and SSE fields; kernel/read-side
coalescing is permitted and no exact physical receive segmentation is claimed. Cancellation is
best-effort: HTTP is checked before submission and between normalized pulls; owned runtime
cancellation waits for its own child ready marker before cancelling. The Codex process is
pull-driven, so its stream is polled while that independent ready watcher runs; the watcher is
joined only after the actual operation settles. Failure to reach the ready marker is a fixture
failure, never successful cancellation coverage. The OpenCode fixture also implements the real
bounded model-catalog command with matching selected IDs before inference. All workers/resources have
an owner, deadlines and cleanup. The fixture never kills an unrelated process.

## Source/caller coverage and gaps

Read current public crate roots/manifests/configuration/profile validation; HTTP clients,
request projectors and stream dispatch; OpenAI stream sequence/output/terminal modules;
Anthropic content/message/state normalization; Google Generate/Interactions/state/value paths;
Compatible Responses/Chat/tool/usage paths; account executable/config/request/output/stream and
process invocation paths; OpenCode profile/config isolation, JSONL/private envelope, request layout,
version pinning and bounded correction. Existing checked-in wire fixtures establish input shape,
not behavioral evidence. Also read provider-core stream/process/HTTP interfaces and the actual
daemon provider registry; the C5 grammar and reducer public contracts ground the oracle.

This is bounded interface coverage, not a claim to have read every line or exercised all provider
features. Live capability discovery, real credentials, hosted gateway network behavior, background
storage/continuations, image/audio/document inputs and non-Linux process behavior are outside this
first campaign. Three concrete defect classes were reproduced and repaired on isolated local
fix branches: one-to-many raw ID projection dropped valid derived events in Anthropic/Google;
clearing numbered queued events on cancellation left local sequence holes in Anthropic/Google;
and compatible Chat mutated private tool assembly before raw retransmission deduplication.
Readable public consumers, original-failing/fixed-passing receipts, minimal patches and bounded
findings are preserved separately. The provider worker composes all three repairs; no commit or
push is part of this campaign.

## Calibration and evidence state

Two canaries change only outbound selected-model serialization: Google Interactions for public
`providers`, and first-party Responses for private `openai_http`. Their exact generated request
metadata exposed the mutation even when the scripted response remained valid. Both final
baseline/mutant/restored cycles passed with stale-binary refusal, two generated failing seeds,
exact source restoration and six witness replays each.

Early worker evidence is under `runs/stage5-providers-early`. Historical initial compiler,
manifest/setup, adapter-only and overstrong oracle attempts are retained. They do not qualify the
final campaign. `full-examples3` ran all 120 public explicit cases against the composed repairs,
with source/binary hashes unchanged during native execution: all passed. This includes all eight
public modes, both OpenCode services/layouts, actual reducer output, supported/unsupported effort,
retry/correction and cancellation controls. `owner-examples3.receipt.json` records all 15 revised
private OpenAI HTTP explicit controls passing through its cfg(test) owner. These worker smoke
checks remain separate from the selected cold final campaigns. Final public `providers` passed
120 fixed examples and 1,024 generated histories; final owner-only `openai_http` passed 15 fixed
examples and 512 generated histories. Every selected report binds final source and executable
identity.

Production strict Clippy for the three changed provider packages passed with `-D warnings`.
The standalone examples-only consumer reported unused campaign exports and nine local fixture
style diagnostics; those fixture style diagnostics were repaired. Its unused exports are consumed
by the central campaign runner. The central cold runner and owner test hook both passed strict
lint and native qualification.
