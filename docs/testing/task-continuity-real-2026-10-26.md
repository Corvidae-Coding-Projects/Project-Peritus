# Failed-read task continuity: issue #160

The reported screenshot shows four `workspace_read` rejections with `$/path is required`,
followed by a return to an older build-identity task instead of the active pinned-file
investigation. The source defect is reproducible through the actual public tool/context
interfaces. This record covers that failure and its recovery, not all causes of model drift.

Baseline: develop `8f4d34310202d074da0618aafea8aa8344202f84`.
Implementation: `local_context/assembly/exchanges.rs`, `local_context/activity.rs`, and
`developer_tools/arguments.rs` in `peritus-product-runner`.

## Contract and reproduction

Before execution, the replay requires:

- The current assistant plan and completed failed tool exchanges remain in the next request.
- Each retained tool proposal has exactly one matching result, including rejected calls.
- A malformed call establishes no filesystem effect or target-unavailability claim.
- A successful sibling read survives another call's argument rejection.
- A corrected read continues the current task; reopening preserves the same recovery evidence.
- Private source bytes remain unavailable. Revoking read permission removes earlier read
  content; restoring permission permits the current corrected result again.
- A later successful step can move older exchanges into the archive. It must retain the
  new current plan; the oracle does not demand that all earlier failures remain inline forever.

The fixture contains a completed identity task, an active pinned-file task, two distinct
payloads, a successful sibling read, and a separately protected source file. Its latest batch
has the active assistant plan and four actual `workspace_read {}` proposals. The real host
returns schema errors; the consumer records them through the same durable context port and
memory-tool wrapper used by the developer loop. It then inspects the exact assembled messages.

On the original program, that batch disappears: zero of its four call/result pairs remain,
and its current plan is absent as an assistant exchange. A successful sibling disappears too.
An older identity-check exchange remains inline. The latest prose may survive only as an
optional prior-observation excerpt, which does not preserve the actual failed exchange.

`IndexedExchange::finish` previously enrolled every proposed read for source-access filtering.
A missing/non-string path became `None`; `groups` treated it as a source that could not be
reused and dropped the entire exchange. A denied private-file read had the same effect.

Only **successful** reads now enter that source-admission list. Failed proposals and their
errors stay together with the current plan. Successful reads still undergo current access
checks and stale-content handling. Argument rejection and repeated-failure feedback explicitly
keep the current task active and distinguish invalid arguments from an unavailable target.

## Executed results

Linux x86-64, workspace-pinned Rust toolchain/dependencies, two Cargo build jobs. No unit,
mock-provider, formal-verification, or repository qualification suites were run.

| Journey | Original program | Changed program |
| --- | --- | --- |
| Four missing-path reads, alone and mixed with a successful read | Latest failed exchange and current plan lost | PASS: plan, all errors, and sibling retained |
| Null, number, boolean, array, object, and misspelled path | Latest failed exchange lost | PASS |
| Private-source rejection | Failed exchange lost | PASS: rejection retained, private bytes absent |
| Empty path, invalid range type, absent file, parent traversal | Continuity checks pass | PASS |
| Close/reopen durable context after each failed batch | Repeats the corresponding lost-exchange failures | PASS |
| Corrected read, then permission revocation/restoration | Existing source-admission behavior exercised | PASS: corrected result retained; revoked content hidden |
| Official Codex / `gpt-6.1-sol` continuation from four rejected reads | Not used as a probabilistic baseline | PASS: 3 model turns, 3 successful current-task reads |
| Official OpenCode Zen / `deepseek-v4.1-flash` continuation from four rejected reads | Not used as a probabilistic baseline | PASS: 4 model turns, 5 successful current-task operations |

The deterministic generator enumerates 12 invalid-input classes in both error-only and mixed
batches: **24 journeys**, without randomness. Sixteen original-program journeys fail the
immediate/reopened continuity requirement; the changed program passes all 24. Each journey
includes a valid retry and read-permission lifecycle. The privacy fixture uses delivery-scope
private-source protection, rather than a leave-alone constraint that protects mutations only.

Both live runs read `pack_lock.json` and the exact two files it names. Their final reports
contain independently created values `ALPHA-7391` / `fortress` and `BETA-2846` / `village`.
Neither launches an identity check. Codex reads the three files directly; DeepSeek also lists
the workspace and payload directory. No model responses or source bytes are fabricated by the
driver. The failed-call history is deliberately supplied as a fixture because spontaneous
model mistakes would make this regression nondeterministic.

Build and supporting checks:

```sh
python3 docs/testing/reproducers/open-issues/build-task-continuity.py CONSUMER_DIRECTORY
cargo fmt --all -- --check
cargo clippy --offline --jobs 2 -p peritus-product-runner --lib --all-features -- -D warnings
```

Build, formatting, and strict affected-library Clippy passed. Mutable artifacts, provider
envelopes, exact context views, stdout/stderr, and JSON case results are outside the repository
under `/home/doll/.cache/peritus-issue160/`: `baseline2/`, `final/`, `live-codex/`, and `live-zen/`.
Replay instructions and the concrete generator/consumer are retained in
[`reproducers/open-issues/README.md`](reproducers/open-issues/README.md).

## Limits

This exercises real library callers, filesystem/state recovery, the developer loop, and two
official provider transports. It does not recreate the reporter's project or network download,
exercise a TUI rendering change, or establish that every model will always follow instructions.
It fixes the deterministic host-side loss of the current failed-read exchange while retaining
the existing authorization boundaries. No user daemon, installed binaries, or active run was
changed by this campaign.
