# Discovery code navigation

[proptest-praxis/](proptest-praxis/AGENTS.md) contains the optional, reproducible property-testing workspace. Its test code uses actual Peritus library interfaces and remains outside the root release dependency graph. Generated run artifacts are local and ignored.

[TESTING.md](TESTING.md) describes optional mutation and coverage-guided campaigns.
`checks.py` runs pinned sanitizer searches, exact replay, and minimization;
`test_checks.py` and `test_mutation.py` verify result precedence, external-engine
evidence parsing, and isolation from ambient compiler caches;
`tools.toml` pins their toolchain and CLI versions. `coverage.py` accounts for every
current root package and names source associations without claiming execution.
`fuzz/` contains the primary libFuzzer harnesses and retained seed inputs;
`fuzz-libafl/` runs the same harnesses with the alternate LibAFL engine.
`mutation/` stages disposable source and connects cargo-mutants to actual property checks.
Keep campaign evidence outside source; never treat inventory, a zero-test baseline,
an expired budget, or a missing report as a passing behavioral check.
