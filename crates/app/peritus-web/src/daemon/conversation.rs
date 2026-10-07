//! Honest observation of a run-backed or prepared durable conversation.

use super::{
    App, AppErrorCode, AppRequestPayload, AppResponsePayload, ConversationId,
    Result, RunId, Value, WorkbenchQuery, WorkspaceId, bytes, hex, interaction_response, json,
    owner_for_project, problem, raw_request_owned,
};
use peritus_app_protocol::{
    ProductActivity, ProductActivityKind, ProductActivityPageQuery, ProductActivitySegment,
};

pub(super) async fn observe(app: &App, session_id: &str) -> Result<Value> {
    let session = app.session(session_id)?;
    let project = app.project(&session.project)?;
    let owner = match app.session_owner(&session.id)? {
        Some(owner) => {
            owner.facts_async(&project).await?;
            owner
        }
        None => owner_for_project(app, &project).await?.0,
    };
    let run = RunId::new(bytes(&session.run)?).map_err(|e| problem(format!("{e:?}")))?;
    let mut restarts = 0_u8;
    'snapshot: loop {
        let mut query = ProductActivityPageQuery::first(run);
        let mut assembler: Option<ActivityAssembler> = None;
        loop {
            match raw_request_owned(
                app,
                &owner,
                AppRequestPayload::QueryInteractionPage(query),
            )
            .await?
            {
                AppResponsePayload::InteractionPage(page) if page.query() == query => {
                    let value = page.interaction();
                    if hex(value.snapshot().workspace_id().as_bytes()) != owner.workspace() {
                        return Err(problem("The observed run belongs to another native workspace"));
                    }
                    let window = value.activity_window().ok_or_else(|| {
                        problem("The conversation page omitted its exact history binding")
                    })?;
                    if assembler.is_none() {
                        assembler = Some(ActivityAssembler::new(window.history(), window.total()));
                    }
                    let assembled = assembler.as_mut().ok_or_else(|| {
                        problem("Conversation history assembly was unavailable")
                    })?;
                    if assembled.digest != window.history()
                        || assembled.total != window.total()
                        || assembled.accept(page.segments(), page.next().is_none()).is_err()
                    {
                        return Err(problem("The daemon returned discontinuous conversation history"));
                    }
                    if let Some(next) = page.next() {
                        query = ProductActivityPageQuery::after(next);
                        continue;
                    }
                    app.bind_session_owner(session_id, owner)?;
                    return Ok(interaction_response(value, &assembled.activities));
                }
                AppResponsePayload::Error(error)
                    if error.code() == AppErrorCode::InvalidIdentifier
                        && query.cursor().is_none() =>
                {
                    return prepared(app, &session, &owner, error.actionable_message()).await;
                }
                AppResponsePayload::Error(_)
                    if query.cursor().is_some() && restarts < 3 =>
                {
                    restarts += 1;
                    continue 'snapshot;
                }
                AppResponsePayload::Error(error) => {
                    return Err(problem(format!(
                        "Daemon rejected conversation observation: {}",
                        error.actionable_message()
                    )));
                }
                _ => return Err(problem("Unexpected conversation response from daemon")),
            }
        }
    }
}

struct ActivityAssembler {
    digest: peritus_types::Sha256Digest,
    total: u64,
    activities: Vec<ProductActivity>,
    partial: Option<PartialActivity>,
}

struct PartialActivity {
    sequence: u64,
    kind: ProductActivityKind,
    next_segment: u64,
    text_bytes: u64,
    text: String,
    detail_bytes: u64,
    detail: String,
}

impl ActivityAssembler {
    const fn new(digest: peritus_types::Sha256Digest, total: u64) -> Self {
        Self { digest, total, activities: Vec::new(), partial: None }
    }

