# H1 resilience qualification

H1 is a release qualification, not a component self-test. Its subject is one exact integrated
Peritus release-candidate build, identified by digest and exercised through real daemon, storage,
workspace, provider, tool, worker, and restart controls. A fake subject may validate the harness;
it cannot produce release evidence.

The `peritus-resilience` crate owns the runtime-neutral qualification model. A release adapter owns
all operating-system and implementation-specific effects. The runner always creates a new isolated
subject for each scenario, executes scenarios in bytewise identifier order, reconciles the result,
consumes the subject through cleanup, and binds the complete report to a canonical SHA-256 digest.

## Production coverage

The immutable catalog has 43 scenarios:

- before-durable-commit and after-durable-before-ack crashes for journal, blob, snapshot, lease,
  patch, gate, and promotion boundaries;
- corruption or hash divergence in the journal, blob, snapshot, projection, acceptance evidence,
  and harness-promotion state;
- disk exhaustion at journal append, blob finalization, and snapshot commit;
- provider, tool, and worker death while work is owned;
- provider, tool, and worker retry-budget exhaustion;
- daemon death during all eleven active E0 phases, from pending writer publication through pending
  kernel acceptance;
- host reboot with an outstanding effect, after durable commit before acknowledgement, and during
  startup reconciliation.

The adapter must prove that each requested failpoint was actually reached. Merely restarting an
idle daemon or returning the expected enum does not satisfy the subject contract.

## Required direct observations

Each case reports the prepared journal head, exact armed fault, documented recovery outcome,
terminal/acceptance truth, journal and referenced-object integrity, projection state, exact
corruption detection, mutation admission, ownership reconciliation, orphan scan, retry counters,
resource counters, temporary-object count, and a canonical six-step lifecycle chronology.

Six content-addressed evidence classes are mandatory per case: fault control, journal integrity,
recovery, ownership/orphan scan, resource accounting, and final authoritative state. The standard
native adapter retains each raw evidence file under a private, subject-addressable artifact root
and verifies its relative path, regular-file type, byte count, and SHA-256 before the deterministic
report retains its typed ID and digest.

## Non-bypassable verdict

`Ready` is derived only when the built-in production profile runs every case and every case passes
execution, private invariants, evidence coverage, and cleanup. A caller cannot construct a ready
report or convert a custom catalog into production evidence through the public API.

The invariant evaluator withholds readiness for false or contradictory acceptance, journal
divergence after a crash, missed corruption, mutation admitted against corrupt authority,
unverified referenced objects, leaked temporary objects, absent ownership scans, non-conserving
ownership counts, unaccounted work, remaining orphans, retry or resource overruns, incomplete
evidence, noncanonical chronology, and incomplete cleanup. Indeterminate work is allowed only when
it is explicitly counted and the run remains non-success; it can never support acceptance.

## Cancellation and cleanup

The runner owns a cancellation token for the current fresh subject. Dropping the runner future
cancels that token before dropping the in-flight operation. A completed run calls asynchronous
factory cleanup once for every created subject and validates its resource and ownership result.
Adapters must also use RAII for process trees, temporary roots, failpoint registrations, quota
devices, and reboot controllers because Rust cannot await cleanup from `Drop`.

No spawned operation may be detached. Cleanup failure, panic, unsupported fault control, or an
unobserved terminal result is infrastructure failure and therefore `NotReadyForProduction`.

## Release adapter responsibilities

`NativeResilienceFactory` supplies the reviewed process boundary. It copies and re-digests the
controller executable inside every fresh subject, launches it with cleared environment state,
owns the complete process tree, applies stage duration and output limits, and cancels synchronously
when the runner future is dropped. One controller remains alive for all four stages so that its
external host or VM control session can survive the fault it invokes. Cleanup is complete only
after the controller exits, all descendants are gone, the private root is removed, and the raw
artifact root remains available for H4.

The controller is invoked as follows; values following each option are supplied by the adapter:

