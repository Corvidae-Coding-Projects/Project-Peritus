# Experimental WebUI security boundaries

The WebUI is an optional, single-user localhost interface. It is not a remote
administration service, an authentication system for multiple users, or a new
execution sandbox. Do not expose it through a public listener, reverse proxy,
shared tunnel, or port-forwarding service. The gateway runs with its user's OS
permissions; untrusted local processes are outside this isolation boundary.

## Authority and local actions

Native agent conversations use the existing negotiated daemon protocol. The
daemon owns agent execution, workspace admission, approvals, and candidate
qualification. Opening a browser project does not register or trust a daemon
workspace. Read-only or draining daemon readiness does not permit native sends.

The file editor and ordinary Git panel are direct, explicit local-user tools.
Their actions do not traverse the agent's approval or sandbox pipeline. Opening
a project makes its root available to those controls even before daemon setup.
Git can execute repository hooks, configured helpers, and credential programs;
only connect repositories you are willing to operate as your local account.
The retained terminal runs the installed Peritus CLI, with its own conversation
selection and interactive setup/approval flows.

## Browser and file delivery

- The gateway binds IPv4 loopback and checks Host, Origin, and cross-site fetch
  metadata. API requests require a per-process token or same-site HTTP-only
  cookie; the same-origin bootstrap supplies that token. This protects the
  browser boundary, not against other processes running on the same machine.
- A development Origin exception permits the loopback Vite server on port 5173.
  Production runs need only the built assets and Rust gateway.
- File and repository resolution rejects canonical paths outside the selected
  project, including outward symlinks. This is application-level path checking,
  not an OS sandbox against another local process concurrently replacing paths.
- Markdown is sanitized. Raw active content retains a restrictive sandbox and
  `nosniff`. The PDF-only route checks a bounded signature, forces PDF MIME, and
  permits native same-origin embedding. The browser performs PDF parsing; the
  signature check is not full format validation or a malware scan.
- Text reads and saves are capped at 50 MiB. Raw media/downloads stream with
  byte-range support. Keep the browser updated for media/PDF engine fixes.
- Saves reject changed revisions and linked files, preserve supported UTF-8
  formats and permissions, and atomically replace the selected regular file.
  Concurrent local editing still warrants care; keep normal backups/version control.

## State, context, and recovery

Browser-local storage retains presentation state and unsent chat drafts. The
gateway stores project/session metadata, operation records, and immutable text
attachment snapshots under the user's state directory. Treat these records as
potentially sensitive. Clearing browser data or deleting state is not a routine
recovery step. Editor drafts are page-local, not crash-persistent.

Viewing a file never attaches it. Native attachments are explicit UTF-8 snapshots,
with documented per-file and total-message limits. PDFs/images/audio are not
silently converted or attached to native chat.

An interrupted mutation may already have taken effect. Original operation IDs
and native request/response frames are retained; the interface holds unresolved
targets instead of blindly replaying them. Where no authoritative daemon receipt
query exists, the user must inspect the original target before acknowledging the
hold. Acknowledgement neither repeats nor declares the old action successful.
Gateway shutdown terminates its retained CLI processes, but not daemon-owned work.

## Review and support limits

Tests exercise local protocol identity checks, confinement, origin/token rejection,
bounded files, save conflicts, Git workflows, and uncertain-operation recovery.
They are not a security audit or complete platform qualification. The initial
contribution does not claim remote hosting, multi-user isolation, full CLI-panel
parity, browser-engine parity, or accessibility certification. Release packaging
and broader qualification require separate review before standard distribution.
