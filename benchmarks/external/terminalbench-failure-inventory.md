# Terminal-Bench diagnostic failure inventory

This appendix accounts for every trial in the frozen 445-trial campaign. Each of the 89 tasks has
five trials. `R1`, `R0`, and `U` are official reward-one, reward-zero, and unscored counts. `A`,
`R`, and `M` are native Peritus accepted, rejected, and report-missing counts. Native failure kinds
and Harbor exception types are listed whenever present. Counts are deliberately not converted into
one blame-adjusted score.

The [adverse-trial ledger](terminalbench-adverse-trials.md) names all 347 individual trials with at
least one adverse official, native, or exception dimension.

## Complete 89-task accounting

| Task | R1 | R0 | U | A | R | M | Native failure kinds | Harbor exception kinds |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- |
| `adaptive-rejection-sampler` | 2 | 3 | 0 | 1 | 2 | 2 | provider | AgentTimeoutError |
| `bn-fit-modify` | 4 | 0 | 1 | 3 | 2 | 0 | provider | RuntimeError |
| `break-filter-js-from-html` | 0 | 4 | 1 | 0 | 5 | 0 | provider | RuntimeError |
| `build-cython-ext` | 5 | 0 | 0 | 0 | 4 | 1 | provider | AgentTimeoutError |
| `build-pmars` | 4 | 1 | 0 | 0 | 5 | 0 | provider | — |
| `build-pov-ray` | 3 | 1 | 1 | 0 | 5 | 0 | provider | RuntimeError |
| `caffe-cifar-10` | 0 | 5 | 0 | 0 | 1 | 4 | provider | AgentTimeoutError |
| `cancel-async-tasks` | 1 | 3 | 1 | 2 | 2 | 1 | provider | RuntimeError |
| `chess-best-move` | 4 | 1 | 0 | 2 | 3 | 0 | provider | — |
| `circuit-fibsqrt` | 4 | 0 | 1 | 2 | 3 | 0 | provider | RuntimeError |
| `cobol-modernization` | 5 | 0 | 0 | 1 | 3 | 1 | gate, provider | AgentTimeoutError |
| `code-from-image` | 4 | 0 | 1 | 2 | 3 | 0 | provider | RuntimeError |
| `compile-compcert` | 3 | 1 | 1 | 1 | 4 | 0 | gate, provider | RuntimeError |
| `configure-git-webserver` | 3 | 1 | 1 | 2 | 3 | 0 | provider | RuntimeError |
| `constraints-scheduling` | 5 | 0 | 0 | 2 | 3 | 0 | provider, waiting_for_user | — |
| `count-dataset-tokens` | 5 | 0 | 0 | 1 | 4 | 0 | gate, provider | — |
| `crack-7z-hash` | 0 | 2 | 3 | 0 | 2 | 3 | repository | RuntimeError |
| `custom-memory-heap-crash` | 5 | 0 | 0 | 3 | 2 | 0 | provider | — |
| `db-wal-recovery` | 5 | 0 | 0 | 3 | 2 | 0 | provider | — |
| `distribution-search` | 4 | 0 | 1 | 2 | 3 | 0 | provider | RuntimeError |
| `dna-assembly` | 1 | 3 | 1 | 1 | 3 | 1 | provider | AgentTimeoutError, RuntimeError |
| `dna-insert` | 0 | 5 | 0 | 2 | 3 | 0 | provider | — |
| `extract-elf` | 4 | 1 | 0 | 1 | 3 | 1 | provider | AgentTimeoutError |
| `extract-moves-from-video` | 0 | 4 | 1 | 0 | 1 | 4 | provider | AgentTimeoutError, RuntimeError |
| `feal-differential-cryptanalysis` | 2 | 2 | 1 | 2 | 3 | 0 | provider | RuntimeError |
| `feal-linear-cryptanalysis` | 2 | 3 | 0 | 1 | 4 | 0 | provider | — |
| `filter-js-from-html` | 0 | 4 | 1 | 2 | 2 | 1 | provider | AgentTimeoutError, VerifierTimeoutError |
| `financial-document-processor` | 0 | 5 | 0 | 0 | 3 | 2 | provider | AgentTimeoutError |
| `fix-code-vulnerability` | 2 | 2 | 1 | 0 | 4 | 1 | provider, waiting_for_user | AgentTimeoutError, RuntimeError |
| `fix-git` | 5 | 0 | 0 | 0 | 5 | 0 | gate, provider, repository | — |
| `fix-ocaml-gc` | 0 | 2 | 3 | 0 | 2 | 3 | repository | RuntimeError |
| `gcode-to-text` | 1 | 4 | 0 | 2 | 1 | 2 | provider | AgentTimeoutError |
| `git-leak-recovery` | 5 | 0 | 0 | 3 | 2 | 0 | provider | — |
| `git-multibranch` | 5 | 0 | 0 | 2 | 2 | 1 | gate, provider | AgentTimeoutError |
| `gpt2-codegolf` | 1 | 4 | 0 | 0 | 2 | 3 | provider | AgentTimeoutError |
| `headless-terminal` | 5 | 0 | 0 | 2 | 2 | 1 | provider | AgentTimeoutError |
| `hf-model-inference` | 4 | 1 | 0 | 2 | 3 | 0 | provider | — |
| `install-windows-3-11` | 0 | 5 | 0 | 4 | 1 | 0 | provider | — |
| `kv-store-grpc` | 4 | 1 | 0 | 2 | 3 | 0 | gate, provider | — |
| `large-scale-text-editing` | 4 | 0 | 1 | 1 | 4 | 0 | provider | RuntimeError |
| `largest-eigenval` | 3 | 2 | 0 | 2 | 3 | 0 | provider | — |
| `llm-inference-batching-scheduler` | 4 | 0 | 1 | 2 | 3 | 0 | provider | RuntimeError |
| `log-summary-date-ranges` | 5 | 0 | 0 | 3 | 2 | 0 | provider | — |
| `mailman` | 5 | 0 | 0 | 2 | 3 | 0 | gate, provider | — |
| `make-doom-for-mips` | 0 | 4 | 1 | 0 | 5 | 0 | provider | RuntimeError |
| `make-mips-interpreter` | 2 | 2 | 1 | 1 | 4 | 0 | provider | RuntimeError |
| `mcmc-sampling-stan` | 0 | 4 | 1 | 0 | 5 | 0 | provider | RuntimeError |
| `merge-diff-arc-agi-task` | 4 | 0 | 1 | 0 | 1 | 4 | provider | AgentTimeoutError, RuntimeError |
| `model-extraction-relu-logits` | 3 | 2 | 0 | 1 | 3 | 1 | provider | AgentTimeoutError |
| `modernize-scientific-stack` | 4 | 0 | 1 | 0 | 5 | 0 | gate, provider | RuntimeError |
| `mteb-leaderboard` | 0 | 5 | 0 | 1 | 3 | 1 | provider | AgentTimeoutError |
| `mteb-retrieve` | 0 | 4 | 1 | 3 | 2 | 0 | provider | RuntimeError |
| `multi-source-data-merger` | 4 | 0 | 1 | 3 | 1 | 1 | provider | RuntimeError |
| `nginx-request-logging` | 5 | 0 | 0 | 2 | 2 | 1 | gate, provider | AgentTimeoutError |
| `openssl-selfsigned-cert` | 5 | 0 | 0 | 3 | 2 | 0 | provider | — |
| `overfull-hbox` | 3 | 1 | 1 | 2 | 3 | 0 | provider | RuntimeError |
| `password-recovery` | 3 | 1 | 1 | 1 | 3 | 1 | provider | AgentTimeoutError, RuntimeError |
| `path-tracing` | 3 | 1 | 1 | 2 | 3 | 0 | provider | RuntimeError |
| `path-tracing-reverse` | 4 | 1 | 0 | 1 | 2 | 2 | provider | AgentTimeoutError |
| `polyglot-c-py` | 0 | 4 | 1 | 2 | 3 | 0 | provider | RuntimeError |
| `polyglot-rust-c` | 1 | 4 | 0 | 3 | 2 | 0 | provider | — |
| `portfolio-optimization` | 4 | 0 | 1 | 3 | 2 | 0 | provider | RuntimeError |
| `protein-assembly` | 0 | 5 | 0 | 1 | 4 | 0 | provider | AgentTimeoutError |
| `prove-plus-comm` | 2 | 0 | 3 | 2 | 0 | 3 | — | RuntimeError |
| `pypi-server` | 3 | 2 | 0 | 3 | 2 | 0 | provider | — |
| `pytorch-model-cli` | 4 | 0 | 1 | 2 | 3 | 0 | provider | RuntimeError |
| `pytorch-model-recovery` | 0 | 2 | 3 | 3 | 2 | 0 | provider | AgentTimeoutError, VerifierTimeoutError |
| `qemu-alpine-ssh` | 2 | 3 | 0 | 0 | 2 | 3 | gate, provider | AgentTimeoutError |
| `qemu-startup` | 4 | 0 | 1 | 2 | 3 | 0 | provider | RuntimeError |
| `query-optimize` | 1 | 3 | 1 | 2 | 2 | 1 | provider | AgentTimeoutError, RuntimeError |
| `raman-fitting` | 1 | 4 | 0 | 2 | 3 | 0 | provider | — |
| `regex-chess` | 2 | 3 | 0 | 0 | 5 | 0 | provider | — |
| `regex-log` | 5 | 0 | 0 | 4 | 1 | 0 | provider | — |
| `reshard-c4-data` | 0 | 2 | 3 | 0 | 2 | 3 | repository | RuntimeError |
| `rstan-to-pystan` | 3 | 2 | 0 | 2 | 3 | 0 | provider | — |
| `sam-cell-seg` | 0 | 4 | 1 | 2 | 3 | 0 | provider | RuntimeError |
| `sanitize-git-repo` | 5 | 0 | 0 | 0 | 1 | 4 | provider | AgentTimeoutError |
| `schemelike-metacircular-eval` | 5 | 0 | 0 | 2 | 3 | 0 | gate, provider | — |
| `sparql-university` | 5 | 0 | 0 | 2 | 3 | 0 | provider | — |
| `sqlite-db-truncate` | 4 | 0 | 1 | 3 | 2 | 0 | provider | RuntimeError |
| `sqlite-with-gcov` | 3 | 2 | 0 | 0 | 4 | 1 | provider, waiting_for_user | AgentTimeoutError |
| `torch-pipeline-parallelism` | 0 | 0 | 5 | 0 | 4 | 1 | gate, provider | AgentTimeoutError, VerifierTimeoutError |
| `torch-tensor-parallelism` | 0 | 4 | 1 | 0 | 3 | 2 | gate, provider | AgentTimeoutError, VerifierTimeoutError |
| `train-fasttext` | 1 | 4 | 0 | 1 | 1 | 3 | provider | AgentTimeoutError |
| `tune-mjcf` | 4 | 1 | 0 | 3 | 2 | 0 | provider | — |
| `video-processing` | 1 | 3 | 1 | 2 | 3 | 0 | provider | RuntimeError |
| `vulnerable-secret` | 1 | 4 | 0 | 1 | 4 | 0 | provider | — |
| `winning-avg-corewars` | 5 | 0 | 0 | 1 | 3 | 1 | provider | AgentTimeoutError |
| `write-compressor` | 5 | 0 | 0 | 3 | 1 | 1 | provider | AgentTimeoutError |

