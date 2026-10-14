# Peritus bug discovery

## Folder guide

Historical component record for `peritus-bug-discovery`. No Cargo package/source entrypoint is present in this directory.

This folder currently retains documentation only. The earlier component description below is historical, not an available package or executable.

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

> Historical record: the external test and proof entrypoints described below have been retired. Application and release tools that shared these directories are retained; use `cargo xtask help` for current packaging commands.

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
