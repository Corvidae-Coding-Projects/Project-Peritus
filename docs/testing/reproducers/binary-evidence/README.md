# Binary evidence through real model-input boundaries

A zero-filled database page is valid UTF-8 and contains literal NULs. It must remain binary evidence, with its content tracked for freshness, while its presentation stays admissible as model text. The original failure occurred after successful source edits and real PostgreSQL operations: a retained 524,307-byte scoped diff contained 208,306 NULs, and the next role input failed with `text: model text is empty, contains NUL, or exceeds its byte bound`. This was NUL contamination in host-created evidence, below the 16 MiB text bound.

Expected behavior is established independently of production output:

- Binary file changes remain visible, including modifications beyond the preview prefix; restoring original bytes restores the earlier baseline.
- Scoped human/model diff previews contain no literal NUL, while ordinary Unicode text is preserved.
- Historical NUL-bearing diff and command-output evidence retain their original byte count, SHA-256 and exact paged retrieval. Model-facing projection omits unsafe inline bytes with an identity-bearing marker.
- The raw text protocol continues rejecting literal NULs. Its invariant is not weakened to accept binary data.
- The full product performs the requested source/binary edits and actual command readbacks, then can make a real provider request after writer completion. A clean operation result before that boundary is insufficient evidence for this regression.

## Public library consumer

Build and run a real caller of `ScopedBaseline`, `context_projection`, `DeveloperEvidenceSources`, and `BoundedText`. The helper writes only a new external build directory and uses two Cargo workers. Cargo dependencies must already be available for offline resolution. It does not run unit, mock, formal, or qualification suites.

```sh
python3 docs/testing/reproducers/binary-evidence/build_consumer.py --output "$CONSUMER_BUILD"
"$CONSUMER_BUILD/target/debug/peritus-binary-evidence-consumer" "$NEW_REPLAY_ROOT"
```

Use a fresh output path. The consumer exercises 21 cases by default: the actual scoped binary diff, public role projection, exact source paging, Unicode, multiple NUL positions in 1/65/8192-byte values, 8192/200000-token targets, a byte change beyond the 16 KiB preview, original-byte restoration, and continued protocol rejection of raw NUL. An optional second argument is a private saved historical diff; it adds a 22nd case without embedding that data in the replay source. Do not commit private user evidence.

The executable exits nonzero if any independently defined case fails and saves actual outputs plus `result.json`. Small-message Unicode behavior should remain unchanged. Exact retrieval pages are JSON-encoded before model-text validation, so NUL source bytes are represented safely and reconstruct unchanged through the normal consumer.

## Actual TUI and independent review

Run as the account with real Codex authentication. Python requires `pyte`; normal provider credentials stay in that account's home. `BINARY_DIR` must hold actual sibling `peritus`, `peritusd`, and `peritus-tui` executables. Choose a new disposable `FIXTURE` path.

```sh
python3 docs/testing/reproducers/binary-evidence/setup.py --root "$FIXTURE" --binary-dir "$BINARY_DIR"
python3 docs/testing/reproducers/binary-evidence/tui_replay.py --root "$FIXTURE" \
  --binary "$FIXTURE/bin/peritus" --seconds 360 --model gpt-6.1-sol
```

The setup includes a dependency-free `pyproject.toml`: it gives the nested binary data a real owning project so the normal product workflow can reach independent review. The first manifestless exploration stopped at a missing check contract for the nested data file; that was a blocked fixture precondition, not the NUL defect or a passing replay.

The task changes `main.py` to emit exactly `SOURCE-REPAIRED` plus one newline and changes the owned 8192-byte page to exactly `bytes([0,65,0,66])*2048`. The independent driver checks exact source behavior, exact binary bytes, enrolled data, preserved fixture files, a real host follow-up command, and a real Reviewing provider request whose ordinal advances beyond the preceding Writing request. A fast Checking phase may occur between polling observations. It continues to the product's terminal outcome within the budget. It never stops merely because the first command succeeded. `result.json` distinguishes the post-writer boundary from whole-pipeline completion; a passing exit additionally requires Completed and a Qualified candidate. Timeout or a separate gate/provider failure remains incomplete and exits nonzero.

Raw terminal observations, durable run state, trace frames, inputs, binary manifests and results remain under the private fixture. Cleanup closes only that fixture's client and daemon. Only the direct-folder product path is exercised by this driver; managed Git untracked-file assembly requires its own real workflow evidence before claiming coverage.

The PTY driver uses Unix terminal APIs and was exercised on Linux. It does not establish Windows/macOS UI behavior.
