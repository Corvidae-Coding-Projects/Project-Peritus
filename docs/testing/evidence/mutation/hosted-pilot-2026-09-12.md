# First hosted discovery pilot

Workflow run [34663371138](https://github.com/Corvidae-Coding-Projects/Project-Peritus/actions/runs/34663371138)
executed revision `2c2be90c03dc4987ff6fb143e031e7efadc83612` on fresh GitHub-hosted
Ubuntu runners. Replay, the POSIX lifecycle suite, the context canary, all four fuzz targets, and
receipt shards 0, 4, 5, and 6 passed. The run failed for two independently classified reasons.

Receipt mutation shards [1](https://github.com/Corvidae-Coding-Projects/Project-Peritus/actions/runs/34663371138/job/103470295834),
[2](https://github.com/Corvidae-Coding-Projects/Project-Peritus/actions/runs/34663371138/job/103470295800),
[3](https://github.com/Corvidae-Coding-Projects/Project-Peritus/actions/runs/34663371138/job/103470295801),
and [7](https://github.com/Corvidae-Coding-Projects/Project-Peritus/actions/runs/34663371138/job/103470295853)
all failed their unmutated ProductRunner baseline with `reserve durable command ordinal: database is
locked`. Four independent failures at the same owner boundary promoted this to an accepted product
defect. Applying the new 500 ms held-writer regression to the old production source fails at the
250 ms timeout; revision `5a7746170f42a6066c4b106e11e2bffb93673499` restores a five-second
contention allowance and passes the regression, the 277-test ProductRunner unit suite, its 20
integration tests, and strict Clippy. The schema-versioned replay is
[`command-ordinal-contention.json`](../../reproducers/command-ordinal-contention.json).

All eight cancellation shards timed out their unmutated baseline. Their retained logs showed the
entire daemon test package running under the 60-second test bound, even though the selected mutants
were confined to `product_run/lifecycle.rs`. This was a campaign configuration failure, so it is
not counted as a caught, missed, or product mutant result. The corrected command always forwards
the `product_run::` owner filter for the cancellation slice and uses a 120-second per-test ceiling
within the unchanged eight-minute campaign limit. A unit test verifies that the filter is installed
unconditionally before the Unix-socket capability probe, including the supported capability path
that exposed the hosted failure.

The final branch revision must rerun every shard. This pilot supplies pre-fix evidence only and
cannot qualify a later source revision.

## Cancellation shard sizing on the follow-up revision

At revision `c1fae087cd43e890eef4a58078efb2dcdc5c46b8`, cancellation
[shard 0](https://github.com/Corvidae-Coding-Projects/Project-Peritus/actions/runs/34670380089/job/103490521768)
passed its baseline in 132.789 seconds of build time plus 71.206 seconds of test time.
Two mutants were caught after another 95.899 and 99.049 seconds. The third mutant rebuilt in
27.288 seconds, but its tests were interrupted at the unchanged 480-second campaign ceiling.
The retained summary correctly reported two caught, one untested, and no missed or timed-out
mutants; the whole campaign remained incomplete. Setup had already consumed about 123 seconds
of the ten-minute hosted job budget.

The cancellation inventory therefore uses twelve shards while receipt retains eight. The pinned
cargo-mutants 27.1.0 slice partition puts two mutants in shards 0 through 10 and one in shard 11;
listing every selected inventory verified that their concatenation equals all 23 unique mutants,
without omissions or overlaps. Full owning tests, baseline execution, phase timeouts, campaign
and hosted ceilings remain intact. The workflow excludes only receipt shards 8 through 11, and
its policy rejects missing, duplicate, or altered exclusions. A final hosted run must establish
the delivered revision's timing and complete outcomes; the measurements above are pre-fix evidence.
