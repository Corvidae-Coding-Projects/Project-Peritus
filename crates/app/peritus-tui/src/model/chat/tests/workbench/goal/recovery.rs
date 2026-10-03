//! A queried resume receipt proves admission, not that its worker was launched.

use super::*;
use peritus_app_protocol::{AppErrorCode, AppProtocolError};

#[test]
fn recovered_resume_admission_retains_exact_retry_without_automatically_launching() {
    for newer_draft in [false, true] {
        let mut model = goal_model();
        let query = model.chat.workbench.selected.unwrap();
        let paused = snapshot(query, 17, WorkbenchGoalState::Paused);
        model.chat.run_id = Some(paused.run());
        model.chat.workbench.goal = Some(paused);
        model.chat.buffer = "/resume".to_owned();
        let sent = request(&key(&mut model, KeyCode::Enter));
        let AppRequestPayload::WorkbenchCommand(command) = sent.payload() else {
            panic!("resume command")
        };
        let command = command.clone();
        respond(
            &mut model,
            &sent,
            AppResponsePayload::Error(AppProtocolError::new(AppErrorCode::Internal, None)),
        );
        if newer_draft {
            model.chat.buffer = "Keep this newer draft".to_owned();
            model.chat.cursor = 4;
            key(&mut model, KeyCode::Esc);
        }
        let draft = model.chat.buffer.clone();
        let features = model.features.clone();
        model.update(Action::Disconnected("projection write failed".to_owned()));
        model.update(Action::Connected {
            context: sent.context(),
            limits: AppProtocolLimits::PRODUCTION,
            server: "restarted".to_owned(),
            downgraded: false,
        });
        let lookup = request(
            &model.update(Action::NegotiatedFeatures { context: sent.context(), features }),
        );
        assert_eq!(lookup.payload(), &AppRequestPayload::QueryWorkbenchReceipt(command.clone()));
        let refresh = request(&respond(&mut model, &lookup, receipt(&command)));
        assert_eq!(refresh.payload(), &AppRequestPayload::QueryWorkbenchGoal(query));
        assert_eq!(model.chat.workbench.unresolved.as_ref().unwrap().0, command);
        assert_eq!(model.chat.buffer, draft);
        assert!(model.chat.workbench.message.contains("/sessions retry"));
        assert!(
            respond(
                &mut model,
                &refresh,
                AppResponsePayload::WorkbenchGoal(snapshot(query, 18, WorkbenchGoalState::Active)),
            )
            .is_empty()
        );
        assert!(model.chat.workbench.message.contains("/sessions retry"));
        assert_eq!(model.chat.buffer, draft);
        if newer_draft {
            assert_eq!(model.chat.cursor, 4);
            assert!(!model.chat.workbench.open, "receipt lookup must not reopen a panel");
        }
        key(&mut model, KeyCode::Esc);
        model.chat.buffer = "/sessions retry".to_owned();
        let retry = request(&key(&mut model, KeyCode::Enter));
        assert_eq!(retry.payload(), &AppRequestPayload::WorkbenchCommand(command.clone()));
        if newer_draft {
            model.chat.buffer = "Another draft while retrying".to_owned();
        }
        let refresh = request(&respond(&mut model, &retry, receipt(&command)));
        assert_eq!(refresh.payload(), &AppRequestPayload::QueryWorkbench(query));
        assert!(model.chat.workbench.unresolved.is_none());
        assert_eq!(
            model.chat.buffer,
            if newer_draft { "Another draft while retrying" } else { "" }
        );
    }
}

#[test]
fn an_uncorrelated_resume_receipt_cannot_retarget_or_resolve_the_pending_operation() {
    let mut model = goal_model();
    let query = model.chat.workbench.selected.unwrap();
    model.chat.workbench.goal = Some(snapshot(query, 17, WorkbenchGoalState::Paused));
    model.chat.buffer = "/resume".to_owned();
    let sent = request(&key(&mut model, KeyCode::Enter));
    let AppRequestPayload::WorkbenchCommand(command) = sent.payload() else {
        panic!("resume command")
    };
    let command = command.clone();
    respond(
        &mut model,
        &sent,
        AppResponsePayload::Error(AppProtocolError::new(AppErrorCode::Internal, None)),
    );
    let foreign_query = WorkbenchQuery::new(
        peritus_app_protocol::ConversationId::new([99; 16]).unwrap(),
        query.workspace(),
    );
    for (operation, scope, revision) in [
        (ControlOperationId::new([99; 16]).unwrap(), query, 18),
        (command.operation(), foreign_query, 18),
        (command.operation(), query, 17),
        (command.operation(), query, 19),
    ] {
        let lookup = request(&model.recover_workbench_receipt());
        let wrong = WorkbenchReceipt::new(
            operation,
            scope,
            revision,
            peritus_types::Sha256Digest::new([9; 32]),
        )
        .unwrap();
        assert!(
            respond(&mut model, &lookup, AppResponsePayload::WorkbenchReceipt(wrong)).is_empty()
        );
        assert_eq!(model.chat.workbench.unresolved.as_ref().unwrap().0, command);
        assert_eq!(model.chat.buffer, "/resume");
    }
}
