# Native Rust plugins and Module Market

The module market contains independently compiled **Rust code plugins**. Feature
implementation belongs to those crates. Peritus supplies a generic native process
host and shared services. Deep Research's coordinator, source-admission ledger,
role schemas, report renderer, and correction routing live in the market repository.

Use the existing `peritus-plugin-host` Process lifecycle and SDK framed IPC. Stable
messages cross the process boundary; no unstable Rust dynamic-library ABI, WASM,
interpreter, or compiled-in research implementation is introduced. `Manifest.code`
declares ABI 1, target-specific native artifacts, and maximum borrowed host tools.
Old declarative workflows retain their previous representation and digests.

The generic product bridge launches an explicitly enabled, digest-verified executable
in its own owned process. The plugin interprets commands and returns either a public
reply/action menu or an agent request. Peritus supplies selected writer/reviewer
models, permitted read-only host tools, plugin-exported functions, actual observation
callbacks, cancellation, retained context and bounded IPC. Plugin code owns sequencing,
state/checkpoints, evidence rules, and its resume semantics. Native code runs with
ambient OS account authority; host-service mediation is not an OS sandbox.

Catalog refresh resolves the official repository's main branch once to an immutable
commit. Install stages declared resources and validates the entire package digest
before selection; installation runs no code and updates start disabled. Cached menus
work offline. Native executables are selected by exact platform/architecture; unsupported
platforms receive an explicit error rather than running another platform's artifact.

TUI and GUI render generic plugin-supplied action metadata bound to installed package
digest and run identity. `/module ID ARGUMENTS` reaches executable plugin code; core
menus contain no Deep Research ID or research command aliases. Versioned private text
attachments can be explicitly supplied to conversation/Design with exact paged reads.

Real-interface acceptance: publish and fetch the actual catalog; install/enable/use in
TUI and GUI; run a separately compiled native Rust example without rebuilding Peritus;
prepare research without network, reject stale Start and implicit Resume approval;
run actual source gathering, independent admission, synthesis and final checking;
reopen durable plugin state; stop/restart/resume; attach a checked report to Design;
confirm ordinary Chat and prior workflow packages still function. Supplement with
independent public SDK/library consumers and varied source/range/identity inputs.
No unit, mock, formal, or repository verification suites.

Linux execution does not establish macOS/Windows, arbitrary PDF extraction, or multi-hour
reliability. Record actual artifacts, observed defects, final replays and these limits
in the campaign evidence. Preserve live user daemons and installations during testing.
