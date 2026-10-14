# OpenCode runtime: real-interface campaign, 2026-10-13

Peritus now routes configured OpenCode Zen and Go through the unmodified official OpenCode
1.19.0 executable. Peritus owns tools, approvals, credentials and conversation replay. The
vendor CLI selects the provider SDK, operation and gateway headers. No separate OpenCode login
was needed: the supplied Zen key completed real inference and host-tool exchanges.

This campaign used actual provider services, the production provider-settings terminal flow,
the daemon's real provider registry, the OS credential store, and a separate public-library
consumer. No unit/mock tests, formal gates, or qualification suites were added or run. Builds
used at most two Cargo jobs. The running personal daemon and its settings were not replaced.

## Origin and reproduction

- Source: develop based on `c609e038c995aeaea14255703f31c70fef28d73a`, with this working change.
- Official runtime: GitHub release `anomalyco/opencode` v1.19.0, Linux x86-64; archive checksum
  matched the release asset digest before installation into the user's local bin directory.
- Artifacts: `/home/doll/Peritus-OpenCode-2026-10-13/`.
- Contract/oracles established before Peritus replay: `REAL-TESTING.md` in that artifact folder.
- Terminal drivers: `provider_setup_replay.py` and `provider_setup_final_replay.py`. These
  select Zen, enter the key through hidden normal input, choose `gpt-6-luna`, and operate the
  optional connection action. Each uses its own XDG application roots.
- Public consumer: `consumer/src/main.rs`; it constructs the production registry from generated
  configuration and resolves credentials through the actual OS store. It invokes the real
  provider, reduces its events, performs a requested filesystem read, and replays its result.
- Independent native replay: `independent-isolation/report.md`, including adversarial plugin
  fixtures and corrected-config replay results.

Normal reproduction is `peritus providers`: choose Zen, supply an entitled key, select
`gpt-6-luna`, and choose the connection test. Expect generation, tool calling, and the tool-result
round trip to pass. A saved or discovered model alone does not prove access. A developer can
replay the retained consumer against this checkout with:

```sh
cargo build --locked --jobs 2 -p peritus-cli -p peritus-daemon
cargo build --jobs 2 --manifest-path /home/doll/Peritus-OpenCode-2026-10-13/consumer/Cargo.toml --target-dir /home/doll/Project-Peritus/target
/home/doll/Project-Peritus/target/debug/peritus-opencode-live-consumer
```

The consumer expects the disposable configuration produced by the terminal driver. Credentials
must remain local; they are not part of the report or committed fixtures.

## Observed coverage

| Journey | Independent expectation | Observed result |
| --- | --- | --- |
| Foreground provider settings, fresh state | Native model discovery; no API-protocol guessing; three connection stages | Passed through the actual terminal UI on initial and hardened builds |
| Real text generation | Rust arithmetic answer exactly `46` | Passed with Zen / GPT-6 Luna |
| Unicode and quoted text | Preserve `café 雪 🦀 "quoted"` | Exact text returned |
| Public JSON answer | Preserve a nested task JSON object inside public content | Exact JSON text returned |
| Host filesystem round trip | Propose the declared read, pause, then consume a previously unseen file token | Actual host read performed; exact token returned on a fresh CLI invocation |
| Parallel proposals | Two distinct declared tool names in one bounded turn | Both host proposals returned; no native execution |
| Durable public request representation | Encode/decode the new wire tag before real submission | Actual requests round-tripped and completed |
| Legacy saved API protocol | Retain model/key reference while selecting the native transport | Registry selected OpenCode for the legacy Anthropic-protocol configuration |
| Disabled Zen model | Fail with model-access diagnostic; never report completion | Permission failure retained |
| Go with this Zen-only key | Reject missing subscription; never substitute another service | Subscription-required failure retained |
| Invalid model | Reject an ID absent from the official catalog before inference | Actionable local rejection |
| Invalid key | Fail generation with an authentication diagnostic | Authentication failure retained after the retry correction; actual terminal UI reported it in 3.169 seconds |
| Cancellation and recovery | End the owned invocation; permit a subsequent ordinary request | Local cancellation completed; subsequent answer `46` passed |
| Unsafe temporary directory | Reject ambient project config before spawning or writing auth | Real consumer rejected the fixture; no private-directory residue |
| Native operation temptation | Explicit native file-creation request cannot create its marker | Marker absent; zero native tool events |
| Corrected hostile ancestor/home configuration | Startup plugins cannot create their markers | Independent corrected replays passed; one visible inference each |
| Fresh model discovery | Query the actual CLI again, with the tested model present | Eight fresh listings passed |
| Immutable configuration upgrade | Preserve old bytes and key/model choices; migrate once and reopen | Both legacy HTTP and early native configurations upgraded and reopened idempotently through the terminal |
| Complex host-tool requests | Replay the actual inputs that previously failed native decoding | All 20 captured requests completed with valid host proposals against the real service |

