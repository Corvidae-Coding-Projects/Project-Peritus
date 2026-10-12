# Actual command-output replay

Build a disposable checkout of Peritus, then compile and run this ordinary executable consumer
against its public `CommandRuntime` API. It uses actual native processes, C4/C2 authorization,
artifact storage, and retained process spools. It contains no test harness or substitute runtime.

```sh
python3 docs/testing/reproducers/runtime-friction/command-output/build-consumer.py \
  --target /ABS/TARGET/release --output /ABS/EVIDENCE/real-command-replay
systemd-run --user --scope --unit=peritus-output-replay \
  -p CPUQuota=200% -p MemoryHigh=1G -p MemoryMax=2G -p TasksMax=128 \
  /ABS/EVIDENCE/real-command-replay /ABS/EVIDENCE/NEW_CASE_DIRECTORY
```

The target must contain the libraries built from that checkout. The build helper selects the
newest release library for each dependency; use an isolated target if other builds may run
concurrently. Choose a fresh absolute case directory. On this machine, the copied checkout used
the existing release target with no concurrent source build, and the tested binaries' hashes
are recorded in the parent report.

The independently specified inputs repeat all 256 byte values, including NUL and invalid UTF-8.
Seven sizes exercise empty output, one byte, the 16 KiB preview boundary on both sides, the former
8 MiB publication boundary, and the observed 37,349,240-byte regression. For each actual `/usr/bin/cat`
process, the consumer requires successful completion, bounded preview strings, exact beginning,
middle, tail and EOF pages, past-EOF rejection, and readback through a reopened runtime. Nonempty
streams must also have a finalized artifact digest and exact artifact page. `results.json` records
each case; the consumer returns nonzero on any mismatch. No random seed is needed because the byte
period, lengths, and read offsets are explicit in `replay.rs`.

The original live defect was an exit-0 command with 37,349,240 captured bytes, failed artifact
publication, and empty model output. The first candidate exposed a mismatched 1 GiB call allowance
and 8 MiB shell descriptor; the next candidate exposed a dispatcher still binding the old
descriptor. Both were corrected without bypassing exact descriptor-digest checks. The final copy
passes all seven sizes, 28 spool pages, seven past-EOF rejections, six nonempty artifact checks,
and seven reopened readbacks. These counts describe distinct actions rather than code coverage.

For the complete model-to-host path, launch the copied `peritus open /ABS/DISPOSABLE_WORKSPACE`
with separate `XDG_CONFIG_HOME`, `XDG_STATE_HOME`, and `XDG_CACHE_HOME`. Use the real account provider
and select its available model through `/model`. Copy `fixture_gen.py` and a disposable notes file
into that workspace. Through the actual TUI, request:

> Inspect the workspace and existing evidence file. Run `python3 fixture_gen.py` exactly once
> through direct `run_command` argv. Use `command_output` with `max_bytes=64` at offsets 0,
> 18,674,620, 37,349,176 and EOF 37,349,240. Try offset 37,349,241 and an unknown handle as
> negative cases. Save only evidence.json containing actual exit, publication, byte count,
> handle, pages and rejection details. Preserve existing files. No unit tests, test suites,
> lint, builds, Git commands or formal verification. Record any failure honestly.

Check the resulting JSON against the durable tool trace, not only the model's summary. The
generator emits literal BEGIN/END markers and an independently known byte count. Each output page
must come from the same command handle; recovering output must never execute the generator again.
Stop only the copied TUI and its owned daemon when finished. This replay does not require stopping
another Peritus session or replacing the installed binaries.
