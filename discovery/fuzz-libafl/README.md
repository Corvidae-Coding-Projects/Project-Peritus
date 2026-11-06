# LibAFL backend for the shared Peritus fuzz targets

This optional workspace compiles the exact `../fuzz/src/lib.rs` consumers and twelve
`../fuzz/fuzz_targets/*.rs` entrypoints with the maintained `libafl_libfuzzer` 0.16.1
runtime. The dependency alias preserves the same `libfuzzer_sys::fuzz_target!` interface.
It adds a second search engine with comparison logging and optional Grimoire mutations;
it does not create a second implementation of the production code or its oracles.

Run from the repository root after installing pinned nightly tools:

```sh
rustup toolchain install nightly-2026-08-09 --profile minimal --component rust-src --component llvm-tools-preview
cargo +1.97.1 install cargo-fuzz --version 0.13.2 --locked
CC=clang CXX=clang++ RUSTC_WRAPPER= CARGO_BUILD_JOBS=1 cargo +nightly-2026-08-09 fuzz build \
  --fuzz-dir discovery/fuzz-libafl --careful --sanitizer address --codegen-units 16
mkdir -p /tmp/peritus-libafl-corpus /tmp/peritus-libafl-artifacts
cp discovery/fuzz/seeds/codec_frame/* /tmp/peritus-libafl-corpus/
CC=clang CXX=clang++ RUSTC_WRAPPER= CARGO_BUILD_JOBS=1 cargo +nightly-2026-08-09 fuzz run \
  --fuzz-dir discovery/fuzz-libafl --careful --sanitizer address --codegen-units 16 \
  codec_frame /tmp/peritus-libafl-corpus -- -runs=1000 -timeout=5 \
  -rss_limit_mb=2048 -malloc_limit_mb=256 -use_value_profile=1 -grimoire=0 \
  -tui=0 -ignore_crashes=0 -ignore_timeouts=0 -ignore_ooms=0 \
  -artifact_prefix=/tmp/peritus-libafl-artifacts/
```

Use fresh run directories and the repository campaign controller for recorded runs.
The controller executes the built binary directly and retains strict ASan ODR checks;
the upstream `cargo fuzz run` example above uses that tool's own ASan defaults.
Inspect both the subprocess result and **new artifacts**. The LibAFL restarter can exit
successfully after recording a crash; exit zero alone cannot establish a passing campaign.
Any crash, timeout, out-of-memory case, signal, or watchdog expiry is a failed or incomplete
run and requires replay. Retain exact case bytes and logs. `-runs` is a bounded number of
fuzzer stages, not an exact count of target invocations.

The runtime only supports part of libFuzzer's flags. Do not pass `-seed`, `-max_len`,
`-max_total_time`, or `-print_final_stats` to this engine. Bounds live in the shared
consumers; a parent watchdog limits total execution and cannot count expiration as pass.
The pinned runtime supports `-use_value_profile`, `-runs`, `-timeout`, `-artifact_prefix`,
`-rss_limit_mb`, `-malloc_limit_mb`, `-grimoire`, and `-fork`/`-jobs`. Grimoire can help
UTF-8/text targets; use zero for binary operation histories. Do not enable the experimental
TUI, which implicitly changes failure-ignore defaults unless explicitly overridden.

The outer Cargo.lock pins this workspace, but upstream's build script generates and
builds a separate runtime Cargo workspace. Preserve that generated runtime Cargo.lock
and its hash alongside each first-build receipt; its dependencies are not represented by
this outer lockfile. The runtime is compiled without the target's instrumentation to
avoid feeding fuzzer internals back into coverage.

AddressSanitizer and `--careful` instrument Rust subjects and rebuilt standard libraries.
C/FFI code built by dependencies and external processes require their own instrumentation
and behavioral checks. This Linux verification does not establish native macOS/Windows
behavior. The second engine improves search diversity; no finite campaign proves every
input, state, schedule, or source line correct.

Primary implementation references:

- https://docs.rs/libafl_libfuzzer/0.16.1/libafl_libfuzzer/
- https://github.com/AFLplusplus/LibAFL/tree/0.16.1/crates/libafl_libfuzzer
- https://rust-fuzz.github.io/book/cargo-fuzz.html
