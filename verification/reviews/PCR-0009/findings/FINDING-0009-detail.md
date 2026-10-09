# FINDING-0009: The streaming diff parser retained unbounded state and deferred validation

Severity: high

Blocking: true

Disposition: fixed in the final reviewed candidate.

An intermediate paging parser still retained full selected and nonselected hunk/line vectors and failed to validate oversized or empty nonselected hunks until a later page. A summary could therefore advertise a structured diff that later cursors rejected, while memory remained proportional to the full diff. The repair hashes and counts the whole stream incrementally, retains only the requested bounded page, and validates every path, hunk header, and hunk body during the initial pass.
