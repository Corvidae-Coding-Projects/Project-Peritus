# Browser and desktop automation

## Scope and requirements

Add real browser and native GUI control to Peritus, including isolated test sessions and explicit attachment to existing applications. The user selected both modes. Use actual interfaces and the existing real-testing workflow; do not create or run unit, formal, or repository verification suites. Preserve the design/check/reviewer/fixer pipeline and existing process and permission controls.

## Source grounding

Inspected official Codex source at `a07c8ebd57feedc4cf5dfe854c8e9101de54186b`, cloned to `/home/doll/Peritus-automation-reference-codex`. Relevant files are `codex-rs/protocol/src/dynamic_tools.rs`, `codex-rs/core/src/tools/handlers/view_image.rs`, `codex-rs/core/src/tools/handlers/mcp.rs`, and `codex-rs/config/src/computer_use.rs`. Dynamic tools return text and image items; image viewing checks model modality, reads the actual bytes, and returns image input. Browser/computer-use capabilities are supplied by extensions rather than a universal GUI engine in the CLI. Do not copy the separately installed proprietary browser plugin implementation.

References: [Codex dynamic tool protocol](https://github.com/openai/codex/blob/a07c8ebd57feedc4cf5dfe854c8e9101de54186b/codex-rs/protocol/src/dynamic_tools.rs), [image tool](https://github.com/openai/codex/blob/a07c8ebd57feedc4cf5dfe854c8e9101de54186b/codex-rs/core/src/tools/handlers/view_image.rs), [Playwright browser connections](https://playwright.dev/python/docs/api/class-browsertype#browser-type-connect-over-cdp), [AT-SPI native actions](https://docs.gtk.org/atspi2/method.Action.do_action.html).

## Architecture

- Add a dedicated ordinary host adapter, `peritus-tools-computer`, with a bundled Python control driver. Browser control uses the Playwright library, not a test framework. Native Linux application control uses AT-SPI accessible actions and editable text; X11 pixel input and isolated Xvfb displays supplement accessible controls. Report unsupported host/backend capabilities explicitly.
- Start the persistent driver through the existing run-owned C4/C2 command runtime. Keep browser pages, native app identities, and observations alive across model tool calls and role invocations sharing that runtime. Use private runtime storage for profiles, snapshots, and RPC metadata, outside the edited workspace.
- Use a local authenticated control channel with bounded requests, timeouts, and idempotent action receipts. Do not expose a network-accessible control service. Scope native attachment to an explicit PID and observed birth identity; bind browser attachment to an explicit local CDP endpoint and selected tab.
- Expose structured tools for status, opening/attaching, observing, acting, and closing. Every action requires a current observation identity and returns a new observation. Snapshots provide real accessible structure, stable references for that observation, actual screenshot bytes, and session identity. User/browser content remains untrusted observation data.
- Extend the developer tool executor with an optional image-observation drain. D0 appends images to the provider-neutral model transcript and existing durable context archive, preserving real visual input rather than putting base64 text into a JSON observation. Models without image input receive an explicit limitation and can use structured observations.
- Apply live host read/write/process/network permissions before adapter effects. Retain role restrictions, task path protection, cancellation, and effect receipts. Existing apps are detached on cleanup; only owned test processes and profiles are closed.
- Bundle usage guidance into every coding-role prompt and document native setup through a normal Peritus CLI command. Optional automation dependencies are installed into a Peritus-owned user runtime, never by generating project tests or modifying unrelated repositories.

## Alternatives considered

A provider-specific Codex plugin proxy would leave other Peritus providers without these tools and depends on private host services. Shell-only scripts can drive a browser but do not supply stable model-visible sessions or screenshots. The provider-neutral host adapter makes those boundaries explicit and reuses Peritus's process ownership.

## Acceptance and real testing

Exercise a real local web app through opening, form input, click, navigation, screenshot, invalid/stale references, and persistence across calls. Attach to a separately launched test browser and show that detaching leaves its process and unrelated tabs alive. Operate a real native GUI, read back an independently defined saved/result value, and test both owned isolated sessions and explicit existing-app attachment. Have a real Peritus model use these tools and interpret visual evidence. Check permission denial and cleanup through actual execution. Record defects, replay artifacts, platform limits, and exact tested build origins in Markdown. No unsupported platform or inaccessible app counts as passed.

## Compatibility and recovery

No changes to existing saved conversation or tool-result formats are required by the optional executor callback. Existing executors inherit an empty image drain. Runtime GUI sessions are live resources, not serialized browser objects; daemon restart reports lost sessions honestly. Preserve immutable screenshots and receipts for replay while requiring a fresh observation before new actions.

## Delivery

Implement the adapter and setup path, wire it to C2 and D0, add tool schemas and prompt guidance, then build and perform the actual browser/native/provider campaign. Commit/push and replacement of the production installation remain separate user actions unless authorized later.
