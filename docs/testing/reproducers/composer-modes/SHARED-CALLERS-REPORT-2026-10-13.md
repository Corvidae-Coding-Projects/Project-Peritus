# Shared editor routing — actual UI replay

**Confirmed and resolved:** three modal character-routing failures and ten file/image word-motion failures at field capacity. The fixed executable passed every directly affected scenario that was reachable in the appropriate real workspace context. No application code was edited by this independent replay.

The replay spawned the actual `peritus open WORKSPACE` client through a PTY, using the real fixture daemon and its normal durable metadata controls. `pyte` decoded Ratatui's real terminal output. The fixture was `/tmp/peritus-ui-issues-20261013/`, with its owned-disposable marker and isolated XDG `config`, `state`, and `cache` directories. Provider selection remained `0` (offline). No model task, process attachment submission, file/image preview, import, confirmation, actual file selection, or configuration change was performed. All draft content was synthetic local text.

Compared executables:

- Pre-shared-caller-fix snapshot: `/tmp/peritus-ui-issues-20261013/pre-shared-caller-fix`, SHA-256 `0f217720446983589b2d404fd96acc375c73e9b49e8f28bf6d6d7ba919f180e2`.
- Final rebuilt CLI: `/home/doll/Project-Peritus/target/release/peritus`, SHA-256 `603cf2ba6369cfc2e93c88e7affbd84817904ee563d7ea8f59981604ce921edf`.

The root agent built and fixed the three caller guards; this agent operated only the real UI and wrote replay/report artifacts. No unit tests, verification gates, extra builds, commits, pushes, issue posts, browser sessions, or live-user daemon/client actions were performed in this pass.

## Modal journeys

Normal route: open the restricted Git fixture `FIXTURE/repo`, type `/runs`, press `6` for Terminal, then `a` to open the local attach-ID modal. The modal was edited and canceled without submitting an attachment.

| Action from `alpha beta` | Required observation after inserting cursor marker | Pre-fix actual observation | Fixed actual observation |
| --- | --- | --- | --- |
| Legacy Alt-b, then bracketed-paste `\|` | `alpha \|beta` | `alpha betab\|` — FAIL | `alpha \|beta` — PASS |
| Home, legacy Alt-f, then bracketed-paste `\|` | `alpha \|beta` | `f\|alpha beta` — FAIL | `alpha \|beta` — PASS |
| Super+b, then bracketed-paste `\|` | `alpha beta\|` | `alpha betab\|` — FAIL | `alpha beta\|` — PASS |

The same independent expectations were used before and after the fix. Modal source routing previously intercepted Alt-b/f and Super+characters as text before the shared motion helper; the rebuilt caller now lets motions reach that helper and leaves Super+characters inert.

## Capacity journeys

The root agent then registered a genuine managed Git workspace through the supported launcher at `FIXTURE/managed`. The old and fixed clients used that workspace sequentially, with no concurrent client or configuration changes. `/files` with no argument opened real file controls; `/attach` with no argument opened the retained-image page, where `i` entered the empty local form and a second `i` focused its path. No path-argument auto-read route or `p`/`c` preview/confirmation actions were used.

| Local field | Capacity exercised | Pre-fix Alt-b/Alt-f at capacity | Fixed Alt-b/Alt-f at capacity |
| --- | ---: | --- | --- |
| File path | 4,096 bytes | Both FAIL | Both PASS |
| File range | 80 bytes | Both FAIL | Both PASS |
| File caption | 8,192 bytes | Both FAIL | Both PASS |
| Image path | 4,096 bytes | Both FAIL | Both PASS |
| Image caption | 8,192 bytes | Both FAIL | Both PASS |

Each backward case pasted exactly `x` repeated `capacity - 11` times followed by ` alpha beta`. Each forward case pasted `alpha beta ` followed by the same padding. Ordinary `|` insertion was first rejected at capacity in both versions. The motion was then followed by Delete and `|` insertion so that cursor position produced an independently visible text change:

- Alt-b should reach the start of the final `beta`; Delete plus marker must produce the visible suffix `alpha |eta`. Before the fix the motion was rejected, Delete at the end removed nothing, and the marker remained rejected.
- Home plus Alt-f should reach the start of the initial `beta`; Delete plus marker must produce the visible prefix `alpha |eta `. Before the fix the motion was rejected and the same final actions produced `|lpha beta `.

The five fields and two directions yielded ten confirmed old-version failures. Both versions also passed all ten ordinary insertion-limit checks. The final managed-workspace run was **21/21 PASS**, comprising these twenty editing checks plus orderly zero exit and exact restoration of the original PTY termios. The old run was eleven PASS and ten FAIL. Large fields are observed through the actual editor's visible window; this evidence establishes the relevant cursor effect and insertion-boundary behavior, not a full readback of every padded character.

## Retained route blocker

Before managed-workspace setup, normal `/files` in `FIXTURE/repo` presented:

```text
Rejected: session-mismatch. Refresh before a new edit; draft retained.
No metadata snapshot selected.
Metadata controls do not start or resume a run.
```

One bounded `/new` plus Ctrl-R reconnect attempt produced the same block. This occurred in the **restricted Git fixture**, whose context was not in the daemon's managed-workspace registry. No ordinary trusted nongit folder was replayed for this condition, and no broader ordinary-folder defect or regression from this diff is claimed. The initial modal runs therefore retain a separate BLOCKED field-route result rather than treating those fields as passed. The registered managed-workspace route subsequently exercised all targeted fields successfully.

## Replay and evidence

```sh
FIXTURE=/tmp/peritus-ui-issues-20261013
"$FIXTURE/venv/bin/python" docs/testing/reproducers/composer-modes/shared_callers_replay.py \
  --binary "$FIXTURE/pre-shared-caller-fix" --fixture "$FIXTURE" \
  --workspace "$FIXTURE/managed" --fields-only \
  --output "$FIXTURE/shared-fields-before" --baseline
"$FIXTURE/venv/bin/python" docs/testing/reproducers/composer-modes/shared_callers_replay.py \
  --binary target/release/peritus --fixture "$FIXTURE" \
  --workspace "$FIXTURE/managed" --fields-only \
  --output "$FIXTURE/shared-fields-fixed"
```

Omit `--fields-only` to include the modal journey. `--baseline` preserves failure records without treating their expected old-version exit as a campaign failure. The driver checks the owned fixture marker and workspace containment, accepts real PTY bytes, saves concrete inputs/actions/observations, and never substitutes application or daemon responses.

Raw results, actual ANSI output, and each scenario's decoded screen are retained under `FIXTURE/{shared-callers-before,shared-callers-fixed,shared-fields-before,shared-fields-fixed}/`. Fixed modal observations and same-case old/fixed field screens were inspected. All clients started by the replay are closed. The root agent owns fixture-daemon cleanup.

The terminal capability replies constitute protocol-level terminal emulation. They do not prove physical macOS keyboard or cmux shortcut delivery. Foreground candidate handoff remains source-reviewed only and was not exercised by these local editor journeys.
