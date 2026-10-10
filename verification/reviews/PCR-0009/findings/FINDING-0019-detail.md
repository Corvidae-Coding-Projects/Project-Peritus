# FINDING-0019: Untimed preview helpers encoded contradictory zero-duration authority

Severity: high

Blocking: true

Disposition: fixed in the final reviewed candidate.

The optional preview timeout reached the process plan as `None`, but authority records projected it through scalar zero. The resulting plan had `UntilCancelled` and no wall resource while committed authority facts used incompatible budget/lease phases, causing capture-helper authorization to fail before execution. The repair gives no-deadline launches an exact settled zero ActiveEffectMilliseconds reservation; timed launches retain a held reservation at least as large as the wall limit.
