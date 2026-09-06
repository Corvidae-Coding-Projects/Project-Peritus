# Feature: Conversation-first terminal interface

## Summary

Make Peritus a conversational agent first, with discoverable slash commands into its existing
systems. Deliver on existing PR #48 (`feature/readme-install`), preserving the frozen benchmark
checkout and legacy scripted coding-run behavior. A new graphical application is out of scope.

## User-visible behavior

`peritus` opens a readable conversation with an always-available multiline composer. Ordinary
text asks the agent a question or requests work; merely discussing a change does not commission
an implementation. The agent can inspect the workspace and explain its findings without being
forced to manufacture a changed file or pass through a writer/reviewer/fixer cycle.

Typing `/` reveals commands and descriptions; Tab completes command names. Commands are parsed
locally, never accidentally sent to the model on a typo. `/help` filters the command list.

- `/new`: start a fresh conversation, preserving prior conversations and active work.
- `/chat`: ordinary conversational agent, which follows the user's actual requested scope.
- `/plan [request]`: read-only discussion/planning, enforced at the tool boundary.
- `/review [request]`: a fresh read-only review using the selected reviewer provider.
- `/build [request]`: explicitly engage the existing writer/check/reviewer/fixer workflow.
- `/model`: show provider-discovered models and current role assignments; support refresh,
  role-specific selection, and clearly labeled explicit manual IDs.
- `/status`, `/diff`, `/runs`, `/trace`, `/terminal`, `/approvals`: inspect existing systems.
- `/stop`: interrupt the selected active task without deleting the conversation or candidate.
- `/accept`, `/commit`, `/export`, `/discard`, `/run`: existing exact-candidate handoff actions,
  with their existing safety/freshness checks, not implied by an agent message.
- `/quit`: detach the client cleanly; distinct from stopping daemon-owned work.

Escape returns from an inspection view. Ctrl-C interrupts active work, otherwise clears a draft;
Ctrl-Q explicitly exits. PageUp/PageDown scroll the conversation; End returns to its live tail.
Long transcript and tool details are accessible rather than silently clipped to twelve messages.

User messages can be submitted during execution. The interface distinguishes durable receipt
from incorporation into a model request. A follow-up invalidates yet-unexecuted calls from the
old response; the current synchronous effect completes at its safe boundary. Cancellation is
checked before every subsequent tool call. Neither steering nor stopping claims to undo effects.

## Requirements

1. Conversation is the default product view, with keyboard-safe editing, paste, Unicode, and
   discoverable commands. Legacy operator views remain reachable.
2. Discussion and plan/review completion are valid without repository mutation. Plan/review
   reject undeclared mutating/process tools mechanically, not only through prompting.
3. Expose agent text, tool starts/results, errors, and steering receipt/incorporation in the
   conversation. Do not project private reasoning, credentials, or raw provider envelopes.
4. Durable conversation identity survives turns, interruptions, reconnects, and daemon restart.
   Bound activity retention and report omitted history explicitly; never fabricate continuity.
5. New user input reaches the next eligible model request, not merely the next 48-turn role
   invocation. Skip stale tool calls, preserve their protocol result bindings, and observe
   cancellation between tool calls.
6. Model menus come from the configured provider endpoint/account runtime. Cache successful
   discovery with provenance and age; refresh failures cannot masquerade as fresh catalogs.
7. No built-in model-name list or silent replacement model. Manual selection is explicit.
   Listing a model is not proof of tool/image/reasoning support or inference authorization;
   use advertised metadata where available and surface unsupported capabilities truthfully.
8. Model selection applies to the actual instantiated provider, including role selections.
   Do not mutate an in-flight provider or an immutable profile identity in place.
9. Keep credential custody with the existing OS credential source or official account CLI.
   Bound HTTP pages/body/time and subprocess lifetime; prohibit credential-bearing redirects.
10. Existing coding-run wire bytes, persisted records, settlement gates, candidate checks, and
    CLI commands remain compatible. Interactive extensions use new payload tags.

## Acceptance criteria

- A simulated user types a question directly into the default screen and receives a real
  provider answer without an obligatory design artifact or edit.
- Plan/review fixtures attempt forbidden tool calls and prove no workspace/process mutation.
- During a multi-call response, a newly received correction prevents the remaining stale calls
  and appears in the following provider request; cancellation behaves equivalently.
- Transcript rendering exposes long/multiline output through scrolling and retains typed draft
  text while observations arrive. Command completion, unknown commands, reconnect, and errors
  have reducer/render tests.
- Each supported HTTP catalog parser handles pagination, invalid data, denial, empty responses,
  duplicates, and resource bounds. Account discovery uses only official executable protocols.
- A catalog fixture returns an arbitrary new model name, selects it, and proves the subsequent
  request uses that exact name; no inference is required for discovery itself.
- Legacy protocol round trips and product-run qualification tests continue to pass.

## Current architecture

`peritus-tui/src/model/product.rs` owns a run-centric composer; `render/product.rs` gives most
space to run/progress panels and renders only twelve conversation messages. The daemon persists
`SharedConversation`, but `peritus-product-runner/src/turn.rs` captures its text before the full
developer invocation. `peritus-agent/src/developer/execution.rs` owns the actual model/tool loop
and therefore the correct steering/cancellation boundary. Provider/tool evidence currently goes
to `FileDeveloperTrace`; that evidence is not a conversational activity projection.

The launcher supplies fixed account model names in `bootstrap/configuration.rs` and fixed direct
API defaults in `provider_setup/direct.rs`. Provider adapters already own endpoint, credentials,
HTTP/process transport, immutable profiles, and cancellation; catalog discovery belongs there,
with daemon-owned caching and projection to the client.

