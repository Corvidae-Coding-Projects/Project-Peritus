# Stage 4 local module selection and ordered job checkpoints

Final bounded qualification is recorded in `../STAGE4-REPORT.md` and `../inventory/stage4-targets.json`; earlier worker-status notes describe their historical snapshot.

This target operates actual public `Definition`, `Registry` and `Job` APIs against owned temporary
directories. Selected implementation read completely: module `lib.rs`, `definition.rs`,
`registry.rs`, `jobs.rs`; package/source navigation and manifest declarations were inspected.
These are declarative package and durable job boundaries. They do not establish native plugin
execution, remote market downloads, source-build process recovery, or every product coordinator
handoff. Source-package installation and hosted stage execution remain distinct responsibilities.

## Package model

A small independent selection record stores the expected version, resource bytes, enablement,
damage status and an opaque checked digest handle. The digest is used as a lookup identity; this
lane does not duplicate the package's canonical manifest encoder as its oracle. Actual selected
version and complete resource bytes are compared independently, and altered installed bytes must
fail validation and appear as an individual listing error. Reinstalling the same or changed
package must restore a valid selection initially disabled. Removing a selection snapshots and
then checks the exact retained manifest/resource bytes; this directly exercises retained package
bytes for historical jobs. It is not a scan of every possible older version or source-build output.

Fixtures contain one actual on-disk manifest and resource, validated and copied through ordinary
installation. Registry roots are explicit; user directories and credentials are not used.
Repeated construction of Registry owners performs real saved-selection readback. Invalid
enable/remove operations must agree with the known selection, and damaged packages cannot be
loaded as runnable. Actual role/tool execution is not inferred from listing enablement.

## Job model and fault boundaries

An ordered list stores exact committed text for one to four declared stages. Complete accepts
only the next stage with nonempty output; rejection preserves the exact durable checkpoint.
Dependency reads compare independently sliced UTF-8 bytes, offsets, SHA-256, retained lengths and
continuations. Undeclared/missing output and non-boundary offsets reject. A second real Job owner
must fail to acquire the held lock; fresh owners after close must recover exactly the same prefix.
Separate task/revision identities must not inherit another job's completed stages.

The checkpoint-publication failure fixture replaces `job.json` with an obstructing directory
while retaining the previous checkpoint separately. It observes the exact newly published orphan
output after the failed next-stage call and verifies that memory did not advance. The old
checkpoint is restored, a fresh owner must recover only its committed prefix, and a retry with
different output must produce exactly the intended final result. Thus orphan non-adoption is
supported by actual file bytes at the intended window, not merely a generic error.

Other cases corrupt or remove an actual committed output, require reopen to reject without
rewriting checkpoint bytes, restore the correct output and continue successfully. These are
controlled single-process publication/readback failures; no actual process kill, power loss or
arbitrary concurrent installer/job schedule is claimed.

## Calibration and limits

`stage4-modules.patch` weakens the actual stage-order/empty-output guard. A fixed wrong-stage
witness and fresh generated histories must expose the invalid checkpoint transition. Successful
compilation, two generated seeds, exact restoration and rebuilt saved-witness replay are required.

Generation mixes job histories, registry histories, checkpoint failure/retry and damage/recovery
families. Text includes Unicode and whitespace, stage selectors include past and future indices,
and rejected operations are followed by continued observations. The ordinary job history caps at
96 operations, registry at 64; minimum fault families have fixed bounded schedules. Exact case
and source identity, not just the seed or configured history flag, determine the exercised scope.
Final reports record actual installs, owner conflicts, retained removals, commits, exact pages,
reopen operations and reached failure/retry windows. Linux is the current native environment;
other OS locking/persistence behavior and hostile package path races remain separate coverage.

The follow-up generator and oracle review is recorded in [state-strengthening.md](state-strengthening.md).
