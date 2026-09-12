# Peritus bug discovery

Test-only adapters exercise production public boundaries with the same assertions in libFuzzer
and stable-toolchain corpus replay. They do not grant execution authority or disable validation.
All inputs are bounded to 8192 bytes; generated working graphs contain at most eight entries.

## Focused checks

```sh
cargo test --locked -p peritus-bug-discovery
cargo run --locked -p peritus-bug-discovery --bin discovery-replay -- sse crates/app/testing/peritus-bug-discovery/corpus/sse
cargo run --locked -p peritus-bug-discovery --bin discovery-replay -- ndjson crates/app/testing/peritus-bug-discovery/corpus/ndjson
cargo run --locked -p peritus-bug-discovery --bin discovery-replay -- working_state crates/app/testing/peritus-bug-discovery/corpus/working_state
cargo run --locked -p peritus-bug-discovery --bin discovery-replay -- provider_sequence crates/app/testing/peritus-bug-discovery/corpus/provider_sequence
```

The fuzz toolchain is `nightly-2026-08-09`, cargo-fuzz `0.13.2`, libfuzzer-sys `0.4.13`.
Build with explicit `--features fuzzing --no-cfg-fuzzing` and keep engine artifacts in root
`target/`; the production Rust pin and Cargo configuration remain unchanged. The maintained
campaign inventory records executed budgets and remaining coverage separately.

SSE and NDJSON inputs encode the frame bound in the first byte (plus one), chunk width in the
second byte (plus one), followed by raw wire bytes. When complete-input parsing succeeds, every
generated chunking must produce identical output. Working-state inputs generate bounded DAGs,
exercise dependency closure and capacity monotonicity, and mutate canonical checkpoint bytes
without bypassing the production decoder's integrity or binding checks. Provider-sequence inputs
generate normalized response, item, tool, usage, duplicate-identity, ordering, terminal, and EOF
events, check owner-contract invariants, and compare twin reducers for deterministic transitions.
