# Actual provider progress replay

The original terminal completed a real conversation without showing runtime acceptance,
reasoning, or incoming structured reply bytes. These replays require that actual observations
arrive before final validated output. They do not impose a generation deadline or lower a user's
configured effort. All account inference below is real, through the official signed-in Codex.

Build the program in a bounded external target, then link the ordinary public-API consumer:

```sh
CARGO_TARGET_DIR=/path/to/target CARGO_PROFILE_DEV_DEBUG=0 cargo build --locked --jobs 2 -p peritus-daemon -p peritus-cli -p peritus-tui
python3 docs/testing/reproducers/provider-progress/link.py --deps /path/to/target/debug/deps --output /path/to/progress-consumer
/path/to/progress-consumer /path/to/new-provider-output
```

The four deterministic cases vary cached/ordinary transport, ASCII/Unicode output, no tools/a
forced inert tool proposal, and normal completion/cancellation after runtime acknowledgement.
Expected strings and arguments are fixed by the consumer. No tool executor is installed in it.
It checks exact output, monotonic events/byte counts, phase notices before validated output,
payload privacy and cancellation. Ordinary `codex exec --json` provides acknowledgement but no
intermediate generation-byte deltas; the cached app-server path provides those deltas.

Operate the actual human terminal with Python `pyte` and a previously configured product-state
file. Credentials remain in the existing official client/OS store; do not print or commit the
selection file. Choose a new output directory and use only disposable state:

```sh
uv run --offline --with pyte python3 docs/testing/reproducers/provider-progress/live-progress.py --binary /path/to/target/debug/peritus --provider-state /path/to/state-NNN.json --output /path/to/new-tui-output
uv run --offline --with pyte python3 docs/testing/reproducers/provider-progress/render.py /path/to/new-tui-output
```

The terminal fixture asks a discussion-only question, selects low effort **inside the fixture**,
checks the final marker and unchanged files, and closes its client/daemon. `render.py` independently
reads the emitted ANSI bytes through a terminal emulator and saves the three actually rendered
stages. It creates no substitute application output.

`--runtime-data /path/to/owned-fixture/data` can reuse a prior fixture's automatically provisioned
browser/Python runtime. `--hold-provider` additionally pauses only the fixture daemon's isolated
native Codex process group for 26 seconds after an observed reasoning event, then resumes it.
The driver requires that group's leader to be the fixture daemon's direct child. This
controlled schedule checks that the 20-second heartbeat preserves the observed phase. Cleanup
resumes that group even on an exception. It does not simulate an AI response or pause a user run.
For this schedule, inspect the visible delayed phase with `render.py FIXTURE --required-stages
accepted reasoning --require-heartbeat`. On resume, an already buffered reply may complete too
quickly to render its transient byte-progress notice; the ordinary unpaused replay checks all
three visible stages.

Run the terminal fixture against the original installed binaries in a separate output directory
to reproduce the missing-progress failure: the conversation completes, but the stage assertion
fails. See [campaign evidence](../../provider-progress-real-2026-10-19.md) and
[small observed results](observed-results.json). Raw prompts/large traces remain outside Git.
