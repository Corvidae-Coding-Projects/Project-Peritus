# G4 single-command product experience

Status: accepted implementation design for Crosslink issue #36 and slices #37 through #41
Owner: G4 product experience
Primary command: `peritus`
Release effect: none; G4 supplies product behavior and qualification evidence but cannot declare a
release ready

## Outcome

Peritus is not a complete product until a person can install it, enter a Git repository, run one
command, and reach a usable coding-agent interface:

```text
peritus
```

That command owns first-run setup, provider authentication, workspace selection and trust, local
identity, daemon lifecycle, session reconnection, and the interactive writer-reviewer-fixer
experience. The ordinary user must not create configuration files, export credentials, construct
binary registries, locate an IPC endpoint, start a service, or understand internal architecture
identifiers.

The existing daemon, CLI, and TUI are production components, but their current operator-facing
composition is not this outcome. G4 closes that product boundary without weakening the explicit
G0 authority, C1 workspace, B1 approval, C3 secret, or C5 provider boundaries.

This design permits staged implementation. No stage is an MVP and no intermediate stage changes
the repository's `NotReadyForProduction` status. The parent issue closes only after every flow and
native qualification in this document is complete.

## Non-negotiable behavior

1. `peritus` in an interactive terminal starts or resumes the product.
2. First run is an in-product, resumable flow. Repeat run normally goes straight to the selected
   workspace dashboard.
3. Provider choices are visible and understandable. A person may enable more than one provider,
   select a default, add or remove one later, and see its current login state.
4. Subscription-backed OpenAI and Anthropic login remains owned by the official `codex` and
   `claude` executables. Peritus invokes their supported interactive login commands and observes
   their status; it never extracts or copies their credentials.
5. Direct OpenAI, Anthropic, Google, and compatible-endpoint credentials are entered through a
   no-echo in-product prompt and stored in the platform credential store. They never enter an
   environment export, command argument, TOML file, trace, or terminal transcript.
6. Current-directory repository discovery is the default. Choosing another recent or entered path
   is always available.
7. Selecting a repository does not silently authorize code execution. A new repository begins in
   browse-only restricted mode and becomes executable only after an explicit, remembered trust
   action.
8. The application creates and migrates its own platform-local state, public approval registry,
   canonical C1 workspace registrations, protected roots, daemon configuration, and endpoint
   record.
9. The application owns daemon reuse, startup, readiness, reconnection, diagnostics, and orderly
   shutdown. Internal service commands remain available for operators but are not part of normal
   use.
10. The main interface can start a coding run from a natural-language task, maintain a durable
    two-way conversation for follow-up and material questions, and expose writer, reviewer, fixer,
    gate, diff, terminal, approval, cancellation, recovery, and completion state.
11. Existing explicit CLI commands remain deterministic and scriptable. Interactive prompts occur
    only when both input and output are terminals.
12. Setup and routine use work without a network connection whenever the chosen action does not
    require a provider login or model request.

## Ergonomic design basis

The interaction contract adopts the following external design guidance as evidence, not as runtime
authority:

