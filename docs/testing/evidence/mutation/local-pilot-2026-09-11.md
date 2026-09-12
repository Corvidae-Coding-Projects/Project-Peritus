# Local mutation pilot

- Receipt campaign source: `310db7e7`
- Cancellation campaign source range: `ea176966` through `52b3f0a7`
- Host: Linux x86_64
- Tool: cargo-mutants 27.1.0

| Campaign | Discovered | Caught | Unviable | Missed | Timeout | Untested |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Receipt ledger, eight shards | 32 | 31 | 1 | 0 | 0 | 0 |
| Cancellation lifecycle, eight shards | 23 | 18 | 5 | 0 | 0 | 0 |

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
