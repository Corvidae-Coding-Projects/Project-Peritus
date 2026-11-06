# Fuzz-target navigation

This optional workspace consumes public production APIs. `src/lib.rs` implements
bounded raw and structured inputs with independent invariants. The twelve small
`fuzz_targets/` binaries expose those consumers to either libFuzzer here or the
alternate runtime in `../fuzz-libafl/`. `seeds/<target>/` contains small retained
starting inputs; `dictionary.dict` supplies protocol tokens to the primary engine.

Use `../checks.py` for recorded sanitizer search, exact replay, and minimization.
The pinned nightly here is independent of the product toolchain. Keep generated
corpus, coverage, and crash artifacts in run evidence, outside committed source.
Read `../TESTING.md` for actual scope and native/integration gaps.
