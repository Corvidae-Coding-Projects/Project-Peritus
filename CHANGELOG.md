# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

### Added
- Implement the complete production C5 Model Providers boundary with six maintainable model-layer
  crates for the provider-neutral protocol, shared provider core, OpenAI, Anthropic, Google, and
  explicitly configured compatible endpoints (#14)
- Add protocol v1 checked identities, messages and bounded multimodal content, JSON Schema tools
  and results, strict structured output, reasoning summaries and opaque replay state, persistence
  and continuation controls, deterministic canonical request identity, complete capability/profile
  negotiation, and immutable accepted/rejected compatibility fixtures
- Add ordered normalized response streams for text, reasoning, tool arguments, refusals, usage,
  cache, rate limits, response identity, provider extensions, finish reasons, and typed terminal
  failures, with exact duplicate handling, fragmented UTF-8/JSON assembly, bounded reduction, and
  fail-closed malformed/incomplete/cancelled outcomes
- Add a hardened provider-core effect boundary with validated redacted endpoints and credentials,
  Reqwest/Rustls ownership, bounded HTTP and byte streams, SSE/NDJSON framing, cancellation-aware
  backoff, conservative retry and ambiguous-submission classification, owned stream teardown, and
  an explicit server-side response cancellation seam, plus bounded subprocess invocation with
  explicit arguments/environment isolation, output/deadline ceilings, cancellation, and child reap
- Add the current first-party OpenAI Responses adapter with multimodal/tool/structured-output and
  reasoning projection, prompt caching, usage/rate metadata, heterogeneous SSE normalization,
  background exact-cursor continuation, and confirmed background response cancellation
- Add the current first-party Anthropic Messages adapter with top-level system projection,
  multimodal content and tools, structured output, adaptive thinking and opaque signature replay,
  prompt caching, required version/beta headers, cumulative usage, and Messages SSE normalization
- Add separately profiled account-backed OpenAI Codex and Anthropic Claude transports that use the
  providers' already-authenticated official executables as stateless credential-owning routers,
  disable native tools and ambient integration surfaces, normalize schema-constrained text/inert
  tool proposals/usage, never inspect account tokens, and advertise advisory output limits
- Add both documented stable-v1 Google Gemini families: Interactions for new development and
  generateContent/streamGenerateContent for existing integrations, including tools, multimodal
  content, response schemas, thinking signatures, cached content, safety/finish observations,
  retention/state policy, and explicit `x-goog-api-key` authentication without an SDK `v1beta`
  fallback
- Add separately validated compatible Responses and Chat Completions profiles whose explicit
  dialect, paths, authentication, framing, supported fields, mappings, lifecycle, retry guarantees,
  limits, and response-ID semantics default to the minimum safe feature set rather than inferred
  OpenAI parity
- Extend A2 with a nonempty fourteen-case provider suite and owned deterministic loopback servers,
  including bounded multi-exchange scripts on one stable endpoint, covering capability honesty,
  ordering/deduplication, fragmented tools, malformed/incomplete streams, cancellation,
  authentication, rate limiting and retry-after, transient recovery, ambiguous submission, usage,
  redaction, and selected/foreign adapter isolation
- Qualify both account-backed routes with fresh-subject hermetic fake executables covering exact
  invocation isolation, structured output, terminal failure, cancellation, and child reap without
  a provider installation, account credential, or live network; separately qualify shared process
  output-overflow and timeout handling through portable real-process tests
- Add provider-specific immutable request/stream/error fixture corpora with manifests and SHA-256
  inventories, crate READMEs, the C5 operating guide, and hosted Linux/macOS/Windows provider
  qualification wiring
- Add thirteen Verus-verified C5 functional-core obligations and connect ordinary runtime paths to
  checked capability intersection, reducer transition and terminal facts, exact deduplication,
  completed-fragment predicates, monotonic usage, retry legality, and provider non-authority

- Implement complete production C3 Platform Security Backends (#12)
- Implement complete production C2 Process/Sandbox Backplane (#11)
- Implement C1 Git, workspace, and atomic patching (#10)
- Implement C0 journal, projections, artifacts, migrations, and evidence (#9)
- Implement B3 domain protocol and canonical codec (#8)
- Implement B0 lifecycle kernel (#7)
- Implement B2 acceptance specification and quality policy (#6)
- Implement B1 policy, leases, budgets, and approvals (#5)
- Implement A2 test/conformance foundation (#4)

### Fixed
- Make malformed completed UTF-8 or JSON establish an irreversible failed reducer terminal, reject
  all post-terminal events without replacing the original outcome, and classify explicit
  non-accepting HTTP responses separately from ambiguous post-send failures
- Preserve bounded first-party provider request IDs on normalized success and failure observations
  while continuing to exclude credentials, response bodies, prompts, outputs, and tool arguments
  from diagnostics and fake-server artifacts
- Exercise retry-after and transient recovery through real two-exchange HTTP servers for every
  direct HTTP adapter instead of substituting an in-memory transport for those conformance paths
- Restore hosted Linux, macOS, and Windows runner portability across native sandbox, process, Git,
  patch, network, durable registry, and tool-shell test boundaries (#12)
- Remove macOS socket-close races from the managed-proxy worker-backpressure conformance test
- Stabilize hosted Windows native shell conformance polling under runner scheduling delays
- Make managed-proxy HTTP fixtures issue each complete request in one socket write, and preserve the
  process cleanup regression's timing distinction with realistic hosted-runner scheduling allowance

### Changed
- Implement C4 tool system (#13)
- Document production architecture for Verus-first coding harness (#1)
- Implement A1 formal foundation (#3)
- Implement A0 workspace and toolchain foundation (#2)