```text
CONTROLLER --serve \
  --candidate-executable FILE \
  --subject-root ROOT \
  --artifact-root ROOT \
  --instance-id ID \
  --subject-id ID \
  --build-sha256 SHA256 \
  --executor-sha256 SHA256
```

It reads one compact JSON request per line from standard input and writes exactly one JSON response
line to standard output. Progress and diagnostics belong on standard error. Requests and responses
follow [`native-controller-request-v1.schema.json`](../resilience/schemas/native-controller-request-v1.schema.json)
and [`native-controller-response-v1.schema.json`](../resilience/schemas/native-controller-response-v1.schema.json).
Each response repeats the stage, sequence, fresh instance, scenario, and exact inner-request digest.
The four stages are `prepare`, `inject`, `recover`, and `cleanup`; the controller exits after its
cleanup response. Unknown fields, stale identities, malformed values, excessive output, timeouts,
unexpected exit, missing evidence, false digests, and surviving descendants fail closed.

## Production controller boundary

The standard production controller qualifies the exact staged `peritusd` executable. It does not
link the daemon's component crates and reproduce their behavior inside the test process. For native
process, dependency, storage, and quota cases, the controller invokes a narrow `peritusd`
qualification-admin surface against a fresh disposable state root. That surface exposes fixed
scenario controls and observations; it does not expose an arbitrary command runner or a way to
construct a successful H1 response.

The persistent controller writes each digest-bound stage request beneath the private subject root,
starts the staged candidate with that request, observes the required checkpoint or terminal state,
and independently retains the candidate output and platform observations. Expected candidate death
is accepted only after the named checkpoint was observed. Starting and stopping an idle process is
not fault evidence.

Host-reboot cases use an external disposable-VM driver owned by the controller. The driver must
bind the guest image, candidate digest, request digest, boot identity, pre-reboot checkpoint, and
post-boot recovery observation. The controller never reboots the developer or CI host, and it does
not substitute a process or container restart for a host reboot. If the reviewed VM driver or its
required image is unavailable, the production campaign is honestly not ready rather than silently
downgrading those cases.

### Current implementation state

The checked-in Rust `peritus-h1-controller` now implements both genuine journal commit routes. For
`h1.crash.journal.before`, the staged daemon builds the production append plan, publishes a
request-bound checkpoint before submission, and is killed. The recovery process runs the production
journal integrity scan and requires zero committed events, aggregate heads, pending outbox claims,
or external effects. For `h1.crash.journal.after-before-ack`, the controller waits for the real
durable effect-before-ack checkpoint, kills the process, and then requires the same staged bytes to
reconcile that exact effect before acknowledging the new live fence. The controller independently
checks the SQLite journal and external effect state in both cases.

Both blob commit routes are also real. Before publication, the staged daemon holds a fully written
production `ArtifactWriteHandle` across its checkpoint; killing it leaves temporary bytes for the
store's restart recovery to remove. After publication, the daemon has finalized and verified the
content-addressed object, committed its metadata, and added a durable evidence-owned reference.
Recovery re-hashes the object, reads its exact bytes, verifies the reference roots, checks that no
temporary file remains, and confirms that the authoritative journal is still valid and unchanged.

Both snapshot commit routes use the production structured Git adapter. Before publication, the
staged daemon creates a changed candidate tree and holds its exact typed identity without invoking
snapshot creation; recovery requires no retained Peritus snapshot ref or manifest. After
publication, the daemon creates the real deterministic synthetic commit and compare-and-swap
retained ref, then persists its canonical manifest. Recovery decodes that manifest, reopens the
snapshot through `peritus-git`, and requires the exact commit, tree, reference, and manifest digest.
The controller separately inspects the loose ref and manifest bytes on both sides of the crash.

