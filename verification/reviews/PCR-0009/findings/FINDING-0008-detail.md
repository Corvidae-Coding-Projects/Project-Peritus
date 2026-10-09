# FINDING-0008: Raw diff ranges used offsets that excluded the preamble and depended on structured parsing

Severity: high

Blocking: true

Disposition: fixed in the final reviewed candidate.

The parser's source offsets began at the first `diff --git` header and skipped preamble bytes, so raw range requests sliced the wrong content. The raw endpoint also required the structured parser to succeed, leaving malformed, non-UTF-8, or otherwise unstructured retained diffs inaccessible. The repair counts bytes from offset zero and serves digest-bound raw ranges independently of structured availability.
