# Hosted configuration upgrade recovery

The published v0.1.9 Linux executable reproduced the reported error through an actual PTY:

```text
peritus: internal: launch interactive product: cannot determine Peritus application directories: generated daemon configuration generation 21 has different content
```

The fixture retains a valid generation-21 product state and its generated hosted configuration,
with only the previously absent `discover_model_limits = true` line removed. No credentials
or user archives were copied. The launcher's configuration rendering changed in v0.1.9 while
the durable user choices, and therefore their generation, stayed unchanged. Its earlier migration
recognized only old OpenCode capability lists, and could not recognize those lists combined
with the newly added discovery flag.

## Repair

Shared compatibility recognizes exact historical generated formats: missing hosted discovery
flags, earlier OpenCode capability lists, and the retired provider timeout, including their
combinations. Folder-catalog migration uses that same narrow compatibility policy. OpenCode's
saved transport migration remains separate from format migration.

Under the bootstrap lock, a known format change validates the desired configuration, commits
a new product-state generation, and then publishes current bytes in a new immutable file.
Every earlier state/config file stays untouched. Preparation now requires its returned parsed
configuration and actual file to match exactly; it no longer silently pairs a current parsed
configuration with an old timeout-bearing file. If publication is interrupted after the state
commit, the next launch resumes publication in that generation.

## Observed behavior

The final program passed **43 scenarios**, including twelve generated provider subsets and
format combinations with seed `271821`.

| Scenario | Independent expectation and observation |
| --- | --- |
| Original released CLI | Reproduced the exact generation-21 collision before provider setup. |
| All seven hosted services | Missing discovery flags migrated to generation 22; current flags were true and provider/model/reference choices were unchanged. |
| Zen/Go historical combinations | Eight combinations of old capabilities, discovery flags and removed timeouts migrated successfully. |
| Mixed thirteen-provider installation | Account/direct/hosted choices, failover and identity remained unchanged while combined old provider, tool and folder formats migrated. |
| Current hosted, account/direct and offline formats | Repeated fresh preparation stayed in generation 21. |
| Twelve seeded subsets | Historical formats became exact current bytes; earlier files and unrelated retained bytes stayed unchanged; fresh repeated preparation did not advance again. |
| Unrecognized edits | Explicit discovery=false, changed model/reference, increased folder authority, unknown fields, and unrelated direct-provider capability changes failed without replacing state/configuration. |
| Interrupted publication | A real directory obstructed the new config path after generation 22 was committed. Removing the obstruction allowed fresh preparation to publish generation 22 without advancing again. |
| Backed-up config | Preparation recreated the missing current config in generation 21 and retained the backup byte-for-byte. |
| Four concurrent preparations | Each either prepared the same current generation or received the existing bootstrap-busy error; final preparation stayed at generation 22. |
| Actual terminal/daemon | Offline configuration migration attached the terminal, exited cleanly, shut down its daemon, then attached and shut down again without another migration. |
| File privacy | Current generated files retained mode 0600. |

The first baseline invocation used redirected input and was correctly rejected as noninteractive;
the retained reproduction uses a real PTY. Automatic browser provisioning initially delayed the
replay, so disposable test processes use `PERITUS_INSTALL_DEPS=0`. An early fixture generator
also removed a direct compatible route's parallel capability; production correctly rejected
that unrelated edit. The generator was corrected to describe only historical OpenCode formats,
while all acceptance and preservation checks remained unchanged. A dedicated direct-capability
rejection case retains that boundary.

## Build and replay

Built actual `peritus-cli` and `peritus-daemon` with Rust 1.97.1, one Cargo job, no debug symbols,
and the locked dependency graph:

```sh
CARGO_TARGET_DIR=/home/doll/.cache/peritus-read-loop-audit-20261020/target \
  CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_DEV_OPT_LEVEL=0 \
  cargo build --offline --locked -p peritus-cli -p peritus-daemon --message-format=json \
  > /home/doll/.cache/pcu-20261021/build.jsonl
python3 docs/testing/reproducers/configuration-upgrade/build.py \
  --cargo-output /home/doll/.cache/pcu-20261021/build.jsonl \
  --output /home/doll/.cache/pcu-20261021/consumer
python3 docs/testing/reproducers/configuration-upgrade/replay.py \
  --consumer /home/doll/.cache/pcu-20261021/consumer \
  --binary /home/doll/.cache/peritus-read-loop-audit-20261020/target/debug/peritus \
  --baseline /home/doll/.cache/pcu-20261021/baseline/peritus-linux-x86_64/bin/peritus \
  --output /home/doll/.cache/pcu-20261021/final-campaign
```

The baseline came from the published v0.1.9 Linux x86_64 package. Binary identities, changed
source hashes, concrete cases and results are in [the evidence record](configuration-upgrade-evidence.json).
Raw terminal and baseline transcripts are retained in the replay output directory. Portable
replay instructions are in [the reproducer guide](reproducers/configuration-upgrade/README.md).
Disposable fixture directories were removed, and all daemons started by the replay were shut down.

Scope: actual Linux bootstrap, persistence and terminal/daemon paths. No Windows/macOS execution,
provider inference or browser operation was attempted for this fix. No unit, mock, formal
verification, or repository qualification suite was created or run. Installed binaries and
live user daemons were not changed.
