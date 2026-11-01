# Stage 5 plugin and MCP boundaries

Implementation draft with successful first compilation and native fixed examples on the recorded
earlier snapshot. Final source qualification, generated search and controlled mutation cycles
are pending. A later framing clarification has not yet been compiled; original receipts remain
historical. Targets are `plugins` and `mcp`, not a whole G3 or daemon-authority certification.
All transport/process fixtures are owned disposable resources. No external service, credential,
user module directory, live daemon or application workspace is used. No Crosslink is used.

## Sources, actual callers and compatibility

Read active manifests and package/source waypoints for the SDK, host and MCP, plus the host's
transport diagnostics and SDK manifest/protocol child waypoints. Selected implementations read:

- SDK `lib.rs`, `framing.rs`, `identity.rs`, `payload.rs`, `canonical.rs`, `error.rs`, manifest
  construction/validation/canonical/trust methods, manifest wire serializers and protocol DTOs,
  request/response tagging and response decoding. `module.rs` and the broad `module_host.rs`
  laboratory/worker contracts are not covered by this campaign.
- Host `host.rs`, `authority.rs`, `quota.rs`, `trust.rs`, `transport.rs`, discovery public
  constructor/load/canonical-artifact/hash paths, cancellation and stable failure declarations.
  Host process ownership is process-wrap session/job + explicit terminate/wait. It is not an OS
  sandbox. This target executes only a disposable scripted Process artifact.
- MCP `lib.rs`, complete `jsonrpc.rs`, `framing.rs`, `protocol.rs`, `server.rs`, `bridge.rs`,
  `bridge/wire.rs`, cancellation and both error constructors. The server owns wire validation,
  lifecycle, nonblocking admission, active IDs, cancellation forwarding, list slices and response
  projection. It delegates present authority and argument schema semantics to AuthorityBridge.
- Current product caller: `peritus-product-runner/src/execution/modules/native/runtime.rs` stages
  source into private native state, checks enabled module/source identity, discovers its staged
  plugin, configures exact trust and uses LiveAuthority to compare current registry, user revision,
  plugin/capability and read permission. Those application checks are source grounding, not executed
  authority proof in this target. No current concrete MCP AuthorityBridge implementation or
  application McpServer construction was found by source search; the public embedding seam and
  `docs/g3-extensions.md` describe that required composition.

The retained `docs/testing/reproducers/module-foundation-consumer.rs` was read for constructor
orientation; its old results and uniformly granting LocalAuthority are not used as qualification.
Archived conformance/formal commands in `docs/g3-extensions.md` do not restore retired packages.
Current `compat/` has no plugin/MCP corpus found by path search. These consumers retain independent
closed-envelope/wire fixtures in their source and exact generated cases through the ordinary
runner. This is a new bounded compatibility fixture set, not evidence of every historical wire
version. `inventory/stage5-extensions-variants.json` records selected modes and gaps.

## Plugin SDK independent oracle

The SDK branch constructs small integer/string/object/array values. A queue-based traversal
independently recomputes depth, complete collection-member totals and UTF-8 byte lengths, then
checks canonical-byte demand. The expected frame is four explicit big-endian length bytes plus
ordinary independently serialized input JSON; exact length, one-byte-short limit, short header,
zero/oversized declaration, truncated body, trailing bytes and malformed UTF-8/JSON are separate
partitions. No production canonical, payload-validation or framing helper supplies expected bytes.
Ordinary serde_json is a serialization primitive; this is not an independent JSON parser proof.

An independently specified complete manifest value determines expected canonical JSON and SHA-256.
The trust preimage is the documented domain tag, explicit unsigned-manifest length, exact manifest
bytes and independent artifact digest. Unknown manifest fields, traversal and duplicate capability
names reject. All six quota dimensions are compared with an explicit expected intersection table.
A closed ready-response fixture preserves exact version, identity and tag; unknown envelope fields
reject. Protocol compatibility is checked by explicit membership intersection over small intervals
and one maximum-u16 endpoint case, without large allocation. Some checks are fixed partitions
inside the SDK branch rather than claims of independently generated arbitrary manifests.