    fn accept(&mut self, segments: &[ProductActivitySegment], final_page: bool) -> Result<(), ()> {
        for segment in segments {
            if self
                .partial
                .as_ref()
                .is_some_and(|partial| partial.sequence != segment.sequence())
            {
                self.finish()?;
            }
            if self.partial.is_none() {
                let sequence = self.activities.last().map_or_else(
                    || segment.sequence(),
                    |activity| activity.sequence().checked_add(1).unwrap_or(0),
                );
                if segment.sequence() != sequence
                    || segment.segment() != 0
                    || segment.text_offset() != 0
                    || segment.detail_offset() != 0
                {
                    return Err(());
                }
                self.partial = Some(PartialActivity {
                    sequence,
                    kind: segment.kind(),
                    next_segment: 0,
                    text_bytes: segment.text_bytes(),
                    text: String::new(),
                    detail_bytes: segment.detail_bytes(),
                    detail: String::new(),
                });
            }
            let partial = self.partial.as_mut().ok_or(())?;
            if segment.sequence() != partial.sequence
                || segment.kind() != partial.kind
                || segment.segment() != partial.next_segment
                || segment.text_bytes() != partial.text_bytes
                || segment.detail_bytes() != partial.detail_bytes
                || u64::try_from(partial.text.len()).map_err(|_| ())? != segment.text_offset()
                || u64::try_from(partial.detail.len()).map_err(|_| ())?
                    != segment.detail_offset()
            {
                return Err(());
            }
            partial.text.push_str(segment.text());
            partial.detail.push_str(segment.detail());
            partial.next_segment = partial.next_segment.checked_add(1).ok_or(())?;
            if u64::try_from(partial.text.len()).map_err(|_| ())? > partial.text_bytes
                || u64::try_from(partial.detail.len()).map_err(|_| ())? > partial.detail_bytes
            {
                return Err(());
            }
        }
        if final_page {
            self.finish()?;
            if self.activities.last().map(ProductActivity::sequence).unwrap_or(0) != self.total {
                return Err(());
            }
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<(), ()> {
        let Some(partial) = self.partial.take() else { return Ok(()) };
        if u64::try_from(partial.text.len()).map_err(|_| ())? != partial.text_bytes
            || u64::try_from(partial.detail.len()).map_err(|_| ())? != partial.detail_bytes
        {
            return Err(());
        }
        self.activities.push(
            ProductActivity::new(partial.sequence, partial.kind, partial.text, partial.detail)
                .map_err(|_| ())?,
        );
        Ok(())
    }
}

async fn prepared(
    app: &App,
    session: &crate::state::Session,
    owner: &super::NativeOwner,
    run_error: String,
) -> Result<Value> {
    let workspace = WorkspaceId::new(bytes(
        owner.workspace(),
    )?)
    .map_err(|e| problem(format!("{e:?}")))?;
    let conversation = ConversationId::new(bytes(&session.conversation)?)
        .map_err(|e| problem(format!("{e:?}")))?;
    let query = WorkbenchQuery::new(conversation, workspace);
    match raw_request_owned(app, owner, AppRequestPayload::QueryWorkbenchExecution(query)).await? {
        AppResponsePayload::WorkbenchExecution(state) if state.snapshot().query() == query => {
            app.bind_session_owner(&session.id, owner.clone())?;
            let started = state.run().is_some();
            Ok(json!({"workbench":{
                "conversation":hex(conversation.as_bytes()),
                "revision":state.snapshot().revision().to_string(),
                "queued":!started,
                "started":started,
                "observation":if started {"Execution was admitted, but its run is not currently observable."} else {"The evaluation inputs are durable and execution has not been admitted."},
                "action":"Open Workbench to inspect the durable queue and resume the exact evaluation request.",
                "detail":run_error
            }}))
        }
        AppResponsePayload::Error(workbench_error) => Err(problem(format!(
            "Run and durable workbench observation failed: {run_error}; {}",
            workbench_error.actionable_message()
        ))),
        _ => Err(problem("Unexpected durable workbench response")),
    }
}
