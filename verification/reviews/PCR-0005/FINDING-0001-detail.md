# FINDING-0001 — host permission admission

Original severity: high, blocking.

Disposition: fixed.

The frozen implementation now has an authenticated request/intent capability map at the session boundary and repeats admission at the direct workbench dispatch and actual run/preview/tool boundaries. Effective permissions are intersected with host policy and read-only branch restrictions. Read, write, process, and network requirements are checked before folder access, enrollment, effect receipts, checkpoints, process launch, provider request, or tool effect; only existing-resource observation and cleanup retain narrow exceptions after revocation.

The retained concern that direct workbench and developer-tool operations could execute outside one current permission system is therefore closed for this candidate.
