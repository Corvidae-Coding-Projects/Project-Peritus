# Local token budgets: real repair, October 16, 2026

The long-running builder job stopped before another provider request with
`RequiredTokenBudgetExceeded`: available 16,709, required 16,903. The retained run is
`59ed85009de6c149d98e99b65e888b72`; its source version was `643441bca`.

## Confirmed defect and correction

Working-state assembly subtracted instruction/tool framing from the provider's estimated input
target, then treated the remaining tokens as a hard ceiling for required memory. Growth in
retained plans, failures, or framing could therefore stop a previously valid long-running job.
This was a local projection rejection, before the selected provider had accepted or rejected it.

Production assembly now renders complete role-visible working state without a token allocation.
The existing selector still enforces scope, role visibility, dependencies, source identity,
and non-authoritative provenance. Rendering does not change entries or their sources.
Optional transcript compaction still uses provider input estimates; actual provider context
windows and submitted-turn transport sizes still apply.

The old working-memory and automatic-evidence token configuration keys are accepted and ignored.
Fixed 32,768-token developer, 4,096-token review, and 8,192-token semantic-compaction response
ceilings are removed. Requests use negotiated provider output capacity. Token-based secondary
tool-output truncation is removed; command duplicate-receipt/progress projection remains.
Token usage remains observable without a cumulative coding-run token allowance.

## Real operation

Build the actual application, using bounded resources:

```sh
cargo build --locked --release --jobs 2 -p peritus-daemon -p peritus-cli -p peritus-tui
```

The [retained consumers](reproducers/token-budgets/README.md) operate the real public libraries,
real files, canonical persistence, the production context host, and the configured Codex
adapter's local serialization. They do not substitute a provider or dispatch application tools.

| Scenario and independent expectation | Observation |
| --- | --- |
| Concrete 29-entry source-backed input exceeds the original explicit 16,709 ceiling; complete rendering preserves it | Original 16,903-token rejection reproduced; complete render passed |
| 96 ASCII required entries survive rendering and durable reopening | 55,656 estimated tokens; none omitted; state bytes unchanged |
| 96 Unicode records and required dependency closures survive reopening | 58,248 estimated tokens; exact identities/source locators retained |
| All 512 storage entries render without a token stop | 303,054 estimated tokens; all selected; state bytes unchanged |
| 24 generated states, seed `0x50e7175`, vary size, Unicode, dependencies, Writer/Reviewer, and reopening | All passed |
| Empty state, reviewer exclusion, and cross-role access | Empty stays empty; reviewer memory excluded; cross-role binding rejected |
| Legacy configuration keys at 0, 1, 4,096, 16,384, 16,385, and `u64::MAX` | All accepted; no token allocation serialized |
| Default generation allowance and explicit output API values above 32,768 | Default uncapped; 32,769, 64,000, and `u64::MAX` accepted; invalid explicit zero rejected |
| Copied actual long-run archive with 7,054 observations and one uncovered journal event | All 104 entries and their complete text/source links preserved; checkpoint published and read back |
| Fresh-process reopening of the copied host | Same 1,953,239-byte assembled view and SHA-256; no entries omitted |

Each generated state's oracle checks exact entry identities, dependency presence, source
locators, untrusted/non-authoritative provenance, unchanged canonical state, and encode/decode
readback. The seed does not replace the saved concrete numeric fixture. Initial fixture-tuning
and error-conversion mistakes in the consumer were corrected and preserved in private evidence;
they were harness failures, separate from the recorded product failure.

The copied host assembled 206,244 estimated input tokens against the real Codex adapter's
174,762-token input target. Its actual serialized transport fit. The identical assembled view
after fresh-process reopening had SHA-256
`0413bad683d301d8b8c47f2228ccd329eb47d2ee43e004f39ca102821d393c04`.
These host runs made zero provider requests and dispatched zero application effects.

## Live deployment

The installed builder CLI, daemon, and TUI were replaced atomically under the launcher lock,
after a clean IPC shutdown and closed backup. Ownership is builder and executable mode is 0755.
The normal installed launcher opened the retained run; `/proc` executable hashing confirmed the
running daemon uses the repaired artifact:

| Artifact | SHA-256 |
| --- | --- |
| CLI | `85b443d6f9d0aa1dbae33e23812b3655c707b1d7cfcb3c532f8bf1726df60c7e` |
| Daemon | `5458b164e018c0a8583b3919196ce0afb6aa270b6cf84488660d6b710fdb486b` |
| TUI | `7429ced8bed358b9d30e342db444a2a2dac3500edcc34e80a16767cf44e3c6aa` |

The real `runs retry` command returned `Queued to resume implementation`; the job reached
Writing and started provider request 1,061, beyond the original 1,060-request failure.
It then advanced to 1,065 requests and 3,056 tools, with no retained interruption. New completed
operations included reading the current proxy/compose configuration and querying retained
context. The model explicitly continued the existing live browser/data-flow investigation.
All four public inputs remained incorporated. The observer client was detached with Ctrl-Q;
the daemon and implementation job were left running.

This establishes successful retained-job recovery and fresh provider/tool progress. It does
not claim the multi-hour application task has completed or that a provider accepts unlimited
input. The original doll daemon was not started or replaced.

## Evidence and limits

Private raw evidence is under `/home/doll/Peritus-Builder-Repair/token-budget-followup/`.
The original closed daemon/state/configuration/binary backup is under builder's private
`repair-backups/token-budget-20261016-181110/`. Original project contents, provider credentials,
large archives, and live run state are not checked into this repository.

This is focused coverage of context assembly and continuation. The 303,054-token library case
does not claim any provider accepts that request size. Provider context/transport limits,
storage bounds, non-token resource limits, and explicit caller-supplied domain selection APIs
remain separate contracts. No unit, mock, formal, or repository verification suite was run.
