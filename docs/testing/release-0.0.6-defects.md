# Release 0.0.6 real defects

## REL-001 — Windows checksum sidecar uses CRLF

The native Windows ARM64 job at https://github.com/Corvidae-Coding-Projects/Project-Peritus/actions/runs/38112168321/job/114389806357 successfully built and packaged the release binaries, then failed in `peritus-release-operator generate` with `native package checksum must contain one SHA-256 digest`.

Cause: `packaging/native_release.py` used `Path.write_text` to write the archive's SHA-256 sidecar. Windows newline translation produced CRLF; the release format and existing consumer require a hexadecimal digest followed by LF.

Fix: write the ASCII checksum bytes explicitly with one LF, preserving the archive content and digest. The application binaries and strict checksum reader are unchanged.

Replay: the actual release operator, using the downloaded release archive, rejects the recorded CRLF input and accepts the fixed LF input while producing inventory, SBOM, and provenance. Evidence: `/home/doll/Peritus-release-publication-0.0.6/checksum-newline-replay.json`. The original native Windows log is retained alongside it. Native Windows completion on the corrected release build remains pending. No unit tests were created or run.
