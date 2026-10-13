# Native RSI real-operation replays

Use fresh disposable roots and actual baseline/candidate programs. Keep one native worker and at
most two Cargo jobs across concurrent work. Never open a second writer against a user's live
daemon. These are real public program consumers, not unit or qualification suites.

1. [Evolution](evolution/README.md) creates genuine Git/C1/E1 materializations, imports fresh
   independent review, stages actual E2 evidence, selects under the frozen policy, and requires
   explicit fresh approval operations for promotion and rollback.
2. [Native execution](native/README.md) runs actual daemon/C2/C3 campaigns, independent evaluators,
   cancellation/recovery/collection, and selected-program adoption. Its
   [results](native/RESULTS-2026-10-13.md) state the exact observed scope.
3. [Record recovery](recovery/README.md) exercises real CLI/evaluator processes, public D3
   coordination and strict finalized profile/record recovery. Its malformed payload count is not
   a count of independent product journeys.

To replay the historical unsupported schedule with the changed daemon:

```sh
python3 docs/testing/reproducers/rsi-production/legacy_replay.py \
  --source /absolute/path/to/closed-original-failed-fixture \
  --root /tmp/peritus-legacy-fresh \
  --daemon "$PWD/target/release/peritusd" --cli "$PWD/target/release/peritus"
```

The original fixture must be closed and retain its daemon configuration, public approval registry,
SQLite journal and actual artifact files. This copies them, checks ordinary daemon readiness and
real acknowledgement, and gracefully stops only the daemon it created. Use the native consumer's
`inspect` mode afterward for owning campaign/diagnostic readback. Unsupported Debug text must
remain an explicit failure, never become execution authority.

The [campaign report](../../rsi-production-real-2026-10-13.md) distinguishes final replay,
earlier increments and unobserved boundaries. Compact observed metadata is historical evidence,
never authorization or an input receipt. Large copied repositories, executable artifacts, private
user state and signing keys are not bundled.
