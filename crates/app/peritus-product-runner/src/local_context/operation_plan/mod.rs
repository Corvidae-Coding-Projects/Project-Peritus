//! Versioned read-only host plans; ordinary executors retain every operation's authority.

mod admission;
mod contract;
mod execution;
mod read;
mod records;
mod schema;
mod state;
mod validation;

pub(super) use admission::revision as input_revision;
pub(super) use contract::{PlanRecord, RecordEnvelope};
pub(super) use records::decode_message;
pub(super) use records::load as load_record;
pub(super) use schema::definitions;
pub(super) use schema::read_definition;

use super::{error, memory::LocalMemory};
use peritus_agent::{
    DeveloperLoopError, DeveloperModelRole, DeveloperToolExecutor, DeveloperToolObservation,
};
use peritus_model_protocol::CompletedToolCall;

pub(super) const PREFIX: &str = "PERITUS HOST INSPECTION RECORD V1\n";

pub(super) fn execute(
    memory: &super::LocalContextHandle,
    base: &mut dyn DeveloperToolExecutor,
    role: Option<DeveloperModelRole>,
    call: &CompletedToolCall,
    host_calls: &mut u32,
) -> Result<DeveloperToolObservation, DeveloperLoopError> {
    if !base.tool_calls_allowed() {
        return rejected("inspection tools are closed for this host phase");
    }
    match call.name().as_str() {
        "inspection_plan" => execution::execute(memory, base, role, call, host_calls),
        "inspection_result_read" => read::execute(memory, call),
        _ => Err(error("unknown inspection plan operation")),
    }
}

fn observation(
    value: &serde_json::Value,
    is_error: bool,
) -> Result<DeveloperToolObservation, DeveloperLoopError> {
    Ok(DeveloperToolObservation {
        output: peritus_model_protocol::CanonicalJson::parse(
            &value.to_string(),
            peritus_model_protocol::JsonBounds::value(
                peritus_model_protocol::ProtocolLimits::PRODUCTION,
            ),
        )?,
        is_error,
    })
}

fn rejected(detail: &str) -> Result<DeveloperToolObservation, DeveloperLoopError> {
    observation(&serde_json::json!({"error":detail,"status":"not_executed"}), true)
}

pub(super) fn ensure_reader(memory: &mut LocalMemory) -> Result<(), DeveloperLoopError> {
    memory.ensure_reader_guard(super::record::PLAN_CHECKPOINT_SCHEMA_VERSION)
}
