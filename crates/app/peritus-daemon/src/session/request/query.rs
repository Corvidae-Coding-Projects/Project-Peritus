//! Owned blocking adapters for synchronous product-run projections.

use super::{AppResponsePayload, ProductRunService, blocking, product_run_error, response};

pub(super) enum Request {
    Checkpoint(peritus_app_protocol::WorkbenchRewindRequest),
    Memory(peritus_app_protocol::WorkbenchMemoryQuery),
    Init(peritus_app_protocol::InitDiscoveryRequest),
    Permissions(peritus_app_protocol::WorkbenchQuery),
    Compaction(peritus_app_protocol::WorkbenchCompactionRequest),
    Preview(peritus_app_protocol::WorkbenchResultQuery),
    Result(peritus_app_protocol::WorkbenchResultQuery),
    Review(peritus_app_protocol::WorkbenchReviewQuery),
    Library(peritus_app_protocol::ConversationLibraryQuery),
    Context(peritus_app_protocol::WorkbenchContextQuery),
    Brief(peritus_app_protocol::WorkbenchQuery, bool),
    Goal(peritus_app_protocol::WorkbenchQuery),
    Queue(peritus_app_protocol::WorkbenchQueueQuery, bool),
    Execution(peritus_app_protocol::WorkbenchQuery),
    ContinuationAdmission(peritus_app_protocol::WorkbenchCommand),
    Binding(peritus_app_protocol::ProductInteractionQuery),
    Workbench(peritus_app_protocol::WorkbenchQuery),
    Receipt(peritus_app_protocol::WorkbenchCommand),
    Doctor(peritus_app_protocol::DoctorQuery),
    Interaction(peritus_app_protocol::ProductInteractionQuery),
    ActivityPage(peritus_app_protocol::ProductActivityPageQuery),
    Observations(peritus_app_protocol::ProductRunQuery),
    RunPage(peritus_app_protocol::ProductRunPageQuery),
}

pub(super) async fn respond(
    service: &ProductRunService,
    actor: peritus_types::ActorId,
    request: Request,
) -> AppResponsePayload {
    let service = service.clone();
    blocking::response(move || match request {
        Request::Checkpoint(request) => service.inspect_workbench_checkpoint(actor, request),
        Request::Memory(query) => service.workbench_memory(actor, query),
        Request::Init(request) => service.discover_init(actor, request),
        Request::Permissions(query) => service.workbench_permissions(actor, query),
        Request::Compaction(request) => service.preview_workbench_compaction(actor, &request),
        Request::Preview(query) => service.workbench_preview(actor, query),
        Request::Result(query) => service.workbench_result(actor, query),
        Request::Review(query) => service.workbench_review(actor, query),
        Request::Library(query) => service.conversation_library(actor, &query),
        Request::Context(query) => service.workbench_context(actor, query),
        Request::Brief(query, request_sources) => {
            service.workbench_brief_projection(actor, query, request_sources)
        }
        Request::Goal(query) => service.workbench_goal(actor, query),
        Request::Queue(query, request_sources) => {
            service.workbench_queue_projection(actor, query, request_sources)
        }
        Request::Execution(query) => service.workbench_execution(actor, query),
        Request::ContinuationAdmission(command) => {
            service.workbench_continuation_admission(actor, &command)
        }
        Request::Binding(query) => service
            .query_interaction_binding(actor, query)
            .map_or_else(product_run_error, AppResponsePayload::InteractionBinding),
        Request::Workbench(query) => service.workbench_query(actor, query),
        Request::Receipt(command) => service.workbench_receipt(actor, &command),
        Request::Doctor(query) => match service.doctor(query) {
            Ok(report) => AppResponsePayload::Doctor(report),
            Err(error) => product_run_error(error),
        },
        Request::Interaction(query) => match service.query_interaction(query) {
            Ok(snapshot) => AppResponsePayload::Interaction(snapshot),
            Err(error) => product_run_error(error),
        },
        Request::ActivityPage(query) => match service.query_activity_page(query) {
            Ok(page) => AppResponsePayload::InteractionPage(page),
            Err(error) => product_run_error(error),
        },
        Request::Observations(query) => response::product_run_observations(&service, query),
        Request::RunPage(query) => response::product_run_page(&service, query),
    })
    .await
}
