# peritus-tui

G2 owns the interactive terminal client. Its deterministic reducer projects A3/G0 observations
into bounded run, diff, review, trace, evolution, approval, and terminal views. Effects are emitted
as typed requests; presentation state is never authoritative.

The crate owns terminal-mode restoration, input mapping, reconnect and resumable-session behavior,
bounded transcript sanitization, and orderly connection shutdown. It depends on the A3 application
protocol and foundation contracts plus Crossterm and Ratatui for presentation. All authorization,
durable state, and acceptance decisions remain in G0 and the verified lower layers.

On Windows, a scoped title owner displays `Peritus`, reclaims the title after foreground commands,
and restores the caller's exact console title on exit. The launcher acquires the same owner before
provider setup and around login handoffs. Detached processes and other platforms are unaffected.

The Runs dashboard presents accepted, candidate-available, waiting, cancelled, stopped, and
recovery-required states directly. Its handoff panel exposes exact paths, checks, review evidence,
remaining work, and run instructions. Users can inspect, continue, run, export, accept, commit, or
discard a candidate without finding an internal worktree or log. Foreground run commands temporarily
return terminal ownership to the candidate and restore the full-screen interface afterward.
Ctrl-C can interrupt the foreground candidate without terminating the TUI owner. While the
interface is active, Ctrl-C or Ctrl-Q requests orderly client exit.
On Unix, the child acquires its own foreground process group before execution; terminal ownership
returns to the UI after interruption, ordinary exit, or an executable-launch failure. Native PTY
tests exercise immediate keyboard input and Ctrl-C. Ctrl-Z suspends the owning shell job; `fg`
restores the candidate, while `bg` leaves it stopped without taking the shell's terminal.
Resume waits for an observed SIGCONT, including when the owner has multiple threads. The existing
locked `signal-hook` dependency provides that notification without an additional worker thread.
This OS job-control boundary is not Miri eligible; native PTY tests cover stop, resume, and interrupt.

Opening a saved run resolves its actual input conversation before another message can be sent.
A failed lookup retains the run and draft for retry. Runs in another configured workspace open
with that workspace's path and trust; an unsent draft must be sent or cleared before leaving its
workspace. Opening or inspecting a saved run does not start inference.

In `/sessions` and its search results, arrows select a conversation and Enter opens it. `n`/`p`
page through all results; `r` retries or refreshes the same search page. Home/End selects the
first/last row, while PageUp/PageDown scrolls long titles and details. Refresh retains the
selected identity when it remains in the result page.

Diff, Review, Preview, and approval details support PageUp/PageDown and Home/End. Structured
review scrolls the selected hunk or comment; arrows select another item and Tab changes focus.
Modal drafts survive launcher-owned daemon recovery, including uncertain submissions. Reconnecting
does not submit them again; inspect the current run before manually resending an uncertain task.
Ctrl-R reconnects without replacing a chat or modal draft. With terminal keyboard capture enabled,
Ctrl-R goes to the attached program; release capture with Ctrl-] before reconnecting.

Interactive previews backed by pipes negotiate `app.terminal-pipes`. Their terminal view shows
a local line editor: type or paste, edit the visible draft, then press Enter to send it. The draft
clears only when the daemon acknowledges that exact input; rejection or timeout preserves it.
After an uncertain delivery, inspect the program before resending. Ctrl-C explicitly cancels
the preview. Pipe previews do not support terminal resizing or full-screen terminal applications;
PTY previews retain direct keyboard input and resizing.

Isolated forks open with the child's registered workspace path and trust, without restarting the
daemon or changing the parent workspace. If you type another draft while the fork is pending, the
interface stays with that draft and shows the saved child's ID. `/sessions open CONVERSATION_ID
WORKSPACE_ID` opens it later. A failed workspace lookup leaves the original conversation usable;
it does not create the fork again. Generated fork titles stay within the title limit, including
when the source title contains Unicode.

Metadata inspectors, file/image details, and goal/context panels support arrows, PageUp/PageDown,
and Home/End. Scrolling stops at the displayed content and adapts to terminal resizing. Multiline
guidance, captions, objectives, and status messages keep their line breaks.
Escape dismisses an inspection and cancels its unfinished read or unconfirmed import. Captions
and drafts remain available, while late replies cannot resume a dismissed preview or deferred
goal change. Submitted mutations keep their receipt tracking until their outcome is resolved.
Read-only request timeouts leave the connection and drafts intact; panels can be refreshed or
their commands retried. Reads needed to reconcile an accepted mutation still trigger recovery.
An unresponsive Runs lookup does not stop conversation or preview polling, and expired replies
cannot change the current view. Doctor supports Home/End and can be dismissed and reopened while
an earlier probe is still running.

Disconnected chat keeps an explicit offline status and Ctrl-R reconnect hint above the composer,
including after transient notices disappear.

Conversation view pins the active elapsed working indicator directly above the message-entry box in
bold white. Tool calls remain visible without enabling diagnostics: one entry shows the command
or operation, then its observed result, exit status and bounded output preview. Long entries keep
their opening and final lines; `/details` expands retained details and host status diagnostics.
The timer stops on disconnect or idle/terminal phases; it never claims provider progress.

`/effort` opens the reasoning-effort picker without requiring model discovery. Use arrows and
Enter, Tab to change roles, or `/effort [writer|reviewer|fixer] LEVEL`. Levels are `default`,
`minimal`, `low`, `medium`, `high`, `xhigh`, `max`, and `ultra`; provider/model support varies.
The model picker also opens this control with `e`. Each role keeps its effort when its model
changes. Active conversations show success only after durable daemon acknowledgement; the next
model turn uses the selection while an in-flight turn remains unchanged. `default` retains the
existing policy: high when reasoning controls are negotiated, otherwise no reasoning control.
Repeated model refreshes share the pending provider lookup. Closing the picker or moving to effort
selection abandons discovery; an old reply cannot replace a reopened picker's current catalog.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-tui
```
