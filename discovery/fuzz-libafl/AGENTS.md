# Alternate fuzz-engine navigation

This optional workspace compiles the consumers and target entrypoints from
`../fuzz/` with the pinned `libafl_libfuzzer` runtime. It supplies search-engine
diversity without copying production code or correctness oracles.

Use `../checks.py fuzz --engine libafl` for recorded runs. Preserve the generated
runtime lockfile and artifact directory in each receipt: the upstream build script
creates a nested runtime workspace whose dependencies are separate from this outer
lockfile. An exit code alone is insufficient because the runtime can retain a crash;
the controller checks both process status and new artifacts. See `README.md` and
`../TESTING.md` for supported flags and coverage limits.
