//! Corrective context for rejected or prematurely stopped developer turns.

use crate::ProductRunnerError;

pub(super) fn rejected_terminal(error: &ProductRunnerError) -> String {
    format!(
        "The harness rejected the previous terminal response during {}: {}. Inspect the current workspace with `workspace_list` and targeted `workspace_read` calls, address the reported contract failure, and only then return the required terminal JSON. If no code change is needed, still ground that conclusion in the current repository and exact evidence.",
        error.operation(),
        error.detail(),
    )
}

pub(super) fn unverified_question(question: &str) -> String {
    format!(
        "The previous turn stopped without changing the workspace and asked: {question}\n\nThe harness independently confirms that this managed workspace is writable and that `workspace_write`, `workspace_patch`, `workspace_remove`, `run_command`, and the `command_start` lifecycle are available host functions even when the provider has no native filesystem tools. Re-ground with `workspace_list` and targeted `workspace_read`, then continue implementing with those host functions. If a material user choice still remains after using the available capabilities, return the same direct question unchanged; otherwise complete the requested work."
    )
}
