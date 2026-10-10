# FINDING-0012: TUI review navigation and raw mode could show stale or incomplete data

Severity: high

Blocking: true

Disposition: fixed in the final reviewed candidate.

The TUI initially navigated only the loaded page, refreshed offset zero without an in-flight guard, rendered an empty file panel as `no changes`, left stale requests wedged, and toggled raw mode to the cached `ProductRunSnapshot.diff` rather than the fresh summary-bound raw endpoint. The repair pages forward and backward under the negotiated summary identity, clears matching stale requests, derives scroll state from the loaded diff page, and always binds raw mode to the current candidate/revision/digest.
