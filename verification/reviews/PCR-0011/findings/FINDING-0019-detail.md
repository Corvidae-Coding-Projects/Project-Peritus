# FINDING-0019: Completed macOS updater groups could report a false cleanup failure

On macOS, a completed updater root could remain visible only as a zombie while `killpg` returned `EPERM`, causing cleanup to report failure even when no live group member remained. The final candidate uses `waitid(WNOWAIT)` and accepts this macOS-only case only when an XNU process-group snapshot proves the owned exited root is the sole member. Empty, erroneous, different, truncated, or multi-member snapshots still fail. Native tests cover the singleton and live-descendant cases.
