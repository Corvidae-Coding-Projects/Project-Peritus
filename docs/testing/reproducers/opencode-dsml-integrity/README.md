# OpenCode DSML argument-integrity replay

This is a minimal real public-library consumer of the production provider registry and adapter.
An external CLI wire fixture supplies exact malformed or valid model replies. It reads no real
credential, makes no network request, performs no inference, and executes no host tool.

The original failures were literal marker rewriting, acceptance of a competing JSON envelope plus
DSML block, and duplicate JSON key collapse. The replay requires exact accepted arguments, rejected
ambiguous proposals, correct retained diagnostic bytes, and safe correction or terminal refusal.
The 24 seeded generated cases vary literals and structural spellings independently of the decoder.
All observed raw inputs, outputs, and canonical provider traces go outside the repository.

After building the actual CLI, daemon and OpenCode provider with Cargo JSON output:

```sh
python3 docs/testing/reproducers/opencode-dsml-integrity/link.py \
  --artifacts /path/to/build.jsonl --output /tmp/dsml-consumer
python3 docs/testing/reproducers/opencode-dsml-integrity/replay.py \
  --consumer /tmp/dsml-consumer --output /tmp/dsml-replay
```

The output directory must not exist. The replay runs 108 cases sequentially; `results.json`
records the cases, seed, scope and result. A mismatch exits nonzero. The runtime uses placeholder
credentials through the actual provider credential interface and private temporary vendor setup.

The captured traces can drive the real CLI regression and memory replay:

```sh
python3 docs/testing/reproducers/trace-streaming.py --cli /path/to/fixed/peritus \
  --baseline /path/to/original/peritus --provider-corpus /tmp/dsml-replay \
  --output /tmp/trace-replay
```

This compares both human and JSON output, errors, truncation, early filtering and peak RSS. It
creates one sparse 10 GiB logical trace for a fixed-CLI filter check, then deletes it. Its measured
ordinary traces are roughly 4, 16 and 64 MiB, using fixed-size records. Use available resource
controls when replaying; no actual 10 GiB allocation is attempted.

For the reader's separate public snapshot/append contract:

```sh
python3 docs/testing/reproducers/opencode-dsml-integrity/link.py \
  --artifacts /path/to/build.jsonl --source docs/testing/reproducers/trace-reader.rs \
  --output /tmp/trace-reader
/tmp/trace-reader /tmp/disposable-reader.trace
```

See [the evidence report](../../opencode-parser-trace-real-2026-10-20.md) for baseline failures,
the actual build, measured results, driver corrections, and limits.
