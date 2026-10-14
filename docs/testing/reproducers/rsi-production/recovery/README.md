# Actual persisted rollout recovery replay

## Folder guide

Real-interface replay resources for recovery; start with the local README/driver and its documented disposable-state prerequisites.

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |
| [build_consumer.py](build_consumer.py) | Build the real public E3/F0 consumer against this checkout, with two jobs. |
| [consumer.rs](consumer.rs) | Local implementation for docs / testing / reproducers / rsi-production / recovery. Entry names: `AnyResult`, `digest`, `eid`, `save`, `read`. |
| [evaluator.py](evaluator.py) | Independent CLI requirements oracle, run only in evaluator stage. |

This public consumer executes the actual seeded Peritus baseline and corrected candidate CLI with the independent Python oracle, writes each genuine terminal record to the artifact store and a retained binary file, reads it through the checked recovery API, closes/reopens storage, and checks the same actual records again. It adversarially supplies truncations, trailing bytes, incorrect advertised digest, and a different frozen rollout specification, expecting rejection. Unknown usage/cost/process measurements must remain unknown. No fabricated execution records or qualification suites are used.

This consumer does not establish E1 materialization, native restricted execution, E3 publication,
D2 review, or F0 activation; the native production campaign supplies that separate evidence.

Build and run sequentially, using a fresh directory and actual baseline/candidate executables:

```sh
python3 docs/testing/reproducers/rsi-production/recovery/build_consumer.py \
  --repo "$PWD" --output /tmp/peritus-record-consumer-fresh
target/release/rsi-f0-recovery-consumer \
  /absolute/path/to/actual-baseline-peritus /absolute/path/to/actual-candidate-peritus \
  /tmp/peritus-record-evidence-fresh \
  "$PWD/docs/testing/reproducers/rsi-production/recovery/evaluator.py"
```

Inspect a retained genuine record or profile without launching child processes:

```sh
target/release/rsi-f0-recovery-consumer inspect-record \
  /absolute/path/to/actual-record.bin /absolute/path/to/actual-frozen-profile.json
target/release/rsi-f0-recovery-consumer inspect-profile \
  /absolute/path/to/actual-frozen-profile.json
```

Coordinate the two Cargo jobs with other builds. Preserve the generated root for failure replay;
do not run against the user's daemon state.

## Initial and final replays — 2026-10-13

Both independent storage-root replays exited 0. Each launched 13 actual candidate and 13 independent evaluator processes: the paired 12-cell matrix retained ten passes and two expected baseline version failures; the corrected candidate passed all six cells. The extra real missing-evaluator attempt remained infrastructure failure. Pre-effect cancellation launched no child.

| Evidence | Initial increment | Final frozen-source replay |
| --- | --- | --- |
| Release build with two jobs | 24.79 seconds | 34.67 seconds |
| Actual candidate / evaluator calls | 13 / 13 | 13 / 13 |
| Genuine persisted records | 14 | 14 |
| Malformed record inputs rejected | 4,466 | 4,466 |
| Public D3 coordination | Not exercised | Twelve real admissions, reservations before effects, exact result-digest completion, capacity release and fresh C0 replay passed |
| Consumer SHA-256 | `5ab90057e1581cbe9f7242e822450cf4a300cff0f22ee3c08743f84648bb0a41` | `a3a0e7a46c97995de5bd5f044f26c5302a71c6c790688cd3b208fa41479e3ae8` |
| Raw root | `/home/doll/Peritus-RSI-2026-10-13/f0-recovery-consumer/actual-run` | `/home/doll/Peritus-RSI-Production-2026-10-13/recovery-final/actual-run` |

Fourteen genuine records per replay—matrix outcomes, no-effect cancellation, and actual missing evaluator—retained exact outcomes, observed elapsed time, unknown provider/cost/process measurements and stage-presence differences through finalized artifact readback and fresh storage reopen. The 4,466 malformed payload cases are proper truncations of each matrix record, trailing bytes, incorrect retained digest and a different frozen rollout specification; they are not thousands of independent product workflows.

Complete frozen-profile storage/readback/reopen passed in both increments. Unsupported version, altered replication count, unrecognized field, trailing input and legacy Debug text were rejected. The final-source read-only inspector also accepted all fourteen retained initial genuine records against their exact original frozen specification and accepted the original canonical profile unchanged. The profile SHA-256 is `8b8b531750dba7c759a7d258560435b1181531a56e50d3aeebae9a5b4f09a5e0`. This library consumer carries inert ModelBacked metadata; it makes no C5 request and reports no observed model usage.

Final driver-source SHA-256: `03301ecca75fe3fda009eb01444028eeab0f4329081a7e92092f61051c921409`.
Final compact evidence is `recovery-final/final-summary.json`; retained-initial readback is `recovery-final/initial-artifacts-final-reader.json`. Each raw root contains `results.json`, `frozen-profile.json`, fourteen record binaries, `peritus.sqlite3` and protected finalized artifacts. No unit, mock, qualification, formal or repository verification suite ran.

This proves bounded public library execution, D3 durable coordination and recovery. It does not establish native daemon outbox admission, E1 materialization, native C2/C3 restricted execution, published E3 evidence, E2 diagnosis, D2 review or F0 activation. Those separate production campaign receipts are recorded in `docs/testing/rsi-production-real-2026-10-13.md`. No late cancellation observation was invented for this replay.