Both lease routes use the production B1 reducer and C0 journal adapter. Before commit, the staged
daemon holds the exact move-only `LeaseCommitRequest` in memory and recovery requires no event,
aggregate head, or lease projection. After commit, the lease event and projection are installed in
one journal transaction. A fresh process integrity-checks the journal and reopens the exact state
key, revision, digest, and producing position; the controller requires those facts to match the
pre-crash committed receipt.

Both patch routes use the production workspace-bound `PatchPlan` and recoverable filesystem
transaction adapter. Before commit, the staged daemon holds the checked plan without creating
transaction state or changing the target. After commit, the exact postimage and installed-manifest
receipt are durable. A fresh process re-hashes the target and requires no pending transaction
metadata; the controller independently inspects the same regular file, byte count, and digest.

Both gate routes use a production contract-bound D1 `GatePlan`, accepted start transition, and C0
commit adapter. Before commit, the accepted transition remains only in the killed process. After
commit, the gate event and complete state checkpoint are installed atomically. Fresh recovery
rebuilds the exact successor from the journal and verifies its plan and state digests, checkpoint
digest, revision, and producing position.

Both promotion routes use the production F0 reducers, artifact dependencies, canonical frames,
approve-once authority, and C0 atomic activation adapter. Before commit, the final accepted campaign
and production-pointer transitions remain only in the killed process; recovery finds the seeded
promotion-review campaign, pending pointer, and unconsumed approval unchanged. After commit, one
transaction installs both events, both complete checkpoints, and approval consumption. Fresh
recovery requires the exact promoted campaign, active pointer generation, authorization digest,
approval revision, event count, and aggregate heads with no partial state.

The projection-corruption route uses the production startup catalog and SQLite projection store.
It changes the active payload bytes while preserving their recorded digest, proves startup detects
the mismatch, and then invokes recovery in a fresh daemon process. Recovery must atomically install
a second generation with the exact genesis-rebuilt payload and digest, leave the authoritative
journal unchanged, and prove the repaired projection is reusable on the next startup.

The journal-corruption route commits a real D1 gate event and complete checkpoint, changes its
stored frame without changing the recorded frame digest, and invokes the full production daemon
startup path in a fresh process. Startup must return typed corrupt state with read-only guidance
before allocating authority or binding an application principal. A second direct integrity scan
must still diagnose the same corrupt frame, and all authoritative row counts must remain unchanged.

The blob-corruption route publishes a real finalized artifact and durable evidence-owned reference,
then changes the active object bytes without changing their recorded digest. A fresh process opens
the production artifact store, which must durably classify and quarantine the divergent bytes,
retain the audit reference, and deny further reads or references. The controller independently
checks the active and quarantine namespaces and the healthy journal.

The snapshot-corruption route publishes a real synthetic commit, canonical manifest, and retained
Git ref, then redirects only that ref to the baseline commit. A fresh process must detect the
manifest/ref divergence and atomically move the observed value from the active namespace to the
quarantine namespace. Repeating recovery must return the same containment observation. The
controller independently checks the loose active and quarantine refs, unchanged manifest digest,
and healthy journal.

The acceptance-evidence corruption route admits a real portable evidence record whose revision,
journal frame, event, command, artifact set, and causal bindings all come from production stores.
It changes only the durable record bytes while preserving the indexed identity. A fresh candidate
process opens the production evidence store, atomically copies every raw indexed field and the
corrupt bytes into a digest-bound quarantine, and denies the quarantined identity to readers. The
controller independently verifies the retained bytes and quarantine digest in SQLite, and a second
startup proves containment is idempotent without changing the authoritative journal.

The harness-promotion corruption route commits the real F0 campaign, production pointer, and
approve-once activation transaction, then publishes both production evolution directives. It
changes only the stored harness-activation evidence bytes. A fresh process must quarantine that
row while preserving readable campaign evidence, all 16 committed events, all four aggregate
heads, and exact replay of the promoted production pointer. The controller independently verifies
the evidence, outbox, pointer-state, and quarantine rows in SQLite, then requires a second startup
to produce the same single containment observation.

