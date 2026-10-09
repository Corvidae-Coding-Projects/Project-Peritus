# FINDING-0018: Active preview evidence probed streams that do not exist for the launch mode

Severity: high

Blocking: true

Disposition: fixed in the final reviewed candidate.

Active output search tried stdout, stderr, and terminal in sequence and propagated a missing-spool error. Pipe launches therefore errored when a miss reached terminal, and PTY launches errored on stdout before checking terminal. The repair derives the exact stream set from the launch I/O mode: stdout/stderr for pipes and terminal for PTY.
