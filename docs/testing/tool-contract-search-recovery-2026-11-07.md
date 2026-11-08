# Tool contracts and search recovery — November 7, 2026

Implemented in the develop checkout after observing rejected design amendments and failed searches in an active Peritus run. The installed daemon and its workspace were left running. This report describes the changed source and isolated verification, not an upgrade of that running process.

## Failure model and repair

Three independent boundaries had been conflated:

1. Valid transport JSON did not establish that a proposal satisfied the selected tool's argument contract. The common developer loop now compiles the current invocation's advertised schemas and admits every call before executor dispatch. Closed objects, required fields, tagged/nullable alternatives, local references, sizes and numeric constraints are enforced independently of the provider. Unsupported schema restrictions fail closed. Live permissions, input revisions, cancellation and existing executor checks still apply.
2. Reading an accepted draft did not recover the rejected candidate. Rejected expert proposals now have private, content-addressed records with run, source, input revision and predecessor version. `design_proposal_read` pages that candidate; `design_proposal_repair` replaces explicitly named JSON Pointer fields and then goes through ordinary admission and publication checks. Repairs cannot rebind old source/version fields. Invalid repairs preserve accepted checkpoint bytes. Evidence inputs and persisted records use typed authority/reference variants: primary/official evidence needs an applicable nonempty version, while the host can supply a redundant engineering-rationale discriminator from the advertised invariant-only input variant.
3. Query text was being treated as search backend health. Origin-scoped leases and operational backoff now survive query changes and fresh processes sharing the source store. Retry-After delta seconds and dates are preserved. Challenges, transport failures, parser drift, operational-state failure and recognized empty pages remain distinct. Partial results survive another backend's HTTP or state-file failure. Built-in search advertises only supported options; external SearXNG retains filters, pages, engine attribution, dates, suggestions and available-result metadata.

Codex receives a typed schema where its constrained-output vocabulary can preserve the host contract. Each proposed call has a single tool-name key whose value is the arguments. This structurally selects the tool before its payload, independent of JSON property ordering. Tools requiring an unrepresentable schema remain explicitly host-validated, using a JSON string value; older named/arguments and named/arguments_json records remain readable. Optional nullable fields also use host validation when projection would conflate omitted and explicitly null values.

The first isolated multi-tool inference exposed a real defect in an earlier named/arguments union: argument properties sorted before the name, and the model repeatedly generated a file path that then forced `view_image`. The final tool-keyed representation eliminated that late discriminator. A repeat of the complete design workflow read source with `workspace_read`, retained official documents with `web_open`, saved amendments, published a validated design, reached Writing, and reused it with zero provider requests after its derived receipt was deleted.

## Coverage record

| Boundary inspected | Exercised behavior |
| --- | --- |
| Model protocol, Codex request/schema, output decoder and runtime provider | Public validator/projection consumer; real gpt-6.1-sol/XHigh typed nested call; historical and new output decoding, duplicate and mixed-format rejection. |
| Developer-loop admission and executor wrappers | Current-invocation schema gate; reject forwarding through measured, memory, design and conversation executors. Full isolated ProductRunner inference uses this path. |
| Expert tools, typed evidence, accepted pass and rejected proposal storage | Filesystem-backed missing-kind and authoritative-null-version failures; exact rejected proposal paging across restart; bounded field repair; unchanged accepted checkpoint on rejection; protected/stale/malformed-pointer rejection; unchanged candidate Markdown and acceptance after repair. |
| Native HTTP, search parsers, backend health, source store and product schemas | Controlled real TCP responses through the public Store API; four HTML parsers; genuine empty result versus challenge/unknown DOM; 429 delta/date; changed-query/fresh-process cooldown; expiry probe; partial results despite HTTP/state corruption; SearXNG metadata/endpoint-query compatibility; page-zero rejection before HTTP; cancellation; invalid root input. |
| Existing inspection-plan integration | Existing generated property tests caught a regression where mutable indexing added a null `required` field to schemas without one. Using `get_mut` preserves an absent constraint; the unchanged acceptance oracle passes. Its failure diagnostics now include the actual rejected response. |

