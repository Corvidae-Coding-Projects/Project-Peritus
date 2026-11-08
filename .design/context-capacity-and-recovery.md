# Model capacity and semantic recovery

Objective: use the maximum context advertised for the exact selected model and route, with output/runtime reserve accounted separately, and stop treating archive/retrieval wrappers as new task evidence. Preserve current instructions, permissions, immutable archives, exact byte readers, pending-effect reconciliation, model/effort selection and accepted checkpoints. Work starts on develop at 44ac8d389. The active run remains on its installed daemon.

## Observed causes

The live selected-model cache advertises 500000 default, 872000 maximum, and 95 percent effective window. CodexRuntimeProvider does not resolve request_profile metadata, leaving its configured 200000 profile. A separate one-MiB text limit also constrains the transport. The public account model/list result omits numeric limits; the official runtime's fresh model cache contains them. No model-name table should substitute for that metadata.

Decision views promote context_read receipts into evidence cards, order old evidence before recent code, and add continuation last. Inspection deduplication includes changing delivery wrappers. Raw canonical message envelopes are valid archive bytes but poor model-facing recovery. Increasing capacity alone would retain more of this cycle.

## Implementation increments

1. Resolve live route/model numeric capacity before request assembly. Keep maximum/default window, runtime percentage, input/output bounds and transport constraints distinct. Refresh across model changes and runtime metadata expiry. Reject contradictory metadata. Preserve profile identity, capabilities and selected effort.
2. Configure the owned Codex transport with the discovered window. Use the isolated app-server injected-data path for large normal requests so CLI text validation does not reduce model capacity. Preserve native-tool isolation and explicit access-program handling; report any actual transport limitation rather than silently dropping context.
3. Separate task evidence from retrieval/audit records. Keep all exact artifacts. Normalize only host delivery wrappers when deduplicating inspections; prioritize current plans, pending outcomes and recent relevant facts. Recovery should provide semantic message content; raw byte access remains explicit and unchanged.
4. Replay the observed histories through real public interfaces, including restart, permissions, changed files, smaller/larger model selection, malformed/missing metadata, opaque archives and retrieval receipts. Verify real selected-model capacity and a payload beyond the previous text ceiling. Preserve independently checked witness inputs/results and document qualified boundaries.

## Design alternatives and invariants

A larger hardcoded profile and extra prompt warnings would leave both failures intact. Replacing stored archive bytes with decoded text would break fidelity, source identities and old offsets. The chosen design keeps durable truth separate from model-facing views and makes capacity discovery a provider responsibility.

Untrusted memory never becomes instructions or execution authority. A retrieval receipt never becomes a new semantic fact. History omission is not outstanding work. Current instructions and the next unfinished action must survive view changes. Model maxima are capacities for a request, never lifetime task budgets. Maximum capacity does not require filling every request with irrelevant history.

Sources: https://learn.chatgpt.com/docs/app-server ; https://learn.chatgpt.com/docs/config-file/config-sample ; https://learn.chatgpt.com/docs/models?surface=web . Installed Codex 0.167.0 protocol schemas and sanitized current model cache were inspected on November 7, 2026.

## Delivery and qualification

The four implementation increments are present on develop's working tree. A later live mode-switch failure is also repaired at the client: composer selection and actual execution mode are distinct; changing modes waits for confirmed cancellation and retains the user's follow-up with destination/draft checks. The live run was returned to Chat/Writing through authenticated daemon APIs without changing its selected models or discarding its workspace.

The recovery compiler also retains the real page required by a current already-visible receipt. This avoids replacing the body with a receipt and forcing another retrieval on the following turn. Only the nearest actual page is retained, not the intervening receipt chain.

See [qualification](../docs/testing/context-recovery-mode-2026-11-07.md) for actual large-model input usage, real restart histories, the live transition, reproduction commands and boundaries. Explicit access-program CLI transport limits and missing output-limit metadata are reported honestly; neither is silently replaced with invented provider support. Changes require a rebuilt process to affect an installed daemon or client.

Live installation also exposed historical-card refill at the new ceiling. Current content-matched evidence now receives the full model envelope first, while optional archive material is bounded by the live working material rather than spare capacity. The saved 24-version regression verifies both reduced automatic growth and exact retrieval of omitted versions.

Working-set selection uses the latest settled workspace-effect invocation and subsequent observations, with current content bindings. Covered original file pages coalesce by exact path/version/range across different read arguments. Other current but older reference material remains optional and retrievable. Typed workspace-read provenance is required for native coverage credit; matching JSON shape is insufficient.
