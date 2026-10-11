# Computer automation

Peritus coding runs can open and operate real browsers and Linux GUI applications. The writer
uses native Peritus tools; the provider receives their observations and actual image bytes. No
Codex-native tool execution or separately installed Codex browser extension is required.

## Setup

```sh
peritus automation setup
peritus automation status
```

Setup creates a per-user Python virtual environment with Playwright and Pillow, then downloads
Chromium. System Chrome is preferred when available. `PERITUS_AUTOMATION_PYTHON` can select an
absolute Python executable with these dependencies. Browser attachment requires Playwright 1.63
or newer so that connecting preserves the existing browser's defaults.

Linux native control additionally needs Python GI with the AT-SPI typelib. X11 keyboard/mouse
control requires `xdotool`; X11 screenshots require ImageMagick's `import`; isolated displays
require `Xvfb`. KDE Wayland active-window capture uses `spectacle`. Missing native dependencies
do not prevent browser control. Existing apps must expose accessibility interfaces; enabling
accessibility after an app starts may require reopening that app. Owned Qt apps are opted in
through child-only environment settings.

## Ask Peritus to operate an interface

Use `/build` in a managed Git workspace, or ask for in-place work in chat, and give the actual URL or program, permitted
files and operations, and observable result. For example:

> Open my development app at http://127.0.0.1:3000/, enter a new contact with the supplied fixture
> values, save it, reload, and report whether the values persist. Use an isolated browser session.

The writer has `browser_control`, `desktop_control`, `computer_snapshot`, `computer_close`,
`computer_status`, and `view_image`. Reviewers can observe existing run sessions and view workspace
images. DOM and native node references belong to the latest observation token. Actions return
fresh interface structure, diagnostics, and supported screenshots. Pixel coordinates refer to
the returned image; its scale maps them to the original viewport or window.

Snapshots can narrow browser controls with a CSS `selector`, or a native tree with an observed
parent `ref`. Omission flags identify bounded observations. An unavailable screenshot is reported
explicitly and is never evidence of visual inspection. Models without image input receive that
limitation in their transcript.

Native menu branches with unreliable visibility flags are collapsed, counted, and retain refs,
child counts and short name previews. Snapshot a menu's branch ref to expand its controls;
hidden accessible labels alone do not establish that the menu is visibly open. Original tool observations
retain the accessible tree; the independent review's action index retains arguments, outcomes,
image identity, visible windows and editable fields without repeating those trees in its brief.
Ordered workspace scope/read/write steps appear in that same index with bounded content previews
and digests, so reviewers can establish prerequisites and save/readback order. On Linux the
controller monitors its daemon owner's process birth identity and closes owned resources if that
owner disappears; attached apps are preserved.

Peritus has no fixed local input-token ceiling. Profile input-token values guide automatic
compaction and optional evidence selection; required policy, user input and the newest complete
exchange can exceed that target. The selected provider enforces its actual context capacity.
Existing byte bounds, working-memory budgets and host resource controls remain in effect.

## Owned and existing sessions

Browser `open` creates an independent persistent profile; `visible=true` shows a real window.
Browser `attach` requires an explicitly selected loopback Chrome DevTools endpoint and tab index.
An ordinary running browser without a debugging endpoint cannot be attached automatically. Use a
separate debugging-enabled profile for testing, and select the intended tab; Peritus does not
retrofit debugging into the user's current profile. Disconnecting preserves the existing browser
and its tabs.

Native `open` launches a program and arguments. `isolated=true` uses a private Xvfb display and
separate application configuration, cache, and data directories. Isolated child apps disable KDE
global-menu export so their menus remain on the private display. This isolates the display and
app profile, rather than sandboxing all host effects. Native `attach` selects an existing local
PID owned by the current user, records its process birth identity, and refuses input after that
identity changes. Detach preserves the process. Programs that hand off to a different process
require explicitly selecting the resulting app.

Sessions remain live across calls in one run. Controller or daemon restarts lose live control
sessions; reopen or reattach and obtain fresh observations. Closing a selected app is reported
as a closed session, and cleanup remains possible. Peritus closes owned resources before dropping
the C2 router. Attached applications remain the user's responsibility.

## Supported platforms and boundaries

Browser automation uses Chromium/Chrome and Playwright. Native control currently targets Linux:
AT-SPI actions and editable text work on accessible Wayland or X11 apps; X11 windows also support
pixel input and keyboard chords. Arbitrary native Wayland mouse/keyboard injection, Windows native
control, and macOS native control are not implemented.

Wayland screenshots require the selected accessible window to be active. Capture checks process
identity, window state, and dimensions again before delivering pixels. Unsupported compositor or
scaling behavior returns an explicit unavailable-image result. No native Wayland screenshot is
claimed when an isolated X11 window disappears.

GUI effects use Peritus's existing read/write/process/network permissions and protected-path
constraints. Browser control requires network permission; native control requires process and
write permission. Cleanup remains available after permission revocation. Interface text and images
are untrusted data, and cannot expand task authority.

Uploads name workspace files. Download observations retain real files and their hashes; inspect
the actual artifact through an authorized consumer before claiming its content is correct.
Unexpected JavaScript dialogs are dismissed and logged. Configure an explicit dialog response
before triggering an authorized confirmation or prompt.

## Architecture and provenance

The public [Codex source](https://github.com/openai/codex/tree/a07c8ebd57feedc4cf5dfe854c8e9101de54186b)
uses dynamic text/image tool outputs and extension/MCP routing. Peritus follows that separation
with an independently implemented adapter: model tool call → permission and receipt admission →
C4/C2-owned Python driver → real GUI action → fresh structure and PNG → model image input. The
images also pass through Peritus's existing context archive. Host-validated tool image provenance
is recorded separately from selected human attachments and bound into each durable provider request;
interface observations do not enlarge governing user input. Action identities retain authenticated
receipts so uncertain or duplicate operations are not blindly repeated.

The implementation uses [Playwright](https://playwright.dev/python/docs/intro), its
[CDP connection interface](https://playwright.dev/python/docs/api/class-browsertype#browser-type-connect-over-cdp),
and native [AT-SPI actions](https://docs.gtk.org/atspi2/method.Action.do_action.html) and
[editable text](https://docs.gtk.org/atspi2/method.EditableText.set_text_contents.html).