Column totals are 239 `R1`, 151 `R0`, 55 `U`, 134 `A`, 245 `R`, and 66 `M`.

## Trial-state overlap

The trial dimensions overlap as follows. Blank reward in the source report means unscored.

| Official reward | Native state | Harbor state | Trials |
| --- | --- | --- | ---: |
| 1 | accepted | no exception | 98 |
| 1 | rejected | no exception | 117 |
| 1 | rejected | AgentTimeoutError | 1 |
| 1 | missing | AgentTimeoutError | 23 |
| 0 | accepted | no exception | 27 |
| 0 | rejected | no exception | 95 |
| 0 | rejected | AgentTimeoutError | 3 |
| 0 | missing | AgentTimeoutError | 26 |
| unscored | accepted | AgentTimeoutError | 1 |
| unscored | accepted | RuntimeError | 6 |
| unscored | accepted | VerifierTimeoutError | 2 |
| unscored | rejected | AgentTimeoutError | 1 |
| unscored | rejected | RuntimeError | 23 |
| unscored | rejected | VerifierTimeoutError | 5 |
| unscored | missing | AgentTimeoutError | 1 |
| unscored | missing | RuntimeError | 16 |

This is why neither “239 passes” nor “151 failures” fully describes the campaign. The most useful
release-quality numerator is the 98 reward-one, natively accepted, exception-free trials; the raw
official and native totals must still be shown beside it.

