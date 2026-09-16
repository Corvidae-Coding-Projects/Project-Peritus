# FINDING-0001 — restart could replay indeterminate provider or tool work

Original severity: high, blocking.

Disposition: fixed.

A provider or tool effect could complete before the corresponding product-run record write failed. Restoring that nonterminal record previously restarted execution automatically, which could repeat an effect whose durable outcome was unknown.

The reviewed candidate cancels live execution after a persistence failure, restores unfinished runs as `RecoveryRequired`, and requires an explicit retry before any new provider or tool request. The regression forces a failure after an effect and proves daemon restart alone does not increase provider or tool request counts.
