# FINDING-0001: Non-Unix builds compiled Unix-only sandbox path codecs

Severity: high

Blocking: true

Disposition: fixed in the final reviewed candidate.

The precursor candidate imported Unix `OsStrExt` and `OsStringExt` unconditionally in `crates/runtime/peritus-sandbox-macos/src/canonical.rs`. A native Windows build therefore failed before the sandbox manifest codec could be qualified. The repair gates the native-byte codec to Unix and uses an exact UTF-8 representation on non-Unix targets, rejecting values that cannot be represented rather than silently changing them. This was a release blocker because the selected scope requires the repository's native-platform checks to remain buildable.
