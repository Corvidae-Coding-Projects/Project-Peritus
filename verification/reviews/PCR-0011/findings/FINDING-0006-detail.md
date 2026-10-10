# FINDING-0006: Idempotent evidence publication flushed a read-only Windows handle

Frozen predecessor 51274d823 failed native Windows evidence publication: the exact-existing path reopened the destination read-only and then called sync_all, which requires a writable Windows handle. The second idempotent publish returned access denied.

The final candidate requests read/write access on Windows without create, append, or truncate, hashes and counts the exact existing bytes, and flushes that same handle. Unix behavior and no-clobber publication are unchanged. Local evidence suites and strict Windows GNU checking pass; the exact final Windows H0 run passed all 16 native tests and the complete conformance case.
