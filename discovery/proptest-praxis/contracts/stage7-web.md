# Stage 7 WebUI draft, recovery, and ordering contract

## Source and caller coverage

- `webui/src/App.svelte`: composer `oninput`, send form, project/session selection, recovery control, start/poll effects.
- `webui/src/lib/workspace.svelte.ts`: session-keyed drafts and pending flags; `sendMessage`, `reconcileOperations`, `refresh`, `loadProject`, `selectProject`, `poll`, `observeConversation`.
- `webui/src/lib/api.ts`: action receipt and uncertain-error paths; mutation request is never automatically repeated after a connection failure.
- `webui/src/lib/operations.svelte.ts`: client operation ledger, session storage, and gateway-ledger restore.
- `webui/src/lib/conversations.ts`: decimal `u64` activity ordering using `BigInt`.
- `webui/src/lib/components/Recovery.svelte`: visible unresolved-operation check and review controls.
- Read-only gateway trace: `crates/app/peritus-web/src/operations.rs`, `state.rs`, `daemon/conversation.rs`. No Rust source was changed or compiled in this lane.

## Independent oracles and schedules

The draft oracle records the exact string entered when send starts. If the user later changes that string, including whitespace-only changes, the edited string remains after either direct success or recovered success. If the string is unchanged, a successful send clears it. When client-only send metadata is unavailable after recovery, the conservative oracle retains the draft. The operation oracle requires one mutation POST; an uncertain outcome is resolved only by querying the original operation ID. Session `s2`'s draft remains unchanged when `s1`'s send resolves after a project switch.

`stage7-web/workspace_property.cjs` compiles and loads the actual TypeScript modules, supplies Svelte rune and browser-storage adapters, and controls pending HTTP responses. Seed `7319` covers 6 fixed examples and 512 generated draft edits. Every draft case runs direct success, uncertain recovery, gateway-ledger restore after reload, and missing-client-metadata recovery. A separate deterministic schedule resolves project `p2` facts/git before delayed `p1` responses; only `p2` remains visible. The conversation oracle compares adjacent decimal sequence IDs beyond JavaScript's safe integer range and checks a new run reset across 513 cases.

`stage7-web/browser_recovery.cjs` drives the actual Svelte composer in headless Google Chrome through Vite's live transformation of current source. It routes API calls to a deterministic fake gateway: original send, edit during the pending request, connection loss, first receipt query unresolved, and manual recovery with the second receipt. Visible textarea readback retains the edit. It records exactly one action POST and two receipt reads. This is a real browser/UI path with a mocked API, not a live daemon or production-bundle end-to-end test.

## Confirmed failure, shrink, and calibration

The pre-fix source failed the first fixed case: original `Shared https://doc.qt.io for reading.`, edited ` Shared https://doc.qt.io for reading. `. Direct success retained the edit while recovered success cleared it. The minimized witness is original `a`, edited ` a`; the same direct/recovered divergence remained. The original `trim()` comparison in recovery activated the fault. These unmodified-source failures calibrate the oracle; no persistent deliberate mutation was needed. Exact source hashes, first failure, and minimized failure are in `stage7-web/pre-fix/`.

The candidate uses the same exact-string comparison as the direct success path. It keeps the original draft string as client-only operation metadata in session storage and merges that metadata with the gateway's pending-operation record after reload. The gateway command/session/project remain authoritative. If metadata is lost, the draft stays for review. Post-fix evidence is in `stage7-web/post-fix-v3/`.

## Replay and bounded result

From the isolated lane root, with `webui/node_modules` installed:

```sh
node discovery/proptest-praxis/stage7-web/workspace_property.cjs --output /tmp/stage7-web-replay-unique
```

For the browser path, start `npm run dev -- --port 5187` in `webui/`, then run:

```sh
node discovery/proptest-praxis/stage7-web/browser_recovery.cjs --output /tmp/stage7-web-browser-unique
```

Candidate validation: 518/518 draft cases; stale-project schedule; 513 decimal sequence cases; headless Chrome browser flow; `npm run check` (0 errors, 0 warnings); `npm run build`; `git diff --check`. The browser run uses mocked gateway responses. Real daemon/gateway reconnect, local-storage quota failures, multiple simultaneous pending sends, and other WebUI subsystems remain outside this bounded lane.