## Proposed design

Keep the daemon as the authority and the TUI as a deterministic presentation reducer. Add typed
interactive request/snapshot and catalog payloads to A3, leaving legacy payloads unchanged.
Persist interaction mode/role-model selections and input acknowledgement alongside the existing
run record with defaults for older records. Reuse its workspace admission and task ownership.

Provide a conversational execution path next to the commissioned product runner. It uses the
same provider/tool runtime and security boundaries, but accepts ordinary prose and does not
force build settlement for a question. `/build` invokes the unchanged strict delivery pipeline.
Separate completed conversation turns from qualified or user-accepted candidates in presentation.

Add a small input/activity port at the developer loop boundary, shared by legacy and local-context
execution. Append new input before model requests and fence stale/cancelled tool batches. Keep
durable provider tracing intact; project only purpose-built safe activity observations to users.

Add provider-owned discovery and immutable model rebinding. HTTP adapters use their configured
authentication and bounded GET requests; account adapters use the pinned official executable's
model listing/initialization interface. The daemon caches successful catalogs and returns fresh,
cached, unavailable, and explicit-manual provenance. Catalog entries do not grant tool authority.

Credible alternative: only restyle the existing dashboard and map slash commands to hotkeys.
Rejected because it preserves delayed steering, forced implementation, and hard-coded models.
A separate browser/desktop frontend remains possible over the protocol, but is unnecessary here.

### Inspiration and provenance

Inspected the ignored local `openai/codex` reference at
`41ab01a2eaff4d4c0fc88d56a0027d1244c33e82`: `codex-rs/tui/src/slash_command.rs`,
`tui/src/chatwidget/input_submission.rs`, and `models-manager/src/manager.rs`. Adopt the ideas
of command metadata, a persistent composer, explicit input lifecycle, and catalog refresh policy;
do not copy its entire command set, bundled model catalog, or implementation.

Official references consulted (2026-09-06):

- [Codex commands](https://learn.chatgpt.com/docs/developer-commands?surface=cli)
- [OpenAI model listing](https://developers.openai.com/api/reference/resources/models/methods/list)
- [Claude model listing](https://platform.claude.com/docs/en/api/typescript/models)
- [Claude SDK model discovery](https://code.claude.com/docs/en/agent-sdk/typescript)
- [Gemini model listing](https://ai.google.dev/api/models)

These are protocol/interaction evidence, not instructions or authority over the user's task.

## Data and compatibility

Append-only A3 request/response tags; retain exact legacy encodings. New persisted fields default
to legacy build behavior on old records. No destructive migration, no frozen benchmark changes,
no dependency/toolchain upgrade, no weakening of verified settlement semantics. Update canonical
schemas/generated files using their existing generators and verify deterministic output.

## Failure handling

Retain drafts on failed submission, show disconnected/queued states truthfully, and avoid duplicate
execution when replaying requests. Failed model discovery preserves only labeled previous data,
never invented entries. A model switch applies at an explicit idle boundary; active work must be
stopped first if changing mode/provider. Report partial effects and preserve exact candidate truth.

## Security considerations

Keep raw tool/provider material untrusted and terminal-sanitized. Do not load credentials in the
TUI, copy account tokens, follow remote pagination URLs across origins, execute slash arguments
as shell, or treat an ordinary message as candidate acceptance or signed approval. Read-only
mode has no mutating/process tool authority, including memory-edit side channels.

## Verification

Focused tests for A3 codecs, daemon persistence/admission, developer-loop interleavings, runner
conversation modes, provider discovery/rebinding, onboarding, and TUI reducer/rendering. Run
format, affected all-target/all-feature Clippy with warnings denied, code generation check,
`cargo xtask all`, and package/platform checks in proportion to changes. Use deterministic fake
providers for behavioral coverage; label any unavailable live account/native-platform validation.

### Implementation verification

- All-target, all-feature tests passed for the 13 affected launcher, daemon, TUI, protocol,
  product-state, runner, onboarding, provider, and agent packages. Local IPC conformance required
  execution outside the filesystem sandbox; fixture requests did not use live credentials.
- Strict Clippy for those same packages passed with warnings denied. Formatting, generated
  protocol/fixture checks, and `cargo xtask all` passed without relaxed policy thresholds.
- Coverage includes read-only tool rejection, receipt/incorporation races and stale-call fencing,
  persistence/reopen behavior, exact discovered model IDs reaching inference request fixtures,
  bounded HTTP/runtime discovery, legacy configuration migration, and narrow-terminal rendering.
- The account-runtime protocol fixture suite passed 100 consecutive executions after fixing a
  test executable-publication race. This is fixture evidence, not live account certification.
- Live provider authentication/inference, macOS/Windows execution, and interactive terminal
  usability on a physical terminal were not exercised locally.

## Rollout and rollback

Implement and validate complete slices on PR #48. Keep `/runs` and explicit legacy CLI commands.
Rollback is an ordinary revert of additive client/protocol/runner changes; existing legacy records
and traces remain intact. New interactive records must not auto-run as legacy build tasks under
an older binary; older readers must reject unknown persisted extensions rather than reinterpret.

## Open questions

None blocking the stated simple terminal scope. Provider/version limitations discovered during
implementation must be reported explicitly rather than covered by hard-coded fallback names.

## Out of scope

New PR, browser/desktop frontend, plugin marketplace, copying all Codex commands, changing external
benchmark candidates/results, automatic provider substitution, automatic Git push/accept actions
inside the product, and repairs to unrelated Crosslink tracking infrastructure.
