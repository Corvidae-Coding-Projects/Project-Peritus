# Real packaged-command symlink replay

Build the CLI normally with at most two Cargo jobs:

```sh
cargo build --offline --release -p peritus-cli -j 2
```

`consumer.rs` is a minimal public consumer of production `SiblingBinaries::from_application`, not a unit-test executable. Compile it against the newly built launcher library, using `rustc --edition=2024 -C panic=abort -O -L dependency=target/release/deps --extern peritus_launcher=PATH_TO_CURRENT_LAUNCHER_RLIB consumer.rs -o /tmp/peritus-discovery-consumer`. Select the rlib produced by the current build, not a stale alternate feature/profile artifact.

Then operate real discovery:

```sh
python3 docs/testing/reproducers/macos-command-path/replay.py \
  --consumer /tmp/peritus-discovery-consumer \
  --cli target/release/peritus --daemon target/release/peritusd \
  --output /tmp/peritus-discovery-real
```

The driver copies actual binaries into a package layout, creates real filesystem aliases, and invokes the public consumer and real daemon version probes. It records concrete inputs, expected literal package locations, observations and artifact hashes. It expects an unused output directory for a fresh replay. It covers broken aliases and package components as well as successful paths; no mock binaries or version answers are generated. Linux already resolves its process executable path differently from macOS, so this replay does not claim to reproduce the customer's native launch failure.

`install_replay.py` performs an additional Linux-only shell/installation campaign. Its root must contain the checksum-validated public v0.0.7 Apple-silicon archive extracted as `peritus-macos-aarch64/` and the unmodified release `published-install.sh` bootstrap. It stages current real Linux binaries into local fixture packages, then runs the published bootstrap's actual download/checksum/extraction/fresh-install/upgrade path through its documented local mirror override. It also exercises the unchanged Mac adapter's portable filesystem steps and the exact repair wrapper with Linux executables. The manifest identifies the Linux fixture truthfully; Mac helper bytes come from the actual public package and are not executed on Linux.

```sh
python3 docs/testing/reproducers/macos-command-path/install_replay.py \
  --repo /absolute/path/to/Project-Peritus \
  --root /tmp/peritus-installer-real
```

Install and runtime writes are isolated by bubblewrap home-directory bind mounts and PID namespaces. HOME is not reassigned, system dependencies are not installed, and no live user configuration, provider, daemon or run is touched. The CLI goes through actual offline setup, reaches a real daemon-connected TUI, exits, and requests Clean shutdown from that daemon before the namespace ends. Raw logs, terminal bytes, version outputs and shutdown receipts remain in the disposable root.

This requires Linux, bubblewrap/user namespaces and the installer's ordinary existing dependencies. It establishes file placement, argument forwarding and real Linux startup. Native macOS executable-path acquisition and Mach-O launch require a separate Mac replay. See [the campaign report](../../macos-command-path-real-2026-10-13.md) for the observed limits.
