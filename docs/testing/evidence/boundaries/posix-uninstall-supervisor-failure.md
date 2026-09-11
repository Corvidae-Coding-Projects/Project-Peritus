# POSIX uninstall supervisor failure

- Source base: `f75f5dee3976b1c72b85e78fa1d6c50475d5d8fa`
- Host: Fedora Linux, x86_64
- Boundary: Linux `systemctl --user` and macOS `launchctl` uninstall routing
- Execution: deterministic fake supervisor commands in a temporary synthetic child profile
- Native coverage: Linux/macOS supervisor integration was not run

## Invariant and oracle

An installed supervisor definition means uninstall must stop or prove absence of the owned job before deleting package files. A supervisor access or stop failure must remain visible and retryable. An absent registration remains an idempotent uninstall.

The independent oracle checks subprocess status and directly reads the registration and installed binary after the injected failure. A sibling canary outside the disposable profile must retain its exact bytes. Each accepted baseline failure is reproduced from three newly populated fixtures, and a successful retry must then remove the owned files. Controller state is varied independently from registration-file state: loaded registrations without files must still be stopped, and a retry after daemon-reload failure must reconcile controller state again.

The harness supplies a temporary fixture profile as the child process's `HOME` environment entry. It does not change the test runner's environment or the operator's natural `HOME`; subprocesses receive a new explicit environment mapping containing only the fixture profile, fake-command `PATH`, and synthetic fault controls.

## Pre-fix reproduction

Command: `python3 packaging/test_posix_lifecycle.py -v`

The base scripts were restored temporarily for this replay. Linux and macOS logic each reproduced three times: the fake controller returned a nonzero status, the uninstaller returned zero, and the regression rejected the false success. Linux recorded only one controller call per fixture because the package registration had already been deleted before the intended retry.

## Fixed replay

Command: `python3 packaging/test_posix_lifecycle.py -v`

Result after the controller/file mismatch expansion: seven tests passed. Linux and macOS controller failures each reproduced at the intended fault boundary three times, retained package state, and succeeded on retry. The Linux absent-registration negative control passed from an explicit controller `not-found` result. Loaded controller registrations without unit/plist files were reconciled. A Linux daemon-reload failure stopped package deletion and the next invocation repeated controller reconciliation. A generic macOS job-query failure was rejected rather than treated as absence. The teardown census removed the temporary root and confirmed the sibling canary before removal.

## Classification and remaining coverage

This is a product invariant violation repaired at its cause. The tests exercise shell control flow with fake supervisors on Linux. Real `systemctl --user`, launchd, Windows Task Scheduler, locked files, ACLs, and native process ownership remain separate platform qualification work.
