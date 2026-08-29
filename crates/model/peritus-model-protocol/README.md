# peritus-model-protocol

Provider-neutral C5 request, capability, normalized streaming, accounting, retry, and failure
semantics. All accepted values are versioned and bounded; provider wire representations and effect
handles are intentionally excluded.

The crate is ordinary Rust with Verus-checked functional predicates. It may depend only on
foundation and model-layer crates and does not authorize tools or alter authoritative budgets.

`ProviderProfile` distinguishes the direct `OpenAiResponses` and `AnthropicMessages` dialects from
the account-backed `OpenAiCodexRuntime` and `AnthropicClaudeRuntime` dialects. It also states whether
`max_output_tokens` is `ProviderEnforced` or merely `Advisory`; direct API profiles use the former,
while final-result executable runtimes that expose no exact output limit use the latter. Consumers
must not promote an advisory limit into an enforcement claim.

## Canonical compatibility boundary

`decode_request` reconstructs canonical v1 `ModelRequest` bytes only when supplied the exact
immutable `ProviderProfile`, a caller-owned `RequestId`, and `ProtocolLimits`. Request identity and
profile lifecycle facts are intentionally absent from canonical bytes. The decoder rejects version
or profile drift, unknown closed tags, invalid booleans and option tags, invalid nested values,
noncanonical JSON, exceeded bounds, trailing bytes, and requests that fail complete domain
validation. A successful decode is guaranteed to re-encode to the same bytes.

The immutable `fixtures/v1/` corpus contains minimal, realistic, boundary, corrupt, unknown-tag,
invalid closed-tag, and trailing-field cases. `MANIFEST` inventories their intent and `SHA256SUMS`
pins the digest of the decoded binary bytes represented by each hexadecimal fixture.

The accepted corpus includes both account-runtime dialect tags. Those tags are part of canonical v1
request identity even though credentials, executable paths, and provider lifecycle facts remain
outside canonical bytes.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-model-protocol
```
