# FINDING-0002 — rejected chat open was invisible in the TUI

Original severity: medium, blocking.

Disposition: fixed.

When the daemon rejected `ChatOpen`, the TUI did not present the daemon's actionable diagnostic, leaving the user with no useful explanation or recovery step.

The reviewed candidate restores navigation and the composer, then displays the protocol error's actionable message. A focused TUI regression exercises the rejected response and asserts that the daemon diagnostic is visible.
