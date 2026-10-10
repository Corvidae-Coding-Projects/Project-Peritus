# FINDING-0003: Verus-only product-runner build attempted a production-only workspace media export

Severity: high

Blocking: true

Disposition: fixed in the final reviewed candidate.

`workspace_media` was correctly absent from the verification-only build, but its public re-export lacked the matching configuration guard. Strict Verus builds of `peritus-product-runner` therefore failed with an unresolved import even though ordinary builds were valid. The repair guards the re-export with the same `not(verus_only)` boundary as the module and preserves the ordinary implementation contract.
