//! Authorized structured argv and explicit script tool adapters.

mod catalog;
mod dispatcher;
mod error;
mod execution;
mod input;
mod json_value;
mod plan;
mod render;

pub use catalog::{
    exec_descriptor, legacy_exec_descriptor, legacy_exec_descriptor_v3, legacy_script_descriptor,
    legacy_script_descriptor_v3, script_descriptor,
};
pub use dispatcher::{RawShellDispatcher, ShellDispatcher};
pub use error::{ShellError, ShellErrorKind};
pub use execution::{RecoveredTerminalExecution, ShellExecution};
pub use input::{ExecInput, ScriptInput};
pub use plan::ExecutionPlanInputs;