Bounds: nesting four, width eight, text 64 bytes, frame/payload sizes small. No module-worker or
laboratory request family, full signature policy, every nominal constructor, huge representation
ceiling, arbitrary floating-point family or complete closed-protocol variant matrix is qualified.

## Real PluginHost history and dependency port

The actual public discover/DigestTrustStore/PluginHost interfaces read a real temporary manifest
and executable artifact. Its explicit artifact SHA is checked against the script's exact source.
Trust is absent, exact, wrong artifact or wrong manifest; successful startup must expose the exact
Ready snapshot and anchor. No permissive fallback trust verifier is supplied.

The authorization dependency fixture is an explicit reference table of one synthetic plugin,
capability, actor/session, generation and enabled state. It denies missing/stale/exact-field
mismatches and is changed only by replayable Authority operations. The host's ordinary typed
InvocationGrant observation is returned only for that exact table row. This is dependency-port
qualification of host admission and context forwarding, **not** evidence of committed B1/C4
capability authority, an opaque permit, daemon registry authorization or an external effect.
The product LiveAuthority adapter's actual composition is outside this target. Parent approved
this bounded seam where a practical public committed-authority token is not required by the host
interface. The SDK constructor itself cannot mint a B1 capability.

A separate phase/count model predicts absent/Ready/Failed ownership, lifecycle-request use,
per-invocation admission, stale/denied/undeclared/pre-cancel rejections and explicit stop/start
reset. The exact mediator-call count must match eligible declared requests, including attempts
later refused by lifecycle quota. Real child input frames are logged to an actual file by the script and
compared against independent request identity, payload, actor/session, role, capability set,
generation and deadline. An unsent rejection must produce no child invocation log entry.

The executable speaks manual SDK frames with deterministic chunked output and implements echo,
truthful invalid-input failure, cancelled status, wrong correlation ID, wrong protocol version,
illegal invocation status, oversized header, partial response then exit, explicit exit, blocked
request, and output-byte boundaries below/at/above the host limit. Child stdout is actual protocol
traffic, not a simulated host success. Wrong frames or lost transport require Failed and explicit
RestartPlugin; effects are never automatically replayed. Independent snapshots compare counters,
identity and phase after every step. Lifecycle-cap rejection must preserve usable state.

After-send cancellation waits for the real invocation log marker and observes exactly one active
host permit before cancelling. A blocked child also supplies a real wall-timeout partition.
Stop/start and later ordinary exchanges test continued use. Every case explicitly stops the host;
PID plus Linux /proc start-time observations distinguish disappearance of the owned child from
PID reuse. The target does not signal unrelated PIDs or manufacture a child wait success.

Histories cap at 48 explicit operations, six lifecycle requests per lifecycle, one small manifest,
4096-byte frames and 128-byte result quota. Startup/shutdown and invocation waits are bounded.
This is a Linux Process-mode campaign; WASI/Wasmtime, Windows jobs, macOS confinement, arbitrary
forked descendants, noisy stderr, all launch/path races, arbitrary concurrent starts/stops and
queued-authority changes remain explicit gaps. The ordinary host serializes admission, so this
campaign does not claim a separately activated concurrent-request quota beyond actual one-active
cancellation observations. Discovery path/race and full signature-policy matrices remain separate.

## MCP independent wire/lifecycle model

The actual McpServer owns one endpoint of an in-memory local duplex stream; the consumer owns
both client halves and sends deterministic small byte chunks. A fresh BridgeContext supplies
known typed actor/session/generation. The bridge port returns fixture catalogue/resource/prompt
observations, records every exact context/name/argument call and denies all tool effects. The
fixture never fabricates a C4 permit or successful external tool effect. It does not qualify
concrete daemon authority or JSON Schema evaluation; raw arguments must cross this boundary
unchanged for the authoritative bridge to decide.

