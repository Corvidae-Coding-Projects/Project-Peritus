# FINDING-0028: Terminal prompt test assumed one bounded event page contained the whole marker

Severity: low

Blocking: true

Disposition: fixed in the final reviewed candidate.

The terminal attachment test polled once and required one returned output event to contain the complete `NAME?` marker. `TerminalRegistry::poll` drains only one bounded event page, while the fixture emits 200 KiB before the marker; platform chunking can therefore place the marker on a later page or split it across events. The repair polls until a monotonic deadline, accumulates raw bytes across pages and events, detects the marker across chunk boundaries, fails early on process exit, and bounds failure diagnostics to the final 256 observed bytes. The adjacent terminal-settlement loop now uses the same 10-second deadline discipline instead of a fixed iteration count.