Evidence includes `product-fixture/providers-transcript.txt`,
`product-final-fixture/providers-transcript.txt`, `consumer-replay.log`,
`consumer-final-replay.log`, `invalid-key-final-replay.log`, `catalog-repeat.log`,
`unsafe-temp-fixture/result.json`, `product-final-fixture/existing-and-invalid-transcript.txt`,
`build-origin.json`, and `independent-isolation/hardened/summary.json`.

## Confirmed defects and corrections

### OC-01: disable flags did not isolate startup plugins

The initial configuration denied all native tools and used `--pure`, isolated XDG roots,
`OPENCODE_DISABLE_PROJECT_CONFIG`, and disabled default plugins. Nevertheless, an ancestor
`opencode.json` or `.opencode/opencode.json` could import a local plugin before inference.
A disposable plugin actually created its marker, including when the private cwd was empty
beneath a Git ancestor. Upstream source also showed home `.opencode` discovery outside the
project-disable flag.

Correction: override the vendor home using its supported-in-source home hook, disable external
skills, set canonical cwd/PWD to the private global configuration directory, and isolate child
temporary paths. Reject project/config ancestors before writing credentials or launching any
vendor code. Require the tested CLI version because these semantics matter.

Replay: independent corrected configurations suppressed all ancestor/home plugin markers,
left operation markers absent, and completed with one visible model inference and no native
tool events. A separate actual Peritus consumer exercised the Rust ancestor guard. The
independent ancestor replay intentionally bypassed that guard to test defense in depth; it
still produced native Git snapshot state, confirming why the production guard is necessary.

### OC-02: permanent access errors were retried and hidden behind deadlines

OpenCode's retry logic treated a response body containing `server_error` as transient even
when the request was an explicit subscription/model-access rejection. Real traces showed six
model streams with exponential delays. The invalid-key connection action instead reached its
45-second deadline, hiding the useful underlying authentication rejection.

Correction: observe bounded private native error diagnostics and immediately stop the owned
process for the recognized subscription-required, model-access-disabled and invalid-credential
errors. Normalize these into fixed, redacted permission/authentication failures. No child error
body becomes public. Other vendor retries retain the official bounded behavior.

Replay: both access errors returned their specific permission diagnostics, and the invalid-key
connection action returned `Authentication; opencode.runtime.authentication`. The normal Zen
route completed again after rejection and cancellation.

### OC-03: an unknown model appeared as an unexpected server error

A real official CLI invocation for a nonexistent model returned `UnknownError` with
“Unexpected server error. Check server logs for details.” The initial bridge could preserve
the failure but could not give useful model-selection guidance.

Correction: validate the exact selected ID against the native CLI catalog before inference.
Reject a mismatched service prefix or absent model with an actionable catalog diagnostic.

Replay: the production registry/public consumer rejected the nonexistent model before
inference, while the valid saved model continued working.

### OC-04: one intermittent native catalog failure

One real consumer invocation could not obtain a complete native catalog. Raw output from that
specific invocation was not retained, so its underlying cause is unconfirmed. Eight subsequent
fresh invocations returned complete parseable catalogs, including the required model.

Correction: retry the metadata-only native command once within the original total discovery
deadline. Preserve an error if a complete catalog remains unavailable; never invent a list or
substitute another model. This bounds recovery but does not establish the cause of the first
failure or guarantee that upstream discovery cannot fail again.

### OC-05: old immutable configuration prevented the provider menu from opening

Opening a disposable previously configured state failed before the provider menu because its
saved generated configuration lacked the new native transport/capability fields. Editing the
old immutable file would have weakened the configuration identity check.

Correction: migrate only Zen/Go profiles, advance the state generation, and generate a new
configuration. Recognize the exact earlier native capability configuration separately. Keep
old configuration bytes, key references, selected models, other providers and failover choices.

