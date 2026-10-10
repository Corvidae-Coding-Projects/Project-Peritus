# FINDING-0025: Filesystem tests conflated platform diagnostics and operation-specific omissions

Severity: low

Blocking: true

Disposition: fixed in the final reviewed candidate.

The filesystem integration fixture creates a non-UTF-8 child only on Linux, but initial discover/search assertions applied its diagnostic and continuation shape to macOS. A later correction still treated discovery and textual search as having the same omission set, overlooking the tracked `blob.bin` that search correctly reports as `BinaryContent`. The final tests exercise a base Unix fixture and a Linux-only invalid-name variant, and assert exact typed causes, total counts, rendered paths, and continuation offsets separately for discovery and search.
