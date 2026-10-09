# FINDING-0027: Git paging fixture exceeded macOS path bounds and weakly identified continuation

Severity: low

Blocking: true

Disposition: fixed in the final reviewed candidate.

The diff paging regression created twelve 200-byte directory components under a temporary repository. That relative path alone exceeded macOS `PATH_MAX`, so native runtime-tools CI failed before exercising paging. The fixture also created an invalid UTF-8 name on every Unix host even though that fixture is not portable across the supported Unix hosts; invalid-name coverage is qualified here only on Linux. An intermediate shorter-path repair retained a generic `continued_path` flag that any partially packed entry could satisfy, so it did not prove that the intended long path was individually continuable; its first exact-index variant also expected a Unix-only fixture on Windows. The final repair uses two 200-byte components at the 1,024-byte production page budget, creates invalid-name coverage only on Linux, and guards the exact long-path index and continuation assertion to Unix while preserving ordinary Windows reconstruction.
