# FINDING-0001 — persistence diagnostic regression assumed Unix error text

Original severity: low, blocking.

Disposition: fixed.

The persistence diagnostic regression constructed raw OS error 13 and required the literal text `Permission denied`. That description is platform-specific: Windows maps raw OS error 13 to a different native error. The assertion therefore failed in both Windows daemon shards even though the diagnostic preserved the supplied cause correctly.

The reviewed candidate captures the exact rendered text from the same `std::io::Error` before moving it into `ProductRunServiceError::persistence`. It then verifies that the diagnostic contains that complete platform-native cause while retaining the existing operation-context and recovery-action assertions. Production code is unchanged.
