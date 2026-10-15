# Native RSI real-operation consumer

## Folder guide

peritus-rsi-native-real-consumer package.

Standalone replay/consumer package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.lock](Cargo.lock) | Locked dependency resolution for the neighboring package manifest. |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-rsi-native-real-consumer`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |
| [RESULTS-2026-10-13.md](RESULTS-2026-10-13.md) | Document: Native RSI final real-operation evidence. |
| [checkpoint_replay.py](checkpoint_replay.py) | Capture a genuine committed C0 boundary, then reopen only a disposable copied prefix. |
| [crash_replay.py](crash_replay.py) | Abruptly kill only a process this driver started, then reopen its actual daemon state. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Real-interface replay resources for src; start with the local README/driver and its documented disposable-state prerequisites. |

This is an ordinary consumer of the public daemon, E3 evaluation, E1 materialization, and artifact-store APIs. It runs actual immutable CLI executables in native restricted C2/C3 processes, then runs an independent frozen evaluator. It is not a unit suite. No executables, private state, credentials, or observed-success substitutes are bundled.

Use a disposable copy of an E1 fixture produced by the sibling evolution consumer. Its `actual-materializations.json`, journal, and artifact store must contain real committed baseline/candidate materialization receipts and the actual executable artifacts. Stop every other writer before copying or opening it. The fixture root must be owned exclusively by this campaign.

Build the real consumer from the repository root with a bounded Cargo job count:

```sh
cargo +1.97.1 build --manifest-path docs/testing/reproducers/rsi-production/native/Cargo.toml --release -j 2 --target-dir target
export PERITUS_RSI_CONSUMER="$PWD/target/release/peritus-rsi-native-real-consumer"
export PERITUS_RSI_HELPER="$PWD/target/release/peritus-linux-sandbox-helper"
export PERITUS_RSI_CGROUP_ROOT=/absolute/path/to/your/delegated/cgroup
"$PERITUS_RSI_CONSUMER" /absolute/path/to/owned-copy complete start
```

The helper must be built from the matching production source. Bubblewrap and Python are read from `/usr/bin/bwrap` and `/usr/bin/python3`; unsupported or unavailable native isolation fails closed. The delegated cgroup parent must support the configured CPU, memory, process, and termination bounds. The consumer uses two daemon workers, a finite 180-second observation deadline, and finite per-stage resource/output bounds. It writes configuration only inside the disposable fixture.

Modes and independent expectations:

| Suite | Actual operation and oracle |
| --- | --- |
| `complete` | Nine CLI cases, two observations per arm: version, three help forms, unknown literal template arguments, empty braces, and Unicode. Independent exit/output expectations; candidate cases must all pass. |
| `version` | Four observations per arm against the declared candidate version; used for the focused unchanged-policy F0 campaign. |
| `cancel` | Cancel through the actual authority after an evaluator receipt appears; logical cancellation and physical teardown must complete. |
| `crash` | Independent evaluator delays; run `crash_replay.py` to kill only the daemon child it created, observe a live owned C2 cgroup, and reopen the same campaign. Actual interrupted work must remain ambiguous. |
| `timeout` | One-millisecond native stage wall bound; infrastructure failure must never become a task pass. |
| `resource` | Eight-MiB memory bound; retain the actual numeric or supervisor terminal without inventing a resource-limit disposition. |
| `gc-active` | Delayed evaluator plus actual artifact GC while its finalized candidate outputs exist. This deliberately reproduces a second maintenance writer on an exclusively owned copy. Raw receipts, cgroup members, terminal envelope, GC roots/actions, and application outcome are preserved. It does not establish arbitrary concurrent GC safety. |
| `gc-stale` | Prepare a real collection plan before campaign admission, durably admit and retain exact inputs through C0, then apply the older plan. A newly rooted artifact must not be quarantined by that plan. |
| `partial-cancel` | Commit a real interrupted inert plan prefix through C0, cancel using the owning operator, and require all committed Planned work to reach Cancelled without creating any scheduling/execution authority. |
| `production-N` | Fresh `current_program` read matched to the exact E1 receipt and ELF digest. Create a fresh genuine paired campaign, then verify selected-arm version output through C2. The pointer capture is inert metadata; execution requires the new E3 admission. Use distinct names such as `production-before`, `production-after`, and `production-rollback`. |

`start` creates/adopts the named exact frozen campaign; `resume` opens its retained profile and pumps ordinary durable work without replacing its identity. `capture` performs only the fresh production pointer read. The generated candidate goldens in this reproducer are `peritus 0.0.7` and baseline `peritus 0.0.0`; update these declared fixture requirements when using different genuine executable versions, before observing results.

After `complete` has Published and closed, run the same suite with action `gc-final` to perform actual quarantine and deletion of an unreferenced witness. Every frozen input and both actual terminal/output stages per rollout must remain explicitly rooted and hash-verifiable. This mode requires the closed fixture's exclusive maintenance ownership.

To inspect retained legacy state without interpreting SQL payloads:

```sh
"$PERITUS_RSI_CONSUMER" /absolute/path/to/closed-copy inspect CAMPAIGN_HEX
```

That mode uses the fixture's `daemon.toml`, the owning `EvaluationOperator::show`, and `failure_diagnostic`. It prints the actual replayed state and reads retained diagnostic bytes. Result, plan, shutdown, pointer capture, and GC observation JSON are written inside the fixture. Keep them with stdout/stderr logs and the immutable source revision when reporting a reproduction.

`checkpoint_replay.py OWNED_E1_SEED NEW_PARENT` attempts a genuine online C0 backup at the last settlement/analysis boundary, using the public `inspect-state` owning-state probe. That probe reads journal state only and makes no claim about artifact availability. The driver then copies the actual finalized files and reopens an intact selected prefix; it requires unchanged child-receipt count and Published/Clean completion. It never edits the source journal or fabricates historical rows. The bound is three actual campaigns and 256 snapshots; an uncaptured boundary is explicitly reported as a coverage gap. Set `PERITUS_RSI_CONSUMER`, helper, and cgroup variables as above.