Replay: `product-fixture/migration-result.json` and `legacy-http-fixture/migration-result.json`
record two fresh opens each, unchanged old files, retained key/model identity, idempotence, and
the updated menu labels/footer.

### OC-06: the native final-step prompt discouraged external tool proposals

Using `steps: 1` injected OpenCode's native final-step message on the first inference, telling
the model that tools were unavailable. This conflicted with Peritus's external tool catalog.
The pinned upstream [prompt implementation](https://github.com/anomalyco/opencode/blob/v1.19.0/packages/opencode/src/session/prompt.ts)
applies that message when the current step reaches the configured maximum.

Correction: use two as the native step ceiling while still disabling and denying every native
tool. One inference remains the expected successful turn. The independent `hardened-step2`
ancestor/home replays produced one visible inference and zero native tool/plugin markers.
`consumer-step2-final-replay.log` retains the successful actual host-tool exchanges.

### OC-07: commentary was concatenated with a valid final transport object

The real application returned malformed-output failures on complex requests. Private captures
showed a GPT commentary text part followed by a valid JSON final-answer part. Concatenating
both corrupted the otherwise valid transport object. One captured final prose answer also
violated the transport contract; no missing JSON result was fabricated from it.

Correction: respect the vendor's explicit `metadata.openai.phase` boundary and skip only parts
labelled commentary. Unlabelled prose, native tool events, missing terminal events and invalid
final objects still fail closed. Front-load and repeat the private envelope instruction while
keeping task JSON reports inside the public content field.

Replay: `captured-request-final-replay.log` records successful real-service resubmission of all
20 original captured canonical requests, with one or two legitimate Peritus tool proposals
per request. `scope-native-final-fixture` additionally exercises the full client/daemon path;
its directory and memory findings are recorded in the companion in-place campaign report.

### OC-08: report JSON was escaped through an extra string layer

The live reviewer returned a private envelope whose public report string had invalid inner
JSON at a quoted `purpose` fragment. The canonical capture is retained as
`scope-native-final-fixture/invalid-public-review.json.txt`. The native transport now accepts
report objects in `content` and serializes them exactly once; report validation stays strict.
Literal conversation text is presented in ordered UTF-8 blocks instead of another escaped
JSON string. Tool metadata and results remain structured JSON with explicit origins.

Whole real consumer replays (`consumer-literal-final.log` and `consumer-every-case-final.log`)
passed nested quoted-report
JSON, text, Unicode, filesystem tool exchange, parallel proposals, rejection and recovery.
However, an exact echo stress case is nondeterministic: another actual model response omitted
a literal closing quote inside an otherwise valid report string (`consumer-supported-final.log`).
This is a retained nondeterministic model value-fidelity failure; a later passing replay does
not erase it. The [concrete echo input](reproducers/opencode-quoted-report/README.md) is retained.
It is not repaired by inventing missing string
characters. Valid transport/report syntax does not establish exact model copying. The original
failed backslash-copy probes also remain retained; none are relabelled passes.

### OC-09: initial native reviewer evidence used stale default input metadata

A cold native reviewer projected evidence using the saved 32,768-token default before loading
the selected model's advertised input allowance. Successful real model discovery advertised
922,000 input tokens for this model; the old projection omitted evidence unnecessarily.
Native context preparation now warms that metadata before initial reviewer projection. The
new provider hook defaults to an immediate no-op on existing providers and is forwarded by
the effort wrapper. Input metadata remains a hint rather than a guarantee about service limits.
Real initial reviewer captures after the correction contained the complete evidence packet,
with zero actual section-omission markers.

### OC-10: native environment cwd was mistaken for the user workspace

A raw-text replay attempted `workspace_list` on successive private `tmp/peritus-opencode-...`
router directories. These are runtime storage rather than task paths. The router instruction
now makes that distinction explicit and directs workspace discovery through `workspace_list`
with `path .`. The same application run resumed, used the actual host workspace, executed the
producer, read its stdout, cancelled and recovered it, patched/read the outputs, and completed
independent review (`scope-native-literal-fixture/tui-result.json`).

### OC-11: missing secondary runtime could block unrelated provider startup

Independent source review found eager executable discovery during daemon route declaration.
A retained secondary Zen selection without OpenCode could therefore prevent a healthy
Codex/Claude default from starting. Native executable selection is now resolved only when
that route is used; malformed provider declarations still fail normally.

The real production registry consumer constructed Codex plus Zen with an intentionally absent
OpenCode path, then completed actual Codex generation/tool calling/tool-result completion.
Zen remained unavailable and its catalog request failed; parent environment values stayed
unchanged. Evidence: `mixed-provider-final.log`.

### OC-12: runtime readiness accepted an incompatible PATH executable

Source review also found that any discoverable binary counted as ready, despite inference
requiring version 1.19.0. Supported-version discovery now probes isolated, credential-free
candidates and skips incompatible binaries. Foreground readiness and model discovery use
that supported selection. Probes run on the existing onboarding worker-thread path, including
when the caller already has a Tokio runtime.

An actual GNU `true` executable placed ahead of the official CLI on a disposable PATH was
rejected as the runtime. Eight fresh catalogs still came from the real official CLI and
contained 84 models (`incompatible-path/{result.json,catalog-result.log}`). No fake model
response or substituted successful application was used. Final foreground provider settings
also retained the saved Zen model/key, displayed the new menu, and neither asked for an API
protocol nor offered an unnecessary install. The nested-Tokio readiness consumer completed
without a runtime panic (`runtime-readiness`).

## Other-provider regression evidence

`consumer/src/bin/provider_regressions.rs` uses the production registry and existing official
account logins. Codex and Claude each passed generation, a host tool proposal, its result, and
an additional previously unseen filesystem token readback. The requests also passed through
canonical encode/decode. The retained adapters were CodexRuntime and ClaudeRuntime, never
OpenCodeRuntime. Parent environment values remained unchanged after those completed cases.

An explicit compatible Responses request to Zen returned `compatible.stream.malformed`.
The actual provider-settings connection action reproduced the identical diagnostic using
both the installed pre-change binary and the changed binary, in independent application
roots (`http-baseline-replay.log`). This is existing Zen/generic-HTTP stream incompatibility,
not a successful HTTP connection or a new regression. The new Zen route uses the official
runtime instead. Positive behavior for unrelated commercial HTTP services remains untested
without their credentials.

The OpenCode child environment and stderr observer are opt-in on ProcessRequest; old adapters
use empty overrides and no observer. Only Zen/Go migrate. Existing canonical dialect tags
remain unchanged, with the new runtime appended as tag nine. The full Codex application
replay also reached `Idle — unqualified changes retained` after the shared history changes,
with expected outputs and all 5,706 unrelated source files preserved.

## Boundaries

Positive service evidence is Linux x86-64 / OpenCode 1.19.0 / Zen GPT-6 Luna. The user confirmed
the key is Zen-only. Go discovery/rejection were exercised; successful Go inference needs an
entitled key. DeepSeek and MiniMax were disabled for this workspace; this is an access outcome,
not proof that those models work or that their earlier screenshot failures are repaired.

macOS/Windows installer execution, every model/protocol, multi-hour runs, remote server-side
cancellation, and actual context/quota/rate-limit failures remain outside this evidence.
Text and host tool history are supported; image attachments, explicit reasoning controls and
native session continuation are not advertised. Native JSONL output is buffered; requested
output length is advisory. Legacy configuration identity was exercised, not every possible
legacy in-flight run or opaque reasoning-history migration.

Private auth files use protected disposable storage and are removed on normal completion and
local cancellation. Evidence captures are scanned for the source credential before delivery;
protected native auth copies created by reference fixtures are removed afterward. Disposable
provider credentials were cleaned through the normal offline provider-settings action in
all 11 selected test roots after the service replays. All owned fixture daemons shut down
cleanly; the personal PID 1579067 stayed running. The final scan checked 93,208 artifact
files plus changed/new repository sources and found no unexpected source-key locations.
The source key remains protected with mode 0600. Cleanup evidence is retained in the campaign
artifact folder.
To replay after cleanup, rerun setup with a fresh fixture credential. Crash or power-loss
cleanup is not established by this campaign.

## Build identity note

The full application replay contained the final directory, command-owner, memory and native
prompt corrections. Subsequent executable-discovery and onboarding refinements were exercised
by the production registry consumer, supported catalog, actual foreground menu and final full
provider consumer. Earlier terminal drivers originally recorded the binary path's hash at
completion; builds during a replay can replace that path, so those historical hashes alone
do not identify the running inode. The retained driver now captures the client hash before
launch. `build-origin.json` identifies the final CLI/daemon binaries and source hashes, and
the evidence above distinguishes the separate affected replays.
