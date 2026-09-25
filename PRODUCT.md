# Peritus WebUI

<!-- impeccable:product-schema 1 -->

## Platform

web

## Stack

The user approved Svelte + TypeScript for the frontend. The approved implementation design specifies a Rust localhost gateway backed by Peritus's existing daemon and application protocol. Production operation must not require a Node server or frontend development server.

## Users

One local user operating the Peritus harness across multiple projects from a single browser tab. Basic tasks must be discoverable without training; experienced users need efficient keyboard workflows and straightforward extension boundaries.

## Product Purpose

Provide a more capable and understandable operating interface for the Peritus harness than its CLI alone offers. Keep project identity, active session, execution target, current state, and next available action readily understandable.

## Operating Context

The application runs on localhost. Multiple projects may remain active simultaneously. Ordinary project directories and managed Git execution workspaces retain their distinct behavior. The browser presents daemon-owned state and submits explicit user commands. Peritus remains the sole host agent and policy authority, owning conversations, execution, approvals, durable receipts, and recovery.

## Capabilities and Constraints

- Project tabs contain session tabs with multiple levels of nesting. Children must share their parent's canonical project root; the server must enforce this restriction.
- Closing a tab does not stop work, archive a conversation, or delete it.
- An integrated explorer exposes all project files, including hidden, ignored, and untracked entries. Bounded loading must preserve access to the complete directory.
- File sub-tabs support code including `.py`, `.rs`, `.js`, and `.ts`; Markdown `.md`; plain text `.txt`; read-only PDFs through the browser's native viewer; browser-supported images including `.png`, `.jpg`, `.webp`, and `.svg`; and standard browser-playable audio. Unsupported content has an explicit explanation and download route.
- Viewing a file and attaching it to model context are separate actions.
- Text previews support files up to 50 MiB. An explicit, dependency-free Edit mode supports small textual tweaks, find/replace, undo/redo, and conflict-checked saves. This is not an IDE or a general source-editing environment.
- Git controls expose add, push, pull, `status --short`, commit, and the supporting staging, diff, branch, and remote workflows. Ordinary Git commit remains distinct from the existing `/commit` candidate action.
- The explorer context menu can preview and add an exact entry to `.gitignore`, preserving existing content and explaining tracked-file behavior.
- The selected Git repository may be any appropriate directory within the project root, even when the root itself has no `.git`. Project root, Git repository, and execution workspace are distinct bindings.
- All normal user-facing Peritus CLI capabilities belong in the WebUI, including advanced workbench, provider setup, approvals, terminals, artifacts, diagnostics, and recovery. A command inventory alone does not establish working parity.
- Mouse controls, keyboard shortcuts, and slash commands share one command dispatcher and the same operation semantics.
- Appearance, density, fonts, layout, shortcuts, viewer preferences, and project Git settings are configurable, validated, persistent, and resettable.
- Reconnection reconciles original operation receipts before any uncertain mutation can be retried. Displayed progress must reflect actual observations.
- Remote accounts, multi-user collaboration, and a general source editor are outside the approved scope.

## Brand Commitments

The user describes Peritus as industrial machinery, comparable to a CNC machine. The WebUI should feel like a control panel for heavy manufacturing equipment: coherent, precise, readable at a glance, and capable of directing powerful operations. It must remain intuitive and avoid excessive density or complexity. The current design confirmation explicitly calls for full physical theater: mechanical transitions, substantial controls, and tactile feedback, with explicit labels and a reduced-motion alternative. Conversation-first operation is the confirmed priority.

## Evidence on Hand

The existing Rust workspace, application protocol, generated TypeScript declarations, CLI help, and terminal command catalog establish real capabilities. The approved implementation design records the required architecture and acceptance criteria. Implementation and verification are ongoing; no completed WebUI, accessibility certification, performance advantage, or full CI result is claimed by this brief.

## Product Principles

1. Make the active target and observed state unambiguous before presenting an action.
2. Keep common tasks discoverable and reveal advanced controls when needed.
3. Preserve daemon authority and exact operation identities across presentation changes and reconnects.
4. Let keyboard, slash, and mouse workflows invoke the same behavior.
5. Make extension understandable through cohesive domain modules and typed interfaces.

## Accessibility & Inclusion

WCAG 2.0 AA is a required acceptance standard. Keyboard navigation, visible focus, meaningful labels, readable contrast, text resizing, error handling, and accessible media controls require direct verification. Automated checks alone do not establish conformance. Arbitrary project media may require user-supplied alternatives; the application must not imply that it has generated equivalent transcripts.

## Validation Constraints

The user requested focused local validation and comprehensive CI on runners, avoiding an hours-long whole-repository laptop run. Use modest local parallelism and report unavailable platform or assistive-technology checks honestly. Preserve unrelated work and the earlier command-journal fixes.
