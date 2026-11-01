# Stage 5 model protocol contract and oracle

The public target `models` drives `ResponseReducer::apply` with event histories constructed from
independent item, chunk, usage, and terminal tables. The oracle records item IDs, output indexes,
content, completed tool JSON, high-water usage, and exact terminal outcomes. It compares event
codec framing and parsed values, but roundtrip equality alone does not qualify an event.

Accepted histories complete each opened item, complete each tool argument JSON object, supply a
finish reason, and close once. Rejected histories cover incomplete UTF-8/JSON, open items,
duplicate item and tool-call identities, conflicting replay IDs, sequence gaps, late argument
bytes, excess output/events, extra terminal events, and postterminal continuation. A malformed
prefix must not publish a completed tool call or a success terminal. Usage observations remain
cumulative and do not regress after an invalid update.

The frozen C5 request corpus is checked against its committed SHA-256 list before any mutation.
Ten original files include accepted and rejected requests. A separate mutation of the minimal
canonical request's dialect byte exercises all nine current dialect tags and invalid tag edges
with a matching external profile; it is a tag/projection check, not a nine-dialect request matrix.

Native source owners: `peritus-model-protocol/src/reducer/{events,stream,terminal}.rs`,
`event_codec/{encode,decode}.rs`, and `canonical_decode.rs`. The finite generator does not prove
all possible provider payloads or OS transport behavior; provider-specific bytes are in the
`providers` target and the owner-only OpenAI HTTP hook.

## Strengthened history search

`ForkReplay` clones a live response with a partially assembled text or tool item, including
inside UTF-8 and JSON. The source response fails on EOF; two clones receive different suffixes
and must independently complete to exact input-derived values. Output-bearing fragments are
retransmitted repeatedly with identical provider identity/digest under both original and fresh
local envelope sequences. This reaches mutable item state that the original duplicate identity
event family never touched. Metrics distinguish tool/text branches, interior UTF-8 splits, and
both local sequence forms.
