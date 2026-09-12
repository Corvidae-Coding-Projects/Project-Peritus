# Local mutation pilot

- Receipt campaign source: `310db7e7`
- Cancellation campaign source range: `ea176966` through `52b3f0a7`
- Host: Linux x86_64
- Tool: cargo-mutants 27.1.0

| Campaign | Discovered | Caught | Unviable | Missed | Timeout | Untested |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Receipt ledger, eight shards | 32 | 31 | 1 | 0 | 0 | 0 |
| Cancellation lifecycle, eight shards | 23 | 18 | 5 | 0 | 0 | 0 |

## Pilot invariant to mutation mapping

| Pilot invariant | Reviewed semantic mutation | Designated behavioral test | Result |
| --- | --- | --- | --- |
| Required context closure may use available headroom but never exceed the hard token capacity | Remove `needed > available_tokens` from the capacity guard | `headroom_preserves_required_roots_and_shared_dependencies_without_optional_expansion` | Curated canary rejected the mutant with exit 101; automatic inventory remains explicitly unreachable |
| A completed receipt cannot be rebound to a different tool or canonical request | At receipt line 99, replace either identity `!=` with `==`, or replace the joining `||` with `&&` | `reused_provider_call_id_conflicts_on_each_identity_dimension` | All three reviewed mutants were `CaughtMutant`; the baseline passed |
| A result that becomes `Complete` while shutdown drains remains `Complete` despite a late cancel | In `ProductRunService::shutdown`, replace the phase/cancellation `&&` guard with `||` | `accepted_result_stays_complete_when_shutdown_follows_late_cancellation` | `CaughtMutant`; the test failed on the terminal-phase assertion |
| Generic retry cannot reopen a terminal complete chat merely because durable input is pending | Delete either negation in the retry admission guard | `chat_hands_off_to_the_existing_pipeline_with_the_selected_independent_reviewer` | Both reviewed mutants were `CaughtMutant`; the exact failure was `generic retry must not reopen a complete chat with pending input` |

The receipt identity review also led to the later-ordinal provider-call-ID defect. Reverting the
global identity check at `310db7e7` is a semantic canary: the checked-in
`reused_provider_call_id_cannot_dispatch_a_second_external_effect` regression dispatches two real
child effects and fails with an independently synced two-line counter. Its three-run old-revision
record is in [`../../reproducers/reused-provider-call-id.json`](../../reproducers/reused-provider-call-id.json).

After the slice-filter correction, receipt shard 0 was replayed from
`5d6f37ac0730d20e92a716fe49cf7161623dfd10`. Its environment record contained
`excluded_tests=[]` and `unix_socket_bind=not_required`; the real baseline passed, three mutants
were caught, one was unviable, and none were missed, timed out, or left untested.

The receipt campaign exposed an actual product defect: a provider could reuse a call identifier at
a later ordinal and cause the runner to execute the same effect twice. The ledger now rejects an
identifier reused by any later effect request. Recovery loading also rejects inconsistent
Started/Ambiguous results and incomplete Completed results. The focused regression proves the
second effect is not executed.

The cancellation campaign killed two valid guards that previously lacked precise race coverage.
One regression now schedules a late cancel while shutdown is draining the task and proves a
terminal Complete run remains Complete. The other gives a terminal Complete chat recovery-shaped
pending input and proves a generic retry neither reopens it nor sends another provider request.

This host's sandbox denies AF_UNIX socket creation. The campaign records that capability failure
and restricts local cancellation mutants to `product_run::`; capable CI hosts run the full daemon
package. Compiler rejected mutants are classified as unviable and were not counted as behavioral
catches. Scratch roots, compiler caches, clean source clones, and shard names are isolated, so
parallel runs cannot share mutable campaign state.
