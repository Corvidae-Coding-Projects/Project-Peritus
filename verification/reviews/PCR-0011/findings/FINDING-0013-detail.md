# FINDING-0013: Post-success Windows proxy teardown consumes the retry owner

`WindowsSession::release_proxy` takes the proxy owner while cleanup is Pending. If shutdown fails or workers do not join, it records RetryRequired but no longer retains an owner that a later release can call.

This post-success session-release behavior is byte-identical at actual base 909, source base 96, and final candidate. L606 covers pre-consumption validation and retention/retry of owners when preparation fails after optional channel selection; it does not promise general retryability of an already-running session's eventual teardown. L605, L607, and L670 do not cover this proxy release. Approval does not claim fully retryable Windows sandbox teardown.
