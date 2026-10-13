# RSI E3/F0 real candidate replay

This consumer runs real baseline/candidate Peritus CLI executables and a separate Python evaluator, uses the real content-addressed artifact store and SQLite journal, then submits a genuine E3 schedule request to a disposable daemon.

The baseline must contain the retained version regression (`peritus --version` prints `peritus 0.0.0` while its Cargo version is 0.0.7). The corrected candidate prints `peritus 0.0.7`. Both must preserve help and invalid-argument behavior. This seeded defect exists only in the disposable source clone; it was never introduced into the product checkout.

Build the public library consumer against the current checkout:

```sh
python3 docs/testing/reproducers/rsi-f0/build_consumer.py --output /tmp/peritus-rsi-consumer-build
```

The launcher generates Cargo path dependencies using the actual checkout location. It builds with two jobs and does not run unit tests, qualification fixtures, or formal verification. Coordinate this build with other Cargo work on a resource-constrained host.

Run the retained failing daemon replay with a fresh evidence root:

```sh
python3 docs/testing/reproducers/rsi-f0/replay.py --consumer target/release/rsi-f0-real-consumer --baseline /home/doll/Peritus-RSI-2026-10-12/baseline-peritus --candidate /home/doll/Peritus-RSI-2026-10-12/native/bin/peritus --daemon /home/doll/Peritus-RSI-2026-10-12/native/bin/peritusd --cli /home/doll/Peritus-RSI-2026-10-12/native/bin/peritus --approval-registry /home/doll/Peritus-RSI-2026-10-12/native/state/peritus/approval-registry.bin --root /tmp/peritus-rsi-f0-replay
```

Supply real executable and approval-registry paths from the disposable campaign. The registry belongs to fixture actor `4fb7c7b74d30ab95665b8d7d9dcd7c25`; no model credentials are required. The replay copies it into the disposable root and starts/stops only its own daemon. It refuses to overwrite existing evidence.

The replay currently exits **1**: the valid `peritus.eval.schedule-rollout.v1` directive has no configured production handler, and the daemon becomes read-only. A passing replay requires writable daemon readiness and actual schedule acknowledgement. Hiding the error while leaving the request undelivered does not pass.

The library portion checks 12 real CLI rollout outcomes, exact paired accounting, report finalization and readback, cancellation before effects, a real missing evaluator subprocess, and a cancelled-campaign SQLite restart. The report is a finalized **artifact**, not a `RecordPublication` transition or published F0 evidence. The frozen-profile Debug artifact is inert retained input material, not a runnable persisted catalog. Actual E1 materialization receipts, D3 runtime scheduling, C2/C3 restricted isolation, provider execution, F0 promotion, and rollback are outside this consumer's verified scope.

See [the campaign report](../../rsi-f0-real-2026-10-12.md) for observations, root cause, raw evidence, and unfinished production boundaries.
