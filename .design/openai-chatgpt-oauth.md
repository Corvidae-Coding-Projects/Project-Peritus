# Optional direct ChatGPT OAuth provider

User outcome: offer a separate direct OpenAI Responses route using the documented open-source
ChatGPT-plan OAuth flow. Keep the existing Codex and API-key routes. Do not migrate existing
selections or remove Codex before direct login, inference, host tools, cancellation and refresh
have real-interface evidence.

The daemon remains the authority for tools and conversation state. OAuth is authentication,
not an agent runtime. Direct streaming reuses the OpenAI Responses decoder and validates host
function calls through the existing executor. The optional route has its own strict request
profile: stream=true, store=false, no background/previous_response_id/max_output_tokens or
sampling fields, and supported namespaced function tools. System instructions become the
Responses instructions field. Existing API-key request serialization remains unchanged.

Authentication owns fresh PKCE/state/nonce, a bound 127.0.0.1 callback, dynamic client registration,
verified OpenAI ID-token issuer/audience/signature/expiry/nonce, account-specific client mapping,
and protected atomic credential records. Refreshes are serialized across processes and rotate
the complete token set together. Account identity is issuer/subject/client, not email. Tokens
never enter product state, model context, command logs or provider diagnostics. Use the public
api.openai.com endpoint and the new route's own registration; do not read Codex's token files.

Delivery and evidence:

1. Add the restricted Responses profile and direct transport without changing existing routes.
2. Add actual OAuth login/session/refresh/catalog support with protected storage and safe errors.
3. Expose a distinct provider choice and carry it through state, generated config and daemon factory.
4. Exercise real login and inference, host reads/commands and current-input cancellation. Preserve
   explicit coverage gaps; authentication or listing models alone is not working inference.

The alternative is another Codex app-server configuration. It preserves the extra agent layer
that this user explicitly wants optional replacement for; keep it as the existing fallback.
Peritus's shared provider wait now checks current input generation every 250 ms, including after
public text begins. The passive daemon read does not refresh files, rebuild the transcript, or
change after-operation pause semantics. Superseded attempts locally abort, retain observed usage, settle their host
reservation, and return to context assembly. Stale output cannot dispatch host effects. This
behavior is qualified with both the existing Codex route and the new direct OAuth route, including
steering submitted after public text begins streaming. Local cancellation cannot guarantee that
the provider stops computation or billing; Peritus discards the superseded response and tools.

Authoritative documentation inspected 2026-10-29:
- https://developers.openai.com/siwc/token-sharing-open-source/sign-in
- https://developers.openai.com/siwc/token-sharing-open-source/profiles-and-sessions
- https://developers.openai.com/siwc/token-sharing-open-source/models-and-inference
- https://developers.openai.com/siwc/token-sharing-open-source/preview-limitations


Implementation and qualification status, 2026-10-29:

| Boundary | Current evidence |
| --- | --- |
| Optional route | `ChatGptOauth` is appended as provider choice 14, with independent profile identity and `chatgpt-oauth` daemon composition. The real CLI displays it beside the retained Codex route. No existing saved selection is migrated. |
| Credential ownership | Separate bounded atomic session files, UUIDv4 host URI, PKCE/state/nonce, verified ID tokens, per-account refresh locking, rotation and explicit revocation are implemented. Unix directory/file modes were observed as 0700/0600. No Codex tokens are imported. |
| Authorization | Browser sign-in and plan-usage consent completed. The callback's issued client registration and signed identity were validated, then saved independently of Codex. The first attempt exposed an invalid opaque host-ID format; the implementation now persists the documented `urn:uuid:` UUIDv4 URI. |
| Callback rejection | The actual listener returned 404 for an unrelated path and 400 for a wrong state without ending the pending authorization. |
| Direct inference | The actual account catalog returned nine visible models. Public Responses generation, a namespaced function call and a tool-result continuation all passed using `gpt-6.1-sol` (`direct-inference-r3.log`). |
| Host reads and plans | A real daemon performed workspace discovery and an inspection plan with two sequential file reads. Random file values absent from the prompt were returned exactly, all three requests settled, and no files changed (`direct-tools/read-plan/result.json`). |
| Complete coding journey | The direct route edited only `greeting.py`, executed a native Python assertion, and completed independent review. The final clean build passed in 62.93 seconds with all 11 requests settled, including two reviewer requests (`clean-final-coding/coding/result.json`). The replay requires Achieved, independent review, settled accounting, and a separate Python import/assertion derived from its random task value. Earlier passes and failures remain recorded. |
| Native command scope probe | A command-only task read and executed `check.py` and returned its exact stdout without edits. The daemon hands pipeline goals off as WorkspaceChanges, so its empty candidate could not qualify. This is retained as a negative scope probe (`direct-tools/command`), not a successful delivery. The coding journey above qualifies native verification with an actual candidate. |
| Streaming direct steering | A question submitted after actual streamed text caused a replacement request in 0.124 seconds and the requested answer in 7.966 seconds. Both requests settled; WaitingForUser, zero tools and an unchanged fixture were observed (`direct-daemon/question-after-text-qualified/result.json`). |
| Refresh rotation | Two separate processes accessed the same naturally expiring registration concurrently. Both live catalogs succeeded; access and refresh tokens changed, expiry advanced, registration identity stayed intact, and the saved file remained 0600 (`refresh-result.json`). No expiry was fabricated and no token was copied to a second registration. |
| Provider settings | The actual CLI displayed choice 14 beside Codex, reloaded the saved OAuth registration, displayed nine current model names/slugs, and saved model settings. Only isolated XDG product state was selected; existing user selections were untouched. |
| Steering regression | A disposable real v0.1.14 daemon with the official Codex model accepted steering but dispatched no replacement request during the 12-second observation. |
| Current steering | A real daemon using the passive input-generation query dispatched a replacement request in 0.33 seconds. A mid-response arithmetic question received the requested answer in 8.13 seconds, both admitted requests settled, and the run reached WaitingForUser. No tools ran and the fixture remained unchanged. |
| Pause semantics | A real after-operation pause retained the running request, made no replacement request, and left it unsettled during a three-second observation. The test then shut down its own disposable daemon. |
| Source checks | Final affected packages pass strict all-target/all-feature Clippy (`clean-final-clippy.log`); full-workspace all-target/all-feature checking passes (`final-snapshot-workspace.log`). CLI and daemon build after removing temporary capture (`clean-final-build.log`). Launcher and its provider dependencies cross-compile for Windows (`final-snapshot-windows.log`); five existing daemon platform warnings remain. Formatting and diff checks pass. No Windows OAuth runtime success is claimed. |

