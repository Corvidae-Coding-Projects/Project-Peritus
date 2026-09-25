# FINDING-0004 — public error ownership and schema contradicted runtime behavior

Original severity: medium, blocking.

Disposition: fixed.

Several product errors identified the wrong subsystem or recovery action. Final review also found that the runtime's new `provider` and `workspace` subsystem tags were absent from the authoritative public schema, so valid daemon errors violated the published client contract.

The reviewed candidate maps Git prerequisites to `Workspace` and unsupported provider effort to `Provider`, preserves the intended retry behavior, adds both values to the authoritative descriptor, regenerates JSON, TypeScript, and registry projections, and adds public payload and wire round-trip regressions for tags 10 and 11.
