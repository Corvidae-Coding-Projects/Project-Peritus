# KernelCommand Copy consumer review

Adding Copy to the closed, scalar-only KernelCommand vocabulary exposed redundant clone
calls in two production paths and ten test/support files. The corrections pass or dereference
the existing command directly and use `copied()` for an optional borrowed command. Parent
review checked every changed line and all twelve source hashes; these substitutions preserve
the same command fields and ownership behavior.

The ten affected packages passed all-target, all-feature Clippy with warnings denied.
Their combined test command reached and passed the changed-path targets in nine packages,
then stopped at a workspace library Unix-socket test with sandbox EPERM. The separate full
workspace suite ran with the required local syscall access and passed 12 library tests,
13 authorized-gateway tests, one production-conformance test, two registration tests, and
three doctests. The combined ten-package command itself did not pass. Its raw failure and
the successful full workspace rerun are retained separately. Existing ignored product-runner
tests remain visible in the output; this increment added no test suppression.

The exact commands and target inventory accompany the source manifest. Formatting passed.
This is ordinary compatibility/lint validation of the Copy substitutions, not a new formal
guarantee or a final all-workspace CI result.
