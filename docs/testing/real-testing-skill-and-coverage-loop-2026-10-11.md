# Real-testing skill and missing-check-contract loop repair

## Behavior

Every model-based design, writer/fixer, and reviewer prompt now includes the bundled real-testing
skill. Behavioral acceptance guidance defaults to actual program interfaces, independent expected
outcomes, relevant input/lifecycle variations, and reproducible failures. The canonical Markdown
and upstream MIT license are retained in peritus-product-runner/skills. The design/check/reviewer/
fixer pipeline remains in place.

Missing executable check contracts now stop at planning with a normal Gate settlement before
reviewer/fixer routing. The current candidate is retained and the uncovered paths are named.
The candidate is not marked qualified. Ordinary covered plans continue to execute the existing
checks; genuine failures from executed checks retain the existing repair path. Empty-candidate
and explicitly authorized external-effect acceptance semantics are unchanged.

## Confirmed defect and replay

A source workspace contained only README.md and executable greet.sh, initially:

```sh
#!/bin/sh
printf 'Hello, world!\n'
```

The request was to fix greet.sh to require one nonempty name, preserve spaces and punctuation,
print the exact greeting with a newline, reject zero/empty/two arguments with usage on stderr and
exit 2, preserve mode 0755, demonstrate the real CLI, and change only greet.sh. Unit tests, mocks,
test suites, and formal verification were prohibited.

Before: the script passed every requested real invocation, but the check planner reported it as
uncovered. That planning failure was collapsed into candidate failure and repeatedly sent to the
fixer. At 5m 19s the unchanged candidate had reached fixer cycle 3, with 23 provider requests and
33 tool calls. The earlier trace and preserved candidate are in
`/home/doll/Peritus-real-testing-skill-integration-2026-10-11/`.

After: the same request through the installed Peritus terminal /build pathway, a fresh managed Git
worktree, and the real ChatGPT-account provider stopped at cycle 1 with Cause=Gate and
Disposition=CandidateAvailable. It named greet.sh and its missing check contract, retained the
SelfChecked candidate, and requested no fixer. Seven fresh executable invocations passed their
predefined stdout/stderr/exit-status contract: Mira Chen, A&B, percent-format characters, a leading
hyphen, and all three invalid argument forms. The executable bit remained set.

Retrying the unchanged candidate returned the same concrete planning blocker. The provider/tool
trace was byte-for-byte unchanged: zero additional model or tool calls. This proves that resuming
at Checking does not restart the failed fixer loop.

A supported JSON artifact workspace (schema_version=1, kind=artifact) then exercised the unchanged
check/review path. Peritus changed only settings.json to name=Mira Chen, enabled=true, port=8087,
loaded it with a normal JSON consumer, passed its native checks and independent review, and
reached a qualified Complete handoff at cycle 1. A fresh JSON readback confirmed exact values and
types, and the workspace manifest remained unchanged.

## Evidence and boundaries

Optimized build: `CARGO_BUILD_JOBS=2 cargo build --release --locked --package peritus-daemon --bin peritusd`.
The updated daemon was installed atomically at `/home/doll/.local/bin/peritusd`; its prior version
is backed up. Build log, run IDs, normal CLI settlement readbacks, the unchanged-retry result,
and direct artifact/command readbacks are retained in
`/home/doll/Peritus-check-loop-fix-2026-10-11/`.

No unit, mock, Clippy, repository verification, formal proof, or qualification suites were invoked.
The native Peritus checks above were exercised as retained application behavior. This is a bounded
loop/continuation and supported-plan replay, not full application or multiple-hour coverage.
Unsupported source contracts still require configuration or an explicitly unqualified human
handoff; this repair stops wasted fixer cycles without treating missing checks as passed.
