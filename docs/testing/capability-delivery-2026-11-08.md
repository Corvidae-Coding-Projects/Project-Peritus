# Capability delivery verification, November 8, 2026

Scope: ordinary ProductRunner construction, qualification and recovery on the develop working
checkout based on `0e07e16dd5fe72156aa210202d2843be359518b1`. This is a source change, not a release or
installation record. The source/caller coverage map is in
[the design](../../.design/capability-delivery-guardrails.md).

## Observed results

| Scenario | Result | Observation |
| --- | --- | --- |
| Durable construction scheduling | PASS | Dependency-ready selection, rotation, incomplete handoff debt and store reopen preserve required work. Repeated unfinished assignments require smaller prerequisites without deleting original outcomes. |
| Capability evidence isolation | PASS | Missing, duplicate, fabricated, unbound and stale proofs cannot qualify. A failing required operation remains unsatisfied while independently mapped passing obligations can remain satisfied. |
| Scope and state identity | PASS | Foreign-run state, omitted requirements, cycles, symlink state and an invalid queue cursor are rejected. A changed task, input revision or design artifact cannot recover the earlier admitted design. A Ready artifact without admitted work cannot take this recovery path. |
| Terminal and role contracts | PASS | Plan, draft and complete are distinct; mixed-kind fields and self-issued verification flags are rejected. Reviewer context excludes developer terminal instructions. |
| Final-candidate handoff | PASS | Absent or stale command bindings block handoff before independent review; a current binding can proceed to capability-specific review. |
| Credentialed native pipeline | PASS | Real Codex adapter, explicit gpt-6.1-sol / High selection, actual host tools, real private stores and a disposable Rust repository. Six required capability drafts were retained after interruption and all six received independent satisfied verdicts with scope coverage. |
| Independent CLI consumer | PASS | `answer`, `double 21`, `double 0`, `double 2147483647` produced exact expected stdout and success. Eight missing/unknown/invalid/extra/overflow invocations returned failure, explanatory stderr and empty stdout. |
| Independent library consumer | PASS | A separately compiled Rust program called the actual library and observed preserved `initial_value() == 1` and public `answer() == 42`. |
| Standalone command compatibility | PASS | A native public-library consumer operated the real executor in a non-Git folder, observed a real child exiting 17 with expected stdout and a nonempty effect receipt. |
| Existing runner suite | PASS | `CARGO_BUILD_JOBS=2 cargo test -p peritus-product-runner --lib --all-features`: 100 passed, zero failed. |
| Workspace static checks | PASS | `CARGO_BUILD_JOBS=2 cargo clippy --workspace --all-targets --all-features -- -D warnings`; workspace formatting and diff whitespace checks. |

## Defects found during the real run

The initial planning invocation duplicated `developer_evidence_read` because the read-only catalog
already declared it. The role-specific catalog assembly now declares it once.

An interrupted run without an S5 handoff attempted to restart design. Baseline code evidence had
changed during the run's own implementation; the designer repeatedly announced an already-published
Ready artifact before encountering a required/undeclared `design_publish` failure. The new recovery
path uses a previously admitted delivery contract to recover that exact frozen intent, with unchanged
identity and revision. Current code receives fresh checks and independent qualification. The final
credentialed replay entered Writing directly and retained all six drafts.

The developer's real verifier passed, then it deleted `verify_real_interfaces.py` from the workspace.
That cleanup changed the candidate and invalidated the earlier source-bound command observations.
The new handoff preflight returns this condition directly to the writer. Large command strings were
also replaced with short host-issued references in reviewer JSON; source/candidate matching remains
mandatory. Writer terminal instructions are delivered once in writer system policy and are absent
from reviewer policy.

Inspection also found that applying candidate capture to every standalone executor would impose a
Git requirement on existing plain-folder consumers. Candidate binding is now enabled by the coding
runner; legacy standalone executors retain their prior behavior. The native compatibility replay
operated the final executor successfully.

## Replay and retained evidence

```sh
PERITUS_DELIVERY_CAMPAIGN=1 CARGO_BUILD_JOBS=2 cargo run -p peritus-product-runner --example live-account-product-run
python3 docs/testing/reproducers/capability-non-git-command.py
```

The first command uses a real authenticated account and incurs model usage. To replay only an owned
disposable fixture after interruption, set both `PERITUS_LIVE_WORKSPACE` and `PERITUS_LIVE_STATE` to
its original locations. The example validates the fixture package before reuse. Normal user runs
do not need these test environment variables.

Local evidence is retained under `/home/doll/.cache/peritus-delivery-native-evidence/`, including
`delivery.json`, `design.md`, `result.json`, and the generated source. The successful process log is
`/home/doll/.cache/peritus-delivery-live-acceptance-final.log`. Final runner/Clippy/legacy consumer
logs use the `peritus-delivery-*-final.log` names in the same cache. These are local observations,
not repository-wide coverage claims.

## Coverage limits

The credentialed campaign ran on Linux through the public native runner interface. It did not
reinstall the user's application or restart the user's daemon. A separate TUI/GUI interaction,
Windows/macOS runtime campaign, and sustained SSH-project performance comparison were not performed.
Cancellation and no-patch code paths were inspected and updated, rather than credentialed through
every client. AI scope and semantic review remain fallible; the host mechanically enforces the
captured stage, identity, dependency, admission and evidence rules. Pre/post fingerprints do not lock
out concurrent or transient source modification.
