# FINDING-0010: Diff page byte budgeting could skip a line and emit an invalid wire preview

Severity: high

Blocking: true

Disposition: fixed in the final reviewed candidate.

When a line exceeded the remaining encoded page budget, the parser could skip it and admit a later short line, producing an incorrect continuation cursor with omitted or duplicated content. In addition, a 1,024-byte preview plus the three-byte ellipsis produced 1,027 bytes while the wire decoder accepted only 1,025. The repair permanently exhausts line admission after the first budget refusal, tracks encoded preview bytes exactly, and aligns the wire cap with the constructed preview.
