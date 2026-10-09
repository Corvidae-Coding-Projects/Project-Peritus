# FINDING-0004: Ordinary and Verus product-runner APIs diverged for attachment and preview evidence

Severity: high

Blocking: true

Disposition: fixed in the final reviewed candidate.

The verification build used opaque or incomplete attachment and command-runtime projections: the real attachment read request/response types and the preview output range/match methods were absent or shaped differently. This prevented strict formal compilation from checking the same public API that production callers use. The repair moved the real attachment DTOs into a shared module, added the exact uninhabited command-runtime projections, and added API-parity checks.
