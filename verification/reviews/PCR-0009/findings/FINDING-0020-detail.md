# FINDING-0020: Preview behavior checks returned a boolean after loading the complete output

Severity: high

Blocking: true

Disposition: fixed in the final reviewed candidate.

The selected L193 repair still read a complete unbounded live spool or finalized artifact into memory and returned only a boolean. It did not retain the matching stream, exact byte range, matched digest, source identity, or complete observed-prefix identity required to qualify behavior after restart. The repair performs a bounded-memory 64 KiB streaming search and returns typed exact match evidence for live and finalized sources.
