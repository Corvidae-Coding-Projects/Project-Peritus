import type { Mode } from './types';

/** UI explanations only; the daemon remains the authority for tools and permissions. */
export const modeDescriptions: Record<Mode, string> = {
  chat: 'Discuss and inspect; explicitly authorized changes go through the build pipeline, within workspace permissions.',
  plan: 'Read-only inspection and planning; cannot edit files or run commands. Switch explicitly to /chat or /build to request changes.',
  review: 'Independent read-only findings; cannot edit files or run commands. Switch explicitly to /chat or /build to request fixes.',
  build: 'Run the design, writer, checks, reviewer and fixer delivery pipeline, within workspace permissions.',
};

export const conversationModes: Mode[] = ['chat', 'plan', 'review', 'build'];
