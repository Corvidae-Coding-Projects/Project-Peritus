# FINDING-0014: Git history rejected subjects longer than its advertised continuation model

Severity: high

Blocking: true

Disposition: fixed in the final reviewed candidate.

History exposed 64-byte subject continuation but rejected any valid commit subject above 4,096 bytes during parsing. This made the continuation protocol unreachable for larger repository data. The repair removes the unrelated aggregate subject cap while retaining bounded response chunks and exact commit paging.
