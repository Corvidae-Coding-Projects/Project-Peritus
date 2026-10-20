# Actual read reuse and recovery replay

The required behavior is fixed before execution:

- An unchanged range already in the current provider view returns a successful reuse receipt without another body.
- Exact/subset/adjacent page coverage survives unrelated edits and long sequences exceeding 16 observations.
- Partly covered requests deliver missing intervals with an accurate continuation, without claiming unseen bytes were delivered.
- Omitted evidence is restored from the actual archive; truncated and UTF-8 continuation pages never establish complete-line coverage.
- Changed content is refreshed, including same-length replacement and file replacement. Permissions, protected paths, symlinks and malformed arguments still reject normally.
- Fresh-process recovery preserves evidence, and an independent role obtains its own evidence.
- Plans remain available as explicitly historical/stale progress after edits, without granting source freshness or completion credit.

Build the production library and OpenAI adapter, then link the ordinary native API consumer:

```sh
cargo build --locked --jobs 1 -p peritus-product-runner -p peritus-provider-openai --message-format=json > /path/to/artifacts.jsonl
python3 docs/testing/reproducers/read-reuse/link.py --deps /path/to/target/debug/deps --artifacts /path/to/artifacts.jsonl --output /path/to/read-reuse-consumer
python3 docs/testing/reproducers/read-reuse/replay.py --consumer /path/to/read-reuse-consumer --output /path/to/new-disposable-results
```

The native consumer calls real workspace tools, the real memory decorator, context assembly and durable recovery. It discovers the installed official Codex executable for a provider profile, without requesting model inference for these deterministic host-boundary scenarios. This replay does not establish live model compliance or every provider's behavior. Fixtures and result artifacts belong outside the repository. No live user daemon is needed or modified.
