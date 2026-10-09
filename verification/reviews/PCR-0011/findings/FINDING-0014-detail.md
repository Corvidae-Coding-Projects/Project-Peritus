# FINDING-0014: Sandbox-helper ConPTY forwarding tasks are detached from teardown

The existing sandbox-helper `TerminalAttachment::start_io` detaches stdin-forwarding and control/resize threads, copying the HPCON into the control task. `Drop` closes the pseudoconsole without owning or joining either thread.

The start_io and Drop blocks are unchanged from source base 96; final23 edits in the file affect the HPCON attribute ABI and add probes. This is the restricted sandbox-helper ConPTY path, separate from L670's new `peritus-process/src/platform/pty/windows/*` C2 owner, which joins its close/drain task. L605's private capability probe, L606 preparation recovery, and L607 projection do not promise joining these successful-session helper tasks. Approval does not claim every native sandbox support thread is joined.
