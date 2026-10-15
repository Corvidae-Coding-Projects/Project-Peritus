# Interactive design mode

## Summary

Add a conversational Design mode to the terminal and browser clients. It helps a person turn an idea into a saved, editable Markdown design that the existing implementation designer receives. The user chose a conversational interview with suggested answers and an editable draft.

## User-visible behavior

`/design` starts or continues the interview. Ask one focused question at a time, suggest a few useful answers while accepting free text, and explain unfamiliar choices. Keep a running draft; the user can change any decision, ask to see the draft, skip an uncertain decision, or resume later. `/design show` requests the complete draft. `/design build` explicitly starts implementation of the saved design, using the ordinary folder or managed-workspace path as appropriate. Browser controls expose the same actions.

## Requirements and acceptance criteria

- A new mode is discoverable beside Chat, Plan, Review, and Build.
- Guide goal, audience, user journeys, scope, constraints, approach, and observable acceptance; avoid a fixed questionnaire when the user already supplied answers.
- Persist the draft and next question before claiming it is saved. Resume without requiring the user to repeat decisions, including after daemon restart.
- Edit the draft by discussing changes or supplying replacement Markdown. Preserve explicit values and unresolved questions.
- Design has read-only workspace tools plus a narrowly scoped daemon-owned draft tool; no process, workspace edit, or pipeline tool.
- Build receives the actual saved Markdown, with newer user instructions taking precedence. No automatic implementation during the interview.
- Distinct conversations keep distinct drafts; failures and stale updates preserve the prior saved document.

## Current architecture

`ProductInteractionMode` travels through A3 execution settings and continuation to daemon `execution/launch.rs`. `ProductRunner::converse` handles read-only Plan/Review and Chat's explicit pipeline handoff. Conversation requests already use durable inputs and local working memory. The automatic designer in `design.rs` and artifact designer in `design/artifact.rs` create the implementation document. TUI slash dispatch and WebUI modes/catalog share their existing message paths.

## Proposed design

Append mode tag 5, leaving existing tags intact. Add runner `ConversationMode::Design` with an interview policy and a dedicated `design_draft` tool. A runner-owned `guided_design` module owns the versioned Markdown artifact beside the run trace, current interview stage, next question, and compare-and-swap version. Only the Design executor can update it. Its fixed path is derived from daemon-owned run identity, never model input. Current draft content enters the next conversation request as evidence, separate from system authority.

Use one atomic, synced Markdown publication with a small versioned metadata comment, so saved state and document cannot diverge. Reopening a run reloads the same artifact. The implementation designers load it explicitly as input, preserving unresolved choices and treating the current user instructions as governing. The existing designer still grounds technical details in the repository.

## Alternatives

Extending Plan with prompt text alone would not give drafts independent durability or a reliable implementation handoff. A new structured wizard protocol and client-specific forms would duplicate the existing durable conversation flow and conflict with the chosen interview experience. Reuse the conversation transport and add only the new mode and private draft contract.

## Data, compatibility, and failure handling

Old mode tags remain stable; older binaries reject the new value rather than interpreting it as Build. Existing runs without a guided draft continue unchanged. The document stores schema version, run/workspace identity, draft version, incorporated input revision, stage, and next question. Validate all fields and bounded Markdown on load/update; missing is distinct from corrupt or unreadable. Atomic replacement preserves the previous draft on publication failure. New input invalidates stale tool updates. No ambient design files are read or overwritten.

## Security

The draft tool can write only the daemon-owned draft for its own run. It does not grant workspace write, process, network, reference-path, or pipeline authority. Read permission and live input admission remain enforced. Repository text and draft text are evidence, never instructions that widen authority. No provider-specific changes.

## Real testing

Build with two Cargo jobs. Operate the actual TUI and browser controls using isolated state and real provider inference: interview, alternatives, edit, show, reconnect, daemon restart, resume, and explicit Build handoff. Read the resulting Markdown and the downstream implementation design. Exercise multiple conversations, an existing repository, skipped/changed decisions, ordinary Chat/Plan behavior, and invalid/stale draft updates through the actual tool/public consumer boundary where reachable. No unit tests or formal/repository verification suites.

## Rollout and rollback

Ship matching clients and daemon. Do not restart or replace the user's live daemon for this feature. Removing the new mode leaves the standalone Markdown artifacts available to humans; existing modes and saved runs retain their original encoding.

## Open questions

None blocking. Draft editing initially uses ordinary conversation or replacement Markdown; it does not introduce a separate form editor.

## Out of scope

Automatic implementation, publishing designs to third parties, collaborative editing by multiple users, and redesigning the existing implementation pipeline.