## Replays and observed results

Run from the repository root with the pinned toolchain:

```sh
CARGO_BUILD_JOBS=2 cargo run --locked -p peritus-model-protocol --example tool_admission
CARGO_BUILD_JOBS=2 cargo run --locked -p peritus-tools-web --example search_recovery
CARGO_BUILD_JOBS=2 cargo run --locked -p peritus-provider-openai --example tool_contract -- --live
CARGO_BUILD_JOBS=2 cargo run --locked -p peritus-product-runner --all-features --example live-design-expansion -- zero
CARGO_BUILD_JOBS=2 cargo test --locked -p peritus-product-runner -p peritus-provider-openai -p peritus-model-protocol -p peritus-tools-web --lib --all-features
CARGO_BUILD_JOBS=2 cargo clippy --locked -p peritus-product-runner -p peritus-provider-openai -p peritus-tools-web --all-targets --all-features -- -D warnings
CARGO_BUILD_JOBS=2 cargo check --locked --workspace --all-targets --all-features
CARGO_BUILD_JOBS=2 cargo check --locked -p peritus-model-protocol -p peritus-tools-web --all-features --target x86_64-pc-windows-gnu
cargo fmt --all -- --check
```

- PASS: 720 generated range cases and 8,000 finite-float comparisons against independent native numeric order, seed `5742a95cd71638eb`; exact u64 boundaries, numeric enum equality, Unicode length, tagged failures, local refs and lossless representable optional/null projection.
- PASS: actual HTTP/parser/state replay, including 16 changed queries and fresh child processes making no additional network requests during backend cooldown. Expiry allowed a probe. Corrupt backend state did not discard another engine's results or reset the failed backend's state.
- PASS: actual Codex inference produced the required typed nested proposal and preserved nullable/omitted semantics.
- PASS: full isolated design publication and zero-request reuse; the test independently validates retained artifacts and recreated receipt and stops at Writing.
- PASS: 94 library tests across the affected packages, including the preexisting generated plan tests and new filesystem/decoder regressions.
- PASS: strict affected-package Clippy, all-feature/all-target workspace checking, Windows cross-compilation of the validator and search package, formatting and diff checks.

Successful replays and checks were captured under `/home/doll/.cache/peritus-*-replay-final.log`, `peritus-design-contract-live-final.log`, and `peritus-contract-*.log`. The earlier failed multi-tool trace is retained privately under `/home/doll/.cache/peritus-format-before-tagged-20261107`.

Commit review also replayed configured endpoint query parameters, recognized `empty_engines`, and the original `available_results` meaning (received array length). The upstream estimated total is returned separately as `estimated_total_results`. Result URL filtering preserves the surviving entries' upstream attribution and date.

## Limits

The HTTP server in the search replay is a controlled upstream fixture, not a claim that public search providers accept every request. No challenge solver or access-control bypass was added. Cancellation during an in-flight blocking request remains bounded by the eight-second network deadline. Persisted health is operational state, separate from successful query-result caching; a corrupt health record is reported rather than silently reset.

Real model qualification covers the configured Codex route on Linux. Other providers share host admission but were not individually exercised against their live services. Windows received a compile check, not a runtime exercise. The checks establish these boundaries and replayed histories, not exhaustive JSON Schema conformance, whole-product qualification or a guarantee that models never propose invalid data. Invalid proposals cannot acquire execution authority through constrained decoding alone.

## Applicable references

- https://developers.openai.com/api/docs/guides/function-calling — strict function schemas and their supported subset.
- https://learn.chatgpt.com/docs/non-interactive-mode — Codex output-schema transport.
- https://www.rfc-editor.org/rfc/rfc9110.html#name-retry-after — delay seconds and HTTP-date forms.
- https://www.rfc-editor.org/rfc/rfc6585.html — HTTP 429 semantics; operational backoff is separate from caching the HTTP response.
