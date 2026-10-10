# FINDING-0007: ConPTY children inherited redirected runner standard handles

Frozen predecessor 51274d823 failed native Windows ConPTY readiness, input, resize, and cancellation tests. The child startup record did not set STARTF_USESTDHANDLES with explicit null standard handles, so redirected parent runner handles replaced the pseudoconsole's standard-handle behavior.

The final candidate sets STARTF_USESTDHANDLES and explicitly nulls all three standard handles while retaining the pseudoconsole attribute, suspended creation, Job assignment before resume, and native lifecycle ownership. GNU Windows strict checking passes; the exact final Windows H0 run passed all 16 native tests and the complete conformance case.
