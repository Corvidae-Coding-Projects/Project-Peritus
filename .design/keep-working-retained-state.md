# Keep working as retained state grows

The goal is durable continued operation across the former observation, working-entry, snapshot,
trace, run-record, and scope-journal ceilings. Provider constraints remain separate. Ownership,
source identity, pending-effect reconciliation, and cross-role isolation remain required.

History is durable data. A model view, read response, transaction, and physical I/O buffer are
pieces of that data. Their sizes may govern paging and scheduling; they must not exhaust the
logical history or make a valid retained run unrecoverable.

## Delivery

1. Remove default logical working-model quotas. Preserve and explicitly widen legacy persisted
   allocation policies when the host opens them. Share immutable content and observation pages;
   use indexed source lookup and nonquadratic graph validation. Existing canonical snapshots
   remain readable; new snapshots use wide logical counts and paged durable checkpoints.
2. Store source indexes and working snapshots as independently content-addressed pages. Publish
   their manifests and roots transactionally. A partial write cannot replace the prior checkpoint.
   Required source bytes and entry identities survive reopening. Page boundaries affect I/O,
   not admissible history size. Keep current host instructions and pending outcomes protected.
3. Stream trace recovery without a total-file cutoff; retain a checked checkpoint cursor so old
   observations need not be decoded again. Stream JSON run-record publication/readback with the
   existing fsync/rename handoff and no total-record quota.
4. Move in-place baseline inventory to indexed transactional storage, importing the existing
   JSON journal without changing its original before-images. Directory traversal uses actual
   ancestry and excludes links/private/build paths; count and depth do not determine authority.
5. Separate protocol field representation from provider capacity. Eliminate the former local
   text/argument/message quotas while keeping positive explicit caller-selected page bounds and
   native wire representability checks. A full view that exceeds an adapter envelope receives
   exact retrievable pages rather than losing durable entries.

Simply increasing all constants would retain the lifetime failure and amplify whole-history
copying. Deleting history would break references and recovery. The selected approach preserves
the logical data and changes storage/access granularity.

## Real-operation contract

Use saved original inputs and fresh processes. Cross 65,535 observations, 512 entries, 2-KiB
entry text, 32 links, 8-MiB snapshot bytes, 1-GiB trace bytes, 128-MiB valid run-record bytes,
64-MiB scope evidence, 4,096 directory entries, depth 64, and former protocol field/message
limits. Verify exact identity/content, old and new checkpoint recovery, no duplicated effects,
role/ownership rejection, interrupted publication, and bounded memory/I/O behavior. Exercise the
native client/daemon on disposable state and actual provider continuation where relevant.

Build with two Cargo workers, one build at a time. Run real interfaces and public library
consumers; do not create or run unit, mock, formal, or repository verification suites. Keep live
user workspaces and running jobs separate from disposable campaign data.