Portable reproducers are retained under `docs/testing/reproducers/chatgpt-oauth/`.
Evidence is under `/home/doll/.cache/peritus-codex-attention-hu21_kcp/`. Reproducers use the
actual production libraries, negotiated daemon IPC, and official provider. They do not substitute
model responses or token grants. `question-final/result.json` includes terminal host accounting;
the earlier `steering-passive-generation` receipt only observed public answer text and is not
used as proof of terminal completion. `steering-stock-v014/result.json` preserves the old delay.
The original checkout's unrelated work is separate from this feature worktree.

Live-contract corrections retained in production:

- OAuth model catalogs can exceed 256 KiB. Catalog bodies allow a bounded 4 MiB and at most
  4096 models; authentication/JWKS bodies retain the smaller bound.
- The plan endpoint sometimes omits Content-Type on otherwise valid SSE. Only this route
  accepts the absent header; declared incompatible types and malformed bodies still fail.
- Native OpenAI provider sequence numbers begin at zero. The protocol preserves that cursor,
  while local event sequences still start at one. Canonical wire layout is unchanged.
- `response.metadata` must match the current response; unsequenced stream errors are classified
  without inventing a provider cursor. Malformed streams retain static validation diagnostics.
- Dynamic `inspection_plan` argument maps cannot use remote strict object closure. That one
  definition advertises strict=false; complete host schema, dependency and authority checks remain.
- A fresh coding replay retained an intermittent failure after editing and before verification
  (`final-coding/coding/result.json`). Capturing actual tool terminals then established that some
  calls provide neither initial arguments nor deltas, but supply complete arguments in the final
  event. The decoder now uses that final snapshot only if its buffer is entirely empty, validates
  it normally, and still waits for actual item/response completion. The live replay exercised this
  path for `run_command` (180 bytes) and `workspace_read` (23 bytes), then reached Achieved.
  Its 1586 canonical envelopes round-tripped, including 12 native zero sequences and two final-only
  argument notices (`terminal-capture-coding/final-only-arguments.json`). Temporary capture was
  removed before the final clean build. That clean replay also reached Achieved, exercised two
  final-only arguments, and round-tripped all 1405 envelopes including 11 native zero sequences
  (`clean-final-coding/trace-codec.log`). The failed fixture is preserved.
- Tool argument assembly also includes arguments present in the initial output-item snapshot.
  Final argument spelling may differ only when bounded, duplicate-key-checked canonical JSON
  values are identical. Contradictory values still fail. The successful coding journey did not
  emit a spelling-normalization notice, so that branch is source-reviewed, not live-qualified.
- Model selection shares the HTTP transport, credential owner and five-minute catalog cache
  instead of constructing a new transport and fetching the catalog on every tool turn.

Review coverage follows actual callers: launcher setup and saved selection → generated route →
daemon profile/factory → OAuth credential preparation → Responses request/stream → developer
loop → host tools and independent reviewer. Recovery paths inspected include callback rejection,
token identity validation, cross-process refresh publication, reauthorization-required errors,
explicit sign-out, malformed streams, cancellation, request settlement and module revision checks.
This is feature-boundary coverage, not a whole-repository audit.

Remaining qualification gaps are Windows OAuth runtime and ACL behavior, remote revocation,
reauthorizing an existing registration, switching between multiple real accounts, and inference
with other advertised models or modalities. These flows are implemented but not claimed as
behaviorally verified. The direct route removes the Codex agent layer; it does not guarantee
that a model can never invent a budget or mishandle a prompt. No task-wide token limit is added.

Architecture verdict: the optional direct route is implemented and working through real login,
generation, tools, independent coding acceptance, streaming steering and token renewal on Linux.
Codex remains available and existing selections remain unchanged. No commit, push, installation
switch, or release has been performed for this feature. Source and binary hashes are recorded in
the evidence root's `qualification-manifest.json`; HEAD alone does not identify this uncommitted
implementation.
