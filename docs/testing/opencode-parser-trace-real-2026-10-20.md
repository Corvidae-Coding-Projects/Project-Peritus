# OpenCode argument integrity and bounded trace memory

The four findings left in the v0.1.5 release notes were confirmed bugs, not technical blockers.
This follow-up repairs all four on `develop`. The already-published v0.1.5 binaries are unchanged.

## Cause and repair

- DSML normalization used global replacements over the complete reply, silently modifying literal
  string arguments. The parser now recognizes structural markers only at its cursor, leaving string
  bodies opaque until the matching parameter delimiter. Plain tags and both retained DSML marker
  spellings, with and without their optional space, remain supported.
- The DSML fallback treated a preceding JSON envelope as commentary. Structured delimiters or a
  fence in the prelude now refuse the candidate rather than select a competing tool block.
- JSON-valued parameters entered `serde_json::Value` before duplicate detection, losing the original
  ambiguity. They now pass the existing duplicate-rejecting canonical parser before conversion and
  legacy nested-argument flattening. Declared names, selection, call count and host authority remain
  downstream checks; the normalizer never executes a tool.
- Trace inspection loaded the whole file, copied its payloads, then retained both output formats.
  `TraceReader` captures one file-length snapshot, traverses framed records, and seeks past excluded
  payloads before allocation. The CLI constructs only the requested representation and spools it to
  an automatically removed temporary file. Success is copied to stdout only after rendering finishes;
  a framing, decoding or storage failure retains the ordinary complete error response.

The existing collecting `read_frames` API remains compatible and is explicitly documented as a
collection API. Streaming callers use `TraceReader`. Trace tags, payload encoding and the separate
local-memory recovery reader are unchanged. Memory scales with one selected record, not total run
length. Selected output needs temporary disk space; no trace-size or run-size cap was introduced.

## Actual program exercised

Source baseline: release merge `724dc23a909419195eb04c898aa085d496296c2f`.
Artifacts and raw results: `/home/doll/.cache/peritus-parser-trace-20261020/`.
The baseline CLI was copied before rebuilding the same debug profile. The baseline parser findings
are retained in `baseline-parser.json`, from the unchanged production parser at pre-release review.

Build, with one job and a 2 GiB memory ceiling:

```sh
systemd-run --user --scope --quiet -p MemoryHigh=1024M -p MemoryMax=2048M \
  nice -n 10 env CARGO_TARGET_DIR=/home/doll/.cache/peritus-tui-feedback-target \
  CARGO_PROFILE_DEV_DEBUG=0 cargo build --locked --offline --jobs 1 \
  -p peritus-cli -p peritus-daemon -p peritus-provider-opencode --message-format=json
```

The build passed. Consumer links select the exact Cargo artifact cohort from `build.jsonl`.
No unit, formal-verification, repository-verification or qualification suite was run.

## Provider input and recovery results

[Public provider consumer and replay](reproducers/opencode-dsml-integrity/README.md): **108 PASS**,
including 24 generated cases with seed `20261020`. The ordinary provider registry, OpenCode adapter,
request encoding, process isolation, correction path, event stream and response reducer were used.
The external vendor process supplies exact fixture replies; it performs no inference or tool work.

The independently specified expectations were:

- preserve literal marker bytes, closing invoke/calls text, Unicode, quotes, backslashes, whitespace,
  empty strings and embedded JSON across two root names and five structural marker spellings;
- reject the original competing-envelope example, nested and escaped duplicate keys, repeated
  parameters, truncated/multiple blocks, trailing content and mismatched structural closers;
- publish the exact valid inert calls, including typed nested arguments and multiple calls, and
  preserve ordinary JSON and quoted-prelude JSON recovery;
- refuse undeclared, disabled, wrong-specific-tool and excessive call proposals; retain the exact
  refused reply, complete a valid format correction, or terminate a persistently invalid turn.

Zen and Go route configurations both passed the fixture journey. No real account, live Zen/Go
inference, native OpenCode executable, or host tool execution is claimed. Accepted/rejected
production provider envelopes were captured outside the checkout and read by the actual CLI below.

## Actual CLI and public-reader results

[Trace CLI replay](reproducers/trace-streaming.py): **9 scenario groups PASS**, comprising output
compatibility, ten torn-tail variants, four corruption/filter combinations, unavailable temporary
storage, 24 before/after RSS measurements, and a filtered sparse 10 GiB tool record.

Human output matched the original CLI byte-for-byte and parsed JSON matched semantically in all
four output/filter combinations. Extension indices retained their positions in the full trace.
Unknown tags and malformed selected provider envelopes returned protocol errors, with empty stdout
and one valid JSON error on stderr. Temporary files were cleaned after success and failure.

Fixed-size roughly 64 KiB records produced traces of 4.0, 16.0 and 64.1 MiB. The expected maximum
RSS variation across these sizes was at most 8 MiB for each fixed-mode series; every series passed.
The largest trace measured:

| Mode | Original peak RSS | Fixed peak RSS |
| --- | ---: | ---: |
| Human, all records | 211.7 MiB | 17.6 MiB |
| JSON, all records | 341.8 MiB | 18.0 MiB |
| Human, extensions only | 143.7 MiB | 16.7 MiB |
| JSON, extensions only | 143.9 MiB | 16.7 MiB |

The sparse 10 GiB record was tested only with filtering on the fixed CLI; no giant baseline
allocation was attempted. This demonstrates early exclusion without introducing a size cap.
It does not establish absence of all memory leaks or a bound independent of selected record size.

[Public reader consumer](reproducers/trace-reader.rs): **PASS** for append-after-open snapshot
isolation, rewind, compatibility with collecting reads, fresh reopen, and completion of an initially
torn tail by an actual append-only writer. The consumer operated a disposable file, then removed it.

The first trace driver invocation placed `--json` after the subcommand, which both binaries rejected
as a usage error. The corrected invocation uses the existing global-option position and preserves
the existing stderr error contract. This was a driver correction, not a product defect.

All activity used disposable files and placeholder credentials. No live daemon or installed binary
was changed, and no user's retained run was mutated. Native macOS/Windows execution was not run.
