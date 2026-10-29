//! Current-admission exact child-result retrieval, independent of plan execution enablement.

use super::{PlanRecord, contract::ReadRequest, error, records};
use crate::{
    control::PermissionCapability,
    local_context::{LocalContextHandle, source_policy, tools},
};
use peritus_agent::{DeveloperLoopError, DeveloperToolObservation};
use peritus_model_protocol::CompletedToolCall;
use serde_json::json;

pub(super) fn execute(
    memory: &LocalContextHandle,
    call: &CompletedToolCall,
) -> Result<DeveloperToolObservation, DeveloperLoopError> {
    if !memory.effective_permissions().allows(PermissionCapability::Read) {
        return super::rejected("inspection result reads require current read permission");
    }
    let Ok(request) = serde_json::from_slice::<ReadRequest>(call.arguments().canonical_bytes())
    else {
        return super::rejected("invalid inspection result read schema");
    };
    if !(256..=65_536).contains(&request.max_bytes) {
        return super::rejected("inspection result page size must be 256 through 65536 bytes");
    }
    let memory = memory.lock()?;
    let Ok(id) = tools::sequence(&memory, &request.step) else {
        return super::rejected("invalid or foreign inspection result handle");
    };
    if !source_policy::allowed(&memory, id)? {
        return super::rejected("inspection result is unavailable under current path permissions");
    }
    let Some(PlanRecord::Step { output, .. }) = records::maybe_load(&memory, id)? else {
        return super::rejected("handle is not an inspection step");
    };
    let bytes = memory.store.read(output)?;
    let handle = tools::source_handle(&memory, id);
    drop(memory);
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| error("inspection result is not canonical UTF-8 JSON"))?;
    let Ok(start) = usize::try_from(request.offset) else {
        return super::rejected("inspection result offset is not representable");
    };
    if start > text.len() || !text.is_char_boundary(start) {
        return super::rejected("inspection result offset must be an original UTF-8 boundary");
    }
    let end = text.floor_char_boundary(start.saturating_add(request.max_bytes).min(text.len()));
    super::observation(
        &json!({"version":1,"step":request.step,"source_sha256":tools::hex(output.digest.as_bytes()),
        "source_bytes":output.bytes,"offset":start,"returned_bytes":end-start,
        "next_offset":(end<text.len()).then_some(end),"data":&text[start..end],
        "sources":[{"handle":handle}],"authority":"none"}),
        false,
    )
}
