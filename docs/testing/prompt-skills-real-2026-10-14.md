# Prompt cues and Rust reliability real testing

The user supplied [a prompt catalogue conversation](https://chatgpt.com/share/6acfc254-4d3c-83ea-bf9e-d22dd2f6ea1b) and `rust-reliability.zip`. The selected cues cover deliberate planning, careful execution, evidence-based independent review, and complete root-cause repair. The conversation calls the exact phrases experimental. This integration makes no claim of a measured performance improvement.

The Rust package is retained in `skills/rust-reliability/` with its original references, agent metadata and optional helper. Source ZIP SHA-256: `a94699f4a07d6ff59a65cbfccaefa59a461fb5f7aa00fddadf4590585ed735ff`. Runtime embedding reads those canonical files at compilation. Existing installed binaries need rebuilding before they receive changes.

Expected observations established before replay:

| Scenario | Expected observable result |
| --- | --- |
| Real production role requests | Designer plans concrete dependencies; writer executes carefully; reviewer derives questions from the contract and observed evidence; fixer traces and replays the underlying failure. Existing public response formats remain intact. |
| Rust task/file/command activity | The next model request includes the complete Rust skill in current System policy. Pure Python work and a mixed repository listing do not activate it by themselves. |
| Read-only planning/review | Guidance can inform analysis; mutation/process tools remain unavailable. |
| User excludes unit tests and verification/lint gates | Neither the guidance nor its optional helper grants permission to run excluded commands. |
| Context reconstruction and retry | Current System guidance is replaced, budgeted and retained; reconstruction does not reset grounding or replay settled effects. |
| Repeated identical inspection with still-visible evidence | Existing advisory feedback reinforces Rust contracts and forward progress; it introduces no new blocker or check. |
| Bundled resources | Exact supplied bytes can be paged with a stable digest and continuation; unknown/traversal resources are rejected and reading grants no workspace grounding. |

Implementation uses a default-no-op executor guidance hook, preserving other executor callers. Rust task hints and successful targeted reads/writes or compiler commands activate the skill for the invocation. A listing in a mixed repository is insufficient. The full skill remains in the current System policy on every request, including required-tool transport retries. The existing identical-inspection detector supplies a specific drift signal; it does not diagnose every possible reasoning drift. Skill resources are read-only, and their receipts remain audit sources rather than recursively retrieved task evidence.

The imported helper recommends Cargo build and Clippy as well as avoiding unit tests. It is optional and subject to the user's command exclusions. No helper, unit tests, Cargo test, Clippy, formatting check, qualification, or formal suite is run by this integration campaign.

Replay results and limits will be recorded below after operating the final program.

## Observed results

Current source built successfully with bounded Cargo jobs and memory scopes. The final debug CLI SHA-256 is `47340c4dabfe2a686b41233fe7e2c0df51256e2fae7c992fd53bef1be49d40d2`; CLI and daemon were copied into an owned `candidate-bin/` and exercised with disposable XDG roots and the real Codex account adapter (`gpt-6-luna`). The optional request proxy was not used by the launcher; evidence comes from actual canonical C5 archives and completed tool trace frames, decoded through the public message API. No provider responses were substituted.

| Actual scenario | Result |
| --- | --- |
| Designer and writer role cues | Present in real role policies; strict design/complete formats retained. |
| Rust writer and repeated reads | Full skill present in all 7 sampled current invocation policies, one copy each. `refresh_count` advanced from 0 to 1; the writer made the change, compiled it and ran the real CLI. |
| Independent generated-CLI readback | `6` returned `double=12\n`; `-7` returned `double=-14\n`; `abc` returned usage on stderr and exit 2. |
| Python work with a protected Rust path | Two current policies had no Rust marker; only `app.py` was read, no command or mutation was dispatched. |
| “Write a Python plan without Rust code” | Two current policies had no Rust marker; only `app.py` was read. |
| “Without unit tests, implement a Rust CLI” in read-only Plan | Rust present at provider step 1; only a folder listing occurred. No implementation/command authority was added. |
| “Review a Rust CLI, but do not run unit tests or verification” | Rust and reviewer cues present at step 1; read-only Review completed without a process, workspace mutation or pipeline handoff. |
| No initial language hint, actual Rust source read | Steps 1 and 2 had no Rust marker; step 3 included the skill after the observed `main.rs` read. |
| All six bundled resources | Exact bytes, complete lengths and SHA-256 digests matched canonical files, including the helper source. No helper execution occurred. |
| Invalid byte offset and pagination recovery | Offset 999999 returned a read error. A valid 4096-byte reference read continued at offsets 3897 and 7780, reconstructing all 8203 original bytes; default helper reading reached EOF. No effects occurred. |

The six bounded read-only sessions completed through the actual TUI in approximately 12–44 seconds each, with real provider requests and no writable/process dispatch. Successful canonical skill receipt lengths were 10448, 5457, 6042, 8203, 13128 and 261 bytes. Raw records, screens and decoded views are outside Git under `/home/doll/Peritus-Prompt-Skills-2026-10-14/prompt-fixture/`. Source ZIP import and byte reconstruction were independently compared with the retained canonical package, not with model claims.

## Defects repaired during integration

1. The local-memory wrapper initially dropped the new guidance hook. Actual archives contained 39 vitamin policies and zero Rust markers. Forwarding through `MemoryTools` repairs that production path; the final runtime observations above confirm delivery.
2. Initial task classification treated protected Rust paths and quoted/generated role context as activation evidence. Classification now uses host-supplied user-authored context and target-local exclusions, with separate observed-file/command activation. Positive Rust tasks with nearby command restrictions and negative “without Rust” wording were replayed on the final binary. Task-language hints remain conservative; successful actual Rust activity is the stronger signal.
3. The new exact skill reader needed the same exemption from secondary output truncation as other paged readers. Skill pages now reach model context intact and remain audit receipts rather than recursively retrieved task evidence.

## Scope boundary

The standalone Rust writer fixture reached its self-checked result, then the existing product check-contract planner stopped because no applicable project contract covered `main.rs` and `outputs/double`. It did not run a forbidden automatic suite or reach the independent pipeline reviewer/fixer. Its real CLI result passed independent replay, but its final pipeline qualification is **BLOCKED by the missing fixture check contract**, not reported as passing. That application gate was not changed by this task. The old pre-forwarding fixture was explicitly cancelled and its owned daemon shut down.

Designer/writer execution and read-only Review were exercised. The distinct fixer cue was source-reviewed through its actual factory/caller; automatic fixer execution, forced transport retries, tiny-capacity requests, native non-Linux execution and every possible long-horizon drift were not exercised. Normal current-policy reconstruction was observed across multiple real requests. This is no general performance A/B study and no whole-application qualification claim. No unit tests or formal/repository verification suites were used, and the user's live daemon/session was not changed.