The provider, tool, and worker death and retry-exhaustion routes use the production D3 scheduler
and C0 journal adapter. Provider attempts execute the staged daemon through
`TokioProcessTransport`. Tool attempts use the ordinary grounded, receipt-backed
`WorkspaceDeveloperTools::run_command` path after listing and reading the workspace. Worker
attempts create a task owned by `WorkerSupervisor` and observe its bounded shutdown abort. Each
failure becomes a real retryable scheduler transition. Fresh replay either requeues the exact work
after one dependency death or retains terminal exhausted non-success after the configured attempt
ceiling. The controller checks the replayed state digest, attempt and event counts, aggregate head,
empty reservation set, effect digest, and tool receipt bytes.

The blob-finalization disk route uses the production artifact store's durable logical quota rather
than filling the developer host. Two exact owned writers are admitted against the same available
capacity. The first finalization consumes the quota; the second publishes its bytes, loses the
catalog quota race, and exercises the store's real rollback path. Fresh-process recovery requires
the rejected object and metadata to remain absent, the admitted referenced object to verify, used
bytes to equal the quota, the temporary namespace to be empty, and the journal to remain healthy.

The journal-append disk route applies SQLite's real page ceiling to the active production journal
connection at its current allocation, then submits a 2 MiB exact event frame through the ordinary
append transaction. The stage requires `SQLITE_FULL` and verifies the command, event, and aggregate
head are all absent. A separate candidate process reopens the same database and repeats those
absence and integrity checks, so the route proves atomic rollback without requiring host-wide disk
exhaustion or pretending a connection-local fault setting survives restart.

The snapshot-manifest disk route fills a valid production artifact-store quota with one referenced
object, then creates and retains an exact Git snapshot through the ordinary workspace publication
boundary. Manifest finalization must reject the new object and compensate by releasing the exact
unpublished snapshot reference. A separate candidate process verifies the reference is absent,
the admitted filler remains exact and referenced, no temporary file survives, and the production
journal remains healthy.

All eleven daemon-lifecycle routes use reducer-produced production E0 states and the ordinary C0
durability adapter. The staged daemon commits the shortest legal prefix ending at the requested
writer, gate, review, fixer, revision, evaluation, or kernel-acceptance phase, publishes a bounded
checkpoint, and is killed by the external controller. A fresh staged process loads every canonical
event and the installed state frame, replays the prefix, and requires exact equality with the
expected state, phase, state digest, event count, aggregate head, active-child set, open handoff,
checked fixer proposal, and B2 certificate where applicable. This qualifies actual process death
and restart without pretending that an idle restart reached an active lifecycle checkpoint.

