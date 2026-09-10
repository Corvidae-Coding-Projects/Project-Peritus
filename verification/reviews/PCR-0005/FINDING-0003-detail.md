# FINDING-0003 — graphical qualification through the real path

Original severity: high, blocking.

Disposition: fixed.

The frozen graphical qualification path requires the exact governed goal/run, source and build identities, an owned ready process that has stopped or exited, a completed observed interaction, a later captured nonempty image artifact, and a later behavior check whose note uniquely equals the graphical criterion description. Stale source/build, foreign ownership, wrong or duplicate criterion text, missing interaction, pre-interaction capture, and screenshot-only/build-only evidence do not qualify.

The TUI constructs host-observed source and build identities for managed and direct-folder launches. Separate native Linux X11 tests exercised a controlled graphical program and the composed image-to-build-to-Tetris interaction/capture/feedback flow. The retained binding and real-path concern is closed for the offered native path.
