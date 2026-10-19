# Visible tool work and resumed-task latency

The first TUI change failed the user's actual requirement: routine results hid matches and
file bodies, edits were shortened, and the account runtime buffered all provider progress.
The user's stopped builder session remains untouched during this campaign.

## Required observations, before replay

- A real workspace search displays matching paths, line numbers, and source text, rather
  than only a count. A directory listing displays the names it returned.
- File and skill reads display the actual returned body with normal terminal foreground.
  More than seven lines, including middle lines, are scrollable without `/details`.
- A successful long patch displays removed and added text, including the middle of the
  replacement. A rejected patch remains failed and changes no file.
- Returned output larger than one public activity is partitioned into readable activities,
  rather than cutting off the body. The protocol's per-activity size bound stays intact.
- Published Codex reasoning summaries appear while the real provider process is still
  running. Raw/private reasoning and unvalidated host tool proposals are never exposed.
- Continuing a disposable task reuses its host-published plan without a new designer
  inference; current steering still reaches the writer. Changed task/workspace/design
  identity invalidates a v2 receipt. Current user requirements retain authority.
- Run with the user's selected effort; do not disguise latency by lowering it. Measure
  provider calls, output and input tokens, repeated reads, and time to the first edit.
- Reconnect, resize, and draft retention still work after restoring transcript space.

Build actual CLI/TUI/daemon binaries with two jobs, launch disposable state, use the real
signed-in provider and actual files/processes, and observe terminal cells and saved state.
No unit tests, fake providers, or formal/repository verification suites are used.

## Read-only cause evidence

Builder's latest stopped attempt made a 128.48-second designer call producing 5,529 output
tokens. Writer decisions then took 17–21 seconds with 84,445–94,301 input tokens and no
reported cached input. Host preparation gaps were generally below three seconds. Across
measured attempts, 73,852 of 411,538 read bytes were identical rereads. A prior attempt
first mutated the workspace 440.5 seconds after starting.

The old design receipt included the entire growing conversation and rejected any pending
input revision. `continue` therefore forced design regeneration. The account adapter also
waited for process completion before returning its stream and discarded published summaries.
Fresh ephemeral Codex sessions have separate cache affinity; the inspected upstream source
offers no public CLI cache-key override. This campaign does not claim to have added stable
session caching or removed provider-side inference latency.

## Final results

All directly changed journeys passed through real interfaces:

| Journey | Observed result |
| --- | --- |
| Read/search/list | Full 100-line source body, directory names, and matching `observations.txt:48:` source text were readable through the terminal. |
| Long edit | Removed and added line 23 in a 42-line replacement were both reachable, along with the final line 41. No middle-of-edit display cut remained. |
| Large skill read | The 14,403-byte guide was split into readable activities; Markdown content appeared as source text rather than a JSON-escaped string. |
| Public provider progress | The corrected second campaign matched a summary after the current model-start activity with a still-running owned Codex process. Actual terminal frames displayed Thinking summaries. |
| Plan reuse | Both continuation campaigns emitted `design-reused`; no subsequent designer model request occurred. |
| Final optimized continuation | First tool was `run_pipeline`. No workspace inspection or designer request preceded the known edit. First edit took 22.991 seconds at unchanged xhigh effort; an actual `python3 patch.py` printed `FINAL-CLI`. |
| Presentation/reconnect | Source-body cells used the normal foreground. Thirteen generated/named sizes (seed 20261019), reconnect, and an unsent Unicode draft passed. |
| Shared process transport | An independent public API consumer observed stdout at 7 ms while its actual Python child remained running until 1,209 ms. Exact final Unicode stdout/stderr, default observer-free success, exit 7, and cancellation/reaping passed. |

The first campaign's initial summary oracle could pair an old summary with a later process.
The saved driver was corrected to require a summary after the latest model-start activity;
the second fresh-provider campaign passed that stricter condition. Screen publication can lag
durable activity publication by a UI tick; the actual Thinking frames are retained separately.

Two additional corrections followed observations during the first campaign: recovered
`context_read` sources now decode their returned text bodies for display, and ordinary Chat
handoff does not inherit the writer's startup-discovery prompt. Read-only questions still stay
conversational; the pipeline retains permissions, current-user authority, and independent review.

## Replays and evidence

Build commands used `cargo build --locked --jobs 2` and `cargo build --locked --release
--jobs 2`, selecting `peritus-cli`, `peritus-daemon`, and `peritus-tui`, with
`CARGO_TARGET_DIR=/home/doll/.cache/peritus-tui-feedback-target`. Development builds disabled
debug information. No unit tests, mocks, formal verification, or repository qualification ran.

Live driver: [tui-visible-work.py](reproducers/tui-visible-work.py). Provide the actual binary,
an existing provider-selection state file, a new disposable output directory, and optionally
`--model gpt-6.1-sol`. Credentials remain in the existing account runtime/key store.
Terminal drivers use `uv run --offline --with pyte python3`.

The final optimized replay was:

```sh
uv run --offline --with pyte python3 \
  docs/testing/reproducers/tui-visible-work-view.py \
  --binary /home/doll/.cache/peritus-tui-feedback-target/release/peritus \
  --instance /tmp/p-visible-work-2 \
  --output /tmp/p-visible-work-final-release
```

This continuation is intentionally stateful: its retained fixture starts with
`CONTINUED-CLI`; the replay makes one authorized change to `FINAL-CLI`. Start with a fresh
live-driver instance for another complete campaign.

Evidence outside the repository:

- `/tmp/p-visible-work-1/`: initial long-body/edit journey and oracle correction evidence.
- `/tmp/p-visible-work-2/`: stricter live-provider campaign, durable run and efficiency records,
  actual Thinking frame, and scrollable read/hunk/skill screens.
- `/tmp/p-visible-work-final-release/`: final optimized-binary PASS, exact foreground cells,
  thirteen sizes, reconnect draft, known edit and observed executable output.
- `/tmp/peritus-provider-observer-real-20261019/`: independent actual-subprocess consumer,
  build/run logs and exact timing/results.
- `/tmp/peritus-visible-work-installation.json`: matching optimized binaries installed for
  doll and builder, owners, hashes, and backup locations.

Final optimized launcher SHA-256:
`73caac6501bd3162ba68ec46a3d651861fce1d2140b731b823214177d00674f8`.
The final binaries were installed for both accounts. Neither account had a running personal
Peritus process at replacement; existing run records were preserved. Builder's job remains
cancelled. These results cover Linux TUI/account transport behavior, not all platforms/providers
or completion of the disposable candidate's independent review.