Fresh focused diagnostics retained passing one-case reports and six raw evidence files under
`/home/doll/.local/state/peritus/qualification/h1/journal-before.YP5d5R` and
`/home/doll/.local/state/peritus/qualification/h1/journal-after-ack.sCtlT9`. Both reports deliberately
use the `custom` profile and `not-ready-custom-catalog` verdict. The blob reports are retained under
`/home/doll/.local/state/peritus/qualification/h1/blob-before.b7G9TE` and
`/home/doll/.local/state/peritus/qualification/h1/blob-after-ack.UZrw1b`. The snapshot reports are
retained under `/home/doll/.local/state/peritus/qualification/h1/snapshot-before.cZLcEc` and
`/home/doll/.local/state/peritus/qualification/h1/snapshot-after-ack.46KKAh`. The lease reports are
retained under `/home/doll/.local/state/peritus/qualification/h1/lease-before.UFyq0t` and
`/home/doll/.local/state/peritus/qualification/h1/lease-after-ack.zurTPp`. The patch reports are
retained under `/home/doll/.local/state/peritus/qualification/h1/patch-before.GlxK0f` and
`/home/doll/.local/state/peritus/qualification/h1/patch-after-ack.Pknry2`. The gate reports are
retained under `/home/doll/.local/state/peritus/qualification/h1/gate-before.p0OBxq` and
`/home/doll/.local/state/peritus/qualification/h1/gate-after-ack.6ob4pt`. The promotion reports are retained
under `/home/doll/.local/state/peritus/qualification/h1/promotion-before.7ATDYH` and
`/home/doll/.local/state/peritus/qualification/h1/promotion-after-ack.7ATDYH`. The passing
projection-corruption report is retained under
`/home/doll/.local/state/peritus/qualification/h1/projection-corruption.78KmoW`. The passing
journal-corruption report is retained under
`/home/doll/.local/state/peritus/qualification/h1/journal-corruption.8WRWr9`. The passing
blob-corruption report is retained under
`/home/doll/.local/state/peritus/qualification/h1/blob-corruption.InuR0D`. The passing
snapshot-corruption report is retained under
`/home/doll/.local/state/peritus/qualification/h1/snapshot-corruption.6sDQYS` with report SHA-256
`0fa6e44e3e4e31a9dd10ee53c8e3217c248d55a09b182e327ebaf6813d63fc1a`. The six passing dependency
reports and 36 raw evidence files are retained under
`/home/doll/.local/state/peritus/qualification/h1/dependency-routes.5nwGZV`. Their report SHA-256
digests, in provider/tool/worker death then provider/tool/worker exhaustion order, are
`e214b237393511b08206c510bb67f5de2e40f011ca8614e566d5ff3ba8557fd7`,
`94ce882f5a66c50f29e30a059f10599221ce12640db704e6507901ddbb3db577`,
`f73d643e855a25abf785fcfb60e6079287d70c0782d8886a33fa021077d43d4d`,
`d4a2b6cf84a13d359ab691f28d7147d0b6ea20654130dfc724ab1878a92ead07`,
`462b2d6bd0582a27b70437dd1eda75c1a1b6ab12a52b551e3927f5edef7c096d`, and
`ff76987c43362cd7bc6f788cb9c4cf59dd007f2790aad6fe41ca5dfc834e9c25`. The passing
blob-finalization quota report and six raw evidence files are retained under
`/home/doll/.local/state/peritus/qualification/h1/blob-finalize-disk.fifreA` with report SHA-256
`0f114d8b503be7259cae2ff3a8666dec1094d8ad257867a3ebee748b57be1f19`. The eleven passing daemon
lifecycle reports are retained under the
`/home/doll/.local/state/peritus/qualification/h1/daemon-<phase>.*` directories. In catalog order,
their report SHA-256 digests are
`f0e0d97e9f551f91a27c672346d2cfb99582d900c3b36fd6c70e3fe9194de560`,
`3d8035f263731deb2c01a4c0bd25a3a128a3880b1a43e56255f74ccbfb7fbeff`,
`718babc3551168ceb665baa0e468e1f7749a305a74850c6f202026b7d5134360`,
`b8db5b2d1866f798ac79825aee23de7dd8313b126dc11500ed39ccbde139e81e`,
`4e5817d7376f3549a1cc7093bcaadb230d4bdb0305361bec40f946bf68ca6848`,
`b272d1101269325531895078f32b0470fa2559f14a74fd2950725a0941debc5e`,
`ba020df425665f6286a51d4eec957f16c758d4c3de95dc47c99f68b2366e9031`,
`bc13bd945de5b8427ff3e38ece1f2e523d2ff20fc0514fa4d53c325db0341a7e`,
`32c5e61d0652f751c0ddb899bb4d5e50d9296250e4385134d65923e1c0e86e21`,
`5f85bbb771fd0deb2dd7d7678cc5a9112b0cb8e7c2161aa88f3a68ab8de02a0d`, and
`94a2ffd1bf260b0977f256d14a8d9f19391ced5f819f290c1d6aa0f582fac418`.
The passing journal-append disk-exhaustion report and six raw evidence files are retained under
`/home/doll/.local/state/peritus/qualification/h1/journal-disk.TdiM1k` with report SHA-256
`7298744b66df4198e1c6041c8c7d8fa064869cef59f919043c31552096aff537`.
The passing snapshot-manifest disk-exhaustion report and six raw evidence files are retained under
`/home/doll/.local/state/peritus/qualification/h1/snapshot-disk-final.c32DUJ` with report SHA-256
`728457cb2f05fbce8f520797780cde595141b634344bd6ca9117739e19ae1158`. The route fills a valid
production artifact-store quota, creates an exact retained Git snapshot, requires rejected
manifest publication to release that unpublished ref, and independently verifies the admitted
object, empty temporary namespace, healthy journal, and absent ref through a fresh process.
The passing acceptance-evidence corruption report and six raw evidence files are retained under
`/home/doll/.local/state/peritus/qualification/h1/evidence-corruption.rYq4Qs` with report SHA-256
`6f424f43ef3a10589433b2750e0958a9c5565df6bcb0e6d82ee4498d17d3e414`.
The passing harness-promotion evidence corruption report and six raw evidence files are retained
under `/home/doll/.local/state/peritus/qualification/h1/promotion-evidence-final.vHYIJy` with report
SHA-256 `986e49e5af24ecfd736af315c07c20a0d3994c702186db1d377a0b7507710feb`.
The final three controller routes use an immutable Alpine 3.24.1 BIOS/cloud-init qcow2 as the base
for a fresh QEMU/KVM copy-on-write guest. The controller creates one disposable SSH identity and
cloud-init payload, copies the exact static candidate into the guest, and waits without aggressive
connection retries. It then proves a real kernel boot-ID transition at the outstanding-effect and
durable-before-ack boundaries. The startup-reconciliation route proves two transitions: one before
startup reclaims and reconciles the effect, and another before the final fresh process acknowledges
that exact live fence. The passing focused reports and their SHA-256 digests are:

