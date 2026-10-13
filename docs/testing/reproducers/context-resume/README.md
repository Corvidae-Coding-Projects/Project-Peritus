# Fixer context resume replay

This standalone public library consumer replays the actual context failure from run
`81f9302c94ce667a2fff398d434f6192`. It requires an operator-created, consistent copy of that
run's writer context. No user context, workspace contents, credentials, provider substitutions,
or live-session mutations are included in the repository.

Create the copy outside the editable workspace. Use Python `sqlite3.Connection.backup` from a
read-only connection for `journal.sqlite3`; copy the immutable `artifacts` directory. Place them
at `CASE_DIR/context/writer/{journal.sqlite3,artifacts}`. The source daemon may remain running.
Do not copy its owner lock into a live process or use this replay to resume effects.

```sh
python3 docs/testing/reproducers/context-resume/prepare_snapshot.py "$CASE_DIR"
python3 docs/testing/reproducers/context-resume/build_consumer.py "$BUILD_DIR"
target/debug/peritus-context-resume-consumer "$CASE_DIR" > "$CASE_DIR/final-render.json"
```

The consumer validates artifact lengths and digests, decodes the exact published C6 state,
replays its uncovered journal reducers, and operates the production public evidence projection
and retrieval APIs. It checks the literal conversation and approved design, the complete
conserved ledger, all 122 unresolved failed approaches, and the unknown operation at source
`obs:005927`. It saves/reopens canonical working state without changing entries or the unknown
operation. Its host instruction replacement changes only the old generated input's instruction
reference; the original archive remains intact.

The original input contains 461,721 bytes of diff and 168,105 bytes of check history. It fails
the working projection at 4,096 and 16,384 available tokens. A successful projection requires
71,882 rendered bytes for the 122 required entries. The repaired prompt retains all 40,983 bytes
of literal conversation/design framing and all 4,199 ledger bytes, while generated evidence is
retrievable. The default copied-case replay produces `projection-report.json`, `projected-input.txt`,
and `reopened-state.bin` under `CASE_DIR`.

The consumer reconstructs the exact diff, check history, and ledger from bounded source pages
and verifies their independently computed SHA-256 values. It also exercises six context targets
(1, 4,096, 32,768, 65,536, 100,000, and 200,000) and a rejected non-progressing tiny read. The
ordinary tool catalog is empty in this context-specific consumer; production request wiring is
compiled separately. No provider inference is performed, and this result does not prove HTTPS
delivery or mutate the original stopped run.

## Actual host invocation and adapter replay

The second consumer opens the copied journal through `LocalContextHandle::for_run`, which
delegates the same delivery-specific lineage selection used by ordinary product invocations.
It operates `DeveloperContextPort::open`, `assemble`, and `checkpoint`, then releases the owner,
reopens the store, and checks the prepared view is exactly equal. It uses the full production
in-place catalog and the real configured Codex adapter's local serialized-context fit method.
It never calls provider `start`, credentials, or any command tool.

Use a fresh `HOST_DIR` outside every live workspace or service store. `--config` reads the actual
daemon configuration but retains only non-secret provider profile and executable metadata.
Do not point `HOST_DIR` at the original run. This replay advances only the isolated copied
journal and writes a checkpoint/trace there.

```sh
python3 docs/testing/reproducers/context-resume/prepare_host.py \
  "$CASE_DIR" "$HOST_DIR" --config "$DAEMON_CONFIG"
python3 docs/testing/reproducers/context-resume/build_consumer.py "$HOST_BUILD_DIR" --host
target/debug/peritus-context-host-consumer "$CASE_DIR" "$HOST_DIR"
```

The observed 25-tool request has 74,426 estimated tokens. The real adapter's local
`context_input_fits` encoder reports that its serialized projection fits the
1,048,576-character envelope (including the adapter's prefix and framing reserve). The exact
serialized byte/character count was not measured; this is no claim that inference accepted a
request. The new published instruction pins are `9770`, `9771`, and `9772`;
the old 675,149-byte observation `9768` remains exact in the source archive and is no longer
pinned. All 126 entries and the unknown operation at `5927` remain unchanged. The replay emits
`host-report.json`. Actual HTTPS delivery is still outside this context-recovery replay.
