//! Authorized structured argv and explicit script tool adapters.

mod catalog;
mod dispatcher;
mod error;
mod execution;
mod input;
mod json_value;
mod plan;
mod render;

pub use catalog::{exec_descriptor, script_descriptor};
pub use dispatcher::{RawShellDispatcher, ShellDispatcher};
pub use error::{ShellError, ShellErrorKind};
pub use execution::ShellExecution;
pub use input::{ExecInput, ScriptInput};
pub use plan::ExecutionPlanInputs;