## Exact native rejection inventory

| Native terminal | Count | Current interpretation |
| --- | ---: | --- |
| `provider returned no tool calls or usable final response` | 175 | Frozen provider lifecycle defect; general recovery and run-scoped circuit changes exist but are not campaign-qualified |
| Two unchanged fixer cycles with checks/findings remaining | 18 | Legitimate fail-closed behavior mixed with gates that extracted impossible requirements |
| Selected provider cannot inspect image inputs | 14 | Image-capability routing/fallback defect in frozen composition |
| Context exceeds 200,000 tokens after compaction | 21 | Frozen context/media/review-packet defect; current corrections need rerun |
| Artifact workspace exceeds bounded design inventory | 6 | Frozen large-workspace inventory limitation |
| Anthropic authentication | 3 | Credential-lifecycle/provider boundary failure |
| Managed worktree HEAD changed | 2 | Requested Git mutation invalidated frozen candidate baseline |
| Missing compiler/toolchain user handoff | 2 | Needless prerequisite escalation in disposable environment |
| In-scope pre-existing test-failure user handoff | 1 | Overconservative scope policy |
| Phantom `alice@example.com` path user handoff | 1 | Deterministic path-extraction false positive |
| Anthropic ambiguous acceptance | 1 | Provider terminal classification |
| Provider subprocess timeout | 1 | Owned provider process exceeded wall-clock limit |

