# FINDING-0013: Raw diff rendering was lossy and non-injective across chunk boundaries

Severity: high

Blocking: true

Disposition: fixed in the final reviewed candidate.

Whole-stream and per-line raw paths decoded arbitrary byte chunks with `String::from_utf8_lossy`, corrupting a UTF-8 scalar split at a 32 KiB boundary. The first byte renderer also mapped raw byte `0xC3` and the literal ASCII text `\xC3` to the same display. The repair uses one injective byte preview: newline remains structural, tab becomes `\t`, literal backslash becomes `\\`, and every nonprintable or non-ASCII byte becomes `\xNN`.
