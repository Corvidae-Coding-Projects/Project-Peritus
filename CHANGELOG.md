# Changelog

- Repair trusted managed workspaces by re-registering their validated current detached HEAD while
  preserving agent commits and unfinished files; make every coding run generate and follow a
  durable evidence-grounded design; and enforce the embedded production-engineering workflow,
  exact formatting and builds, a 500-line source-file ceiling before acceptance, and productive
  goal supervision across bounded model turns, malformed replies, stalls, and daemon restarts.

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

### Added
- Add the checked-in Rust `peritus-h1-controller` and an explicit focused diagnostic mode. Its 43
  genuine routes cover both sides of journal submission, content-addressed blob publication,
  retained Git snapshot publication, exclusive-lease persistence, and recoverable patch
  application, plus D1's atomic gate event/checkpoint commit and F0's campaign/pointer/approval
  promotion commit. They also cover active projection corruption with fresh-generation repair,
  committed journal-frame corruption with fail-closed startup before authority mutation, and
  referenced blob corruption with durable quarantine and reference denial, and retained Git
  snapshot reference divergence with atomic movement out of the active namespace. Acceptance
  evidence corruption now enters a durable digest-bound quarantine during evidence-store startup;
  recovery preserves every raw indexed field and corrupt record byte for audit while denying that
  evidence identity to all readers, and a second startup proves containment is idempotent. The
  harness-promotion corruption route commits and publishes the real atomic F0 activation, corrupts
  only its harness-activation evidence, and proves fresh recovery quarantines that record without
  changing the 16-event, four-head journal or production pointer. F0 publication directives now
  carry their exact `RevisionTuple`, and publication admits the complete artifact dependency set
  from the producing journal batch instead of trusting caller-supplied revision or partial artifact
  context.
  Provider, tool, and worker death now run through the real executable provider transport,
  ordinary grounded receipt-backed product tool, and daemon-owned worker supervisor. Scheduler
  replay requeues exact owned work after death and preserves exhausted non-success after consuming
  the configured retry ceiling. The artifact-finalization disk route opens two production writers
  against one durable logical quota, admits the first, requires the second finalization to lose the
  real catalog quota race, and proves fresh-process recovery retained no rejected metadata,
  published bytes, or temporary file. The journal-append disk route fixes the production SQLite
  connection's page ceiling at its current allocation, requires an oversized exact append to
  return `SQLITE_FULL`, and proves through a fresh process that no command, event, or aggregate
  head partially survived.
  The snapshot-manifest disk route fills a valid production artifact-store quota, publishes an
  exact retained Git snapshot, then requires manifest finalization to reject the new object and
  compensate by releasing that unpublished reference. Fresh recovery independently proves the
  reference is absent, the admitted filler is intact, no temporary file remains, and the journal
  is healthy. Candidate creation and rollback share this compensation boundary so any real
  manifest-publication failure cannot strand an authoritative-looking Git ref.
  The final three routes run the exact static candidate in a fresh copy-on-write Alpine guest,
  record real kernel boot-ID changes, and prove exactly-once C0 effect recovery when rebooted before
  the effect, after its durable effect but before acknowledgement, and during startup
  reconciliation. The immutable guest image is supplied outside Git and re-digested for every
  subject.
  All eleven daemon-lifecycle routes construct their production E0 state through
  legal reducer commands, commit it through C0, kill the staged daemon at the named active phase,
  and require fresh-process replay to preserve state, phase, ownership, handoff, proposal, and
  acceptance-certificate truth. The exact
  staged `peritusd` is killed before and after each corresponding durable commit. Recovery verifies
  the expected rollback or exact replay, retains all six evidence classes, and cleans its private
  state.
  Unimplemented H1 routes fail closed instead of borrowing the shell fixture or manufacturing a
  result (#31)
- Add the executable `peritus-h1` qualification operator: bind the declared subject to the exact
  candidate bytes, stage and re-digest that candidate inside every fresh subject, run the complete
  43-case native H1 protocol, and atomically retain a full machine-readable Ready/NotReady report
  without allowing a fixture controller to stand in for the release candidate (#31)
- Add the production `peritus-h4` evidence operator: generate exact candidate-bound signing
  envelopes without handling private keys, verify detached Ed25519 evidence, admit exactly one of
  every required signed input, replay signature-bound fresh-subject cleanup, compare independent
  artifact builds, reconstruct all 25 acceptance mappings and the independent audit, evaluate the
  verified release policy, and retain one deterministic no-overwrite Ready/NotReady bundle; add its
  strict plan schema and complete-shaped non-passing operator template (#34)
- Add `peritus-h0-prepare` and the native H0 security workflow: derive one reproducible integrated
  candidate from documented committed source subsets, bind each native Rust host and exact
  controller binary in schema-defined facts, execute the fixed Linux/macOS/Windows shard matrix,
  and retain candidate, report, and raw evidence artifacts without manufacturing the independent
  review required for final aggregation (#32)
- Add strict external-review admission and the `peritus-h0-aggregate` final H0 reducer: combine
  exactly one passing Linux, macOS, and Windows shard, evaluate the independently supplied review
  through verified policy, and atomically retain a candidate-bound Ready/NotReady report with the
  exact canonical evidence manifest and stable failure codes (#32)
- Add the production `peritus-h0-controller`: verify a clean candidate's exact Git-archive digest,
  execute a closed candidate-bound plan for every H0 probe, reconcile migration/recovery, unsafe,
  TCB, threat, and control inventories, exercise native Linux, macOS, or Windows sandbox activation,
  enforce bounded process/output/memory accounting, and retain digest-bound raw evidence (#32)
- Add candidate-bound H0 native-platform shards with a fixed Linux 40, macOS 1, and Windows 1
  partition plus fail-closed three-host aggregation, preventing one host from manufacturing native
  security evidence for another operating system; add bounded deterministic shard JSON that
  round-trips exact candidate, receipt, evidence, limits, and cleanup values while refusing failed
  or inconsistent documents at aggregation; add the no-overwrite `peritus-h0` native shard
  operator and standalone candidate schema (#32)
- Exercise the installed TUI itself in H2 through the host PTY or ConPTY: answer the bounded
  standards-defined cursor-position handshake, require a real daemon connection and rendered
  frame, send the documented Ctrl-Q action, require a successful exit, and verify alternate-screen,
  cursor, and bracketed-paste restoration before accepting the lifecycle (#31)
- Run the complete 18-scenario H2 campaign from the first-class
  `product-native-qualification` command on every hosted Linux, macOS, and Windows package runner,
  retain its report and raw evidence as workflow artifacts, and wire the checked-in controller to
  the real macOS Seatbelt and Windows AppContainer/Job Object capability probes instead of
  returning a platform placeholder; a fresh Linux regression remained 18/18 `Ready` (#31)
- Add the checked-in Rust H2 native controller that validates its exact bound request, installs the
  staged package on the current host, exercises service configuration, daemon crash recovery,
  local transport, same-user authentication, CLI/TUI and terminal paths, executable equivalence,
  process cancellation, the admitted sandbox, upgrades, injected checksum rollback, uninstall,
  protected-state preservation, and zero-resource cleanup; the first real Linux development run
  completed all 18 scenarios as `Ready` (#31)
- Add run-level workspace-growth and observed-memory accounting to the production coding loop,
  persist both across daemon progress snapshots, show them in live status, and fail with the typed
  budget category before generous 50 GiB growth or 12 GiB resident-memory ceilings are exceeded
  (#31)
- Add the standard H2 native platform controller and `peritus-h2` operator: stage and re-digest the
  exact manifest package into one private subject per each of 18 scenarios, clear ambient user
  state, bind versioned requests and independent scenario/cleanup responses to the exact target,
  package, controller, subject, and scenario, own Unix process groups and Windows kill-on-close Job
  Objects, verify retained raw artifacts, atomically publish a complete no-overwrite report, and
  prove all protocol translations plus stale response, false digest, and descendant termination
  without claiming the fixture is real Linux/macOS/Windows qualification (#31)
- Add the standard H1 persistent native-controller adapter: stage and re-digest one reviewed
  executable inside each fresh private subject, bind all four line-delimited protocol stages to the
  exact scenario/build/controller/instance, own Unix process groups and Windows kill-on-close Job
  Objects, enforce cancellation/time/output limits, retain and verify raw evidence artifacts, and
  prove all 43 translations plus false-digest and descendant-cleanup behavior without fabricating
  real reboot or fault evidence (#31)
- Add the standard H0 fresh-native-subject process adapter: stage and digest one reviewed executor
  per private case root, bind versioned requests and structured responses to the exact candidate,
  enforce time/output/cancellation limits, own Unix process groups and Windows kill-on-close Job
  Objects, preserve one subject-addressable raw-artifact root per case, verify every named artifact's
  path, byte count, and digest, reject stale evidence, and prove complete root and descendant cleanup
  without creating external-review evidence (#31)
- Add a Rust-owned HarnessBench campaign publisher and versioned schema that require exact pinned
  task coverage, retain the digest and provenance of every selected result, recompute scores, time,
  and token/cache usage, expose legacy identity coverage, and require native build identity for the
  final candidate campaign (#31)
- Retain a verifier-passing DNA assembly trial whose frozen native agent still timed out after
  discovering but not installing an available ordinary prerequisite, strengthening the general
  disposable-environment installation diagnosis without adding task-specific behavior (#31)
- Add a Rust-owned Terminal-Bench campaign reporter that reconciles Harbor's aggregate state with
  directly visible child results, distinguishes in-progress snapshots from immutable final
  reports, retains per-trial evidence paths and token/cache totals, binds the benchmark pin and
  independently measured executable digest, infers source identity from consistent trial metadata
  with explicit legacy coverage, and publishes versioned JSON atomically without overwriting
  evidence (#31)
- Add a benchmark-integrity appendix that separates real product and model failures from genuine
  evaluator gotchas, links each retained result, and records the score-only shortcut Peritus
  refuses (#31)
- Add the H3 production campaign coordinator: execute short load plans sequentially and four
  eight-hour plans concurrently against disposable public-A3 daemon subjects, enforce an exact
  reference-machine match before launch, share one combined resource ledger, retain deterministic
  bounded samples across the full horizon, and return one fail-closed evaluation and receipt set
  (#35)
- Add atomic H3 evidence publication that revalidates the exact profile, workload catalog, and
  accepted-baseline documents; streams and verifies the subject and runner executables against
  their campaign identities; retains private measurements, receipts, accounting, and machine
  facts; and binds them into a content-addressed manifest and qualification report (#35)
- Add the `peritus-h3` operator command with strict load/full modes, bounded profile/workload/
  baseline inputs, duplicate-free arguments, automatic CPU/core/memory observation, an explicit
  reviewed storage class, exact runner identity, fail-fast reference-machine diagnostics, atomic
  evidence publication, and a distinct exit status for a completed `NotReady` qualification (#35)
- Derive an inert `baseline-candidate.json` from every campaign with complete objective samples,
  binding its entries to the exact evidence-manifest digest; require a later operator to supply
  both that reviewed file and its exact SHA-256 before it can participate as an accepted baseline
  (#35)
- Add a checked-in profile for the exact Intel Core Ultra 9 275HX qualification host while
  preserving every production resource limit, SLO, and regression threshold from the separate AMD
  reference profile; validate that invariant in the Rust dataset contract (#35)
- Bind every new external-benchmark invocation to the exact clean Peritus source revision, Cargo
  package version, and SHA-256 of the native executable; make Harbor verify the uploaded binary
  against the native report and retain both identities in its trial metadata (#31)
- Add durable, default-off automatic provider failover for designer, writer, reviewer, and fixer
  roles when at least two routes are selected; preserve ordinary same-provider recovery first,
  require compatible tool or media capability, exclude safety, refusal, cancellation, and
  ambiguous-acceptance outcomes, and retain every switch in trace, persisted progress, and live
  status evidence (#31)
- Add a narrow Rust release operator that turns each exact native archive into a candidate-bound
  artifact inventory, locked Cargo-graph SPDX 2.3 SBOM, and SLSA provenance statement; sign the
  archive and SBOM through GitHub's keyless Sigstore identity; verify the exact repository,
  workflow, source commit, tag, predicate, artifact digest, and hosted-runner claim before retaining
  both bundles and complete evidence checksums; and publish only after every platform job succeeds
  (#31)
- Add `cargo xtask docs-check` to inventory 134 maintained Markdown files, validate headings,
  fences, line endings, local links, and crate-focused commands, and include it in the normal
  repository policy gate (#31)
- Add one-command POSIX and PowerShell installers backed by host-native GitHub release archives,
  exact detached SHA-256 verification, and the existing transactional install/upgrade adapters;
  add a retained-draft, three-platform tagged release workflow and executable bootstrap
  qualification that proves both successful installation and checksum rejection (#31)
- Add a six-hour cached, offline-tolerant startup release check and `peritus update`, with exact
  semantic-version comparison, bounded streaming downloads, verified native upgrades, installed-
  version confirmation on Unix, finish-after-exit Windows replacement, and persistent explicit
  enable/disable commands for automatic checks (#31)

### Changed

- Retain bounded, sanitized native-controller stderr in supervision failures so an exited H1
  controller reports its precise operational cause instead of only `response stream closed` (#31)

- Add a production Git snapshot quarantine operation that refuses healthy references, atomically
  moves a divergent retained value out of the active namespace, preserves it for inspection, and
  remains idempotent after restart; exercise it through the public daemon and
  `h1.corruption.snapshot` (#31)
- Make artifact-store startup durably contain cataloged content corruption: add a migrated
  integrity state, move divergent bytes out of the active namespace, retain their audit roots,
  deny reads and new references, and prove repeated restart safety through the public daemon and
  `h1.corruption.blob` (#31)
- Correct the C0 operator guide to describe the production daemon's implemented migration,
  recovery, projection, authority, outbox, and readiness startup composition instead of retaining
  the obsolete pre-daemon limitation (#31).
- Move the native H0 and H2 evidence uploads to the current immutable `upload-artifact` v7.0.1
  action, removing GitHub's forced Node 20 compatibility warning without changing retained paths or
  artifact policy (#31).

### Fixed

- Give a native controller that closes its response stream a bounded exit grace before process-tree
  termination, then drain its diagnostic pipe completely. This keeps fast terminal stderr visible
  on slower hosted schedulers without weakening process ownership or deadlines (#31).
- Add a provider-free protocol handshake between Harbor and the uploaded native benchmark agent.
  Setup now verifies the report schema, compiled source revision, package version, and executable
  SHA-256 before a Terminal-Bench task can spend its agent budget, so a stale portable build fails
  one explicit preflight instead of silently invalidating a campaign (`TBF-029`, #31).
- Exclude the dependency-failure qualification admin boundary from `verus_only` builds, matching
  the product runner's existing effect-boundary classification while retaining the complete route
  in ordinary release binaries. This restores full-workspace Verus verification after the real
  provider/tool/worker H1 routes were connected (#31).

- Make projection shadow installation verify the stored payload bytes before reusing an identical
  generation, so startup repair replaces a corrupt active projection instead of reactivating it
  (#31)
- Validate an F0 checkpoint's producer against the exact final event from its shared C0 command
  instead of requiring the producer to be the same aggregate's last event. This preserves strict
  replay checks while allowing genuine multi-aggregate campaign/pointer promotion transactions to
  recover after restart (#31)

- Accept the installed TUI's stable rendered `online`, `ReadyReadWrite`, and active event-stream
  state as native H2 connection evidence when a later subscription notice replaces the transient
  `connected to daemon` message before Windows ConPTY renders its first frame (#31)
- Normalize only trusted Windows paths returned by `std::fs::canonicalize` from extended-length
  drive syntax before native sandbox path validation, keeping ordinary device paths rejected while
  allowing packaged H2 subjects to prove their reparse-free helper identity; retain every required
  capability fact when the host still cannot support sandbox execution (#31)
- Serialize model-visible workspace image paths with forward slashes on every host, so Windows
  image collections keep the same repository-relative manifest contract as Linux and macOS (#31)
- Give a cold installed TUI up to 30 bounded seconds to complete its real daemon-connected
  PTY/ConPTY lifecycle, and retain rendered, connected, quit, cursor-handshake, and sanitized
  transcript-tail diagnostics if that deadline is still exceeded on a hosted runner (#31)
- Ground image-capable runs in every bounded image beneath an explicitly named workspace
  directory, and recognize ordinary image-file and OCR wording, so document and asset collections
  reach the model instead of provoking wasteful hand-built binary decoders (#31)
- Create Windows restricted primary tokens with the zeroed restricting-SID attributes required by
  `CreateRestrictedToken`; keep AppContainer package identity in its separate process security
  capabilities instead of misusing it as a restricting SID; and require native qualification tests
  to observe both the restricted token and its low-integrity label (#31)
- Resolve the nested Windows H0 Cargo linker to an absolute Visual Studio MSVC tool and propagate
  its discovered SDK environment, preventing Git's unrelated GNU `link.exe` from being selected
  after the native qualification boundary clears ambient process state (#32)
- Drain the native terminal's bounded combined stream concurrently with child execution, answer
  Windows ConPTY's cursor-position startup query, keep the input writer alive until the child exits,
  and then close the writer and master to publish reader EOF before accepting evidence; this avoids
  Unix wait-before-read deadlocks, unanswered ConPTY startup, and premature Windows Ctrl-C exits.
  Validate the packaged Task Scheduler template against its exact direct-command placeholders
  rather than a rendered path that the packaged template intentionally does not contain (#31)
- Drain and join the persistent H1 controller's response reader after a successful cleanup exit
  before deciding that its terminal response is missing, removing a macOS scheduling race without
  accepting failed exits or absent response frames (#31)
- Serialize the high-fanout native H1 integration cases within their test binary so concurrent
  qualification fixtures cannot exhaust host process capacity and create unrelated runner flakes
  (#31)
- Pass conventional Windows paths to PowerShell from canonical native H2 subjects so hosted
  qualification can use explicit private install/data roots without `Join-Path` rejecting Rust's
  verbatim path prefix; retain the required Windows process-launch variables across the cleared
  controller boundary (#31)
- Preserve the complete allowlisted MSVC build environment when H0 launches its candidate-bound
  controller, including the Visual Studio discovery roots needed after environment clearing, so
  Rust can locate Visual C++ instead of Git's unrelated `link.exe`; run the TCB inventory against
  the repository's documented local trust aggregate while leaving the separately authorized
  protected-base proof-impact review outside the native shard (#32)
- Pass explicit private Windows install and data roots through the real package lifecycle scripts,
  retaining the ordinary `LOCALAPPDATA` default for users while making native H2 qualification
  independent of PowerShell environment propagation on hosted runners (#31)
- Reconcile Windows qualification cleanup from every owned Job Object PID's actual signaled exit
  state instead of the reference-sensitive active-process accounting count, so hosted runners may
  retain handles to exited controllers without hiding or excusing a running descendant (#31, #32)
- Release exited Windows controller process handles before reconciling Job Object accounting, then
  allow a bounded two-second drain for genuinely terminating descendants across H0, H1, and H2;
  persistent descendants are still killed and fail qualification (#31, #32)
- Retry a freshly staged native H0 executor only for the bounded Unix `ETXTBSY` launch race seen on
  parallel workspace filesystems; persistent executable-busy errors and every other spawn failure
  still fail the probe normally (#32)
- Keep native H2 lifecycle children inside the outer controller's already-cleared environment and
  override the fresh subject's private home, local application data, configuration, state, data,
  and temporary roots without clearing Windows PowerShell's process environment a second time;
  accept the macOS helper's reserved nonzero protocol exit without requiring stderr, and invoke
  `sandbox-exec` with its native option-to-command syntax and a runnable allow-default activation
  profile so the live Seatbelt compilation probe is meaningful; install Bubblewrap plus Ubuntu's
  packaged restricted-userns AppArmor profile on the Ubuntu native qualification host while
  retaining the probe's fail-closed functional check (#31)
- Give the production-daemon A3 conformance client a bounded fifteen-second local-I/O window so a
  slow hosted filesystem sync cannot turn a healthy artifact transfer into a five-second runner
  flake (#31)
- Preserve native H2 reports and raw artifacts from failed hosted campaigns, print the retained
  report path and exact not-ready scenario reasons from `xtask`, and retain a bounded sanitized TUI
  transcript tail when an interactive lifecycle fails before a scenario response (#31)
- Resolve the H2 daemon runtime through the host's canonical temporary-directory spelling before
  startup, so native macOS qualification uses `/private/tmp` instead of the `/tmp` alias while the
  production daemon continues to reject genuinely aliased or symlinked state roots (#31)
- Keep the authenticated A3 shutdown requester alive as a bounded reporting connection, release
  its connection-owned transfers and terminal attachments before authority teardown, stream all
  six correlated progress observations plus the truthful final disposition, and join the IPC
  server only after delivery so `peritus shutdown --wait` no longer fails with an early EOF (#31)
- Require parameterized deliverables derived from a supplied example to exercise an independent
  input and prove or vary example-derived constants before the harness accepts a general interface
  (`TBF-027`, #31)
- Canonicalize both benchmark campaign roots and retained evidence before containment checks, then
  serialize report-owned evidence paths as validated forward-slash relative strings; this keeps
  HarnessBench and Terminal-Bench reports stable across macOS `/var` aliases, Windows short-name
  roots, and Windows path separators while still rejecting actual path escape (#31)
- Require command-driven API and structured-data exploration to select decision-relevant fields,
  keys, counts, or bounded samples instead of filling model context with complete nested metadata
  (`TBF-020`, #31)
- Require empirical or heuristic algorithms calibrated from one supplied example to exercise an
  independent segment, contract-preserving perturbations, or independently derived cases before
  claiming generalization; the calibration sample alone is no longer sufficient evidence
  (`TBF-026`, #31)
- Require explicit live operational requests to retain a successful caller-authorized effect and a
  later fresh verification even when the writer also changes supporting scripts or documentation;
  helper artifacts alone no longer substitute for the requested running result (`TBF-025`, #31)
- Interrupt long writer/fixer inspection spirals with at most two deterministic in-session progress
  corrections when no workspace mutation or declared external effect occurs, directing the role to
  use authorized standard tooling and take a concrete delivery step (`TBF-024`, #31)
- Retain both opening context and final diagnostics when a developer command exceeds its bounded
  output window, omitting the noisy middle so long compiler and dependency logs cannot hide the
  actual terminal failure that a recovery turn needs (`TBF-023`, #31)
- Supervise each Terminal-Bench native agent process tree and, when Harbor cancels the adapter,
  terminate and reap that exact tree before allowing the shared task environment to enter verifier
  execution; this restores phase isolation without changing task timeouts or scores (`TBF-022`, #31)
- Put both explicitly authenticated Terminal-Bench provider routes into its bounded fallback chain,
  preserving the default writer/reviewer assignments while allowing existing capability-aware
  routing to move an image-grounded role to the image-capable route (`TBF-021`, #31)
- Direct model-authored commands toward filters, bounded ranges, and summary modes before inspecting
  large binaries, logs, databases, or generated files, while preserving the hard output cap as an
  accuracy fallback instead of routinely filling model context (`TBF-020`, #31)
- Reconcile explicit request-derived output paths, including quoted extensionless executable names,
  with the actual candidate before acceptance, mapping absolute paths through the managed workspace
  root and rejecting missing or relocated same-basename artifacts while leaving inputs and
  command-created temporaries alone (`TBF-019`, #31)
- Require quantitative and scientific work to establish input/output units, coordinate systems,
  and transformations before calculating or fitting, then validate results in the requested
  physical domain instead of accepting a structurally valid raw-coordinate result (`TBF-018`, #31)
- Ground every model workspace listing in its exact managed root and explicit relative-path
  convention so absolute paths from a task lose that prefix exactly once instead of being written
  beneath a duplicated root directory (`TBF-017`, #31)
- Make caller-authorized external-effect runs attempt ordinary scoped prerequisites, including
  build or runtime dependency installation in disposable task environments, before asking the user
  to supply tools that the environment can install itself; turn missing-executable failures into
  actionable, authority-bounded guidance to install the prerequisite and retry the real command
  instead of fabricating a stand-in deliverable (`TBF-016`, #31)
- Prepare each external benchmark's durable developer trace before product execution so a
  pre-stream product failure retains an honest zero-usage invocation report and its original
  diagnosis instead of being replaced by a missing-trace adapter error (#31)
- Include the required schema version when serializing H3 baseline manifests so a generated,
  reviewed baseline round-trips through the accepted-baseline parser in a later campaign (#35)
- Compile H3 campaign-only sampling and shared accounting only on its supported Unix boundary and
  scope Linux process-status imports to Linux, keeping strict macOS and Windows Clippy builds free
  of platform-created dead code and unused imports (#35)
- Finish partial queue-saturation cycles with an exact planned drain so every production catalog
  workload returns its command, exporter, or provider queue to a balanced terminal state instead of
  failing readiness solely because its operation count ends between complete saturation cycles
  (#35)
- Start a bounded fresh repository-grounded invocation when designer, writer, fixer, or reviewer
  exhausts one invocation's empty, connection, malformed, interrupted/incomplete, transient,
  transport, rate-limit, timeout, or generic provider responses; preserve useful workspace
  progress, keep authority and ambiguous-acceptance failures terminal, and stop after three total
  same-provider role invocations (`TBF-009`, #31)
- Enforce user-declared closed mutation contracts across the complete candidate diff so helpful
  adjacent edits cannot escape exact path, value, or transformation restrictions (`TBF-028`, #31)
- Expand dirty tracked and untracked files inside cloned nested Git repositories into the managed
  candidate set and reviewer diff, while retaining generated-tree filtering and imported-source
  ownership boundaries (`TBF-014`, #31)
- Add an explicit, default-off external-effects delivery scope for authorized administration and
  recovery tasks whose accepted result lives outside Git: require retained successful effect
  evidence, a later fresh verification command, and independent blocker-free review while keeping
  ordinary workspace runs' exact changed-target requirement unchanged (`TBF-013`, #31)
- Preserve each Harbor task image's authoritative executable search path and prepend Peritus's
  uploaded routers instead of replacing installed administrative directories such as `/usr/sbin`;
  validate unsafe path lists and retain deterministic precedence (`TBF-012`, #31)
- Record Terminal-Bench's unpublished PyTorch forward-interface expectation separately from product
  defects, retain the honest lower score instead of leaking verifier or solution details into
  Peritus behavior, and document the focused thin-bridge regression command (`TBI-005`, #31)
- Resolve each Harbor task image's authoritative working directory instead of assuming every
  Terminal-Bench environment uses `/app`; validate the discovered path and pass the same workspace
  to the native Peritus runner so valid `/workspace` and other image layouts remain portable
  (`TBF-011`, #31)
- Require contemporaneous snapshots, source revisions, or archived records for `as of` and other
  historical source-state requests, instead of treating current mutable data filtered by an item's
  own date as proof of the source's earlier values, membership, ranking, or calculations
  (`TBF-010`, #31)
- Teach external benchmark trace projection to validate compaction, retry, and provider-switch
  metadata in addition to response and tool frames, and use retry/switch evidence to close an
  incomplete projected response instead of reporting a valid recovery trace as corrupt (#31)
- Create each durable developer trace before the first provider request, without truncating prior
  events, so a provider failure before its first response remains the reported cause instead of
  being masked by a missing-trace filesystem error; empty traces now retain accurate zero-request
  benchmark accounting (`TBF-008`, #31)
- Pass Windows release archive and self-update extraction paths through explicit PowerShell
  environment bindings, so `Compress-Archive` and `Expand-Archive` receive non-empty literal paths
  on hosted Windows runners instead of losing positional arguments after `-Command`; compute the
  public bootstrap checksum through the built-in .NET SHA-256 implementation so an isolated user
  profile does not depend on PowerShell's optional module-command cache (#31)
- Replace the root and HarnessBench chronological status walls with plain-English product,
  qualification, benchmark, and documentation guides while retaining task-level forensic evidence
  in the external failure journal; add focused checks to all 75 crate READMEs (#31)
- Persist a bounded effect-receipt ledger for every writable G4 developer-tool invocation, binding
  deterministic role/invocation/effect identity to the provider call ID and canonical request
  digest before execution and to the exact result afterward; replay completed calls without a
  second effect, refuse conflicting recovered actions, and classify an interrupted external
  command as explicitly ambiguous instead of launching it again (#31)
- Preserve the complete selected source value whenever a request requires an output component to
  match a named authoritative source, applying only explicitly named transformations instead of
  letting outside domain labels such as tag, wrapper, metadata, artifact, or non-native content
  trigger speculative deletion during writer, reviewer, or fixer turns (`TBF-007`, #31)
- Account for provider requests, retries, context compactions, application tool calls, normalized
  token and cache usage, provider-estimated cost, and elapsed time across the complete
  designer-writer-reviewer-fixer run; enforce generous eight-hour and cumulative runaway ceilings;
  persist the latest counters; and show live elapsed time, last durable progress, and remaining run
  horizon in the polled TUI status instead of making a healthy slow provider call look dead (#31)
- Classify stable Codex account-runtime authentication, safety, rate-limit, quota, and context-limit
  terminals without retaining untrusted provider text, and preserve non-retryable provider category
  and diagnostic identity through the product error instead of collapsing it to an empty response
  (#31)
- Replace the product developer loop's fixed linear retry sleep with the shared checked
  exponential planner, stable bounded jitter, provider `Retry-After`, cancellation-aware waiting,
  and durable reason, attempt, elapsed-time, and delay evidence for account and direct routes (#31)
- Resolve visual inputs from explicitly named workspace paths or direct image-inspection requests
  instead of unrelated media mentioned only as an external verifier reference, and apply the
  mandatory 500-line source ceiling to baseline and directly authored code without claiming
  ownership of upstream source trees imported by commands (#31)
- Keep developer-tool wire encoding and product-runner prompt regressions in cohesive submodules
  below the repository's reviewed source budgets, and express command-test fixtures through the
  ordinary formal-boundary subset so candidate policy validates the same code exercised locally
  (#31)
- Challenge an unchanged writer or fixer request for user input against the harness's confirmed
  writable workspace capabilities before pausing, so missing provider-native filesystem tools
  cannot be mistaken for a read-only managed workspace (#31)
- Enforce the 500-line source ceiling on exact changed source files within each affected project,
  so new and modified code remains modular without rejecting untouched vendored, generated,
  submodule, or legacy source inherited from the baseline (#31)
- Run external Linux task environments with a static-musl Peritus adapter, carry Codex's matching
  inert code-mode host companion beside the credential-owning executable, parse machine reports
  after PTY progress output, and make rootless Podman resolve Docker Hub short names without an
  interactive prompt (#31)
- Make partial and checkpoint result artifacts self-contained for their captured round with
  completed results, pending or failed stop boundary, and stop reason, and preserve those snapshots
  across later rounds unless revision is explicitly requested (#31)
- Keep time-window accepted/seen identities disjoint from ignored or out-of-window identities while
  allowing duplicate-observation state to reference the retained first-seen record; unchanged
  HarnessBench task 104 improves outcome from 0.8875 to 0.98 (#31)
- Preserve every explicitly required named artifact, identifier, field, clause, command, or path
  literally at least once in its owning output instead of replacing traceable references with prose;
  unchanged HarnessBench task 101 now passes all 15 outcome checks (#31)
- Cite both requirement and failed-validity clauses for missing or invalid required items, cite
  governing clauses and use positive scope wording in boundary documents, and preserve scalar
  identity/name lists when sibling audit artifacts own richer issue and policy metadata (#31)
- Keep one canonical, self-contained record per decision dimension; preserve included and excluded
  scope, conditions, and governing gates; distinguish a decided requirement from unresolved
  satisfaction; cite decisive rejection authorities; and retain `path#record_id` anchors for
  evidence inside multi-entry source files (#31)
- Require independently resolvable evidence locations using stable clause IDs, structured record
  coordinates, exact counterexample identities, or literal missing paths; keep those locators in
  their declared location fields, and make every not-reproducible artifact carry a decisive missing
  prerequisite without relying on richer sibling outputs (#31)
- Cite stale, draft, superseded, unapproved, or unsafe evidence without unnecessarily reproducing
  the rejected actionable literal, preventing invalid contact details, credentials, prices, and
  commands from being mistaken for an answer while retaining provenance and rejection reasons (#31)
- Preserve an explicit empty/null applicable-authority sentinel for true insufficient evidence,
  retain every evaluated result-affecting losing source across priority, date, expiry, scope, and
  exceptions, and keep exact source IDs, paths, keys, and names separate from explanatory reason
  text so downstream matching, joins, and deduplication remain reliable (#31)
- Continue premature writer, reviewer, and fixer terminal responses inside the same tool session
  until the executor's deterministic completion evidence is satisfied, preserving partial grounding
  across correction instead of exhausting fresh retries before a known gate failure can be fixed
  (#31)
- Treat detail ledgers named for one closed classification as projections of that class unless the
  contract explicitly allows overlap, preventing review and informational rows from leaking into
  regression-only outputs (#31)
- Route reconciliation identities once across primary, synthetic, and reject outputs unless dual
  recording is explicit; preserve material status, distinguish absent references from present
  invalid records, and reconcile exception summaries across every artifact, letting unchanged
  HarnessBench task 091 pass all 20 outcome checks (#31)
- Preserve typed identity when flat outputs reference records from heterogeneous source categories,
  and aggregate semantic cause summaries by category rather than individual record; unchanged
  HarnessBench task 090 now passes all 16 anomaly-attribution checks (#31)
- Preserve declared aggregate semantics when a separate exclusion or adjustment ledger lacks
  record-level membership and metric effects; the workflow, developer, and reviewer now require an
  authoritative schema or reconstructible join before arithmetic changes and treat unresolved
  provenance as advisory, letting unchanged HarnessBench task 089 pass all 18 checks (#31)
- Require external pagination and retry loops to prove bounded forward progress, reject repeated
  page or cursor tokens, retry only declared transient failures, and surface permanent client errors
  immediately; unchanged HarnessBench task 088 now passes hidden compatibility instead of timing
  out in a repeated-cursor loop (#31)
- Derive nested Cargo manifest expectations with host-native path semantics and compare structured
  list/search JSON plus image manifests without Unix-only separator assumptions, keeping production
  behavior unchanged while allowing Windows Gate A and Foundation suites to validate native paths
  (#31)
- Map every explicitly requested regression behavior to a direct repository test and assertion in
  the production workflow, writer skill, and independent reviewer; unchanged HarnessBench task 087
  now adds the previously missing descending-sort regression and improves from good to excellent
  while preserving perfect hidden CLI behavior (#31)
- Require affirmative source evidence for hard eligibility, compatibility, and placement
  constraints instead of treating a missing field as a permissive default, keeping optimization
  inside the proven feasible set unless an authoritative input declares fallback semantics (#31)
- Keep mandatory design proportional to the declared workspace: render explicit generated-artifact
  designs deterministically in Rust from the exact conversation and bounded sorted inventory while
  retaining model-authored, read-grounded architecture for source repositories; add per-role model
  output ceilings, independent category predicates, and genuine multi-observation periodic polling
  guidance (#31)
- Let the structured workspace removal tool delete one explicitly listed empty directory
  non-recursively, while rejecting the workspace root and nonempty directories and preserving the
  existing ownership boundary for regular files (#31)
- Isolate explicitly named staged inputs until the round that introduces them, and make change
  reports account for changed, added, removed, and already-satisfied constraints with their literal
  values so preserved requirements are visibly verified rather than silently omitted (#31)
- Honor the negotiated provider parallel-tool width in the production developer loop, execute
  batched calls in stable proposal order, report identical full-file writes as unchanged no-ops,
  and preserve exact identifiers, enum-like values, paths, fields, and commands byte-for-byte
  across generated artifacts instead of paraphrasing contract syntax (#31)
- Isolate the daemon deliverable Git fixture from host `core.autocrlf` settings and compare discard
  results with its captured baseline bytes, keeping the exact-path export/discard test portable on
  Windows hosted runners (#31)
- State the enforced current-turn grounding sequence in writer/fixer prompts and provider-facing
  tool descriptions, identify harness-owned internal gates as unavailable workspace commands, and
  require reviewers to reread every conserved finding location before repeating it after a fix
  (#31)
- Keep advisory findings visible without starting fixer cycles, reject circular resolution of
  ambiguous trailing modifiers and unsupported expansion of named categories, and conserve one
  stable review finding when later reviewers refine its free-form location evidence, including
  fail-closed coalescing of pre-v2 duplicates during durable restore (#31)
- Recover declared Peritus host calls when the Claude account runtime returns its schema-valid
  outer call array empty but embeds the reserved call envelope inside application JSON; validate
  the recovered names, arguments, and limits through the same fail-closed path, require an explicit
  reviewer findings array before terminal admission, and keep the feature-disabled conformance
  test target documented under ordinary `cargo test` (#31)
- Run independent review through a fresh repository-grounded D0 loop with an executor-enforced
  read-only tool surface, require listing before targeted reads, reject undeclared reviewer
  mutation/process calls, and retry malformed or ungrounded typed reviews with their exact failure
  instead of accepting a one-shot account of files the reviewer could not inspect (#31)
- Project the complete typed Peritus host-tool catalog and selection policy into Claude account
  requests, so the official executable returns inert structured calls for Peritus to execute while
  Claude native tools, MCP, plugins, hooks, and session state remain disabled (#31)
- Retain sandbox-relative workspace, trace, usage-proxy, and observation locations in external
  invocation evidence schema 4, keeping native evidence resolvable when HarnessBench moves a
  mixed-provider sandbox under the last observed model after execution (#31)
- Parse every changed JSON deliverable with a bounded native exact-target gate before review, so a
  model-authored validation command cannot be the only evidence that the generated artifact is
  structurally valid (#31)
- Recognize standalone changed Python production modules even without a manifest or supplied tests,
  and require same-workload baseline/candidate measurements for performance-change claims instead
  of inferring improvement solely from the candidate implementation (#31)
- Require changed conventional Python dependencies to be satisfied by the real installed package
  through a read-only offline gate, and reject test-process substitutes as compatibility evidence
  for the dependency being added or upgraded, while retaining legitimate unrelated mocks (#31)
- Bind changed Python production sources to their nearest conventional manifestless test project,
  extending exact compile and pytest evidence beyond files physically located below `tests/` (#31)
- Discover manifestless CommonJS and JavaScript modules through adjacent conventional test files
  and execute those tests directly in stable order, so exact-target acceptance does not fall back
  to general artifact checks merely because `package.json` is absent (#31)
- Discover conventional Python test projects even when they have no `pyproject.toml` or
  `pytest.ini`, represent their absent manifest honestly, and independently compile and test the
  nearest affected package instead of accepting only general artifact checks (#31)
- Keep constructible requested work moving when authoritative inputs expose an awkward canonical
  identifier: treat registered identifiers as opaque contract values, preserve factual evidence,
  ask only for genuinely material choices, make matching superseding rules own primary authority
  fields, and stop a blocker that survives two complete fixer/reviewer attempts even when candidate
  bytes continue changing (#31)
- Validate every changed CSV in an artifact workspace with a native exact-target gate before
  acceptance, rejecting ragged rows, invalid quote placement, unterminated quoted fields, invalid
  UTF-8, and oversized inputs even when a model-authored check incorrectly reports success (#31)
- Preserve files created by evaluators, services, hooks, and other external actors during a complete
  writer-reviewer-fixer run; route model-requested deletion through an ownership-aware exact-file
  tool; preserve literal requirement scope; keep artifact-only work free of unrequested package
  scaffolding; and restrict blocking review findings to explicit requirements, failed deterministic
  gates, or concrete contradictions (#31, #36)
- Separate non-authoritative source text from malicious intent in every built-in role, classifying
  supplied content by its requested effect so benign policy advice does not become a false-positive
  quarantine while harmful actions and cross-input triggers remain isolated (#31)
- Ground image tasks in actual bounded workspace pixels instead of filenames, carry validated
  raster inputs through the developer loop and Codex account runtime, fail clearly for text-only
  providers, and let HarnessBench visual rubrics use the same credential-preserving route (#31)
- Project large assistant, tool-argument, and tool-output values into digest-labeled bounded
  previews for external process grading while retaining every exact byte in the native trace,
  preventing HarnessBench's rubric context cap from hiding late write and verification events (#31)
- Stop the production fixer loop after two consecutive cycles make no candidate change while exact
  checks or blocking findings remain, resetting the detector after real edits or new conversation
  input; this turns benchmark deadline kills into bounded, inspectable outcomes (#31)
- Reassemble streamed tool-call argument bytes before UTF-8 decoding and let completed external
  benchmark attempts reach their upstream oracle even when Peritus's stricter internal review
  rejects the candidate, preserving process grades and multi-round task continuity (#31)
- Teach malformed design retries the exact rejected heading contract and give generic artifact
  workspaces an explicit native gate kind, eliminating a repeated-retry failure and a 600-second
  no-progress timeout found by unchanged HarnessBench task 001 (#31)
- Retry recoverable malformed, empty, timeout, and transport provider turns without failing the
  coding goal; retry malformed design, developer-terminal, and reviewer output contracts; replenish
  bounded developer segments only after exact candidate content changes; feed rejected ungrounded
  or malformed terminals their exact corrective context; and automatically resume interrupted
  goals after daemon restart while stopping repeated no-progress work (#36)
- Refuse interactive coding-run completion unless every exact changed project is discovered and
  passes its locked native compile/test/lint commands; unrelated root checks can no longer accept a
  broken nested target, and generated dependency/build trees no longer contaminate candidate or
  reviewer evidence (#36)
- Give independent review authoritative current file permissions and a bounded ledger of developer
  command requests and observations, count permission-only edits as real candidate progress, and
  distinguish model-run evidence from deterministic harness gates so valid behavior can be
  verified without trusting unsupported claims (#31)
- Bound every model-issued structured command, concurrently retain capped stdout and stderr, and
  kill the owned process tree on deadline so a long-running program becomes a recoverable tool
  observation instead of freezing the writer-reviewer-fixer run (#31)
- Request portable high reasoning effort throughout the developer loop whenever the selected
  provider advertises that control, and make both official account routers honor the negotiated
  effort instead of silently fixing every turn at low (#31)
- Derive review blockers from typed category and severity policy and durably conserve every finding
  through fixer proposal and fresh reviewer confirmation instead of trusting a model-supplied
  Boolean or losing findings across daemon restart (#36)
- Restart a stale or unavailable packaged daemon after upgrades, wait for its process lock to be
  released before replacement, and show `R restart/reconnect` directly in the offline TUI status
  instead of leaving recovery hidden behind the help screen (#36)
- Recover from malformed writer/fixer output, and turn terminal coding-run failures into
  actionable conversations that can be corrected and resumed in the same managed worktree (#36)
- Keep Codex Code Mode disabled without disabling its inert host feature, avoiding the current CLI
  0.149.1 nonfatal host-unavailable event that the account-runtime decoder correctly rejects
- Give native plugin test fixtures a runner-safe process-startup allowance while preserving the
  stricter invocation, cancellation, and shutdown deadlines they are intended to verify (#27)
- Format each workspace package independently on Windows hosted runners so Rustfmt stays below the
  operating system command-line limit as the production workspace grows (#27)
- Give Gate A's verified release and strict no-cheating build lane the same measured 40-minute
  execution bound as the foundation workflow after the workspace expansion (#27)
- Normalize every managed-proxy client socket to bounded blocking I/O at the production accept
  boundary, avoiding platform-dependent inheritance of the nonblocking listener flag and
  premature CONNECT closure on macOS (#22)
- Bound managed-network integration fixture accepts and reads, and serialize canonical test
  execution per binary so a transient macOS socket stall fails promptly instead of exhausting a
  hosted runner (#20)

### Added
- Bind every production developer turn to the selected provider's C6-checked input budget;
  deterministically compact only complete old tool exchanges with durable policy, source, and
  replacement digests; and negotiate automatic prompt caching only for profiles that advertise it,
  including the no-flag account-backed Codex and Claude routers (#31)
- Add the native Harbor custom-agent boundary for Terminal-Bench 2.0, including unchanged-workspace
  product execution, durable design/trace/observation evidence, provider and cache accounting,
  resource-aware Podman commands, and a passing unchanged single-task qualification (#31)
- Exercise unchanged HarnessBench task 093 across JSONL parsing, identity stitching, ordered
  sessionization, exact inactivity boundaries, bot exclusion, deduplication, rejects, and summaries;
  isolate unpublished campaign carryover, dual-routing, and lexical conventions from product rules
  (#31)
- Exercise unchanged HarnessBench task 092 across daily schema drift, row-level rejection, summary
  reconciliation, and changelog contradiction reporting; retain complete evidence while isolating
  unpublished severities, multi-defect priority, summary shape, and a rubric-oracle category conflict
  from general product behavior (#31)
- Exercise unchanged HarnessBench task 091 across FX conversion, refunds, bank fees, void and
  missing-invoice cash, rejection routing, summary reconciliation, and close notes, improving
  outcome/process/security/combined from 0.7705/0.9767/1.0/0.7525 to
  1.0/0.9633/1.0/0.9633 after general ledger-semantics fixes (#31)
- Exercise unchanged HarnessBench task 090 across thresholded anomaly detection, priority
  attribution, typed cause lineage, impact summaries, low-volume suppression, and uncertainty notes,
  improving outcome/process/security/combined from 0.5424/0.96/1.0/0.5207 to
  1.0/0.9633/1.0/0.9633 after a general reference-semantics fix (#31)
- Exercise unchanged HarnessBench task 089 across aggregate A/B metrics, exact exclusions,
  statistical testing, and caveat reporting, improving outcome/process/security/combined from
  0.9524/0.8633/1.0/0.8222 to 1.0/0.9933/1.0/0.9933 after a general provenance-grounding fix (#31)
- Exercise unchanged HarnessBench task 088 against both generations of a live local API contract,
  replacing an unbounded repeated-cursor timeout with exact normalized results, bounded 429 retries,
  immediate 400 errors, and a perfect outcome score (#31)
- Exercise unchanged HarnessBench tasks 081 through 087 across local DOM/form interaction, Compose
  repair, monorepo interfaces, JavaScript state, flaky-test determinism, SQL migration/rollback, and
  CLI parsing; tasks 083 and 085 pass every outcome check while task 087 validates a general
  requirement-to-test traceability fix (#31)
- Exercise unchanged HarnessBench tasks 076 through 080 across administrative claim checking,
  nested-archive defense, cursor/retry recovery, high-cardinality batch normalization, and schema
  round trips; task 078 passes every outcome check while unpublished lexical and schema conventions
  remain isolated from general product behavior (#31)
- Exercise unchanged HarnessBench tasks 071 through 075 across support routing, logistics response,
  research reproducibility, rubric grading, and policy-amendment appeal review; preserve correct
  grounded outputs where private identifiers, an absent fixture, a rubric contradiction, and an
  overlapping confidence rule prevent exact hidden-ground-truth agreement (#31)
- Exercise unchanged HarnessBench tasks 066 through 070 across rollback readiness, canary metrics,
  launch operations, policy-governed copy remediation, and fair resume screening; task 069 passes
  every outcome and process check, while unpublished enums, negation-blind matching, and a shortlist
  contradiction remain recorded rather than driving benchmark-specific behavior (#31)
- Exercise unchanged HarnessBench tasks 062 through 065 across Kubernetes audit, topology-aware
  alert reduction, multi-source incident triage, and capacity optimization; retain two unpublished
  evaluator conventions while a general evidence-positive constraint fix improves task 065 from
  failing 0.6617 outcome to excellent 0.9873 with perfect process/security scores (#31)
- Exercise unchanged HarnessBench task 061 through timed status polling, replacing two design-bound
  deadline failures and one category/cadence miss with a 26-second multi-observation run that passes
  all seven checks and scores outcome/process/security/combined 1.0/0.9867/1.0/0.9867 (#31)
- Exercise unchanged HarnessBench task 058 through three durable project-state rounds: replace a
  1,200-second Day 3 timeout with negotiated batched writes, preserve `conditional_go` across the
  JSON and CSV artifacts, complete natively, and score outcome/process/security/combined
  0.9375/0.9233/1.0/0.8656 while retaining one unpublished lexical oracle miss (#31)
- Exercise unchanged HarnessBench tasks 056 and 057 through native inventory and two-round resume
  workflows, retaining their 0.69 and 0.8077 outcomes where hidden ground truth requires an
  explicitly contradictory SKU omission and unpublished JSON/log shapes despite correct requested
  calculations and state reuse (#31)
- Exercise unchanged HarnessBench task 055 before and after the grounding-protocol correction,
  preserving 24/24 output checks while moving from three cycles, 48 requests, 808,266 tokens, and
  process 0.76 to one cycle, 17 requests, 306,944 tokens, and process 0.9867 (#31)
- Exercise unchanged HarnessBench tasks 053 and 054 through native first-cycle transaction-anomaly
  and budget-variance runs, passing all 29 and 34 oracle checks respectively while retaining process
  efficiency deductions for later longitudinal analysis instead of adding task-specific behavior
  (#31)
- Exercise unchanged HarnessBench task 051 through exact offline SQLite reporting with 21/21
  checks and task 052 through a diagnostic review-convergence series whose final identity-v2 run
  completes natively with all 17 checks and outcome/process/security/combined scores of
  1.0/0.93/1.0/0.93; retain every regression, provider stall, and native failure without
  benchmark-specific special cases (#31)
- Exercise unchanged HarnessBench task 050 through six-table financial reconciliation: all 26
  oracle checks pass and the native before/after recovery comparison moves from failure with
  outcome/process/security/combined 1.0/0.7433/1.0/0.7433 to success at
  1.0/0.9267/1.0/0.9267 while reducing provider requests from 36 to 31 (#31)
- Exercise unchanged HarnessBench task 049 through exact local data cleaning and independent
  source reconciliation: all 22 oracle checks pass, native review begins with `workspace_list` and
  reads all three authoritative fixtures, and final outcome/process/security/combined scores are
  1.0/0.9867/1.0/0.9867 (#31)
- Exercise unchanged HarnessBench task 048 through exact release-note, migration, decision, and
  audit artifacts: every named oracle check passes, three JSON outputs receive native structural
  acceptance, and embargoed security details remain undisclosed (#31)
- Exercise unchanged HarnessBench task 047 through a complete evidence-supported security review,
  retaining its nine correctly classified findings while replacing generic artifact acceptance
  with deterministic JSON parsing before independent review (#31)
- Preserve HarnessBench task 046's perfect correctness and performance outcome while replacing
  generic artifact acceptance with native Python syntax evidence and raising process/combined
  quality from 0.8867 to 0.93 through comparative performance verification (#31)
- Exercise unchanged HarnessBench task 045 against pinned `python-slugify` 8.0.4, improving its
  false-substitute baseline from outcome 0.838/process 0.8267/combined 0.6927 to a real-dependency
  outcome 0.98/process 0.9467/combined 0.9277 with security 1.0 (#31)
- Exercise the pinned, unchanged HarnessBench suite through task 046, retaining task 039's excellent
  0.9673 architecture result, task 040's complete 1.0 mutation-tested Python result, task 041's
  0.9962 immutable-state result, task 042's official 0.4 oracle-crash result, and task 043's 0.995
  transactional SQLite result without hiding regressions or conflating benchmark infrastructure
  defects with Peritus defects (#31)
- Record task 044's safe GitHub Actions repair and automatic provider-stall recovery, with an
  unchanged 0.98 final outcome after closing its native YAML and root-level Python coverage gap
  (#31)
- Discover conventional SQLite migration workspaces and independently execute their schema,
  forward migration twice, foreign-key checks, postcheck, and rollback in a disposable Rust-owned
  database before exact-target acceptance (#31)
- Recognize manifestless Python projects with root-level test files, parse every changed YAML file
  through a bounded Rust-owned gate, and run Python syntax and pytest checks without leaving
  bytecode or pytest caches in the managed workspace (#31)
- Retain the exact last product-run diff, gates, review ledger, summary, and durable finding state
  beside every external benchmark invocation so a strict internal rejection remains independently
  diagnosable after the upstream scorer exits; unchanged HarnessBench task 038 proves this evidence
  path on a successful four-artifact synthesis run with a recovered provider timeout (#31)
- Pin the complete local HarnessBench Python and oracle environment, including pytest, so executable
  task oracles cannot disappear from a full-suite run because of undeclared host tools (#31)
- Add deterministic workspace image discovery with signature, count, per-file, and total-size
  bounds; private Codex CLI image staging; and multimodal external-rubric parsing and tests (#31)
- Add the native external-benchmark adapter and normalized trace projection, pin HarnessBench 2.0
  at its exact upstream commit, retain a reproducible failure journal, and route its text process
  rubric through the already-authenticated official Codex executable without copying credentials
  (#31)
- Embed reusable architect, developer, and reviewer skills in the ordinary product workflow;
  require successful repository listing and targeted reads before design or mutation, append exact
  observed paths to each design artifact, refuse mutation of an unread existing file, enforce a
  deterministic 500-line source-file ceiling, and record exact Cargo format and build checks beside
  compile, test, and Clippy evidence (#36)
- Replace the one-shot complete-file JSON product runner with the production D0/D1/D2/E0
  composition: writers and fixers now inspect, search, read, write, patch, run, test, observe, and
  retry through bounded structured tools with append-only provider/tool traces and an aggregated
  task-level completion summary (#36)
- Add a durable completion handoff containing the managed path, exact changed files, exact passing
  commands, and run instructions, with TUI actions to inspect, accept, commit, export, or discard
  the exact deliverable. A worktree remains reserved until a pending handoff is committed or
  discarded (#36)
- Add durable two-way coding-run conversations across the A3 protocol, daemon persistence,
  product runner, and Runs UI. Users can message active work, answer direct material questions,
  redirect or continue failed/completed runs, and inspect the persisted transcript with Enter or
  `m`; legacy stored runs migrate without data loss (#36)
- Complete the G4 coding surface with canonical A3 start/control/query messages and generated wire
  assets; a daemon-owned, persisted product-run registry; managed-worktree and provider resolution;
  progress observation; cancellation, retry, and interrupted-run recovery; and bounded execution
  state exposed without client-side authority. Active runs are serialized per managed worktree,
  and retry is limited to failed, cancelled, or interrupted work (#40)
- Add the modular `peritus-product-runner` writer-reviewer-fixer coordinator with bounded repository
  context, checked JSON edit plans, rollback on application failure, tracked and new-file diff
  capture, native Cargo/npm/pytest/Go gate discovery, independent structured review, two bounded
  fixer cycles, provider-neutral C5 reduction, and focused correctness tests (#40)
- Turn the Runs, Diff, and Review views into the ordinary coding experience with an accessible task
  composer, independent writer/reviewer/fixer provider selection, textual phase timeline, live
  check/review/diff projection, selection, cancellation, retry, reconnect, keyboard help, and
  automatic daemon polling (#40)
- Add resource-capped host-native package assembly through `cargo xtask product-package`, per-user
  release installation through `cargo xtask product-install`, canonical manifest/checksum output,
  atomic Linux/macOS/Windows installers, package-only upgrade rollback, state-preserving uninstall,
  and automatic Windows user-PATH registration (#41)
- Exercise package assembly, install, repeat command launch, upgrade, uninstall, and protected-state
  preservation on hosted Linux, macOS, and Windows while reusing already checked debug artifacts to
  keep runner memory and elapsed time bounded; retain optimized locked builds for actual packages
  (#41)
- Document the completed single-command onboarding, coding-run controls, native lifecycle,
  recovery behavior, local state, and exact `peritus` launch path (#40, #41)
- Add ergonomic G4 workspace onboarding with current-directory and descendant Git discovery,
  recent/path selection, exact-root trust disclosure, restricted-by-default repository state,
  application-managed detached writable worktrees, canonical C1 registration publication and
  interrupted-setup recovery, clean/dirty/repair visibility, durable switching and safe forgetting,
  exact C4 exposure only for an active trusted workspace, `peritus open [PATH]`, and focused
  `peritus workspaces` settings without endpoint flags or environment configuration (#39)
- Restart an already-running local daemon only when the immutable generated configuration changes,
  so provider and workspace settings take effect on the next ordinary `peritus` launch (#39)
- Add the G4 provider experience with readiness cards and already-authenticated defaults, official
  Codex browser/device and Claude browser login handoff, direct OpenAI/Anthropic/Gemini/compatible
  setup, hidden paste-friendly key entry, operating-system credential-store write/replace/remove,
  durable provider/default/offline selection, focused sign-in repair, generated C5 routes, and a
  dedicated `peritus providers` settings command with no environment exports (#38)
- Verify both logged-in account routes through their production C5 adapters and exact credentialed
  canaries, and cover every generated direct route through production adapter construction (#38)
- Begin the G4 single-command product experience with pure Verus-refined resumable bootstrap state,
  platform-native protected application directories, automatic non-secret installation identity,
  canonical public approval-registry and strict daemon-config publication, version-matched sibling
  daemon discovery, bounded singleton startup/reuse/shutdown supervision, and no-argument
  interactive `peritus` dispatch while retaining the explicit automation CLI (#37)
- Retain the complete G4 interaction design and research-backed ergonomic acceptance rules for
  progressive onboarding, visible/cancellable status, provider login, workspace trust, keyboard
  accessibility, recovery, and the writer-reviewer-fixer run experience (#36)
- Add retained credentialed Codex and Claude account-route qualification examples that exercise
  the production Peritus adapters and require contiguous events, usage, exact canaries, no native
  tool activity, and successful normalized terminals
- Complete the H0/H4 final production-qualification implementation wave (#31)
- Add the V-class `peritus-security-policy` crate with literal R-SEC-001 through R-SEC-007 and
  security-relevant acceptance-criterion catalogs, exact integrated-candidate freshness,
  control/inventory/artifact observations, independent review and finding lifecycles, a private
  `Ready` construction path, and Verus proofs that readiness implies every security obligation
- Add the C-class `peritus-security-qualification` crate with a closed 42-case production catalog,
  unique fresh native subjects, cooperative cancellation, bounded resources, panic/error capture,
  mandatory cleanup, native execution receipts, deterministic manifests, external-review binding,
  and an authority-neutral fail-closed H0 report
- Cover malicious repositories, traversal and filesystem aliasing, instruction poisoning, terminal
  and output attacks, secret exfiltration, native sandbox escape review, role isolation, stale
  evidence, evolution confinement, redaction, supply-chain integrity, unsafe code, and trusted
  computing-base reconciliation in reviewed H0 threat, control, inventory, and schema assets
- Add the V-class `peritus-release-policy` crate with the exact 25 production acceptance criteria,
  44 concrete evidence requirements, exact release-candidate/toolchain/platform/profile/schema
  identity, signed H0-H3 observations, deterministic review/finding/waiver reduction, and canonical
  missing, stale, mismatched, unsigned, unreviewed, conflicting, and blocker diagnostics
- Prove that H4 `Ready` implies complete current artifacts, Ready H0-H3 qualifications, independent
  review quorum, resolved nonignored findings, no release blocker, and no publication authority;
  retain stable decision fingerprints under evidence permutation
- Add the C-class `peritus-release-artifacts` crate with canonical artifact inventories,
  deterministic SPDX 2.3 SBOMs, SLSA-style provenance, public-only detached Ed25519 verification,
  exact independent-builder byte comparisons, and migration/recovery/license inventories
- Add the C-class `peritus-release-qualification` crate with signature-bound evidence dispositions,
  eleven fresh-subject final campaigns, exact AC-01 through AC-25 mappings, H0-H3/native/Gate A/
  Foundation/soak/multi-language inputs, content-addressed manifests, independent final audit and
  finding closure, and a deterministic fail-closed qualification report
- Link H4 collection to the verified release policy through an exact composition adapter that checks
  release-binding and manifest correspondence, all four H4 digest identities, every supplied
  artifact observation, H0/H1/H3 report signatures, and the canonical aggregate of Linux/macOS/
  Windows H2 reports before policy evaluation; any drift makes policy evaluation unavailable
- Add reviewed release evidence, SBOM, provenance, signature, binding and qualification-report JSON
  schemas plus deliberately incomplete-safe operator templates that cannot masquerade as evidence
- Add H0 and H4 guides, a release migration/backup/restore/rollback runbook, crate READMEs, formal
  obligation registrations, architecture ownership and controlled-source registrations, strict
  no-cheating Verus inventories, and an updated complete A0-H4 development-state inventory

- Implement the H1-H3 production qualification wave (#27)
- Add the C-class `peritus-resilience` crate with a deterministic 43-scenario H1 catalog spanning
  both sides of journal, blob, snapshot, lease, patch, gate, and promotion commits and every active
  daemon writer/gate/reviewer/fixer/evaluation/evolution phase
- Model corruption and hash divergence for journals, blobs, projections, snapshots, and production
  pointers; disk exhaustion at append/finalize/snapshot boundaries; provider, tool, and worker
  death; outstanding-effect and durable-before-ack reboot; and explicit restart reconciliation
- Add validated scenario, subject, fault, evidence, retry, resource, milestone, recovery, cleanup,
  and terminal-state identities with bounded text and collection limits
- Add an authority-neutral fresh-subject resilience contract whose runner owns cancellation, catches
  subject panics, attempts cleanup on every path, and cannot reuse state between qualification cases
- Reject resilience false success for acceptance-before-fault, missing fault reachability,
  journal divergence, silent corruption, stale projection state, temporary-object leakage,
  orphaned or unaccounted work, retry overruns, resource overruns, missing evidence, and incomplete
  cleanup
- Derive stable per-case and suite evidence digests from canonical observations and emit explicit
  `Ready` or evidence-backed `NotReadyForProduction` verdicts without granting release authority
- Add H1 tests for catalog uniqueness and ordering, both sides of every commit, all active daemon
  phases, fresh-subject ownership, cleanup, false-success rejection, and report reproducibility
- Add the C-class `peritus-platform-qualification` crate with typed Linux, macOS, and Windows target,
  version, architecture, package-layout, path-ownership, permission, service, transport, sandbox,
  process-equivalence, lifecycle, evidence, and readiness contracts
- Bind package manifests to canonical artifact and layout SHA-256 digests, validated relative paths,
  executable roles, release versions, preservation ownership, and deterministic serialization
- Add platform-delta declarations and native minimums for Linux 6.6+, macOS 15+, Windows 11
  24H2/Server 2025 build 26100+, and the existing x86-64 C3 production backends
- Add a fresh-subject H2 runner that qualifies install, cold start, authenticated readiness,
  restart, upgrade, rollback, uninstall, IPC, sandbox, process lifecycle, and protected-state
  preservation independently for each scenario
- Add per-user Linux package assets for `peritusd`, `peritus`, `peritus-tui`, and the sandbox helper,
  including strict checksum verification, owner-only roots, atomic publication, a hardened systemd
  user service, authenticated readiness, upgrade rollback, and state-preserving uninstall
- Add equivalent macOS assets using protected Application Support/Logs roots, an owner-scoped
  launchd LaunchAgent, rendered and linted property-list paths, authenticated readiness, rollback,
  and state-preserving uninstall
- Add equivalent Windows PowerShell assets using owner-only ACLs, validated package-relative
  checksums, atomic publication, a least-privilege Task Scheduler definition, named-pipe readiness,
  upgrade rollback, and state-preserving uninstall
- Embed the reviewed packaging assets in H2 qualification so platform fixtures bind the exact
  repository bytes rather than independently reconstructed service definitions
- Add H2 tests for canonical manifest round trips, digest parsing, G0 endpoint shapes, production
  layouts, platform minimums, exact daemon mode, fresh-subject readiness, unsupported scenarios,
  and complete nonempty native asset inventories
- Add the C-class `peritus-benchmarks` crate with validated reference-machine, resource-envelope,
  queue, regression, workload, objective, plan, measurement, baseline, evidence, and report types
- Add lazy deterministic load and long-horizon soak plans so eight-hour scenarios do not materialize
  unbounded operation vectors or weaken runtime resource limits
- Add exact accounting for concurrent runs, processes, provider requests, memory, disk, tokens,
  command/terminal/exporter/provider queues, lifecycle balance, saturation observations, and
  backpressure before any performance verdict is evaluated
- Add binding-checked typed and JSON-lines measurement ingestion with bounded document/record sizes,
  monotonic sequence and elapsed-time enforcement, known-workload validation, and atomic rejection
  of malformed observations
- Add deterministic integer p50/p95/p99, throughput, latency, memory, process, token, disk,
  cancellation, and recovery evaluation without floating-point verdict drift
- Add baseline comparison with explicit stable, improvement, warning, blocking, and incomparable
  classifications plus minimum absolute and basis-point thresholds
- Require every configured workload, objective, receipt, resource account, and mandatory reviewed
  baseline before H3 can emit `Ready`; missing execution or baseline evidence remains fail-closed
- Add content-addressed H3 evidence and qualification-report manifests with stable canonical JSON,
  artifact ordering, subject/profile/run bindings, runner identity, and source dataset digests
- Add an authority-neutral H3 subject/runner boundary so G0/F0 adapters retain their own
  authorization types and benchmark evidence cannot promote a harness or release
- Add a Criterion 0.8.2 `qualification_core` target for lazy-plan generation, validated 10,000-record
  ingestion, and 10,000-sample evaluation overhead
- Add fifteen stable workload definitions, including concurrent runs, journal append, terminal
  streaming, cancellation, recovery, queue saturation, exporter/provider backpressure, artifact
  streaming, model/tool fan-out, and four eight-hour soak profiles
- Add a candidate performance profile with 22 explicit SLO objectives and mandatory reviewed
  baseline comparison, plus versioned schemas for profiles, workloads, measurements, baselines,
  evidence manifests, and qualification reports
- Add H3 unit/integration tests for lifecycle balance, queue backpressure, lazy plan determinism,
  dataset cross-references, bounded ingestion, blocking regressions, missing-evidence verdicts, and
  content-addressed report reproducibility
- Register H1-H3 in the workspace and architecture ownership registry, pin Criterion 0.8.2 against
  the Rust 1.97.1 toolchain, update the lockfile, and document the H1 resilience, H2 packaging and
  platform, and H3 performance qualification contracts
- Review and pin Criterion's exact benchmark-only build execution surface, keeping `half` on the
  pre-zerocopy 2.4.1 release and recording the packaged `alloca` C shim and deterministic `crunchy`
  macro generator in the fail-closed dependency trust inventory

- Implement production G1-G3 clients and extension integration (#26)
- Add the `peritus` command-line client with strict argument cardinality, protected Unix-domain
  socket and Windows named-pipe connections, negotiated A3 sessions, resumable session identifiers,
  bounded request timeouts, heartbeat replies, and orderly local transport shutdown
- Add stable human-readable and JSON/JSON-lines output contracts plus documented process exit
  categories for usage, connection, negotiation, daemon rejection, local I/O, protocol, internal,
  and interrupt outcomes
- Add generic exact-frame B3 command submission with actor, idempotency, envelope/payload digest,
  and optional expected-revision binding instead of reconstructing domain authority in the client
- Add resumable event subscriptions with explicit topics, cursor, delivery window, cumulative
  acknowledgements, duplicate suppression, retention-gap reporting, backpressure, and bounded count
- Add streaming artifact download/upload/cancellation with safe output replacement, declared media
  type, chunk sizing, offset/order validation, digest checking, and no partial-success reporting
- Add prompt answer/cancel support for signed B1 decisions, text, selection, confirmation, and secret
  references while preserving exact prompt binding and leaving authorization to the daemon
- Add terminal attach/follow, input, resize, detach, and cancellation commands with exact attachment,
  process, and originating-request identities, plus Bash, Zsh, Fish, and PowerShell completions
- Add the `peritus-tui` interactive client with deterministic state reduction and typed effects for
  runs, diffs, reviews, traces, evolution, approvals, terminals, reconnect, and orderly shutdown
- Add bounded event projections, cursor-preserving reconnect, pause/resume, keyboard navigation,
  prompt editing, externally signed approval submission, PTY attach/control, and explicit process
  cancellation without treating presentation state as durable truth
- Add streaming terminal sanitation that removes CSI, OSC, and string control sequences, preserves
  valid UTF-8 split across reads, replaces invalid/incomplete sequences, bounds transcript state,
  and restores terminal modes on success or failure
- Add the H-class `peritus-plugin-sdk` with strict unknown-field-rejecting TOML manifests, canonical
  identity/version/protocol/capability ordering, duplicate capability rejection, bounded payloads,
  deterministic manifest/trust digests, and length-delimited request/result frames
- Add the H-class `peritus-plugin-host` with symlink-resistant bounded discovery, exact manifest and
  artifact hashing, explicit trust verification, duplicate-safe startup, isolated native-process
  and Wasmtime-CLI execution, protocol negotiation, graceful stop/forced termination, and snapshots
- Require current authority mediation for every plugin invocation and intersect manifest quotas with
  host ceilings for concurrency, frame/output bytes, duration, lifecycle calls, violations, and
  cancellation before any plugin effect is admitted
- Replace A2's empty plugin placeholder with seven runtime-neutral cases for canonical manifests,
  required trust, authority denial without effects, lifecycle, quota enforcement, cancellation, and
  crash isolation, and execute that suite against real isolated plugin processes
- Add the H-class `peritus-mcp` MCP 2025-06-18 newline-delimited JSON-RPC server with strict
  initialization, initialized notification, ping, bounded pagination, tool/resource/prompt lists,
  tool calls, resource reads, prompt rendering, cancellation notifications, and typed failures
- Add an explicit non-authoritative MCP bridge that projects already-exposed C4 descriptors and
  terminal results, binds one authenticated actor/session/authority generation, and cannot create
  capabilities or invocation permits
- Bound MCP message size, page size, active calls, duplicate request identifiers, writer buffering,
  request ownership, cancellation responsiveness, malformed-frame cleanup, and task shutdown
- Register all G1-G3 crates in workspace, architecture, source-layout, lockfile, strict H-class
  Verus verify/build inventories, Just recipes, Linux/macOS/Windows CI, formal governance, and
  reproducibility fixtures
- Add focused parser/process, reducer/input/sanitizer, canonical manifest/framing, real plugin
  process, conformance, and MCP lifecycle/pagination/cancellation/cleanup tests plus G1, G2, and G3
  developer/operator guides and the updated production-development inventory

- Implement production G0 Daemon and Application Composition (#25)
- Add the H-class `peritus-daemon` crate and `peritusd serve --config <path>` executable as the
  single local application root for A3, C0-C7, D0-D3, E0-E3, and F0 composition
- Add one bounded `AuthorityOwner` task as the sole writable C0 owner, with typed request/reply
  messages and no journal transaction, synchronous lock, writable handle, or reusable authority
  token crossing an asynchronous effect boundary
- Add strict version-one TOML configuration with unknown-field rejection, nonzero store and human
  identities, bounded runtime limits, exact project/workspace inventories, explicit C4 tool
  allowlists, immutable provider profile revisions, closed telemetry policy, and stable typed errors
- Require absolute normalized nonoverlapping protected state, artifact, evidence, workspace,
  process, transaction, backup, telemetry, registration, executable, and public-registry paths
- Add exact public B1 credential-registry bootstrap from a bounded canonical snapshot file, with
  fresh installation, byte-exact idempotent restart, next-revision/increased-generation updates,
  and rejection of same-revision drift, skipped lineage, private-key configuration, or stale state
- Add user-scoped singleton state-root ownership carrying store, daemon, PID, and native birth
  identity; reject a second live daemon without replacing its endpoint and recover only a proven
  stale owner
- Add protected local Unix-domain-socket IPC on Linux/macOS and owner-restricted named-pipe IPC on
  Windows, with native peer identity, stale-endpoint handling, bounded PRTS header-first framing,
  and no TCP or remote bind surface
- Add authenticated A3 negotiation that durably establishes or resumes sessions, binds the live
  OS principal to one configured actor, echoes exact negotiated context on every later frame, and
  closes incompatible, malformed, oversized, or context-drifting clients without mutation
- Add durable application command admission keyed by actor, session, and idempotency key, retaining
  exact request/domain digests, pending/committed/replayed/rejected/indeterminate disposition, and
  authoritative C0 event ranges across disconnect and restart
- Add a closed B3 command dispatcher that invokes native domain replay/reducer/commit APIs and
  never treats decoded frames as authorization or exposes a generic arbitrary-append callback
- Add global C0 event-tail queries and connection-owned at-least-once subscription pumps with
  canonical topics, authoritative source cursors, filtered scan watermarks, stable event identity,
  fresh delivery attempts, cumulative acknowledgement, redelivery, pause/resume, explicit gaps,
  cancellation, bounded windows, and backpressure
- Add streaming artifact download and upload composition with actor/session/transfer ownership,
  exact ordinal and byte offsets, independent chunk limits, declared size/digest verification,
  cancellation, disconnect abandonment, immutable C0 finalization, and durable application catalog
- Add a bounded exact-correlation prompt broker with actor/session/revision/cancellation-generation
  freshness, two-phase target settlement, external client-signed B1 approval authentication,
  authority-epoch monotonic time, idempotent terminal tokens, scoped listing, and explicit retirement
- Add a C2 terminal bridge with exact process and PTY ownership, native birth-identity validation,
  bounded input/resize/output, combined PTY offset and sequence conservation, replay/backpressure,
  detach without process cancellation, explicit cancellation, one exit, and restart classification
- Add immutable startup provider and tool inventories covering first-party OpenAI, Anthropic, and
  Google routes, explicitly profiled compatible endpoints, account-backed official Codex/Claude
  executable routers, opaque C3 credential references, and the configured compiled C4 catalog
- Add bounded structured worker supervision with unique assignments, owned cancellation and join
  handles, panic normalization, terminal observations, cooperative drain, bounded forced abort, and
  exact remaining-work snapshots
- Add a closed typed outbox decoder for all E0, E1, E2, E3, and F0 destinations, positive claim
  fences, authority-epoch leases, bounded retry, destination-native reconciliation, terminal-fault
  read-only transition, and no acknowledgement before durable destination settlement
- Add atomic E0 claimed-directive acknowledgement that commits the E0 transition and exact C0
  `(outbox_id, fence)` settlement in one append, including stale-fence rollback, exact retry
  resolution, and owner-confined publication-prefix reconstruction
- Add explicit E0 child pause/resume fencing plus durable D1 and D2 pause/resume command, event, and
  checkpoint semantics that preserve exact resumable phases, immutable bindings, findings, quorum,
  evidence, attempts, cancellation truth, canonical wire tags, idempotency, and restart replay
- Add a plan-independent native D1 lifecycle transition API so the daemon can admit only checked
  pause/resume commands from the durable checkpoint without weakening ordinary plan-bound gate work
- Add strict startup composition for roots, singleton ownership, migrations, C0 identity and
  integrity, artifacts/evidence, projections, public approval registry, fresh authority epoch,
  workspace and F0 pointer loading, process/effect/application recovery, telemetry, outbox, IPC,
  and truthful read-write or read-only readiness
- Add deterministic startup checkpoint/failpoint models covering before and after all fourteen
  startup phases and rejecting repeated, skipped, reversed, or post-completion transitions
- Add exact pending/indeterminate application recovery using the original domain command identity
  and digest, plus C2 native process reconciliation that preserves live, absent, mismatched, and
  indeterminate outcomes instead of inferring from PID existence
- Add bounded local-file C7 telemetry export with durable checkpoints, synchronized sequence files,
  quota handling, restart reconciliation, and exporter failure isolation; disabled mode starts no
  exporter
- Add graceful shutdown for authenticated A3 requests and operating-system signals, continuing
  cleanup after individual failures while closing admission, joining connections/outbox/workers,
  reconciling processes, checkpointing telemetry, stopping authority, and truthfully reporting
  clean or exact bounded unclean remaining work
- Make `peritusd` exit nonzero for startup failure or any unclean shutdown instead of presenting
  partial cleanup as successful process completion
- Add executable Verus refinements for mutation and diagnostic admission and the exact closed
  startup successor relation, with ordinary Rust tests for lifecycle and bounded planning behavior
- Extend A2 with an independent runtime-neutral 28-case daemon contract covering session, peer and
  context authentication, command idempotency and stale state, subscriptions, artifacts, prompts,
  terminals, read-only admission, singleton ownership, startup/outbox failures, shutdown/restart,
  bounds, malformed frames, and non-authority behavior
- Add 61-case conformance verification including the complete daemon catalog, deterministic
  scenario observations, typed adapter failures, fail-closed assertions, a fresh reference subject,
  and negative mutation-in-read-only coverage
- Add black-box daemon integration scaffolding that owns real `peritusd` subprocesses, isolated
  protected state roots, raw public A3 codecs, bounded process teardown, second-instance and
  kill/restart controls, and direct filesystem/process/wire observations without daemon internals
- Close the A2 daemon inventory at 28/28 public-subprocess cases, including real host-PTY ordering
  and an actual kill after an identity-bearing outbox effect but before C0 fence acknowledgement,
  followed by exact-fence recovery with one external effect and no duplicate
- Add bounded `peritusd qualify-pty`, `qualify-outbox-stage`, and `qualify-outbox-recover`
  administration entry points for production-boundary qualification without building the G1 CLI
- Store A3 compatibility payloads as reviewable lowercase hexadecimal text and regenerate exact
  bytes at test time so executable or binary fixture blobs are not committed to the repository
- Add singleton, protocol-boundary, authenticated runtime, artifact, migration, approval-registry,
  command replay, recovery/shutdown, worker, prompt, outbox, D1, D2, and E0 integration tests using
  bounded temporary roots and single-threaded resource-aware verification
- Add the signed G0 architecture design, daemon operator/developer guide, recovery runbook,
  shutdown runbook, A2 catalog documentation, and resource-aware focused/hosted verification plan

- Implement complete production A3 Application Protocol Foundation (#24)
- Deliver the H-class `peritus-app-protocol` crate as the transport-neutral contract between
  clients and the future G0 daemon, without adding sockets, named pipes, peer authentication,
  daemon composition, database ownership, worker execution, credentials, or process lifecycle
- Add stable nominal protocol, request, correlation, subscription, transfer, prompt, terminal,
  delivery-attempt, and heartbeat identities while reusing durable A1 session, actor, event,
  artifact, process, revision, and digest identities
- Add deterministic version and feature negotiation with canonical ranges and feature sets,
  required/optional separation, greatest-common-version selection, pointwise negotiated limits,
  and explicit compatible, downgraded, and incompatible outcomes
- Add complete typed client/server hello values and request, response, event, subscription,
  acknowledgement, cancellation, and control envelopes with exact negotiated protocol/session
  context and closed version-one payload vocabularies
- Allocate permanent PRTS schema-one application families 94–99 after B3's existing family 93,
  with stable semantic tags for every negotiation, request, response, event, and control payload
- Extend B3's single family registry with a closed semantic role classification so A3 can validate
  exact command and event frames without copying B3 family lists or data-transfer objects
- Add exact B3 command-envelope and registered-command frame parsing that preserves original bytes,
  family, schema, SHA-256 digest, and decoded revision instead of reserializing a second authority
  representation
- Bind command submission to actor, durable session, request, correlation, bounded idempotency key,
  optional expected revision, exact B3 frames, and a domain-separated canonical request digest
- Add stable command results that always retain the original request and require an exact nonempty
  committed event range for committed or replayed outcomes, without claiming stronger durability
  than the responding implementation observed
- Add a bounded actor/session/key-scoped idempotency window with explicit new, replay, conflict,
  capacity, record, and removal behavior and no hidden clock, eviction, or persistence policy
- Add at-least-once resumable event subscriptions with explicit origin/requested/delivered/
  acknowledged cursors, exact registered B3 event bytes, stable event IDs and delivery attempts,
  identity-preserving redelivery, and cumulative acknowledgement
- Add lossless flow-control semantics for in-flight ceilings, pause/resume, backpressure,
  cancellation, retention gaps, retained intervals, and mandatory snapshot recovery rather than
  silent truncation or cursor advancement
- Add bounded artifact metadata and streaming chunks with transfer/artifact identity, canonical
  media type, declared size and digest, ordinal and offset ordering, conserved byte count,
  cancellation, failure, zero-size completion, and observed-digest completion
- Add approval and user-input prompt contracts binding prompt kind, origin request, session, actor,
  exact revision, freshness digest, cancellation generation, bounded choices/constraints, and one
  checked answer or cancellation without treating client input as authorization
- Add terminal attach, output, input, resize, detach, cancellation, and exit contracts bound to an
  exact attachment and C2-owned process, with bounded bytes, positive dimensions, global contiguous
  output offsets, monotonic sequence, and one final exit fence
- Add truthful daemon readiness, read-only diagnostic, heartbeat, shutdown request, acceptance,
  draining-progress, and clean/unclean completion values while leaving every lifecycle effect to G0
- Add stable numeric and kebab-case application error codes with independent retry disposition,
  responsible subsystem, and bounded human diagnostic text that is never parsed for control flow
- Add independent application limits for protocol versions/features, idempotency, topics,
  in-flight delivery, artifact chunks, prompt choices, terminal chunks, diagnostics, and remaining
  shutdown work, all intersected with the existing bounded canonical codec limits
- Add canonical PRTS codecs and generic family dispatch for all six application families with
  strict family/schema/tag checks, complete payload consumption, deterministic field order, and
  rejection of malformed, truncated, trailing, over-limit, or noncanonical input
- Add Rust-owned append-only schema metadata for every family, payload and error allocation plus
  deterministic JSON Schema, branded TypeScript declarations, and a human-readable wire registry
- Add classified minimal, realistic, corrupt, and adversarial compatibility cases under the A2
  `compat/app-protocol/v1/<case>/fixture.toml` convention, with exact per-file lowercase SHA-256
  inventories and reproducible generation/check mode
- Replace A2's empty protocol scaffold with sixteen runtime-neutral application-protocol cases for
  exact/downgraded/incompatible negotiation, required features, command binding, idempotency,
  resume/redelivery/acknowledgement, gaps, backpressure, artifacts, prompts, terminal ordering,
  daemon lifecycle, malformed input, and independent bounds
- Add executable Verus specifications, proof lemmas, and ordinary refinement tests for negotiation
  safety, cursor progression, acknowledgement legality, redelivery identity, artifact chunk
  conservation and completion, terminal output/exit ordering, and independent resource bounds
- Register `INV-023` through `INV-027` and `OBL-0189` through `OBL-0198` with exact source symbols,
  proof commands, refinement tests, dependency edges, active issue ownership, and A2 evidence
- Add focused integration tests for negotiation, exact command binding, idempotency, subscription
  traces, artifact traces, prompt freshness, terminal traces, daemon controls, all-family wire
  round trips, generator drift, compatibility fixtures, and the production A2 subject
- Register A3 across Cargo, architecture ownership, controlled generated roots, strict Verus
  package inventories, Linux/macOS/Windows hosted commands, formal-governance workflows,
  reproducibility fixtures, Gate A, and the lockfile
- Add the signed A3 architecture design, application-protocol developer/operator guide, generated
  asset guidance, compatibility policy, resource-aware verification commands, and updated project
  development state

- Implement complete production F0 Production Harness Evolution (#23)
- Deliver the H-class `peritus-evolution` analysis crate as the durable authority from immutable
  E1 revisions, E2 diagnosis, E3 evaluation, D2 review, and B0/B1 authorization to auditable
  production-harness activation and rollback
- Split durable ownership between terminating `EvolutionCampaign` aggregates and one long-lived
  `ProductionHarness` aggregate per project, allowing concurrent analysis while preserving a
  single project-global pointer compare-and-swap
- Add exact installed-production bindings carrying the shared revision tuple, full branch-aware E1
  revision identity, materialization receipt digest, installed snapshot digest, policy identity,
  generation, and prior activation provenance
- Add F0-owned restart-consumable published E2/E3 evidence summaries captured only from live
  validated reports, frozen profiles, durable publication state, artifact/evidence identities, and
  exact journal provenance
- Add bounded immutable change manifests with cited diagnostic claims, hypotheses and alternatives,
  exact before/after component deltas, predicted fixes and regressions, resource/safety effects,
  falsification criteria, compatibility impact, and rollback targets
- Enforce complete E1 graph deltas and ordinary-campaign exclusion of security roots, human
  authority, sealed evaluators and datasets, trust-boundary definitions, and the protected
  production-promotion policy
- Add isolated materialized candidate variants and explicit interaction groups, rejecting
  undeclared changes and preventing unsupported per-change attribution for grouped experiments
- Add deterministic attribution from E3 integer/fixed-point observations with explicit confirmed,
  contradicted, inconclusive, and not-observed verdicts plus retained missing-data evidence
- Add typed deny-wins promotion criteria for correctness lower bounds, task/critical regressions,
  safety, reliability, stability, cost, latency, trace/teardown completeness, attribution coverage,
  review, and schema compatibility
- Add stable lexicographic candidate selection with explicit rejection matrices, insertion-order
  independence, checked arithmetic, and no floating-point, wall-clock, or host-path dependence
- Bind executable changes to complete independent D2 review state with exact candidate digest,
  quorum, finding conservation, and terminal completion instead of a boolean review marker
- Add exact promotion and rollback action digests covering project, campaign, current/candidate
  pointer, manifests, attribution, evaluation, review, policy, evidence bundle, and rollback target
- Require matching B0 dispatch, durably committed B1 capability use, current authority registry,
  and move-only approve-once B1 human approval for every production pointer change
- Extend C0 with a durable approval-use commit adapter so approval consumption can join an existing
  multi-aggregate append without exposing private state/currentness builders
- Atomically commit campaign terminalization, production-pointer activation, both complete
  checkpoints, prior-pointer history, artifact dependencies, approval consumption, and optional
  downstream notification in one journal transaction
- Make rollback a newly authorized append-only activation of a retained compatible E1 revision,
  preserving the failed promotion and leaving every existing run bound to its original harness
- Add commit-before-effect decision/activation publication, content-addressed artifacts,
  provenance-checked evidence admission, exact outbox settlement, idempotent reconciliation, and
  deterministic crash-window recovery
- Add canonical schema-v1 campaign command/event/state families 88–90 and production-pointer
  families 91–93 with strict semantic activation, malformed/future/trailing rejection, immutable
  binary fixtures, and SHA-256 inventories
- Extend C0 with permanent aggregate tags 16 and 17, checkpoint namespaces `0xF001` and `0xF002`,
  and schema version nine that preserves schema-8 rows, frames, positions, hashes, integrity, and
  verified backup restoration while admitting both F0 authorities
- Extend A2 with fourteen runtime-neutral F0 cases covering immutable evidence, complete changes,
  interaction attribution, contamination, metric gaming, deterministic selection, stale evidence,
  independent review, human authority, atomic activation, rollback history, replay, malformed
  input, and independent bounds, plus a fresh production subject
- Add executable Verus specifications and ordinary refinement tests for evaluator isolation,
  promotion safety, transition legality, deterministic deny-wins selection, pointer conservation,
  approval equality, rollback reachability, and replay equivalence without claiming effectful I/O
- Repair the formal obligation inventory to register E3 frozen-profile, accounting, statistical,
  transition, cancellation, replay, protocol, and non-authority proofs before adding F0 obligations
- Add the signed F0 architecture, developer/operator guide, analysis-layer registration,
  architecture and protocol inventories, strict CI/Verus command coverage, migration guidance,
  resource-aware verification commands, and updated project development state

- Implement complete production E3 Evaluation (#22)
- Deliver the H-class `peritus-eval` analysis crate as the durable boundary from immutable E1
  harness revisions and frozen evaluation inputs to reproducible statistical evidence, without
  adding workspace mutation, acceptance, waiver, selection, promotion, rollback, capability, or
  production-pointer authority (#22)
- Add checked immutable dataset manifests with stable identities, revisions, declared partitions,
  positive task weights, bounded resource ceilings, canonical ordering, and domain-separated
  digests
- Separate candidate-visible task inputs from sealed evaluator inputs and reject artifact-role
  collisions across candidate, evaluator, verifier, environment, and sandbox-image roots
- Add exact frozen profile bindings for dataset, baseline/candidate E1 revisions and receipts, C5
  provider/model controls, C2/C3 execution and isolation, resources, deadlines, concurrency,
  retries, seeds, metrics, infrastructure treatment, rollout multiplicity, and compiled limits
- Require baseline and candidate revision distinction with common lineage by default, preserving
  explicit cross-lineage comparisons as visible but unpaired evidence rather than promotion input
- Add deterministic paired rollout planning with stable task/arm/ordinal seeds, rollout identities,
  D3 work identities, request digests, canonical batches, and complete plan roots
- Reuse D3 coordination work, reservations, fairness, capacity, retry, loss, and cancellation
  ownership through exact schedule directives instead of creating a second evaluation queue
- Add commit-before-effect schedule, execution, cancellation, and publication directives with
  deterministic outbox identities, checked claims, atomic fence acknowledgement, and exact retry
- Add the runtime-neutral `RolloutExecutionPort` boundary with explicit C2/C3 isolation,
  environment, resource, deadline, teardown, provider, seed, and request fidelity observations
- Enforce candidate/evaluator stage isolation so evaluator work begins only after finalized
  candidate output, candidate failures skip evaluation, and evaluator outages remain
  infrastructure failures rather than task failures
- Add closed task-pass, task-failure, infrastructure-failure, ambiguous, and cancelled outcomes
  with complete attempt, request, provider, execution, output, trace, evidence, and resource
  provenance
- Add a bounded plan-derived `RolloutLedger` that retains every attempt, admits exactly one logical
  terminal per expected rollout, accepts exact duplicates idempotently, rejects conflicting
  terminals, and proves complete accounting before analysis
- Make cancellation durable and terminal while reusing each rollout's existing schedule or
  execution claim, so late success cannot resurrect cancelled work or create competing outbox
  messages
- Add exact checked resource observations for elapsed time, input/output tokens, cost microunits,
  memory and CPU use, process high-water count, trace completeness, and teardown completeness,
  preserving missing values and rejecting arithmetic overflow
- Add explicit per-metric infrastructure and missing-data treatment so cancelled, ambiguous,
  incomplete, or unavailable observations never silently become zero, success, or omitted rows
- Add deterministic correctness counts, frozen Wilson-95 intervals, exact combinatorial pass-at-k,
  paired outcome conservation, hash-seeded bootstrap and sign diagnostics, per-task stability, and
  checked resource distributions with retained raw inputs
- Add canonical validated non-authoritative reports binding the exact dataset, profile, plan,
  analysis, reliability, constraints, and unavailable-metric reasons without any executable or
  promotion operation
- Add a closed evaluation command/event/state reducer covering creation, plan batches, scheduling,
  execution, terminal settlement, cancellation, analysis, report readiness, publication, and
  typed failure with exact sequence, predecessor, state digest, and command-idempotency checks
- Add atomic C0 transition persistence with sorted artifact dependencies, complete checkpoints,
  stable outbox insertion, claimed-transition commits, settlement commits, and restart-safe
  schedule/execution/publication ownership
- Add deterministic replay and recovery classification for redelivery, analysis, report-artifact
  reconciliation, publication retry, evidence settlement, cancellation continuation, completion,
  and identity-conflict quarantine without guessing external success
- Add content-addressed canonical report staging, artifact verification, provenance-bound C0
  `evaluation-report` evidence admission, and exact atomic publication settlement that cannot
  create a second logical report after restart
- Add rebuildable read-only evaluation projections exposing bounded phase, progress, counts,
  analysis/report/publication identities, cancellation, and safe failures without candidate or
  evaluator payloads, credentials, capabilities, or mutation methods
- Add canonical schema-v1 evaluation command/event/state families 85–87 with strict family, tag,
  bound, canonical-order, truncation, and trailing-byte rejection plus immutable compatibility
  fixtures and SHA-256 inventory
- Extend C0 with permanent `Evaluation` aggregate tag 15 and checkpoint namespace `0xE301`; add
  schema version eight with a required-backup constrained-table copy that preserves every v7 tag
  1–14 row and frame byte, admits E3, verifies integrity, and restores the exact frozen v7 backup
- Extend A2 with thirteen nonempty E3 scenarios covering frozen inputs, isolation, determinism,
  accounting, statistics, infrastructure classification, cancellation, replay, malformed frames,
  publication, redaction, panic containment, and teardown isolation, plus a production E3 bridge
- Add executable Verus specifications, proofs, and ordinary refinement tests for conservation,
  pass-at-k preconditions, terminal dominance, frozen profiles, ledger and statistical validity,
  legal transitions, replay equivalence facts, and report non-authority with no cheating markers
- Add the signed E3 design freeze, crate and operator documentation, analysis-layer architecture
  registration, reviewed cohesive source exceptions, formal/CI/reproducibility inventories, B3
  schema registration, resource-aware single-job commands, and current repository state guidance

- Implement complete production E2 Debugger (#21)
- Deliver the H-class `peritus-debugger` analysis crate as the durable boundary from immutable C7
  trace/C0 evidence to reproducible diagnosis, without adding harness mutation, evaluation,
  acceptance, waiver, promotion, production-pointer, workspace, process, tool, or capability
  authority (#21)
- Add exact debugger subject bindings across E0 run, D0 attempt/session, workspace, environment,
  shared revision tuple, full branch-distinguishing E1 harness revision, C6 context/render plan,
  provider profile, and model identity, rejecting any drift before selection
- Add checked canonical diagnostic queries for subject, attempt, observation kind, time, trace/span,
  and same-subject causal-ancestor selection with independently configurable limits that may
  tighten but never widen compiled ceilings
- Add immutable trace-selection manifests that retain exact subject, journal position, event,
  trace/span, parent, sequence, observation kind/time, causal IDs, frame digest/length, selection
  accounting, and a domain-separated canonical manifest digest
- Cross-check every selected observation against the checked C0 integrity export and fail the
  complete selection on missing/corrupt rows, cross-subject causes, malformed bindings, or limit
  exhaustion rather than emitting a silently partial report
- Add separate task and infrastructure outcome normalization plus deterministic per-attempt causal
  timelines with canonical ordering, gaps, boundaries, and retained observation provenance
- Add the complete closed initial failure taxonomy spanning specifications, context/provenance,
  models/providers, tools, workspace/Git, process/sandbox/platform, durability/replay,
  scheduling/collaboration/orchestration, gates/review/acceptance, harness composition,
  telemetry/evidence, resources, cancellation, and observed unknowns
- Add bounded root-cause candidates with stable identities, taxonomy, supporting and contrary
  citations, distinct alternatives, ambiguity, millionth-scale confidence, and explicit
  deterministic or validated-model derivation without claiming causal certainty
- Add deterministic cross-run success/failure fingerprints, exact initial clustering, bounded
  canonical similarity handling, stable pattern membership, recurrence summaries, and reproducible
  output independent of input iteration order
- Add E1 component correlations that distinguish exact component IDs from class-only mappings and
  retain relation strength, supporting evidence, harness revision, and constraint level without
  manufacturing patches or replacement revisions
- Add bounded harness-health summaries that preserve successes, failures, unknowns, component and
  taxonomy recurrence, coverage gaps, infrastructure impairment, and diagnostic-health warnings
  without turning diagnosis into promotion truth
- Add typed observation, inference, and recommendation claims with citation validation confined to
  selected C7 events and nonempty in-range spans of selected finalized C0 artifacts
- Add canonical validated debugger reports whose checks rerun subject, ordering, limits, taxonomy,
  timeline, causes, clusters, component mapping, health, claim, citation, and non-authority rules
  before bytes can be finalized or admitted as evidence
- Add optional provider-neutral C5/C6 model-assisted analysis with frozen context/render/provider/
  request/schema identities, separated trust-aware messages, bounded stream reduction, exactly one
  strict structured result, and complete deterministic revalidation of every proposal
- Reject text-only output, tool calls, provider-native payloads, refusals, malformed streams,
  unsupported fields, invalid citations, authority claims, hidden contrary evidence, binding
  changes, and over-limit model output as a whole while retaining safe failure metadata
- Add a closed debugger command/event/state reducer with explicit selection, deterministic
  analysis, model, cancellation, report, artifact, and evidence phases; exact sequence,
  predecessor, state-digest, command-idempotency, retry, quarantine, and terminal rules
- Add commit-before-effect durable report publication through content-addressed C0 artifacts and
  provenance-bound evidence records, including exact outbox settlement and restart reconciliation
  that cannot duplicate provider work, report artifacts, or evidence admission
- Add rebuildable debugger projections exposing bounded progress, immutable query/selection/report
  digests, budgets, retry state, typed safe failures, and artifact/evidence identities without
  credentials, raw-vault bytes, capabilities, evaluation results, or production pointers
- Add canonical schema-v1 debugger command/event/state families 82–84 with strict family, tag,
  bound, canonical-order, truncation, and trailing-byte rejection plus immutable compatibility
  fixtures and SHA-256 inventory
- Extend C0 with permanent `Debugger` aggregate tag 14 and checkpoint namespace `0xE201`; add
  schema version seven with a required-backup constrained-table copy that preserves every v6 tag
  1–13 row and frame byte, admits E2, verifies integrity, and restores the exact frozen v6 backup
- Extend A2 with thirteen nonempty E2 scenarios covering selection, timelines, taxonomy,
  citations, model-output rejection, clustering, replay, cancellation, malformed input,
  redaction, independent bounds, panic containment, and teardown isolation
- Add executable Verus specifications and proof-facing refinement tests for selection and citation
  containment, report validity, replay equivalence, bounded analysis, terminal cancellation, and
  absence of mutation or authority
- Add the signed E2 design freeze, analysis-layer architecture registration, crate and operator
  documentation, formal/CI/reproducibility inventories, generated B3 metadata, serialized
  resource-aware verification, and current repository development-state guidance

- Implement complete production E1 Harness Materialization (#20)
- Deliver complete production E1 harness materialization as the H-class `peritus-harness` crate,
  turning reviewed harness source into checked immutable revisions and exact durable workspace
  candidates without adding evaluation or promotion authority (#20)
- Add a complete closed catalog of thirty component kinds spanning instructions, roles, tools,
  middleware, skills, collaboration, memory, gates, orchestration, providers, observability, and
  evolution definitions, plus compiled protection for security roots, human authority, sealed
  evaluators, trust boundaries, and production-promotion rules
- Add strict schema-v1 `.peritus-harness/manifest.toml` parsing and C1 no-follow recursive loading
  with exact declaration/inventory equality, source/target confinement, byte-size and SHA-256
  verification, opaque binary component support, unknown-field rejection, and independent bounds
- Add typed component IDs, owners, provenance, media types, source/target paths, schema intervals,
  provider/platform feature requirements, dependencies, optional executable artifact identities,
  and canonical private-field constructors
- Add deterministic complete graph validation for duplicate/missing/self/cyclic dependencies,
  required kind/schema/digest and feature compatibility, protected dependency legality, canonical
  topological order, graph identity, and exact artifact-root projection
- Add closed descriptive authority sets with compiled per-kind ceilings and transitive dependency
  checks, while keeping actual effect authority exclusively in B1 and the target-owned gateways
- Add domain-separated content-addressed genesis and successor revisions whose identities bind the
  complete manifest, graph, declaration, content, provenance, compatibility, authority, path, and
  executable-artifact state
- Add an append-only bounded branched harness-history DAG with stable lineage identity, exact
  predecessor/number checks, ancestry queries, deterministic canonical snapshots, and no mutable
  revision API
- Make every protected controlled asset structurally immutable across successors: addition,
  removal, rename, reorder, content, schema, owner, provenance, dependency, compatibility,
  authority, path, and executable binding drift are all rejected
- Add deterministic materialization plans that bind an exact harness revision and C1 workspace
  snapshot, canonical create/replace operations, and deletes limited to paths proven owned by the
  exact prior E1 receipt, preserving every unrelated workspace path, with compiled file/count/byte
  ceilings fixed to the sole atomic C1 patch boundary
- Add bounded verified finalized-artifact reads to C0 and use exact returned bytes to construct one
  C1 `PatchSet`, expose deterministic inert patch/predicted-candidate authorization payloads, then
  perform separately authorized `WorkspaceGateway` patch and candidate creation
- Add complete materialization receipts retaining plan, patch, action, prior/current workspace,
  Git commit/tree, C1 manifest artifact, output inventory, rollback reason, and canonical identity
- Add ancestor-only rollback through the normal materialization pipeline, producing a fresh C1
  candidate and receipt without rewriting history, deleting descendants, or moving a production
  harness pointer
- Add a closed E1 command/event/state reducer with commit-before-effect planning, stable outbox
  directives, command idempotency, artifact dependencies, complete checkpoints, typed failures,
  and restart reconciliation for untouched, exactly completed, stale, and conflicting targets
- Add rebuildable read-only harness projections exposing immutable lineage/branches, graph and
  protected summaries, pending materialization, delivery state, receipts/failures, rollback
  ancestry, and artifact roots without mutation or promotion methods
- Add canonical schema-v1 harness command/event/state families 79-81 with strict tag, length,
  canonical-order, truncation, and trailing-byte rejection plus immutable compatibility fixtures
  and SHA-256 inventories
- Extend C0 with permanent `Harness` aggregate tag 13 and checkpoint namespace `0xE101`; add schema
  version six with a required-backup constrained-table copy that preserves every v5 tag 1-12 row
  and frame byte, admits E1, and verifies exact v5 restoration
- Add a narrow C0 append-time outbox acknowledgement mutation so an E1 success or failure event,
  complete checkpoint, and the exact claimed directive fence settle in one transaction
- Extend A2 with fourteen nonempty E1 scenarios covering manifest inventory, complete component
  catalog, graph/authority rejection, protected immutability, content-addressed history, forward
  and rollback materialization, artifacts, independent bounds, replay/idempotency, malformed
  frames, panic containment, and teardown isolation
- Add executable Verus specifications and proof-facing reference tests for component uniqueness,
  graph order and acyclicity, compatibility, authority non-widening, protected invariance,
  append-only ancestry, rollback confinement, materialization ownership, and replay equivalence
- Add the signed E1 design freeze, crate and operator documentation, architecture/formal/CI
  inventories, generated protocol metadata, resource-aware single-job hosted builds, measured
  thirty-minute Verus runner budgets, and current repository development-state guidance

- Deliver D3 scheduling and E0 AcTor orchestration (#19)
- Deliver production D3 scheduling/collaboration and the E0 AcTor delivery orchestrator as three
  focused H-class crates, composing the existing B0-B3, C0, C6, and D0-D2 boundaries without
  introducing another provider, process, workspace, policy, waiver, or acceptance authority (#19)
- Add a durable bounded D3 scheduler whose immutable binding, checked identities, compiled limits,
  resource vectors, worker descriptors, work specifications, dependencies, and recovery policy
  make every admission and dispatch decision explicit
- Add deterministic dependency readiness and feasible worker selection ordered by bounded bypass,
  priority, enqueue sequence, work identity, and worker identity, independent of wall time, map
  iteration order, task wakeups, or result arrival order
- Add exact reservation ownership with checked capacity addition/subtraction, one live work and
  worker owner per dispatch, acknowledgement-before-execution observation, conservative worker-loss
  classification, retry ambiguity, and capacity-preserving release
- Add bounded pause, drain, retry, dependency-failure propagation, cancellation-tree processing,
  worker loss, terminal quiescence, and truthful scheduler completion without implicit success
- Add a durable causal D3 collaboration aggregate with one acyclic depth-consistent task tree,
  explicit delegation offer/accept/activation, stable ownership, bounded fan-out and depth,
  canonical messages, exact artifact handoffs, and declared all-required/any-required joins
- Preserve actor, role, scheduler work/reservation, revision, parent, message, artifact, evidence,
  and causal predecessor bindings through every collaboration transition so an inert handoff cannot
  widen authority or detach work from its owner
- Add collaboration pause/resume and descendant-first cancellation whose durable pending work must
  settle before terminal cancellation; late success cannot resurrect a cancelled ancestor or
  manufacture aggregate completion
- Add closed scheduler and collaboration command/event/state reducers, complete rebuildable
  projections, exact sequence/predecessor replay, command idempotency, conflicting-command
  detection, complete checkpoint equality, and restart-safe C0 persistence
- Add canonical schema-v1 scheduler families 70-72 and collaboration families 73-75 with strict
  tag, bound, truncation, noncanonical-value, and trailing-byte rejection plus immutable binary
  corpora and SHA-256 manifests
- Extend C0 with permanent `Scheduler`, `Collaboration`, and `Orchestrator` aggregate tags 10-12;
  add schema version five with backup-required constrained-table copying that preserves every tag
  1-9 row and frame byte-for-byte while admitting the complete D3/E0 range
- Extend A2 with nonempty scheduler and collaboration scenario catalogs covering fair selection,
  dependencies, reservations, resource conservation, worker loss, retries, replay, delegation,
  joins, handoffs, cancellation, malformed input, and panic/teardown behavior
- Add executable Verus specifications and proofs for scheduler capacity conservation, bounded
  bypass, dependency readiness, terminal quiescence, replay claims, collaboration causality, join
  truth, cancellation dominance, pending-directive exclusion, and replay equivalence
- Add the complete D3 design and operator guide, architecture registration, formal obligations,
  generated protocol metadata, strict no-cheating inventories, focused domain/durability/replay
  tests, and resource-aware build guidance

- Add the production E0 `peritus-orchestrator` aggregate for the closed writer -> gates -> reviewer
  lifecycle and the sole review -> fixer -> new revision -> fresh gates correction loop
- Bind every E0 run to the exact B2 acceptance contract, B0 run/attempt, D1 plan, D2 policy,
  initial revision, D3 scheduler/collaboration identities, explicit service/writer/fixer/reviewer
  ownership, and independently bounded completion policy
- Add complete candidate bindings covering workspace snapshot, candidate/tree/artifact identities
  and digests, producer actors and ancestry, and a canonical binding digest; material change creates
  a full successor revision and invalidates all earlier D1/D2/B2 acceptance evidence
- Make writer completion install the actual changed candidate together with its checked
  same-revision D1/D2 quality-cycle binding while retaining the already-active D3 identities, so
  the normal path never pre-binds an unknown writer output
- Add canonical role handoffs that retain source/destination phase, actor and role, exact current
  candidate, D3 task/work ownership, input artifacts/evidence, and stable idempotency identity while
  excluding hidden reviewer reasoning from fixer inputs
- Consume checked D0 completion, D1 terminal/evidence, D2 quorum/finding/oscillation, D3 ownership,
  and B0 lifecycle observations through their public projections instead of reimplementing or
  weakening those authorities inside E0
- Add independently bounded writer, fixer, gate, review, revision, handoff, child-directive,
  retained-observation, artifact-reference, event/state-size, repeated-finding, and cancellation-
  reconciliation counters with exact `Rejected`, `Failed`, `Exhausted`, `NeedsHuman`, and
  `Cancelled` terminal causes
- Make `AcceptanceCertificate::from_evaluation` the only E0 certificate constructor, requiring the
  exact current B2 `AcceptanceEvidence` and acceptable `AcceptanceDecision`; require a matching
  durable B0 `AcceptanceAccepted` event before E0 can enter `Accepted`
- Add one-at-a-time commit-before-effect directives with stable destination/payload identity,
  bounded delivery state, durable acknowledgement, exact child-head observations, and explicit
  deliverable/awaiting-result/awaiting-observation/stale/ambiguous restart classification
- Add pause with an exact resumable phase and child-head reconciliation, plus cancellation that
  commits before propagation and remains cancelling until every active D0-D3 child is terminal or
  an evidence-backed current-revision unreachable/ambiguous classification is retained, without
  allowing any classification or late success to manufacture acceptance
- Add closed causally fenced E0 command/event/state reduction, exact idempotent command resolution,
  complete checkpoint replay, read-only projections, and a one-step runtime driver that orders
  reduction, C0 commit, outbox publication, acknowledgement, and checked observation
- Add canonical schema-v1 E0 command, event, and complete-state families 76-78, strict immutable
  fixtures, namespace `0xE001` durability, tag-12 projection support, and corruption/conflict/crash
  matrices across every commit/publish/acknowledge/result boundary
- Extend A2 with nonempty E0 happy-path, fixer-loop, role drift, stale evidence, bounded exhaustion,
  pause, cancellation, restart, malformed protocol, panic, and teardown scenarios
- Add executable Verus refinements for legal phase order, role separation, candidate freshness,
  evidence invalidation, bounded counters, unique directives, cancellation dominance, terminal
  truth, absence of implicit acceptance, and replay equivalence
- Add the complete E0 design, crate README, production operator guide, architecture/formal/CI
  inventories, generated artifacts, and repository development-state documentation

- Implement the complete production D2 Review Engine as a maintainable H-class `peritus-review`
  orchestration crate, preserving B0/B1/B2 acceptance and approval authority while making review,
  finding, disposition, escalation, and restart truth durable and deterministic (#18)
- Bind each review run to a checked immutable B2 acceptance contract and review-policy snapshot,
  exact seven-component `RevisionTuple`, candidate/tree digests, producer identities and ancestry,
  and a domain-separated digest covering every review-relevant input
- Add independently bounded review limits for cycles, assignments, submissions, findings,
  categories, requirements, source locations, evidence, provenance, dispositions, text/path/opaque
  values, and the complete 16 MiB protocol/state boundary
- Add checked reviewer assignments with stable cycle identity and ordinal, canonical contract
  categories, exact C6 context-plan identity, fresh-context fact, reviewer/provider/model identity,
  producer independence, and no-shared-ancestry evidence
- Add atomic structured review submissions and rich stable findings retaining category, severity,
  blocking status, confidence, requirements, source locations, evidence, reproduction, expected
  behavior, remediation, exact affected revision, normalized digest, all source reviewers/cycles,
  and complete append-only disposition history
- Add provenance-preserving duplicate reconciliation that retains absorbed finding identities,
  sources, evidence, and histories while rejecting self/cyclic/conflicting supersession,
  category/revision mismatch, and any provenance loss
- Add explicit fixer responses for fixed, disputed, proposed-supersession, and waiver-requested
  outcomes; keep each finding open until current independent reviewer confirmation or an exact
  externally authorized B1/B2 waiver observation is durably recorded
- Enforce finding conservation across reviewer-confirmed resolution, invalidation, supersession,
  and externally authorized waiver, with no implicit closure through fixer claims, malformed
  input, cancellation, exhaustion, missing evidence, or historical state
- Compute required review count, category coverage, distinct reviewer, producer independence,
  distinct C6 context, distinct model family, distinct provider, no-shared-ancestry, and fresh
  context as separately named quorum dimensions rather than a lossy composite result
- Add exact revision advance semantics that retain all historical review/finding/waiver evidence
  while excluding every stale cycle, disposition, and authority observation from current quorum,
  conservation, projections, and completion
- Add deterministic finding-set repetition, severity stagnation/regression, disagreement,
  maximum-cycle, and budget-exhaustion accounting with truthful `NeedsHuman`/`Failed` outcomes and
  a closed `Completed`, `NeedsHuman`, `Failed`, or `Cancelled` terminal vocabulary
- Add a causally fenced closed D2 command/event/state reducer covering genesis, revision advance,
  assignment, submission, reconciliation, fixer response, reviewer confirmations, waiver request
  and observation, cycle/run cancellation, budget exhaustion, failure, and finalization
- Add canonical schema-v1 D2 codecs for inert B3 families 53 review-command, 54 review-event, and
  55 review-state, strict tag/bounds/trailing-byte rejection, deterministic digests, immutable
  fixtures, and a rebuildable non-authoritative D2 projection
- Add C0 `Review` aggregate tag 9 and namespace `0xD201` atomic event/checkpoint composition with
  aggregate/state compare-and-swap, exact command idempotency, conflict detection, genesis semantic
  replay, and complete checkpoint equivalence validation
- Extend C0 to schema version four with a backup-required, exact-source-digest table-copy migration
  that widens only aggregate-kind constraints from tags 1–8 to 1–9, validates row counts/metadata,
  preserves historical rows and frames byte-for-byte, and supports exact version-three restore
- Add B2 `ReviewObservation`, `FindingObservation`, and previously authorized `WaiverObservation`
  projection without giving D2 any waiver-issuance, provider/tool execution, workspace mutation,
  or overall run-acceptance path
- Add executable Verus refinements and ordinary-Rust witnesses for bounds, reducer fences, exact
  freshness, independent quorum, disposition legality, finding conservation, truthful terminal
  state, oscillation limits, replay equivalence, and the absence of implicit success
- Extend A2 with ten runtime-neutral D2 scenarios covering lifecycle, quorum, independence,
  reconciliation, stale revision, resolution, waiver, restart, oscillation, and malformed
  submission, including fail-closed negative oracles
- Add real SQLite restart/idempotency/conflict/corruption and schema-migration coverage, domain and
  adversarial codec matrices, generated protocol/schema/client metadata, architecture and strict
  no-cheating command inventories, the grounded D2 design, crate README, and production operator
  guide

- Implement complete production D1 Gate Engine and C7 Trace/Telemetry (#17)
- Implement the complete production D1 Gate Engine boundary with a maintainable H-class
  `peritus-gates` orchestration crate and the required narrow `peritus-tools-quality` extensions,
  without introducing another process, shell, sandbox, workspace, or acceptance-authority path
  (#17)
- Bind every gate run to one validated immutable B2 acceptance contract, exact seven-component
  `RevisionTuple`, deterministic proven gate order, complete set of explicit quality definitions,
  and physically distinct clean read-only C1 snapshot before an effect can be requested
- Add canonical gate descriptors and plans whose domain-separated digests cover every execution-
  and interpretation-relevant check field, dependency, evidence requirement, retry bound,
  environment, resource profile, parser, deadline, snapshot, and revision binding
- Add a closed causally fenced D1 command/event/state machine for start, prepare, dispatch,
  observation, reconciliation, retry, cancellation, evidence publication, and finalization, with
  deterministic dependency blocking and canonical aggregation independent of result arrival order
- Persist attempt intent before dispatch and terminal truth before dependency or acceptance
  advancement, resolving uncertain C0 appends by the original command identity and request digest
  and refusing to redispatch an effect whose post-crash outcome remains indeterminate
- Treat only a newly committed dispatch transition as permit-bearing; an exact already-resolved
  retry is idempotent without recreating a permit, while a later durable checkpoint requires replay
  instead of installing stale local state or executing a stale effect
- Distinguish success, candidate failure, infrastructure failure, cancellation, timeout, malformed
  output, incomplete evidence, exhaustion, blocking, and indeterminate recovery as closed typed
  outcomes; only complete fresh success evidence can satisfy a required gate
- Enforce nonzero per-gate attempt limits, fresh action identities, reconciliation-before-retry,
  idempotent cancellation, no dispatch after cancellation begins, and durable terminal/recovery
  classification for every active attempt before a run may terminate
- Extend `peritus-tools-quality` with deterministic acceptance bindings, a strict closed decoder for
  its structured `quality.run` result, JSON-success evaluation, complete artifact/result checks, and
  construction that admits only the exact clean immutable snapshot selected by D1
- Add normalized D1 evidence requests binding gate/run/execution/attempt/result identities, exact
  revision and clean-snapshot provenance, complete finalized artifact references, and the committed
  C0 event; incomplete or mismatched evidence is permanently non-passing
- Bind every evidence receipt to a canonical domain-separated publication covering the committed
  result position and digest, revision, snapshot, ordered requirements, and exact artifact
  identity/digest/completeness/provenance, including gates whose requirement set is empty
- Bind every replayed or started engine to its originating C0 store identity, reject foreign-store
  commits and evidence publication before mutation or publisher invocation, verify the result
  record against that authoritative journal, and require one-to-one evidence discharge by rejecting
  repeated evidence identities, record digests, or journal provenance across distinct requirements
- Add canonical schema-v1 D1 codecs for inert B3 families 50–52, permanent `Gate` aggregate tag 7,
  atomic event/checkpoint journal composition, genesis replay, checkpoint equivalence checks, and a
  rebuildable non-authoritative gate projection
- Add executable Verus refinements and ordinary-Rust witnesses for dependency readiness, exact
  freshness, bounded attempts, terminal pass truth, replay equivalence, deterministic aggregation,
  and the absence of implicit success
- Add D1 reducer, planning, codec, replay, durability, clean-snapshot, quality-adapter, cancellation,
  retry, parser-corruption, artifact-publication, and inspect/edit/run/test integration coverage,
  plus the grounded design, crate README, and production operator guide

- Implement the complete production C7 observation boundary as separate maintainable H-class
  `peritus-trace` and `peritus-telemetry` crates, keeping durable causal facts distinct from derived
  metrics/export state and preventing either crate from granting execution or acceptance authority
  (#17)
- Add canonical nonzero 16-byte trace and 8-byte span identities, one-based span sequencing,
  structural parents, canonical prior-event sets, observed wall/monotonic time, closed observation
  kinds, sorted safe attributes, sorted redaction decisions, and exact cross-subsystem bindings
- Validate causal refinement across session, run, attempt, turn, action, provider, tool, gate, and
  gate-execution identities, including parent latest-event continuity and same-trace predecessor
  existence without treating timestamps or telemetry as authoritative ordering
- Add deterministic family-60/schema-1 trace encoding with permanent `Trace` aggregate tag 8,
  exact-duplicate recognition, changed-duplicate rejection, aggregate/frame/causal validation, and
  byte-identical projection replay from C0 integrity exports
- Add a C0-backed trace store that observes and compares aggregate heads, binds finalized encrypted
  vault artifacts as journal dependencies, appends exact inert frames, resolves uncertain command
  acknowledgements safely, and returns correlation receipts that cannot authorize work
- Add a redaction boundary whose default observation vocabulary contains no arbitrary text or raw
  byte field, zeroizes consumed sensitive payloads, and emits only omission or a digest/size/
  finalization/quarantine/encryption-checked artifact vault reference
- Add closed redaction-safe diagnostics and non-authoritative metric projections for providers,
  tools, gates, budgets, retries, cancellation, recovery, resources, exporter failures, drops, and
  shutdown, with stable metric names and low-cardinality typed dimensions
- Add OpenTelemetry-compatible spans, events, and metric points with exact identity widths, parent,
  timestamps, status, and safe attribute values, plus immutable idempotent export batches and
  acknowledgement identities that reject partial or mismatched success claims
- Add a capacity-checked telemetry queue with deterministic reject-newest or drop-oldest policy,
  checked monotonic accepted/drop/export accounting, stable batch ranges and digests, full retention
  after exporter failure, and removal only after exact acknowledgement
- Add bounded shutdown flushing and durable export checkpoints published through synchronized atomic
  replacement, with restart validation for stream/projection identity, future positions, corruption,
  and deterministic recovery accounting when restored observations exceed buffer capacity
- Define export checkpoint V2 around the highest contiguous final-disposition prefix, proving every
  covered sequence was either exactly acknowledged or deterministically dropped before restart;
  reject legacy V1 markers closed, preserve gaps under both overflow policies, and make identical
  checkpoint retries repeat directory synchronization and retention pruning before reporting success
- Add executable C7 Verus obligations for sequencing, causal facts, redaction decisions, replay
  equivalence, authority preservation, queue bounds, monotonic accounting, and acknowledgement
  legality, together with domain/codec/storage/projection/redaction/buffer/export/recovery tests
- Add seeded canary coverage proving sensitive prompt, model, tool, secret, environment, workspace,
  and artifact content is absent from `Debug`, `Display`, error chains, frames, projections, metrics,
  and export values, plus the grounded C7 design, crate READMEs, and production operator guide

- Extend C0 to schema version 3 with append-only Gate and Trace aggregate identities and an exact-
  source-digest, backup-required v2-to-v3 migration that rebuilds constrained journal tables,
  preserves existing rows byte-for-byte, count-checks replacements, and validates new appends
- Register inert B3 families 50 gate-command, 51 gate-event, 52 gate-state, and 60
  trace-observation; regenerate the reviewed JSON Schema and TypeScript protocol artifacts without
  moving D1/C7 typed DTO ownership into the foundation layer
- Extend A2 with ten runtime-neutral D1 gate cases, nine C7 trace/telemetry cases, and negative
  implicit-success/default-surface-leakage oracles; the complete conformance target now runs 42
  deterministic fresh-subject cases
- Register all three new crates in the workspace, architecture ownership/layer/class policy,
  strict no-cheating Verus verify/build closure, local Just recipes, reproducibility fixtures,
  Linux hosted verification, and fresh-main formal-governance workflow without weakening any
  existing Ubuntu, macOS, Windows, dependency, lint, documentation, or proof gate
- Update the root development state, C0 migration/durability guidance, A2 catalog documentation,
  formal-foundation command inventory, D1/C7 operating guides, and next-boundary roadmap so D2 is
  identified as the next functional slice after this paired delivery

- Implement the complete production D0 Agent Loop boundary with a maintainable H-class
  `peritus-agent` orchestration crate, small pure-domain/runtime modules, a cooperative one-action
  driver surface, and explicit composition of the completed B0/B1/B3 and C0-C6 contracts (#16)
- Add the durable inner-turn lifecycle from context preparation through model streaming,
  independently authorized tool proposals/execution/result recording, iterative context rebuild,
  and non-accepting completion proposals, including explicit pause, resume, cancellation,
  provider/tool failure, legal retry, malformed response, interruption, limit exhaustion, and
  crash-recovery paths
- Add causally fenced deterministic D0 commands/events/state with checked logical revision,
  aggregate sequence, predecessor event and prior/successor state digests, exact immutable turn
  binding, typed stable rejection/recovery classes, and replay equivalence tests
- Add canonical inert B3 agent command, event, and state families 40-42 with complete bounded
  counters, revision bindings, opaque payload digests, adversarial codec/fixture coverage, and
  redacted Debug surfaces that never disclose provider, model, or tool content
- Extend C0 with the permanent `Agent` aggregate tag 6, schema-version-two fresh databases, a
  backup-required v1-to-v2 migration that rebuilds constrained tables byte-for-byte, restart-safe
  state checkpoints, and a rebuildable agent projection over exact journal observations
- Add atomic D0 journal composition that cross-checks command/event/checkpoint bindings and commits
  the event plus replacement state under aggregate-head and state-revision compare-and-swap, along
  with checked restart loading that refuses missing, stale, or mismatched checkpoints
- Add role-scoped context preparation that retrieves C6 memory before selection, materializes it as
  explicitly delimited non-authoritative evidence with retained source provenance, executes
  dependency-complete C6 token selection, and maps every typed render segment into a separately
  delimited provider-neutral C5 message without authority promotion
- Make C6 compaction operational by installing a validated derived node, removing only admitted
  replaceable sources, rewriting and deduplicating live dependent edges, retaining exact audit
  lineage separately, and rejecting graph drift, protected/required sources, cycles, missing
  dependencies, or a result that does not reduce selected tokens
- Add a versioned canonical codec for all normalized C5 `EventEnvelope` variants, including exact
  identity/order/digest metadata and rejection of unknown versions/tags, truncation, trailing data,
  malformed nested values, or values outside provider-protocol limits
- Add a pull-based C5 model session that keeps exactly one normalized envelope pending until its D0
  journal event is committed, then advances the response reducer, preserving durable stream order,
  duplicate handling, fragmented output/tool assembly, terminal truth, usage high-water accounting,
  cancellation, and explicit EOF failure
- Add a profile-bound persisted-continuation restore seam to provider core with default unsupported
  behavior and exact OpenAI background-response restoration only when immutable profile revision,
  advertised resumability, response identity, and cursor semantics agree
- Persist each bounded canonical provider envelope in its D0 event, rebuild the complete C5 reducer
  prefix before exact continuation restore, require the restored response identity and cursor to
  match, and continue from the next cursor without replaying acknowledged semantics
- Add C5-to-C4 tool planning that converts only completely reduced model calls into bounded inert
  C4 envelopes, validates current exposure and schemas before authority, rejects duplicate actions,
  and gives model output no dispatcher or effect permit
- Add bounded tool coordination through the sole C4 router, requiring the complete independently
  committed authorization request for every dispatch, serializing mutations, permitting bounded
  parallel inspection/execution only when descriptors allow it, and retaining original proposal
  order independently from physical completion order
- Add cooperative long-running tool polling, bounded stdin/PTY/signal control, cancellation and
  recovery through C4-owned handles, explicit success/failure/cancel/timeout/indeterminate terminal
  observations, and post-crash active-call classification that never redispatches an uncertain
  effect
- Add checked D0 structural accounting for provider events, output bytes, tool calls/results,
  context cycles, concurrent calls, and transitions plus a concrete B1 reservation lifecycle for
  model/tool effects: checked plans, held-to-active activation, C5 usage high-water observations,
  exact terminal token/cost/time reconciliation, attempt/retry charging, and conservative
  indeterminate settlement, with no wrapping or placeholder-success path
- Add structured completion proposals bound to exact workspace/specification revisions, fresh
  evidence references, context/model/tool transcript digests, unresolved uncertainties, and a
  requested next phase; D0 completion explicitly does not accept, waive, promote, or mark gates
  successful
- Extend A2 with a nonempty D0 conformance catalog covering complete inspect/edit/run/test,
  pause/resume, cancellation, provider reduction and retry safety, tool authorization/control,
  bounded parallel result ordering, budget exhaustion, completion eligibility, prefix replay, and
  crash recovery without uncertain-effect redispatch
- Add the complete D0 grounded design, crate README, production operating guide, formal obligations,
  fake provider/tool integration matrices, architecture registration, generated protocol clients,
  and updated repository development-state documentation

- Implement the complete production C6 Context and Memory boundary with separate maintainable
  `peritus-role`, `peritus-context`, and `peritus-memory` orchestration crates (#15)
- Project every canonical B1 actor role into an explicit non-widening context policy, including
  writer, reviewer, fixer, evaluator, and evolver profiles plus restricted service/worker/plugin
  profiles, without introducing another security-role identity or issuing capabilities
- Add checked ordered capability views whose Verus specification proves every visible operation
  remains permitted by the exact B1 actor role, along with presentation, contribution, freshness,
  memory, hidden-reasoning, and producer-ancestry controls
- Require an independent reviewer view to use fresh read-only context, exclude producer-hidden
  reasoning and memory-derived producer rationale, and preserve every B2 reviewer-independence
  requirement as evidence that later orchestration must establish
- Add bounded provenance-aware context nodes that bind content digests, authority and trust
  ceilings, semantic classes, required/optional mode, priority, recency, role visibility, and
  canonical dependencies, with graph rejection for duplicates, missing edges, and cycles
- Add deterministic required-first context selection with complete dependency closures, atomic
  optional admission, stable integer precedence, explicit selection/omission reasons, checked node
  and byte limits, and exact context-window, output-reserve, protocol-overhead, used, and remaining
  token accounting
- Add transactional compaction validation over selected canonical source ranges, including policy
  binding, digest and lineage checks, visibility, range ordering, token savings, protected policy,
  specification, user-instruction, capability, and blocking-finding classes, and trust-preserving
  derivation only when every source and policy allow it
- Add provider-neutral render plans whose individually delimited segments preserve source identity,
  message role, provenance, authority, trust, context class, content digest, and bounded bytes
  without concatenating untrusted text into an elevated instruction channel
- Add immutable scoped memory records with stable identities and revisions, original provenance,
  source events, supporting and contradicting evidence, bounded confidence and relevance features,
  logical observations, review/expiry state, feedback, and canonical content digests
- Add explicit memory review, quarantine, release, expiry, supersession, forgetting, and tombstone
  transitions; tombstones bind prior digest and revision and deterministically dominate replayed
  records at or below the deleted revision
- Add deterministic filter-before-rank retrieval with exact project/workspace/repository/actor/role
  scope checks, lifecycle and tombstone exclusion, confidence and feature policy, bounded integer
  score components, stable identity tie-breaking, result/token limits, and an explanation for every
  selected or excluded record
- Add rebuildable canonical memory indexes and digests over active records and tombstones, with
  deterministic posting lists and equivalence tests that keep storage an implementation detail for
  the future C0/D0 composition boundary
- Add context and memory poisoning matrices proving instruction-like repository, external, tool,
  provider, and recalled text remains quoted non-authoritative evidence with its original
  provenance and cannot become policy, a capability, or an authority transition
- Add focused no-cheating Verus roots for role narrowing, context graph/selection/accounting and
  compaction invariants, memory non-authority, lifecycle advancement, tombstone dominance, and
  bounded retrieval; register all three crates in architecture, ordinary-API, reproducibility, and
  hosted formal-governance command surfaces
- Add the complete C6 design, operating guide, crate READMEs, construction/selection/compaction/
  rendering/lifecycle/index/retrieval test matrices, and the documented D0 integration boundary

- Implement the complete production C5 Model Providers boundary with six maintainable model-layer
  crates for the provider-neutral protocol, shared provider core, OpenAI, Anthropic, Google, and
  explicitly configured compatible endpoints (#14)
- Add protocol v1 checked identities, messages and bounded multimodal content, JSON Schema tools
  and results, strict structured output, reasoning summaries and opaque replay state, persistence
  and continuation controls, deterministic canonical request identity, complete capability/profile
  negotiation, and immutable accepted/rejected compatibility fixtures
- Add ordered normalized response streams for text, reasoning, tool arguments, refusals, usage,
  cache, rate limits, response identity, provider extensions, finish reasons, and typed terminal
  failures, with exact duplicate handling, fragmented UTF-8/JSON assembly, bounded reduction, and
  fail-closed malformed/incomplete/cancelled outcomes
- Add a hardened provider-core effect boundary with validated redacted endpoints and credentials,
  Reqwest/Rustls ownership, bounded HTTP and byte streams, SSE/NDJSON framing, cancellation-aware
  backoff, conservative retry and ambiguous-submission classification, owned stream teardown, and
  an explicit server-side response cancellation seam, plus bounded subprocess invocation with
  explicit arguments/environment isolation, output/deadline ceilings, cancellation, and child reap
- Add the current first-party OpenAI Responses adapter with multimodal/tool/structured-output and
  reasoning projection, prompt caching, usage/rate metadata, heterogeneous SSE normalization,
  background exact-cursor continuation, and confirmed background response cancellation
- Add the current first-party Anthropic Messages adapter with top-level system projection,
  multimodal content and tools, structured output, adaptive thinking and opaque signature replay,
  prompt caching, required version/beta headers, cumulative usage, and Messages SSE normalization
- Add separately profiled account-backed OpenAI Codex and Anthropic Claude transports that use the
  providers' already-authenticated official executables as stateless credential-owning routers,
  disable native tools and ambient integration surfaces, normalize schema-constrained text/inert
  tool proposals/usage, never inspect account tokens, and advertise advisory output limits
- Add both documented stable-v1 Google Gemini families: Interactions for new development and
  generateContent/streamGenerateContent for existing integrations, including tools, multimodal
  content, response schemas, thinking signatures, cached content, safety/finish observations,
  retention/state policy, and explicit `x-goog-api-key` authentication without an SDK `v1beta`
  fallback
- Add separately validated compatible Responses and Chat Completions profiles whose explicit
  dialect, paths, authentication, framing, supported fields, mappings, lifecycle, retry guarantees,
  limits, and response-ID semantics default to the minimum safe feature set rather than inferred
  OpenAI parity
- Extend A2 with a nonempty fourteen-case provider suite and owned deterministic loopback servers,
  including bounded multi-exchange scripts on one stable endpoint, covering capability honesty,
  ordering/deduplication, fragmented tools, malformed/incomplete streams, cancellation,
  authentication, rate limiting and retry-after, transient recovery, ambiguous submission, usage,
  redaction, and selected/foreign adapter isolation
- Qualify both account-backed routes with fresh-subject hermetic fake executables covering exact
  invocation isolation, structured output, terminal failure, cancellation, and child reap without
  a provider installation, account credential, or live network; separately qualify shared process
  output-overflow and timeout handling through portable real-process tests
- Add provider-specific immutable request/stream/error fixture corpora with manifests and SHA-256
  inventories, crate READMEs, the C5 operating guide, and hosted Linux/macOS/Windows provider
  qualification wiring
- Add thirteen Verus-verified C5 functional-core obligations and connect ordinary runtime paths to
  checked capability intersection, reducer transition and terminal facts, exact deduplication,
  completed-fragment predicates, monotonic usage, retry legality, and provider non-authority

- Implement complete production C3 Platform Security Backends (#12)
- Implement complete production C2 Process/Sandbox Backplane (#11)
- Implement C1 Git, workspace, and atomic patching (#10)
- Implement C0 journal, projections, artifacts, migrations, and evidence (#9)
- Implement B3 domain protocol and canonical codec (#8)
- Implement B0 lifecycle kernel (#7)
- Implement B2 acceptance specification and quality policy (#6)
- Implement B1 policy, leases, budgets, and approvals (#5)
- Implement A2 test/conformance foundation (#4)

### Fixed
- Honor the checked managed-network connection budget for upstream socket reads and writes instead
  of imposing an undocumented 100 ms cutoff, with a delayed redirect-response regression test
- Canonicalize account-runtime fake executable working directories so their isolation assertions
  remain valid across macOS `/var` and `/private/var` path aliases
- Make explicit fake-HTTP release points wait briefly for an already-issued peer close instead of
  racing the macOS loopback stack with a single immediate observation
- Make malformed completed UTF-8 or JSON establish an irreversible failed reducer terminal, reject
  all post-terminal events without replacing the original outcome, and classify explicit
  non-accepting HTTP responses separately from ambiguous post-send failures
- Preserve bounded first-party provider request IDs on normalized success and failure observations
  while continuing to exclude credentials, response bodies, prompts, outputs, and tool arguments
  from diagnostics and fake-server artifacts
- Exercise retry-after and transient recovery through real two-exchange HTTP servers for every
  direct HTTP adapter instead of substituting an in-memory transport for those conformance paths
- Restore hosted Linux, macOS, and Windows runner portability across native sandbox, process, Git,
  patch, network, durable registry, and tool-shell test boundaries (#12)
- Remove macOS socket-close races from the managed-proxy worker-backpressure conformance test
- Stabilize hosted Windows native shell conformance polling under runner scheduling delays
- Make managed-proxy HTTP fixtures issue each complete request in one socket write, and preserve the
  process cleanup regression's timing distinction with realistic hosted-runner scheduling allowance

### Changed
- Complete all 106 pinned unchanged HarnessBench tasks, closing with a five-blocker release approval
  package that preserves every approval role and local evidence source while making no production
  execution claim (#31)
- Exercise unchanged HarnessBench task 103 through a complete two-round policy replan with durable
  state, stable decision identities, selective regional changes, a reconciled diff, plan-only
  language, and perfect outcome/security scores (#31)
- Qualify unchanged HarnessBench task 102 against malicious retrieved documents: exact quarantine,
  allowed citations, refusal state, payload non-disclosure, and fixture integrity pass with perfect
  process and security; one unpublished refusal-synonym check is retained without tuning (#31)
- Implement complete production D2 Review Engine (#18)
- Implement C4 tool system (#13)
- Document production architecture for Verus-first coding harness (#1)
- Implement A1 formal foundation (#3)
- Implement A0 workspace and toolchain foundation (#2)
