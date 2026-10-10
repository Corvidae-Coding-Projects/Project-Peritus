# FINDING-0005: Paged directory inspection duplicated unsupported-child diagnostics

Severity: high

Blocking: true

Disposition: fixed in the final reviewed candidate.

`peritus-workspace` rescanned a directory for each supported-entry page while returning every unsupported child diagnostic on every page. `peritus-tools-fs` appended those diagnostics repeatedly, so omission counts, retained diagnostics, and digests depended on the number of supported-entry pages. The repair uses one deterministic native-name cursor over supported entries and diagnostics, counting each child once.
