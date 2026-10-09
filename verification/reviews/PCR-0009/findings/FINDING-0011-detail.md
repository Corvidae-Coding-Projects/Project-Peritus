# FINDING-0011: Control-character expansion hid preview truncation

Severity: medium

Blocking: true

Disposition: fixed in the final reviewed candidate.

The safe line preview inferred truncation by comparing sanitized length with raw length. Escaping or replacement can expand controls, so a 500-byte control-heavy source produced a 1,023-byte preview while incorrectly reporting `truncated=false`; the TUI then skipped exact raw retrieval. The repair records whether the bounded conversion stopped before consuming the source and advertises the exact raw length.