- The [Command Line Interface Guidelines](https://clig.dev/) recommend human-first output,
  prompts only on a TTY, useful progress, and retaining diagnostic logs when a concise progress
  display fails.
- Nielsen Norman Group's
  [usability heuristics](https://media.nngroup.com/media/articles/attachments/Heuristic_Summary1-compressed.pdf)
  emphasize visible system state, familiar language, user control, consistency, error prevention,
  recognition instead of recall, and useful recovery.
- Apple's [onboarding guidance](https://developer.apple.com/design/human-interface-guidelines/onboarding)
  recommends short interactive onboarding, contextual teaching, reasonable defaults, and
  postponing nonessential customization.
- W3C guidance requires
  [keyboard-operable functionality](https://www.w3.org/WAI/fundamentals/accessibility-principles/),
  [visible focus](https://www.w3.org/WAI/WCAG21/Understanding/focus-visible), predictable focus
  changes, and perceivable success, waiting, progress, and error status.
- [RFC 8628](https://datatracker.ietf.org/doc/html/rfc8628) establishes the device-flow interaction
  pattern: show the human code and verification address, prefer a complete verification link when
  available, retain a textual fallback, and keep polling state visible and cancellable.
- VS Code's [Workspace Trust](https://code.visualstudio.com/docs/editing/workspaces/workspace-trust)
  demonstrates the appropriate distinction between safe browsing and actions that may execute
  repository-controlled code.

These sources become the following testable Peritus rules:

- Every screen has one clear primary action and a visible current step.
- Setup requests only facts needed to make the first useful run. Advanced limits, model tuning,
  compatible headers, and service policy live behind an Advanced action.
- Defaults are derived from current evidence: current Git root, already authenticated providers,
  last successful provider, last workspace, and terminal capabilities. A default is displayed, not
  hidden.
- `Enter` activates the focused primary action, `Tab` and `Shift-Tab` follow a stable focus order,
  arrows move within a list, and `Esc` returns without side effects. `Ctrl-C` always offers or
  performs bounded cancellation and restores terminal state.
- Global single-letter shortcuts are disabled while a text editor has focus. Every shortcut is
  shown in contextual help and has a menu-equivalent path.
- Color reinforces state but never carries it alone. Ready, working, warning, failed, restricted,
  and offline states also have text and stable symbols.
- Progress never spins without naming the operation. After a reasonable interval it shows elapsed
  time and a cancel action. Failure expands the retained bounded diagnostic and provides Retry,
  Back, Settings, or Open diagnostics as applicable.
- Destructive or authority-widening actions state their target and consequence before activation.
  Routine reversible choices do not receive noisy confirmation dialogs.
- Interrupted onboarding resumes at the last durably completed phase. It never repeats a provider
  login already confirmed successful or trusts a workspace merely because it was previously
  selected.
- The user never needs to remember an internal ID. IDs remain available in detail views, JSON
  output, and diagnostics.

## User journeys

### First launch from a repository

```text
$ peritus

Welcome
  -> Use /home/laurel/project                         [default]
     Choose another workspace
  -> Browse safely / Trust and allow agent actions
  -> Choose providers
       OpenAI with ChatGPT account        Ready / Sign in
       Anthropic with Claude account      Ready / Sign in
       OpenAI API                         Add key
       Anthropic API                      Add key
       Google Gemini API                  Add key
       Compatible endpoint                Configure
  -> Validate selected provider(s)
  -> Preparing workspace
  -> Starting Peritus
  -> Dashboard with prompt composer focused
```

If the current directory is inside a Git worktree, the repository root is selected. If it is not a
Git repository, the first screen offers recent workspaces and path entry without displaying an
error page. An empty provider selection may continue in offline browse mode, but starting an agent
run explains that a provider is required and opens provider settings in context.

### Repeat launch

`peritus` reuses the last workspace associated with the current repository, validates the retained
configuration generation, connects to the existing daemon or starts it, restores the durable
session, and opens the dashboard. Healthy repeat launch has no wizard.

A recoverable problem becomes a focused repair card instead of replaying all onboarding. Examples:
provider logged out, workspace moved, OS credential store locked, daemon version changed, or
configuration migration pending.

### Provider login

Subscription provider login leaves the alternate screen before invoking the official interactive
command with inherited terminal input/output:

```text
OpenAI with ChatGPT account
  1. Peritus checks `codex login status`.
  2. If needed, it runs `codex login` or the user-selected device flow.
  3. The official client opens or describes the browser authorization.
  4. Peritus rechecks status and runs a minimal adapter canary with explicit consent.
  5. The provider card becomes Ready or shows an actionable failure.

Anthropic with Claude account
  1. Peritus checks `claude auth status`.
  2. If needed, it runs `claude auth login`.
  3. Peritus rechecks status and runs a minimal adapter canary with explicit consent.
```

Peritus does not parse, store, print, or proxy account tokens. Cancel returns to the same provider
screen. A provider executable that is absent offers installation guidance and another provider;
it does not terminate onboarding.

Direct-key login uses a no-echo editor, writes the secret through the credential-store adapter,
persists only an opaque secret reference and digest, validates with the configured provider, then
zeroizes the input buffer. Pasting remains supported. The UI warns before replacing an existing
credential and can remove it from the store.

### Workspace trust

New repositories are usable immediately for inert browsing. Restricted mode allows filesystem and
Git inspection that cannot execute repository-controlled programs. It disables shell commands,
build/test tasks, tools that execute repository binaries, model-triggered effects, and agent runs.

The trust screen names the exact canonical repository root and explains that trusting permits the
agent to run tools and modify a managed worktree. Trust is recorded against repository identity and
canonical path. A repository identity change or incompatible relocation returns it to restricted
mode; merely reopening a trusted repository does not reprompt.

The default writable target is an application-managed worktree created from the selected baseline,
not an unreviewed mutation of the user's current checkout. The dashboard always shows the active
workspace and branch. Applying or publishing changes remains a separate explicit action.

### Coding run

The dashboard opens with a multiline task composer. Submitting a task shows the selected workspace,
provider/model, requested operating mode, and applicable trust/approval policy. The run view then
shows one comprehensible timeline:

```text
Understanding -> Writing -> Checking -> Reviewing -> Fixing -> Verifying -> Complete
```

Internal D0/D1/D2/D3/E0 identifiers remain in details. The main view explains current work in user
language, names the active file/tool where safe, shows elapsed time and resource use, and exposes
Pause, Cancel, Review diff, Terminal, and Details. A failure distinguishes provider, tool, test,
policy, approval, infrastructure, and recovery outcomes and gives a next action.

The run is not a one-shot form submission. Its chronological user/agent conversation is persisted
beside the run state and is independently queryable through additive A3 messages. A user message
during execution is consumed at the next safe model boundary; a message after failure,
cancellation, recovery, a material agent question, or completion resumes the same managed
worktree. The writer may enter Waiting for user only for a material choice that cannot reasonably
be inferred. Invalid edit-plan JSON receives one bounded model correction turn before becoming an
actionable conversational failure.

Completion displays the result, changed files, gate status, unresolved findings, commits if any,
and exact evidence. It never equates model completion with accepted code.

## Command contract

### Interactive entry

```text
peritus                         Open current or last workspace
peritus open [PATH]             Open a specific workspace
peritus providers               Open provider settings
peritus workspaces              Open workspace manager
peritus doctor                  Run bounded local diagnostics
```

`peritus` with no arguments requires terminal input and output. In a pipe or CI it exits with the
stable usage category and points to explicit noninteractive commands. It never waits for input that
cannot arrive.

### Expert and automation compatibility

The existing `status`, `shutdown`, `command`, `events`, `artifact`, `prompt`, `terminal`, and
`completions` families remain available. Explicit `--endpoint`, `--session`, `--json`, and timeout
behavior remain stable. The interactive product resolves its endpoint internally but does not
remove the operator surface.

Machine-readable product operations added later require all required choices as flags and fail on
ambiguity. They do not silently adopt interactive defaults.

## Component architecture

G4 is split across small crates and existing clients. No crate owns product authority.

### `peritus-product-state` (H class)

Pure and Verus-refined product state:

- stable setup phases and permitted transitions;
- provider selection and health states;
- workspace selection and trust state;
- daemon connection intent;
- resumable configuration generation and migration decisions;
- user-facing recovery classifications;
- canonical non-secret configuration schema and digest;
- invariants preventing `Ready` when required phases are incomplete.

Suggested modules:

```text
src/
  lib.rs
  config.rs
  config/codec.rs
  migration.rs
  phase.rs
  provider.rs
  recovery.rs
  transition.rs
  trust.rs
  verified.rs
  workspace.rs
```

The crate performs no filesystem, process, terminal, network, keyring, or daemon operation.

### `peritus-provider-onboarding` (C class)

Provider-specific effect adapters:

- executable discovery and version observation;
- bounded status commands and parsers;
- interactive login process ownership with inherited terminal;
- device/browser-flow presentation facts;
- direct credential capture handoff and credential-store persistence;
- exact C5 route/profile construction;
- minimal canary validation and redacted diagnostics;
- logout/removal and cancellation.

Each provider lives in its own module. The account-backed adapters reuse the same production C5
runtime constructors qualified by C5; the onboarding crate owns only setup and observation.

### `peritus-launcher` (C class)

Product composition and host effects:

- platform application/runtime/cache/log path resolution;
- atomic state transaction and migration orchestration;
- local identity provisioning and public approval-registry publication;
- workspace discovery and C1 registration composition;
- strict daemon TOML rendering from typed values;
- sibling executable resolution and version matching;
- existing-daemon probe, child/service start, readiness, endpoint publication, and shutdown;
- setup effect execution driven by `peritus-product-state`;
- bounded diagnostic bundle creation;
- handoff into `peritus-tui`.

Suggested modules:

```text
src/
  lib.rs
  app.rs
  daemon.rs
  daemon/binary.rs
  daemon/config.rs
  daemon/endpoint.rs
  daemon/supervisor.rs
  diagnostics.rs
  error.rs
  identity.rs
  layout.rs
  persistence.rs
  setup.rs
  workspace.rs
```

The launcher must not absorb A3 client code, TUI rendering, provider protocol implementations, or
daemon authority reducers.

### `peritus-tui` (G2/G4 presentation)

The existing reducer/effect separation remains. It gains:

- onboarding and repair screens driven by product-state projections;
- provider and workspace settings;
- a home/dashboard and task composer;
- explicit run-start effects over A3;
- conversation query and continuation effects over A3, with a prominent selected-run transcript;
- user-language AcTor phase projection;
- persistent status and diagnostics panels;
- focus model, accessible palette, mouse-optional navigation, and contextual help.

Large views are separate modules under `render/` and interaction reducers under `model/`; no single
view or reducer file may become the product state machine.

### `peritus-cli` (G1 entry)

The `peritus` binary dispatches no-argument interactive use to `peritus-launcher`. Existing explicit
commands retain the current G1 code path. The dependency direction is:

```text
peritus-cli -> peritus-launcher -> peritus-product-state
           \-> peritus-tui ------^
peritus-provider-onboarding -----> peritus-product-state + C5 adapters
peritus-launcher ----------------> G0/C1/B1/C3 public APIs
```

No dependency points from G0 or a verified foundation crate back into the launcher or TUI.

## Persistent state and paths

Platform defaults follow native conventions and can be inspected with `peritus doctor`:

| Data | Linux | macOS | Windows |
| --- | --- | --- | --- |
| Configuration | `$XDG_CONFIG_HOME/peritus` or `~/.config/peritus` | `~/Library/Application Support/Peritus` | `%APPDATA%\Peritus` |
| Durable data | `$XDG_DATA_HOME/peritus` or `~/.local/share/peritus` | `~/Library/Application Support/Peritus` | `%LOCALAPPDATA%\Peritus` |
| Runtime endpoint/PID | `$XDG_RUNTIME_DIR/peritus` with protected fallback | per-user Application Support runtime | per-user named pipe and protected record |
| Cache | `$XDG_CACHE_HOME/peritus` | `~/Library/Caches/Peritus` | `%LOCALAPPDATA%\Peritus\Cache` |
| Logs | state-local protected logs | `~/Library/Logs/Peritus` | `%LOCALAPPDATA%\Peritus\Logs` |

Environment variables are not required for use. Native path variables are respected as operating
system conventions; product behavior does not ask the user to define them.

The canonical product document contains schema version, generation, stable installation and actor
identities, selected workspace, workspace trust records, provider profiles and opaque credential
references, default provider/model, daemon configuration digest, last durable session, and
completed setup phases. It contains no private key, provider token, API key, raw prompt, terminal
output, or repository content.

Updates use write-new, flush, atomic replace, and parent-directory synchronization where supported.
The previous valid generation is retained for bounded recovery. On startup, an incomplete pending
generation is either completed from its recorded phase or discarded in favor of the last valid
generation; partial state never becomes `Ready`.

## Local identity and approvals

First run provisions a device-local Ed25519 approval identity using operating-system randomness.
The public credential enters the canonical B1 registry snapshot. Private signing material is
stored through a dedicated local-signer boundary:

- macOS Keychain and Windows Credential Manager are preferred;
- Linux Secret Service is preferred when available;
- a protected owner-only local signer file is the explicit Linux fallback, with its protection
  status visible in Settings and diagnostics.

The fallback is never a provider-token store and is never written inside a repository. Key bytes
are zeroized after use. Replacing or losing the local identity is a visible registry transition,
not silent regeneration against existing durable state.

The TUI may request a signature from this local signer only for an approval decision the user is
currently viewing and explicitly activates. It cannot sign arbitrary model output or bypass B1
scope, freshness, one-use, or policy validation.

## Daemon ownership

The launcher resolves `peritusd` adjacent to the current executable in packaged installs and uses
an explicit development override only in developer commands. It rejects incompatible binary
versions before changing durable state.

Startup sequence:

1. acquire the product-state singleton;
2. load/migrate the last valid product generation;
3. render and digest strict daemon configuration;
4. probe the protected endpoint publication;
5. reuse a matching healthy daemon or start the matching sibling binary;
6. await bounded endpoint publication and A3 readiness while showing progress;
7. enter the TUI and retain reconnection ownership;
8. on UI exit, leave healthy daemon work running by default and explain that state; an explicit
   Stop Peritus action requests orderly shutdown.

Stale PID, endpoint, or version records are reconciled by probing process and A3 identity. They are
never trusted merely because a file exists. Logs are bounded, protected, and available from the
failure screen.

## Provider catalog

| User-facing choice | Credential owner | Login/setup | Stored Peritus data |
| --- | --- | --- | --- |
| OpenAI with ChatGPT account | official Codex executable | `codex login` / supported device flow | executable selection, status observation, C5 profile |
| Anthropic with Claude account | official Claude executable | `claude auth login` | executable selection, status observation, C5 profile |
| OpenAI API | OS credential store | hidden key entry | opaque reference, endpoint/profile |
| Anthropic API | OS credential store | hidden key entry | opaque reference, endpoint/profile |
| Google Gemini API | OS credential store | hidden key entry | opaque reference, dialect/profile |
| Compatible endpoint | OS credential store when required | endpoint, dialect, optional header, hidden key | validated endpoint/profile and opaque reference |

Provider cards show Available, Sign-in required, Validating, Ready, Offline, Unsupported version,
and Failed. They show what account mechanism will be used before launching it. Multiple Ready
providers are retained; one exact provider/model is selected for each run.

## Failure and recovery contract

| Failure | User experience | Durable behavior |
| --- | --- | --- |
| Setup interrupted | Resume at last completed step | Pending generation remains non-ready |
| Provider login cancelled | Return to provider list | No provider marked ready |
| Provider status malformed | Show unsupported-version action | Retain prior valid provider, no secret access |
| API key rejected | Re-enter/remove/back | Secret reference not selected as ready |
| Credential store locked | Explain unlock/retry | No plaintext fallback for provider tokens |
| Repository not Git | Recent/path chooser | No workspace registration created |
| Workspace untrusted | Browse-only banner | No execution authority exposed |
| Workspace moved/drifted | Repair or register anew | Old registration remains historical |
| Daemon absent | Start with progress | One owned startup attempt |
| Daemon version mismatch | Restart/upgrade guidance | No mixed-version state mutation |
| Daemon startup fails | Summary plus expandable logs | Partial endpoint is reconciled |
| TUI disconnects | Visible reconnect countdown/cancel | Durable session cursor retained |
| Terminal too small/noninteractive | Clear fallback/instructions | No raw-mode leak or prompt hang |

Errors name the failed action and next recovery, not only an internal subsystem. Diagnostic detail
may include stable codes and bounded paths but never credentials, raw provider content, or secret
input.

## Delivery slices

The slices are complete production increments with explicit dependencies. They are not release
milestones by themselves.

### #37 G4.1 local bootstrap and daemon ownership

- add `peritus-product-state` and `peritus-launcher` with registered architecture boundaries;
- implement native layout, canonical resumable state, automatic empty/public identity foundation,
  typed daemon config rendering, endpoint publication, sibling daemon supervision, and `peritus`
  no-argument dispatch;
- preserve every existing explicit CLI command;
- prove interrupted state and stale daemon records fail closed;
- finish with a launcher test that reaches a real disposable `peritusd` status using no hand-made
  files.

### #38 G4.2 provider onboarding and account management

- add `peritus-provider-onboarding`;
- implement all provider catalog entries, official account login/status, credential-store writes,
  C5 profiles, canaries, removal, switching, and repair;
- test both fakes and authenticated opt-in routes; retain secrets-leak scans.

### #39 G4.3 workspace selection, trust, and registration

- implement repository discovery, recent list, path editor, restricted mode, durable trust, managed
  worktree creation, C1 registration, repair, switching, and removal;
- qualify clean, dirty, moved, deleted, untrusted, and interrupted repositories.

### #40 G4.4 interactive prompt and AcTor run experience

- implement the complete information architecture and task composer;
- bridge run start and control to A3/E0 without granting UI authority;
- persist and resume bounded per-run conversations, including direct writer questions and
  malformed-plan correction;
- render writer/reviewer/fixer/gates/diffs/approvals/terminal/recovery/completion;
- complete keyboard, focus, narrow-terminal, reduced-color, and terminal-restoration tests.

### #41 G4.5 packaging and native qualification

- make installed `peritus` locate the exact companion and native assets;
- integrate per-user service and state migration semantics;
- run clean-machine, interrupted-first-run, repeat-run, upgrade, rollback, and uninstall campaigns
  on Linux, macOS, and Windows;
- run account-backed Codex and Claude E2E outside ordinary CI, direct-provider fakes in CI, and a
  representative full coding task per supported platform;
- retain evidence and update all user-facing documentation.

Slices #38 and #39 may proceed in parallel after the #37 state interfaces stabilize. #40 consumes
their public models. #41 consumes every preceding slice. H3 production load/soak and H4 final
qualification then run against the exact integrated candidate.

## Verification and acceptance

### Deterministic tests

- product-state transition tables and Verus invariants;
- canonical configuration round trips, unknown-field rejection, migration, atomic recovery, and
  generation monotonicity;
- provider status parsing for supported/unsupported/failure output and secret-redaction scans;
- setup cancellation after every effect boundary;
- workspace discovery, trust, identity drift, registration, and managed worktree recovery;
- daemon binary/version resolution, singleton startup, readiness, stale record, crash, restart, and
  shutdown;
- TUI reducer snapshots for every setup and repair phase;
- keyboard-only traversal, focus visibility state, editor shortcut isolation, cancellation, narrow
  layout, conversational follow-up/resume, and terminal restoration;
- non-TTY no-prompt behavior and compatibility of every existing CLI command.

### End-to-end evidence

For each supported native platform:

1. install into a clean user profile;
2. enter a representative Git repository;
3. run only `peritus`;
4. select or complete provider login without exporting a variable;
5. trust and register the workspace;
6. start a real writer-reviewer-fixer task;
7. observe edits, tests, review, repair, diff, terminal, and completion;
8. exit and rerun `peritus` to prove resumption;
9. interrupt login, setup, daemon startup, and an active run at controlled points and prove useful
   recovery;
10. upgrade and uninstall while preserving or purging data according to the selected operation.

The acceptance observer records command count, prompts, time to dashboard, time to first task,
errors, recoveries, provider route, workspace/trust state, daemon lifecycle, terminal restoration,
and final authoritative outcomes. A scripted smoke test cannot substitute for interactive native
evidence.

## Completion rule

G4 is ready only when a clean supported machine can complete the end-to-end evidence sequence with
one initial command and no manual configuration or environment exports, every provider path has a
usable in-product setup/recovery flow, the TUI can initiate and finish a real coding run, existing
automation remains compatible, native qualification is green, and the exact evidence is admitted
to H4.

Until then the product is implemented in substantial parts but is not pleasantly playable and is
not complete.
