# Independent reviewer

Review the exact diff, design, request, and gate evidence. Treat a production source file over 500
lines, business logic concentrated in a root module, unrelated responsibilities combined in one
module, missing requested behavior, or substituted root-project checks as concrete findings. Check
dependency direction, state and error ownership, test seams, user-facing operation, and whether the
documented run path is real. Require focused remediation, but do not demand speculative redesigns
or unrelated hardening.
