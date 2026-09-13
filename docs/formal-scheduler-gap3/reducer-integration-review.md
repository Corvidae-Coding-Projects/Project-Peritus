# Reducer integration review

Date: 2026-09-13

Baseline commit: `c6db6c9e6e9a64fc86040a86821e8b554251bd95`

Reviewer: `/root/gap3_schema_registry`. This is a technical agent review, not human approval or
protected authorization.

This is a bounded, read-only review of the current working-tree integration around worker loss,
event replay reconstruction, cursor advancement, and state-digest installation. The reviewed files
had the following SHA-256 hashes:

| File | SHA-256 |
| --- | --- |
| `crates/orchestration/peritus-scheduler/src/reducer.rs` | `85d8fe4110c4f1f9af5fe771284e59215ba603b6c2db73777695db16128b9da6` |
| `crates/orchestration/peritus-scheduler/src/reducer/apply.rs` | `065aefa285a87b69ba9db077a38166d8f5e30ed0d417d294c2a8f00c63ea71cd` |
| `crates/orchestration/peritus-scheduler/src/state/mutation/cursor.rs` | `1077a5865307c6b678c8cecc86c426c598619a2fe39c508692550deba86a4b6f` |
| `crates/orchestration/peritus-scheduler/src/reducer/reconstruction.rs` | `103adb661e4e80410e298ae5d5230c5c8f9090c13d933488bed9471566379ff6` |

## Verdict

PASS for the reviewed integration boundary. I found no missed production call path, changed error
mapping, changed event payload order, or moved canonical digest boundary.

- `LoseWorker` still snapshots the target worker's dispatches in retained order, hashes each
  immutable dispatch ID with the same `sha256(dispatch_id.as_bytes())` input, releases those
  dispatches in that order, marks the worker `Lost` last, and emits outcomes in release order.
  Building the complete `(dispatch_id, digest)` plan before release is equivalent because every
  digest depends only on its immutable dispatch ID. The missing-worker, already-lost/removed,
  disappeared-reservation, disappeared-work, and disappeared-worker cases retain their previous
  `SchedulerError` kind and detail mapping.
- `reconstruction::command_from_event` is reached by both genesis and continuation replay. Its
  payload mapping and copied command fences match the implementation extracted from `reducer.rs`.
  Derived event fields remain non-authoritative: replay reruns `start` or `decide` and compares the
  complete produced event with the supplied event.
- `decide` retains the same order around canonical bytes: apply the command, refresh derived state,
  enforce the encoded-state limit, advance sequence/event/command identity, hash that complete
  post-cursor state, install the digest, and construct the event with the original prior digest and
  computed successor digest. `cursor.rs` performs the same assignments as the removed inline
  mutation functions.

## Scope and independence

I authored parts of the cancellation and scheduler-phase contract work in this increment. Those
contracts are excluded from this independent verdict. This review covers root-owned reducer wiring,
the worker-loss batch integration, replay reconstruction wiring, and the cursor/digest boundary.
Separate bounded reviews cover cancellation/reconstruction composition and worker-loss behavior.

No build, test, or verifier command was run for this review because the root agent owns the single
validation lane. I inspected the current source and its diff from the baseline and ran:

```text
sha256sum crates/orchestration/peritus-scheduler/src/reducer.rs \
  crates/orchestration/peritus-scheduler/src/reducer/apply.rs \
  crates/orchestration/peritus-scheduler/src/state/mutation/cursor.rs \
  crates/orchestration/peritus-scheduler/src/reducer/reconstruction.rs
git diff --check
```