The two repeated context estimates account for four records: two at 919,141 and two at 1,343,398.
The other seventeen over-limit records each have their exact estimate preserved in the final JSON.

## Exact Harbor exception inventory

| Exception | Count | Subdivision |
| --- | ---: | --- |
| `AgentTimeoutError` | 56 | 36 × 900 s; 7 × 1,200 s; 8 × 1,800 s; 5 × 3,600 s |
| `VerifierTimeoutError` | 7 | 7 × 900 s |
| `RuntimeError` | 45 | 29 unsupported schema; 3 router qualification; 3 missing `/app`; 9 missing trace; 1 surviving process tree |

The unsupported-schema trials used a stale portable agent across an adapter protocol change. The
missing-trace failures affected `crack-7z-hash`, `fix-ocaml-gc`, and `reshard-c4-data`. The missing
`/app` failures affected `prove-plus-comm`. None is a valid reason to assign a candidate-quality
zero silently, and none is removed from the campaign.

## Known task-level candidate misses and unresolved capability gaps

- `kv-store-grpc__r9sYjKe` is a legitimate request-schema miss (`val` versus `value`), not a
  benchmark defect.
- The malformed-HTML sanitizer candidate is not browser-equivalent. This remains distinct from the
  evaluator contradictions recorded as `TBI-008`.
- `query-optimize__hbtMdm7` was correct but slower; Peritus accepted an unmeasured optimization.
- `cancel-async-tasks__5rUhw9b` passed an internal cancellation simulation but failed the named
  process-signal case.
- `qemu-alpine-ssh__tzUDHMQ` exposed that the frozen build lacked a persistent
  interactive-terminal surface. The active command lifecycle has since landed in `e9da73a0`, but
  it has not been qualified by a complete immutable-build campaign.
- Several timeout trials did productive work but still failed to deliver before the real deadline.
  Productive partial work is evidence, not a pass.

## Known benchmark and evaluator defects

- `install-windows-3-11`: unpublished QEMU monitor socket path.
- `pytorch-model-recovery`: unrecoverable hidden model call signature.
- Torch-based verifier trials: cold package downloads consumed verifier deadlines.
- HTML filtering: byte-preservation/browser-semantics conflict and leaked browser process behavior.
- `sam-cell-seg`: one output argument is specified as both directory and CSV.
- `mteb-retrieve`: hidden prompt convention changes embedding rank.
- `dna-insert`: equivalent circular junction rejected by one hidden decomposition.

Their official zeros and unscored trials remain in the table. The main report explains the
non-cooking rules governing any future response.
