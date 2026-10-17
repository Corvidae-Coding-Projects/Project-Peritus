# Real module foundation replays

Run only in disposable state. These are executable consumers of the actual program
and public libraries, not unit/mock runners. The terminal/browser journey uses an
already signed-in real provider and therefore makes inference requests.

Build the application with one Cargo process and `--jobs 2`:

```sh
cargo build --release --jobs 2 -p peritus-cli -p peritus-daemon -p peritus-web -p peritus-plugin-host
cd webui
npm run build
```

`module-registry.py --binary /absolute/path/to/peritus --output /fresh/directory`
exercises installation and registry behavior using separate real CLI processes.

`module-daemon.py` uses Python with `pexpect` and `pyte`. Supply `--binary`,
`--provider-state` (an existing nonsecret product-state JSON containing a working
provider selection), and a fresh `--output` directory. It creates independent
XDG roots and an inert project, opens the actual TUI, starts a disposable gateway
on port 4243 (override with `--port`), runs Source brief, inspects its checkpoints, then stops/restarts
only its own daemon and reattaches to resume an installed two-stage module. Use
`--managed --no-browser` for a second journey through a real Git source checkout
and managed worktree. Source checkout status must remain clean. The fixture selects
gpt-6.1-sol for both model roles without changing the user's saved configuration.

Once the terminal log reports `four role workflow, status, completed-job resume
passed; awaiting browser`, run from the repository root:

```sh
node docs/testing/reproducers/module-browser.mjs http://127.0.0.1:4243 /same/output
```

It drives Chromium's actual page, composer, sends and displayed replies. It uses
`webui/node_modules/playwright` by default; `PERITUS_PLAYWRIGHT` can point to an
existing Playwright installation. Chromium defaults to `/usr/bin/google-chrome`.
No request interception or backend substitution is used. The terminal replay waits
for `browser-done` before stopping its disposable daemon.

`module-foundation-consumer.rs` is an independent executable embedding the public
`peritus-modules`, `peritus-plugin-host` and `peritus-plugin-sdk` APIs. Build it in a
temporary Cargo project with `[workspace]`, path dependencies to these three
crates, `serde_json = "=1.0.149"`, and `tokio = { version = "=1.53.1", features =
["macros", "rt-multi-thread", "time"] }`. Use the repository's release profile and
`--jobs 2`; it can reuse the root target directory. Run the resulting executable
with a fresh output directory as its sole argument.

That executable also acts as the actual SDK protocol plugin subprocess. The host
launches its copied binary, negotiates framed initialization, invokes it, and
owns its process group. Modes include a responding plugin, an invalid frame, and
a plugin that stops draining stdin and owns a sleeping descendant. Linux `/proc`
provides the independent process-liveness observation. No external user process
is targeted. The module consumer generates 24 Unicode/length variants and checks
close/reopen, ownership exclusion, explicit dependency reads, exact paging,
ordered publication, and independently corrupted retained artifacts.

The scripts leave their evidence and disposable data in the supplied output roots.
They do not change the installed Peritus binaries or user daemons.

`module-steering.py` takes the same binary/provider-state/output arguments. It
sends a new message during an actual module provider request, checks that the
superseded stage remains uncommitted and the new message is handled by ordinary
Chat, explicitly resumes the original module, then checks an ordinary Chat reply.

`module-resume-existing.py --binary PATH --fixture EXISTING_MODULE_DAEMON_OUTPUT`
reuses an interrupted disposable journey. It verifies a single submission at exact-run
reattach, committed-stage reuse, completed resume without inference, Plan/Chat mode
across another reattach, and rejection of a declared effectful tool before inference.
`--allow-completed` replays the remaining client/permission checks against a job that
already finished. The terminal driver drains rendered protocol receipts between
commands; private server completion alone is not a client acknowledgement.
