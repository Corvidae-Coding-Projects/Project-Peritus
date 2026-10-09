# FINDING-0024: Old behavior evidence could retarget a newer edited goal

Severity: high

Blocking: true

Disposition: fixed in the final reviewed candidate.

Persisted behavior admission originally bound command, launch, and output but not the target goal generation. After a crash, an intervening goal edit could let old output qualify the current criterion by matching kind and description. The repair records goal ID, criterion index, user revision, and required input generation in the evidence binding and requires exact equality before publication.
