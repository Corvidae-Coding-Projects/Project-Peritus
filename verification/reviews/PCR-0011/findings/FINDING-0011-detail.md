# FINDING-0011: Updater root success could abandon a descendant Job process

Frozen predecessor 51274d823 relied on process-wrap's KillOnDrop marker for Windows Job cleanup. The pinned wrapper temporarily takes its wrapper list before JobObject checks that marker, so kill-on-close was not enabled. A successful root could exit while leaving a descendant alive.

The final candidate waits for the root and then explicitly calls the outer JobObject child's start_kill before reporting success; Drop requests the same full-job termination best-effort. Native regressions cover cancellation and successful-root-exit descendants with readiness/completion markers. The complete local launcher suite passes; the exact final Windows H0 run passed all 16 native tests and the complete conformance case.
