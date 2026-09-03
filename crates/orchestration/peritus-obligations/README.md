# peritus-obligations

`peritus-obligations` is the pure S2 contract for turning public task clauses into checked
requirements. A ledger retains each exact clause and its source span, distinguishes mandatory
outputs from inputs and examples, and binds every evidence observation to the current ledger and
candidate.

Qualification is fail-closed. Required direct, performance, lifecycle, schema, browser, and
external-effect obligations need matching current evidence. Conditional obligations activate only
after their public condition is observed, and an alternative group succeeds only when every member
of at least one branch succeeds. Internal lifecycle simulation and hand-written HTML parsing remain
supplementary evidence and cannot satisfy claims about real ingress or browser behavior.

The crate also owns a closed failure taxonomy. Only `CandidateDefect` authorizes another fixer
cycle; ambiguity gets at most one material question, provider and harness failures recover when a
recovery route exists, and external-evaluator failures settle without changing candidate quality.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-obligations
CARGO_BUILD_JOBS=2 cargo verus verify --package peritus-obligations --locked --check-toolchain \
  --fwd-verus-args-to roots -- --no-cheating --rlimit 20
```
