# Startup and folder recovery replay

Build the actual `peritus` and `peritusd` binaries in the same directory. Use a short output path: the real Unix socket must fit the operating system's path limit. The driver uses isolated XDG directories, a disposable folder, real Git, real PTY rendering and the actual daemon. It selects offline browsing and never requests inference.

```sh
uv run --with pyte python docs/testing/reproducers/startup-recovery/launch.py --binary target/release/peritus --output /home/USER/ps-replay --delay-listen
```

`--delay-listen` uses `strace` to delay the actual daemon's `listen` syscall for 35 seconds, reproducing slow startup without substituting a daemon or its response. The launcher must wait beyond its previous 30-second cutoff, display progress, and attach. Run the old binary pair with `--expect-timeout` to demonstrate the original failure.

After an old-version timeout, run the updated binary with the same output directory and `--reuse-existing` to exercise legacy trust migration and interrupted-socket recovery.

The following real launches initialize Git in a remembered folder, replace the folder while preserving its predecessor, decline renewed trust, and restore trust through the actual workspace settings menu. Every step must retain the original workspace ID; replacement cannot inherit write authority. Raw terminal output and sanitized state readbacks are saved in the requested output directory. The driver shuts down only its isolated daemon.

The large-archive campaign also opens a private copy of a real control database with the actual daemon, streams comparisons of every original state/event/history row, and reads the original run through `runs show`. See the linked [campaign report](../../startup-recovery-real-2026-10-15.md). User archives are intentionally not committed as fixtures.
