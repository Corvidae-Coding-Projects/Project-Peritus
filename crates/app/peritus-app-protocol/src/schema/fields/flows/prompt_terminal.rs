//! Prompt-answer and terminal flow field metadata.

use super::super::AppTypeDescriptor;

mod prompt;
mod terminal;

pub(super) const PROMPT_TERMINAL_TYPES: &[AppTypeDescriptor] = &[
    prompt::APPROVAL_CHALLENGE,
    prompt::SIGNED_APPROVAL_DECISION_FRAME,
    prompt::PROMPT_BINDING,
    prompt::PROMPT_ANSWER,
    prompt::PROMPT_CANCELLATION,
    terminal::TERMINAL_BINDING,
    terminal::TERMINAL_INPUT,
    terminal::TERMINAL_RESIZE,
    terminal::TERMINAL_DETACH,
    terminal::TERMINAL_CANCELLATION,
    terminal::TERMINAL_OUTPUT,
    terminal::TERMINAL_EXIT,
    terminal::TERMINAL_EXIT_DISPOSITION,
];
