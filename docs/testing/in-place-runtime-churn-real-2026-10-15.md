# In-place runtime writes: real repair and replay

The reported failure was observed in builder's ordinary-folder project at
`/home/builder/precient`, run `59ed85009de6c149d98e99b65e888b72`. Its saved interruption was
`observe candidate identity: workspace changed repeatedly while capturing content and
repository context`. Its scope journal included a live PostgreSQL directory alongside
application source. No application files were manually repaired to hide this harness error.

## Causes and changes

The in-place candidate recorder required separate before/content/after scans to agree, with
three attempts. Running PostgreSQL changed enrolled files between scans, so ordinary tool
effects could succeed and then fail their bookkeeping boundary. In-place observations now
derive the changed-source and full-scope identity axes from the same captured file
observations. Later observations still detect new content and invalidate dependent evidence;
the change does not freeze identity, ignore enrolled runtime data, or assert an atomic
filesystem snapshot. Managed Git candidate capture retains its existing repository fences.

The independent replay also found an earlier failure: automatic command before-images
included every existing descendant of a declared directory. Capturing an actively written
service file rejected even a subsequent read command before execution. Automatic command
checkpoints now use exact file declarations. Directory descendants remain comparison
evidence; declaring a directory does not imply rewind coverage of every database page.
Explicitly reading or editing a child records its exact declaration while preserving its
earliest parent baseline. Direct file mutation checkpoints retain their strict capture and
seal requirements. Existing checkpoint records are not erased.

Independent final review found an edge case in the first repair: explicitly re-declaring an
already covered child directory could capture a newly created file as its baseline or
reapply admission limits after permitted growth. Three added public-API replays reproduced
those failures against the first repair. Covered directories now keep their earlier parent
evidence without recapturing descendants; later exact file declarations remain supported.

Discovery tolerates files/directories disappearing during observation, recording absence
or skipping vanished inventory children. Other I/O errors still fail. The existing digest
encoding and version-one/version-two scope journal formats remain unchanged.

## Replays and independent expectations

| Actual operation | Original 0.0.9 | Fixed binary |
| --- | --- | --- |
| Real TUI/daemon/Codex; runtime writer active before directory enrollment | Five commands rejected before effects by automatic checkpoint capture; source unchanged. Deliberately cancelled after 245 seconds. | Source update, created output, and Peritus's own `python3 main.py` command succeeded with writer alive; zero host-tool errors. |
| Same product path; writer starts after the first command creates its output | Both files changed, then the exact candidate-identity interruption stopped execution at 203.605 seconds before Peritus's follow-up command. | Both files changed and the follow-up command returned exact `SOURCE-REPAIRED` plus newline, exit 0, while a 16 MiB runtime file kept changing. |
| Public scoped-baseline consumer on copies of Peritus source | Existing retained contract, plus three new child-directory failures reproduced on the first repair | All 24 journeys passed: directory/file lifecycle, earliest baselines, legacy journals, bounded admission, growth, protected paths, sockets/FIFOs, and external-link non-traversal. |
| Concurrent runtime file/parent creation and removal | New targeted exploration | 40 fresh consumer observations passed with seed 260215. A separate source edit remained visible; after churn, exactly that source change remained. |

The first-repair full-product cases took 197.536 and 181.975 seconds, within a 360-second budget
each. They used 1 MiB and 16 MiB fixtures, eight-byte updates each millisecond, real official
Codex inference, ordinary host tools, and independent CLI readback. Both preserved their
AGENTS guide and created no unrelated files. Fixtures were deliberately cancelled only
after the operation contract was observed, then their daemons/writers were stopped.
**These operation checks do not establish whole-pipeline qualification while runtime-inclusive
candidate state changes continuously.**

## Build and retained artifacts

```sh
cargo build --locked --release --jobs 2 -p peritus-daemon -p peritus-cli -p peritus-tui
```

The separate real consumer used the current public `ScopedBaseline` API, an offline-resolved
dependency lock, the same supported toolchain, and two Cargo workers. No unit tests, mocks,
formal verification, Clippy gates, or repository qualification suites were run.

First-repair full-product replay SHA-256 values (retained separately from the final rebuild):

| Binary | SHA-256 |
| --- | --- |
| peritus | bd81ef6090578893810d162e2d427cec40a321d74f08f013122230b3444db353 |
| peritusd | 3a89e00aeedabe7f6ae4be258213518a8f2bf17ac470ba0b159b181760a9f2d6 |
| peritus-tui | 7429ced8bed358b9d30e342db444a2a2dac3500edcc34e80a16767cf44e3c6aa |

The [scope replay guide](reproducers/in-place-scope/README.md) describes runnable public-API
and full-product replays. The drivers generate ordinary owned fixture data, copy actual
binaries, and retain raw observations. Credentials are neither copied nor committed.

Private evidence is under `/home/doll/Peritus-Builder-Repair/`: independent original/fixed
screens, trace frames, binary manifests, follow-up observations, and `scope-replay` results.
The existing replay copied real Peritus source before applying its fixture mutations.

## Builder installation and retained run

The old daemon was asked to shut down through its normal IPC command. Its shutdown report
retained `processes=2`; independent process inspection confirmed the old daemon and both
owned PostgreSQL/API services had exited before backup and replacement. No force kill or
discard/acknowledgement of unknown effects was used.

The three old binaries, closed durable state, and configuration were saved in builder's
private `repair-backups/candidate-observation-20261015-231420` directory. Fixed binaries were
atomically replaced under the launcher lock with builder ownership and executable modes.
The normal installed launcher opened the retained project/run on its first attempt in
0.397 seconds. The running daemon's executable digest matched the tested binary above.
The exact original run accepted Retry and resumed real provider/tool work; the application
itself remains an ongoing implementation, not a completed deliverable certified by this repair.

After the directory edge-case correction, the final public-API replay passed all 24 journeys,
including all three failures reproduced against the first repair, and the 40 concurrent
runtime observations. The live run had progressed from Designing into Writing with 46 new
host-tool observations and no errors before final replacement. Normal cancellation retained
its effects; the daemon then reported a clean shutdown with no remaining processes.

A second closed-state backup is retained under
`repair-backups/candidate-observation-final-20261016-043737`. The final daemon SHA-256 is
`101613f278a5e5dcf6f574c0d0eeeebea1c89288609861aade95c221cb8efd72`; CLI and TUI hashes
remain as above. All three installed files retain builder ownership and mode 0755. The
normal launcher connected to the retained run on the first attempt in 0.231 seconds, and
the running `/proc/PID/exe` digest matches the final daemon. Retry resumed the original run
in Writing. Final replay output is retained separately from the first repair's evidence.

Both full-product cases were repeated with these final installed binaries and passed in
130.833 and 117.787 seconds. Each final oracle requires the actual parent declaration plus
runtime descendant in the journal, writer liveness and counter progress (123097 and 7045),
the real follow-up command, exact independent output, and unchanged unrelated files. Both
had zero host-tool errors; `/proc/PID/exe` confirmed the final daemon digest in each fixture.
These final fixtures were also deliberately stopped after the operation contract, without
claiming whole-pipeline qualification. Raw final frames, journals and summaries remain in
`independent/final-before-effect`, `independent/final-after-effect`, and
`independent/final-operation-summary.json` under the private evidence directory.