A separate three-state model requires valid initialize then initialized notification before
ordinary methods; ping remains available. Initialization rejection permits corrected continuation;
reinitialization rejects. Independent fixture lists determine exact pages, cursor boundaries and
nextCursor bytes. Empty/invalid names and malformed envelope fields must not route to the bridge.
Known resource and prompt bytes are checked exactly. Valid tool calls preserve the explicit
Authorization failure and stable peritusCode. Notifications are fenced with an ordinary ping so
silence is not mistaken for a successful response. String/numeric request IDs alternate.

A cancellation wave holds one actual resource future after its admission signal. Duplicate active
IDs reject; a second wave occupies both real semaphore slots and requires an excess request to
refuse while cancellation notifications remain responsive. Exact cancelled IDs/results are
compared without relying on response order, and the same ID must work afterward. The server task
is explicitly closed/joined, with bounded abort/join on a fixture failure. These are controlled
finite schedules, not arbitrary concurrency or transport service deployment.

Framing cases distinguish partial EOF, empty line, unambiguously oversized bytes (4097 bytes
before LF or CRLF), malformed JSON and invalid UTF-8 with complete newline. Valid ping frames
independently reach the exact 4096-byte JSON body with LF and a 4095-byte body with CRLF. The
raw-line versus delimiter-excluded interpretation for an exact 4096-byte body plus CRLF is
ambiguous in the public wording and is not asserted as a correctness requirement. Invalid complete JSON produces a parse-error
response without reaching authority; framing-terminal inputs return the exact protocol/limit
class. Other operations continue after rejected requests in normal histories.

Separate public diagnostic-constructor cases independently accumulate complete Unicode scalars
until the byte bound, checking response messages (512), bridge code (128), bridge detail (1024)
and transport detail (1024). Generation deliberately puts 2/3/4-byte scalars across each bound,
including safe exact-bound and neighboring prefix cases. This exposed a confirmed original
String::truncate panic; the focused public example and four-call floor-boundary repair are on
`fix/mcp-utf8-diagnostics`. Original native exit1 and fixed exit0 are retained separately; root
performs exact final formatted original/fixed correspondence. A caller-level exploit claim is
not made from constructor failure alone.

Bounds: 64 dialogue operations, eight catalogue entries, pages one through four, 4096-byte messages,
two actual active requests and client chunks one through sixteen. No external MCP service, stdio
application wrapper, real daemon adapter, arbitrary blocking writer failures, all malformed DTOs,
all RpcId extrema, subscriptions or future MCP protocol versions is qualified.

## Generation, metrics and calibration

Plugins generation mixes broad histories, warm echo → authority change → stale denial → varied
transport failure → stop/start, constructive actual-send cancellation and independent SDK cases.
MCP mixes broad histories, constructively initialized cancellation/paging histories, framing
faults and Unicode diagnostic boundaries. Shrinkable cases contain all semantic operands; replay
has no hidden randomness. Short requested bounds truncate constructive prefixes rather than
claiming their later interactions were reached.

Metrics distinguish real starts/observed reaps, exact child frames, successful/typed/cancelled
results, sent versus unsent failures, output quota edges, current mediation calls, SDK acceptance
and framing partitions; MCP counts lifecycle refusals, exact pages, bridge denial preservation,
active duplicate IDs, saturation, cancelled futures, subsequent ID reuse, joined owners and
Unicode constructor boundaries. A high case count alone does not prove those interactions.

`calibration/stage5-plugins.patch` disables response-ID correlation in the actual host exchange.
The fixed and warm generated wrong-ID family must reveal that a successful result was attributed
to the wrong request. `stage5-mcp.patch` disables Ready gating in the actual server; independently
expected preinitialization refusal must then fail in both fixed and generated histories. Parent
owns compiled mutation, two-seed generated detection, stale-binary refusal, exact restoration,
rebuild and retained witness replay. No successful mutation cycle is claimed by this source draft.
