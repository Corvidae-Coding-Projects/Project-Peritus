# Actual TUI heartbeat replay

## Folder guide

Real-interface replay resources for heartbeat; start with the local README/driver and its documented disposable-state prerequisites.

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |
| [campaign.py](campaign.py) | Deterministic timing exploration of the same actual TUI workflow. |
| [public-approval-registry.bin](public-approval-registry.bin) | Retained binary/archive data for public approval registry; not a source-code entrypoint. |
| [replay.py](replay.py) | Real Peritus TUI/daemon heartbeat replay; no replacement protocol or application. |

Operate built, real `peritusd` and `peritus-tui` binaries using a PTY and fresh per-case folders. The driver starts its own daemon, waits for its local socket, starts the TUI, handles terminal cursor-position queries, and captures the TUI's actual ANSI output. It never implements the Peritus application protocol or substitutes provider/application behavior.

Expected results, established before execution:

| Case | Action | Expected result |
| --- | --- | --- |
| ordinary | Leave an authenticated TUI responsive for 27 seconds | Connected; no heartbeat errors; Ctrl-Q exits 0 |
| delayed | Pause only the fixture TUI at age 5 seconds for 17 seconds; resume and observe for 11 seconds | Valid queued heartbeat replies accepted; connected; no mismatch or timeout; Ctrl-Q exits 0 |
| timeout | Pause only the fixture TUI at age 5 seconds for 42 seconds | Actual missed-reply timeout; TUI shows disconnected; daemon logs timeout, not nonce mismatch |

The daemon interval is 10 seconds and the missed-reply bound is 3 intervals. A 17 second delay crosses two daemon heartbeat ticks but resumes before the genuine timeout. Timing is recorded per action; endpoint, PIDs, binaries' SHA-256 hashes, logs, and exit statuses are retained. The public approval registry fixture contains only public credential registry bytes, not keys or provider credentials. The driver copies it into each new state directory.

Run under bounded resources (a unique output directory is required):

```sh
systemd-run --user --wait --collect --unit=peritus-heartbeat-replay \
  --property=CPUQuota=200% --property=AllowedCPUs=2,3 \
  --property=MemoryHigh=1536M --property=MemoryMax=2G --property=TasksMax=128 \
  python3 /ABS/CHECKOUT/docs/testing/reproducers/runtime-friction/heartbeat/replay.py \
  --bin /ABS/BIN_DIRECTORY --output /ABS/NEW_EVIDENCE_DIRECTORY
```

`BIN_DIRECTORY` must contain the actual three Peritus binaries: `peritusd`, `peritus-tui`, and `peritus`. Use available CPU IDs in `AllowedCPUs` on other machines. The driver returns nonzero if any scenario fails. `--cases delayed --pause-seconds 6`, `11`, `17`, or `29` explores delay boundaries in independent fresh sessions. It waits 11 seconds after resuming so a subsequent ordinary heartbeat is exercised too.

Only subprocess PIDs returned by this driver receive signals. A stopped child is always resumed, and both child processes are reaped. No existing daemon, user state, provider, workspace, install, or repository build/test command is touched. The fixture's folders and raw evidence remain for inspection.

Baseline `8d5c1b256`: ordinary passed; a 17 second delay disconnected with `heartbeat reply does not match the outstanding nonce and sequence`, TUI exit 1; a genuine 42 second pause produced the correct heartbeat timeout. Baseline daemon SHA-256: `e6de524a05f6340c8f150eb559ac869c94e2ca91ebb4e2b50f88a6b116658366`.

The fixed candidate was replayed in six fresh TUI/daemon sessions: ordinary, delays of 6, 11, 17, and 29 seconds, and the genuine 42 second timeout. Every predefined expectation passed. Delayed sessions stayed visibly connected, returned clean exits, and produced no heartbeat errors. The timeout remained enforced and emitted `ts_unix_ms=1791812147864 pid=614519`; the PID matched the actual fixture daemon recorded by the driver. The campaign ran for 3 minutes 39 seconds with a 48.7 MiB peak reported by systemd. All 12 child processes were reaped and all six endpoint sockets were removed. This is bounded heartbeat/lifecycle evidence, not qualification of other Peritus behavior.

Fixed candidate SHA-256 hashes:

- `peritusd`: `b017ef7a553869ae869d77865629979772fdca1632e38982ac24026afcc5b809`
- `peritus-tui`: `04ce864cc904059655909160beca2f3967802de8f1c3f73e78637fe61ba71a6b`
- `peritus`: `1978ef4018c9c3d65c4bf59b43cb83e96808f8ef31098345326991f7687d48bc`

`campaign.py --bin BIN_DIRECTORY --output UNIQUE_NEW_DIRECTORY` runs the original scenarios and the three additional timing boundaries sequentially through `replay.py`. Use the same bounded systemd wrapper shown above, replacing only the script filename. It preserves each actual session's artifacts plus aggregate `campaign.json`. No seed is needed: all pause schedules are explicit in that file.

After an unrelated command-capture descriptor correction, the 17 second regression and genuine 42 second timeout were replayed against daemon SHA-256 `75e4873d85a12dace8bb95621de58a4768fe0794496333e26b8d6dc94e92452d`, and both passed their original expectations. The timeout's actual diagnostic contained `ts_unix_ms=1791813390719 pid=628163`, matching the recorded daemon PID. All four child processes were reaped and both endpoint sockets were removed. This replay used 1 minute 29 seconds wall time and a 73 MiB peak under the same 2 CPU / 2 GiB bounds.

The newest final daemon includes a further unrelated command descriptor binding correction. Its SHA-256 is `4040fea9c24dc01aae5e0b219789221545f72d410162426d3c7b45ec1c8744e2`; the CLI and TUI hashes above are unchanged. Only the saved 17 second delayed-heartbeat regression was replayed against this newest binary, using fresh owned state: PASS. The TUI resumed at age 22.051 seconds, stayed visibly connected through a subsequent heartbeat, and exited 0; the daemon exited 0 with no mismatch or timeout error. Both child PIDs were reaped and the endpoint socket was removed. The final replay used 35.171 seconds wall time and a 37.8 MiB peak under the same 2 CPU / 2 GiB bounds. Genuine timeout, diagnostics, ordinary operation, and other timing boundaries retain their earlier passing evidence against unchanged heartbeat/diagnostic source; those cases were not repeated again.
