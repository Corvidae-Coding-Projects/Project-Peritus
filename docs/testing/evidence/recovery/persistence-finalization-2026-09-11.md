# Product-run persistence and finalization recovery evidence

This campaign was authorized defensive reliability testing of the repository owner's local Project-Peritus checkout. It used synthetic records, fake providers, temporary directories, and process-owned fixtures only. It did not contact live providers, use credentials or third-party systems, or alter host-wide state.

## Environment

- Source before this slice: `0f753f1105b25ecb3c14c209c7b04239f6f7236d`
- Platform: Fedora Linux, kernel `7.2.4-200.fc44.x86_64`, `x86_64`
- Rust: `rustc 1.97.1 (8bab26f4f 2026-07-14)`
- Compilation concurrency: `CARGO_BUILD_JOBS=2`
- Fault schedule: deterministic named one-shot barriers; no random seed

## Executed probes and independent oracles

`product_record_fault_boundaries_preserve_an_old_or_complete_new_record` injects one failure before write, before file sync, before rename, after rename, and (on Unix) before directory sync. A fresh temporary service and run ID is used for each hit. The canonical JSON file is independently read and decoded after the injected error. Pre-rename failures preserve the complete old record; post-rename failures expose the complete new record. The product record includes the durable runner resume field, although this probe changes only snapshot status; existing resume restoration tests exercise decoding and use of that field.

`follow_up_admitted_at_finalization_is_processed_once` blocks the owned runner after its result exists and before daemon finalization. It admits a public continuation, releases the barrier, and asserts two received/two incorporated conversation revisions, exactly two fake-provider requests, and exactly one public user activity for the follow-up. This covers follow-up versus finalization without timing sleeps.

Receipt owner-local probes assert:

- a non-`NotFound` ledger read error fails closed;
- provider call-ID reuse is refused when only the tool identity changes and when only the canonical request digest changes;
- interrupted non-command `Started` remains executable, while the existing command case remains ambiguous;
- command ambiguity is persisted once and a second restart does not grow the ledger;
- a sparse synthetic ledger exactly at 128 MiB reaches decoding, while one byte over is rejected at the byte bound;
- a sparse synthetic record exactly at 2 MiB reaches decoding, while one byte over is rejected at the record byte bound;
- existing real-child `Started` and `Completed` crash schedules retain a one-effect counter.

No new production defect was confirmed by these cases, so no three-run defect reproduction or production root-cause patch was warranted. The persistence and finalization additions are deterministic test hooks compiled only under `cfg(test)`.

Two receipt mutants were classified as semantic equivalents. Removing `output: None` or `is_error: None` from the `Started` to `Ambiguous` struct update inherits values that are already `None`. Flipping the direct tool-name comparison in the same-ordinal conflict predicate is redundant with the request digest, whose input includes the exact tool name followed by a separator and canonical arguments. Separate tests retain both the public semantic contract and each independently meaningful identity dimension.

## Commands

Focused tests passed:

```text
CARGO_BUILD_JOBS=2 cargo test -p peritus-product-runner developer_tools::receipt::tests -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p peritus-daemon product_run::tests::interaction::follow_up_admitted_at_finalization_is_processed_once -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p peritus-daemon product_run::tests::recovery::product_record_fault_boundaries_preserve_an_old_or_complete_new_record -- --nocapture
```

Final verification passed:

```text
timeout 180s env CARGO_BUILD_JOBS=2 cargo test -p peritus-product-runner
  271 unit passed, 2 ignored; 20 integration passed; 0 failed
timeout 180s env CARGO_BUILD_JOBS=2 cargo test -p peritus-daemon
  171 unit passed, 3 ignored; 44 integration passed; 0 failed
cargo fmt --all -- --check
git diff --check
timeout 180s env CARGO_BUILD_JOBS=2 cargo clippy -q -p peritus-product-runner -p peritus-daemon --all-targets --all-features -- -D warnings
```

After the final ambiguity-stability assertion, its focused owner test passed again. A process census found no surviving fixture or daemon child.

## Limits

The directory-sync failpoint is Unix-only. The tests inject errors immediately around real filesystem operations; they do not emulate storage-controller lies, power-loss cache behavior, or filesystem-specific atomic-rename violations. Command-control runtime semantics are covered by the existing executor control and receipt child-process suites; this slice adds no portable PTY crash harness.
