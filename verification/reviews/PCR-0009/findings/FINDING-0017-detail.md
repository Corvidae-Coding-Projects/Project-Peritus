# FINDING-0017: Process output accounting counted bytes that never reached durable storage

Severity: high

Blocking: true

Disposition: fixed in the final reviewed candidate.

The supervisor incremented retained-byte accounting before writing to the spool. A failed or partial write could publish `retained` larger than the durable spool, contradicting range/artifact identity even though the terminal disposition was incomplete. The repair separates observed/dropped accounting from successful writes and commits only each written prefix to retained totals and events.
