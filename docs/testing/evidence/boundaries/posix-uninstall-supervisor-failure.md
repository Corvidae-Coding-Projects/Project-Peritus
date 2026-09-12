# POSIX uninstall supervisor failure

- Source: `fab1aa1dba476de0a2f7bafd9051e5d0b0ca2e43`
- Host: Fedora Linux, x86_64
- Boundary: Linux `systemctl --user` and macOS `launchctl` uninstall routing
- Execution: deterministic fake supervisor commands in a rootless disposable container
- Image: `docker.io/library/alpine:3.22@sha256:14358309a308569c32bdc37e2e0e9694be33a9d99e68afb0f5ff33cc1f695dce`
- Native coverage: Linux/macOS supervisor integration was not run

## Invariant and oracle

An installed supervisor definition means uninstall must stop or prove absence of the owned job before deleting package files. A supervisor access or stop failure must remain visible and retryable. An absent registration remains an idempotent uninstall.

The independent oracle checks subprocess status and directly reads the registration and installed binary after the injected failure. A sibling canary outside the disposable profile must retain its exact bytes. Each accepted baseline failure is reproduced from three newly populated fixtures, and a successful retry must then remove the owned files. Controller state is varied independently from registration-file state: loaded registrations without files must still be stopped, and a retry after daemon-reload failure must reconcile controller state again.

The harness uses rootless Podman or Docker with no scenario network, a read-only container root,
32-process and 256 MiB limits, and a 32 MiB temporary filesystem. The exact image manifest is
pinned and pulled before the campaign. The image's unchanged natural `/root` home is backed by a
fresh mounted profile. The harness never sets `HOME`, changes the test runner environment, or maps
the operator's profile. Repository input is read-only; fake commands and the campaign root are the
only other mounts. Each scenario receives a unique owned container name. Normal, failed, and timed
out runs force-remove that exact container within a bounded cleanup call and poll until inspection
confirms it is absent.

## Pre-fix reproduction

Command: `python3 packaging/test_posix_lifecycle.py -v`

The base scripts were restored temporarily for this replay. Linux and macOS logic each reproduced three times: the fake controller returned a nonzero status, the uninstaller returned zero, and the regression rejected the false success. Linux recorded only one controller call per fixture because the package registration had already been deleted before the intended retry.

## Fixed replay

Command: `PERITUS_CONTAINER_ENGINE=podman cargo xtask discovery-posix-lifecycle`

Result after the controller/file mismatch and containment expansions: 12 tests passed in 9.488
seconds. A controlled five-second hang reached its marker, timed out, and proved the exact named
container was absent after cleanup. Linux and
macOS controller failures each reproduced at the intended fault boundary three times, retained
package state, and succeeded on retry. The Linux absent-registration negative control passed from
an explicit controller `not-found` result. Loaded controller registrations without unit/plist files
were reconciled. A Linux daemon-reload failure stopped package deletion and the next invocation
repeated controller reconciliation. A generic macOS job-query failure was rejected rather than
treated as absence. The teardown census removed the temporary root and confirmed the sibling canary
before removal.

## Classification and remaining coverage

This is a product invariant violation repaired at its cause. The tests exercise shell control flow with fake supervisors on Linux. Real `systemctl --user`, launchd, Windows Task Scheduler, locked files, ACLs, and native process ownership remain separate platform qualification work.
