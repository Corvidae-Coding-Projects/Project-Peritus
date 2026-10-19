# Incremental context and document pages

These are ordinary consumers of the production public product-tool/context APIs, not Cargo test
targets. The fixture owner independently creates text, mutations and expected outputs. No provider
inference is submitted by these consumers; the real configured provider supplies the input envelope.
The separate retained `checkpoint-cache/live-memory.py` journey operates the actual TUI and daemon.

Build the current application using the repository toolchain and a resource allowance appropriate
to the host. The recorded Linux build used `CARGO_PROFILE_DEV_DEBUG=0`, an external
`CARGO_TARGET_DIR`, `cargo build --locked --jobs 2 -p peritus-daemon`, CPU quota 200%, memory high
2500 MiB, hard memory maximum 3 GiB and no additional swap. No unit/formal/repository suites.

Link and operate the freshly built cohort (use an external target directory and fresh output paths):

```bash
python3 docs/testing/reproducers/incremental-context/link.py \
  --deps /path/to/target/debug/deps \
  --output /path/to/reuse-consumer \
  --ordinary-output /path/to/context-consumer
python3 docs/testing/reproducers/incremental-context/replay.py \
  --consumer /path/to/reuse-consumer --output /path/to/fresh-behavior
python3 docs/testing/reproducers/incremental-context/pressure.py \
  --consumer /path/to/reuse-consumer --output /path/to/fresh-pressure
```

`replay.py` uses seed 731944. It drives 96 varied document pages plus line/byte continuation,
UTF-8 errors, mutation and access changes, retention eviction, large-document streaming,
unchanged/extended context, pending tools, current policy and new invocation boundaries.
`pressure.py` observes 48 large sources (enough to exceed decoded retention), revisits an old
source, then changes every source while preserving mtimes and assembles repeatedly. It rejects
loading omitted old bodies on every request or loading obsolete bodies just to make small receipts.

For before/after IO and exact public-output comparison, build the same ordinary context consumer
against the parent revision's production libraries and retain its identity separately:

```bash
python3 docs/testing/reproducers/incremental-context/compare.py \
  --before /path/to/parent-context-consumer \
  --after /path/to/current-context-consumer --output /path/to/fresh-comparison
```

The comparison samples Linux logical read bytes after all API operations, while the child is
blocked writing its large single JSON result. It compares exact page outputs and exact assembled
model messages. Counts include process setup and cold loads, not just warm operations. Wall time
is supporting local evidence, not a portable speed guarantee or vendor cache measurement.

The immediately affected saved replays are `../checkpoint-cache/context-replay.py` and
`../checkpoint-cache/mixed-batch-replay.py`; use the linked ordinary consumer. They cover fresh
process recovery, current permission/source validity, distinct scopes and mixed-batch ordering.

Use disk-backed storage for larger fixture campaigns. The first campaign encountered the host's
per-user `/tmp` quota despite filesystem free space. Preserve an actual storage failure separately
from behavior/latency failures and rerun the affected operation with writable disposable storage.
