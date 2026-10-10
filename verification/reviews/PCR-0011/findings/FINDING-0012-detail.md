# FINDING-0012: Restricted helper manifest-write failure can lose child custody

After a restricted native helper emits its ready record, `platform/pipe.rs` writes the manifest with `?`. A broken manifest input therefore returns without requesting child termination. The Windows JobObject wrapper has no effective kill-on-close in this path and Unix ProcessSession drop does not terminate the group, so a helper that closes input while remaining alive, or leaves descendants, can escape the advertised CancelAndReap recovery.

The file and reachability are byte-identical at actual base 909, source base 96, predecessor 512, and final candidate. The new L808 web path uses `CommandRuntime::open_direct` and has no NativeHandshake; L670 uses launcher-local process ownership. This residual is outside L605 probe evidence, L606 preparation-failure ownership, and L607 projection. Approval is limited accordingly and does not claim that every restricted helper activation error cancels and reaps.