- outstanding effect:
  `/home/doll/.local/state/peritus/qualification/h1/reboot-outstanding-final.jGfR3w/report.json`,
  `718183bf3ad03839dba46cdf73f7e8b3fb3c642d7a2ce87ca3305e9ed6e2f0db`;
- durable before acknowledgement:
  `/home/doll/.local/state/peritus/qualification/h1/reboot-durable-final.mcHRcP/report.json`,
  `dc747eb74233917a7032761ebb80e7721c42ab69f7b39ba626757ad7fb29e784`;
- startup reconciliation:
  `/home/doll/.local/state/peritus/qualification/h1/reboot-startup-final.7W4YhX/report.json`,
  `7f6d64d964882a821114acc53e2f8550f1b0599d70969db47c69dcd3590f28fa`.

All 43 catalog routes are now connected to genuine production or disposable-host effects. The full
H1 production qualification is still pending because these are focused development diagnostics,
not one complete report bound to the final release revision.

The release integration owner must:

1. build and independently review the platform controller executable used by the standard native
   adapter;
2. provision its disposable project, state, quota, and external host or VM resources per case;
3. connect catalog faults to deterministic C0/C1/D1/F0 failpoints and supervised G0 lifecycle
   controls without bypassing the production interfaces being qualified;
4. use controlled quota or faulting storage for disk cases and a supervised host/VM boundary for
   real power-loss/reboot evidence;
5. retain the six evidence records per case and export their digests into `RecoveryObservation`;
6. execute the suite against the locked release build on every release platform where the fault
   mechanism has equivalent semantics;
7. archive the report, canonical digest, build digest, platform identity, and raw evidence bundle
   under the H4 release record.

The repository tests prove the persistent protocol, all 43 fresh-subject translations, retained
artifact verification, cancellation, descendant ownership, cleanup, and bounded controller-error
diagnostics. The focused Linux reports above additionally prove the real disposable-host reboot
effects. H1 is complete only after the reviewed controller runs the full catalog against the exact
release candidate and produces a `Ready` report with retained evidence.
