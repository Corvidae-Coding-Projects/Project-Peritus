# Provider wait diagnosis and actual progress — 2026-10-19

The reported 220-second wait was a working provider request, with misleading feedback. Read-only
inspection of the builder run and the official client's local logs found:

- Runtime turn started at 19:15:28 UTC; its WebSocket connected at 19:15:29.
- Model sampling began at 19:15:31 with the user's selected `xhigh` effort.
- The first structured reply delta arrived at 19:19:09, after about 218 seconds of generation.
- Reply deltas continued through 19:19:33, when that runtime's logs end. There is no retained
  completed response for this interrupted request. This evidence does not establish completion.

This was separate from the earlier command/checkpoint bottleneck. Recent completed calls independently took
11–135 seconds, with longer calls returning thousands of output tokens. The specific slow request
was connected and producing output. The old adapter exposed only completed summaries and validated
final replies, so its terminal still said it was waiting while structured bytes were arriving.

Actual vendor cache usage remained low in this live sample. Four saved request manifests had
matching early messages and tool-definition hashes. This inspection does **not** establish prompt
churn as the cause of this particular delay or promise a new vendor cache-hit rate.

## Changed contract

The official Codex adapter now publishes content-free observations of runtime acknowledgement,
reasoning-item start and incoming structured-reply byte counts. Notices contain only a validated
phase and a byte count. They are throttled to one per second within a phase, enter the ordinary
durable provider-event trace and never enter the authoritative final-response decoder transcript.

The host carries the last observed phase through its existing 20-second heartbeat. Terminal and
browser clients receive the same public activities: runtime accepted, provider reasoning, or
structured response arriving and awaiting completion/validation. Long waits update one notice
instead of evicting useful conversation. No private reasoning text is exposed. Tool proposals
still pass through complete existing decoding and validation before becoming executable calls.

No automatic replay, new generation deadline, effort reduction, provider replacement or cache
algorithm change was added. Real model generation can still take 220 seconds. Other provider
adapters retain their existing output behavior; no new external-provider campaign is claimed.
The ordinary uncached `codex exec --json` route exposes acknowledgement but has no intermediate
generation-byte deltas to report.

## Real observations on the final artifact

Built with `--locked --jobs 2 -p peritus-daemon -p peritus-cli -p peritus-tui`, followed by the
corrected daemon build, using an external target and disk-backed compiler temporary files.
Transient scopes capped compilation at two CPU cores/3 GiB, and terminal fixtures at two CPU
cores/1.5 GiB. No unit tests, formal verification or repository verification suites ran.

Final daemon SHA-256: `e875b9b09dadfdd89f3441af4d019a0b6bef71b748937ff5c4a1163be76b80c6`.
Official Codex: 0.163.0. Inference below used low effort explicitly inside disposable fixtures;
the live user's setting was unchanged.

| Actual interface and independent expectation | Observed result |
| --- | --- |
| Original installed TUI, same discussion-only conversation | Reply completed; no accepted/reasoning/generating stages. Retained replay fails specifically for that missing feedback. |
| Corrected actual TUI → daemon → official provider | Accepted at 1.40 s, reasoning at 4.21 s, receiving bytes at 8.22 s; final reply at 12.63 s. All three stages independently read back from actual emitted ANSI bytes. Files unchanged. |
| Same real TUI with only its isolated native provider process group paused for 26 s after reasoning | Reasoning heartbeat still visible after 20 s; group resumed and exact final marker arrived at 30.52 s. This is controlled scheduling, not a natural provider latency measurement. |
| Independent cached ASCII provider consumer | Accepted at 1.496 s, generating at 4.041 s, exact final text at 4.321 s. |
| Independent ordinary uncached Unicode consumer | Exact 768-byte output at 12.353 s; runtime acknowledgement observed, no unsupported byte-progress claim. |
| Independent forced inert host-tool proposal | Accepted at 0.160 s, reasoning at 2.614 s, generating at 2.932 s; validated proposal at 3.346 s. Exact Unicode arguments, no executor/tool effects. |
| Independent cancellation after runtime acknowledgement | Acknowledged at 0.172 s, canceled at 0.178 s; zero delivered text or tools. |

The four deterministic API cases vary transport, text encoding, output length, tool choice and
cancellation. They check exact fixture-defined values, event/byte ordering and the absence of
private reasoning payloads. They are bounded interface coverage, not an all-provider or multi-hour
campaign. A buffered reply can complete immediately on resume, making its byte notice too brief
for a frame; the ordinary unpaused TUI replay separately proves all three rendered stages.

## Failures caught while making the change

The first implementation put progress records into the strict final decoder's JSONL input. The
actual TUI rejected three otherwise valid turns, and independent inspection confirmed the unknown
event type. The correction calls the progress observer directly, leaving decoder input unchanged.
The final successful terminal and all four independent provider cases exercise that corrected path.

An initial terminal scenario asked read-only chat to write a file while explicitly forbidding its
execution pipeline. The application correctly declined; this was a bad scenario, not a product
failure. The retained terminal scenario is a legal discussion-only request. The delayed fixture
also guards process-group ownership: pausing only an executable wrapper leaves its native child
running and cannot establish the intended schedule.

## Reproduction and cleanup

[Retained ordinary consumers and terminal drivers](reproducers/provider-progress/README.md)
include exact expectations, build/link/run commands, actual terminal readback and small results.
No fake provider or generated replacement UI was used. Raw data remains outside the repository:

- `/home/doll/.cache/peritus-provider-wait-real-20261019/`
- `/home/doll/.cache/peritus-provider-progress-independent-20261019/`

Only owned fixture clients, daemons and provider groups were stopped. The initial behavioral
campaign changed no live run, installed binary, provider selection or user workspace. This
feedback fix was developed after publication of v0.1.3; it is not part of that release tag.
