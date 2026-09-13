# Release377 source-review command correction

This note corrects only the working directories quoted in
`/tmp/peritus-sol-release377-independent-source-review-20260912T001.md`.
The original report and reviewed source are unchanged.

Both parent manifests contain workspace-relative paths beginning with
`crates/foundation/peritus-release-policy/`. Therefore the rerunnable identity checks are:

```text
cd /tmp/peritus-parent-release336-source-exact
sha256sum -c /tmp/peritus-parent-release336-source-exact.sha256

cd /tmp/peritus-parent-release377-source-exact
sha256sum -c /tmp/peritus-parent-release377-source-exact.sha256
```

I reran those exact read-only checks. The first checked all 52 baseline files and exited 0;
the second checked all 64 final files and exited 0. Raw outputs are:

- `/tmp/peritus-sol-release377-review-correction-baseline52-check.log`
- `/tmp/peritus-sol-release377-review-correction-final64-check.log`

The earlier report's package-subdirectory `cd` lines were transcription errors and would
double-prefix each manifest path. They were not the working directories used for the valid source
identity comparison described by the report.
