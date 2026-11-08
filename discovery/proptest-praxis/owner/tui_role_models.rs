//! Replay role selection through actual slash parsing, request correlation, and rendering.
use super::*;
use crate::runtime::{ProductLaunchContext, ProductProviderOption};
use peritus_app_protocol::*;
use peritus_types::{ProviderProfileId, RunId, SessionId, WorkspaceId};

fn choice(id: &str, effort: ProductModelEffort) -> ProductModelChoice {
    ProductModelChoice::new(id.into(), true).unwrap().with_effort(effort)
}
fn model() -> AppModel {
    let launch = ProductLaunchContext::new(
        WorkspaceId::new([1; 16]).unwrap(),
        "role-replay".into(),
        (2..=4)
            .map(|v| {
                ProductProviderOption::new(
                    ProviderProfileId::new([v; 16]).unwrap(),
                    format!("Provider {v}"),
                )
            })
            .collect(),
        Some(0),
    )
    .unwrap()
    .with_role_models(
        [0, 1, 2],
        ProductRoleModels::new(
            choice("writer-exact", ProductModelEffort::High),
            choice("reviewer-exact", ProductModelEffort::Medium),
            choice("fixer-exact", ProductModelEffort::Low),
        ),
    )
    .unwrap();
    let mut m = AppModel::with_product([5; 32], Some(launch));
    m.context = Some(ProtocolContext::new(
        ProtocolId::new([6; 16]).unwrap(),
        ProtocolVersion::new(2, 1).unwrap(),
        SessionId::new([7; 16]).unwrap(),
    ));
    m
}
fn submit(m: &mut AppModel, command: &str) -> Vec<Effect> {
    m.chat.buffer = command.into();
    m.chat.cursor = command.len();
    m.slash_command(command)
}
fn request(effects: &[Effect]) -> &AppRequestEnvelope {
    effects
        .iter()
        .find_map(|e| if let Effect::Send(AppMessage::Request(r)) = e { Some(r) } else { None })
        .expect("real protocol request")
}
fn reply(m: &mut AppModel, req: &AppRequestEnvelope, advertised: &str) -> Vec<Effect> {
    let AppRequestPayload::QueryModels(q) = req.payload() else { panic!("catalog query expected") };
    m.handle_message(AppMessage::Response(AppResponseEnvelope::new(
        req.context(),
        req.request_id(),
        req.correlation_id(),
        AppResponsePayload::Models(
            ProductModelCatalog::new(
                q.profile(),
                "configured-exact".into(),
                vec![
                    ProductModelInfo::new(advertised.into(), advertised.into(), Some(true))
                        .unwrap(),
                ],
                1,
                false,
                String::new(),
            )
            .unwrap(),
        ),
    )))
}

#[test]
fn cold_catalog_command_saves_exact_model_and_effort_atomically_for_each_role() {
    for role in ["writer", "reviewer", "fixer"] {
        let mut m = model();
        m.chat.run_id = Some(RunId::new([8; 16]).unwrap());
        let before = m.chat.models.clone();
        let effects = submit(&mut m, &format!("/model {role} next-model xhigh"));
        let req = request(&effects);
        assert!(matches!(req.payload(), AppRequestPayload::QueryModels(_)));
        assert_eq!(m.chat.models, before);
        let effects = reply(&mut m, req, "next-model");
        let AppRequestPayload::UpdateModels(update) = request(&effects).payload() else {
            panic!("atomic update expected")
        };
        for (name, actual, prior) in [
            ("writer", update.models().writer(), before.writer()),
            ("reviewer", update.models().reviewer(), before.reviewer()),
            ("fixer", update.models().fixer(), before.fixer()),
        ] {
            if name == role {
                assert_eq!(actual.id(), "next-model");
                assert_eq!(actual.effort(), ProductModelEffort::XHigh);
                assert!(!actual.manual());
            } else {
                assert_eq!(actual, prior);
            }
        }
        assert_eq!(m.chat.models, before, "do not claim success before daemon acknowledgement");
        assert!(
            reply(&mut m, req, "next-model").is_empty(),
            "duplicate response never repeats mutation"
        );
    }
}

#[test]
fn late_catalog_cannot_change_destination_or_overwrite_a_new_draft() {
    for fence in 0..6 {
        let mut m = model();
        let before = m.chat.models.clone();
        let effects = submit(&mut m, "/model fixer next-model xhigh");
        let req = request(&effects);
        match fence {
            0 => m.chat.run_id = Some(RunId::new([9; 16]).unwrap()),
            1 => m.chat.buffer = "new draft".into(),
            2 => m.chat.model_role = ModelRole::Reviewer,
            3 => m.connection_generation += 1,
            4 => {
                m.model_picker_key(KeyEvent::new(
                    KeyCode::Esc,
                    crossterm::event::KeyModifiers::NONE,
                ));
            }
            _ => {
                submit(&mut m, "/models");
            }
        }
        assert!(reply(&mut m, req, "next-model").is_empty());
        assert_eq!(m.chat.models, before);
        if fence == 1 {
            assert_eq!(m.chat.buffer, "new draft");
        }
    }
}

#[test]
fn unavailable_model_and_failed_lookup_retain_commands_and_prior_choices() {
    let command = "/model reviewer absent xhigh";
    for failed in [false, true] {
        let mut m = model();
        let before = m.chat.models.clone();
        let effects = submit(&mut m, command);
        let req = request(&effects);
        if failed {
            m.handle_message(AppMessage::Response(AppResponseEnvelope::new(
                req.context(),
                req.request_id(),
                req.correlation_id(),
                AppResponsePayload::Error(AppProtocolError::new(
                    AppErrorCode::InvalidIdentifier,
                    None,
                )),
            )));
        } else {
            assert!(reply(&mut m, req, "some-other-model").is_empty());
        }
        assert_eq!(m.chat.buffer, command);
        assert_eq!(m.chat.models, before);
    }
    for bad in [
        "/model fixer next-model nonsense",
        "/model reviewer refresh extra",
        "/model writer next-model high extra",
    ] {
        let mut m = model();
        let before = m.chat.models.clone();
        assert!(submit(&mut m, bad).is_empty());
        assert_eq!(m.chat.buffer, bad);
        assert_eq!(m.chat.models, before);
    }
}

#[test]
fn disconnect_or_timeout_restores_the_command_without_applying_a_selection() {
    for disconnect in [false, true] {
        let mut m = model();
        let before = m.chat.models.clone();
        let command = "/model fixer next-model xhigh";
        let effects = submit(&mut m, command);
        let req = request(&effects);
        if disconnect {
            m.interrupt_connection();
        } else {
            m.tick_count = u64::MAX;
            m.expire_pending_requests();
        }
        assert_eq!(m.chat.buffer, command);
        assert_eq!(m.chat.models, before);
        assert!(reply(&mut m, req, "next-model").is_empty());
    }
}

#[test]
fn new_conversation_defaults_and_manual_command_preserve_other_roles() {
    let mut m = model();
    let providers = m.chat_providers().unwrap();
    assert_eq!(providers.writer(), ProviderProfileId::new([2; 16]).unwrap());
    assert_eq!(providers.reviewer(), ProviderProfileId::new([3; 16]).unwrap());
    assert_eq!(providers.fixer(), ProviderProfileId::new([4; 16]).unwrap());
    assert_eq!(m.chat.models.fixer().id(), "fixer-exact");
    let before = m.chat.models.clone();
    assert!(submit(&mut m, "/model fixer manual explicit-fixer xhigh").is_empty());
    assert_eq!(m.chat.models.writer(), before.writer());
    assert_eq!(m.chat.models.reviewer(), before.reviewer());
    assert_eq!(m.chat.models.fixer().id(), "explicit-fixer");
    assert_eq!(m.chat.models.fixer().effort(), ProductModelEffort::XHigh);
    assert!(m.chat.models.fixer().manual());
}

#[test]
fn models_command_renders_all_agents_and_menu_controls_on_normal_terminals() {
    for (width, height) in [(80, 24), (120, 30)] {
        let mut m = model();
        submit(&mut m, "/models");
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| crate::render::draw(frame, &m)).unwrap();
        let output = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect::<String>();
        for expected in [
            "writer-exact",
            "reviewer-exact",
            "fixer-exact",
            "Writer",
            "Reviewer",
            "Fixer",
            "Choose an agent",
            "Enter next: model",
        ] {
            assert!(output.contains(expected), "{width}x{height} missing {expected}: {output}");
        }
    }
}

fn key(m: &mut AppModel, code: KeyCode) -> Vec<Effect> {
    m.handle_chat_key(KeyEvent::new(code, crossterm::event::KeyModifiers::NONE))
}

#[test]
fn menu_selects_agent_then_model_then_effort_and_saves_once() {
    for (agent, role) in
        [('1', ModelRole::Writer), ('2', ModelRole::Reviewer), ('3', ModelRole::Fixer)]
    {
        let mut m = model();
        m.chat.run_id = Some(RunId::new([8; 16]).unwrap());
        let before = m.chat.models.clone();
        assert!(submit(&mut m, "/model").is_empty(), "agent selection precedes discovery");
        assert_eq!(m.model_setup_step(), Some(ModelStep::Agent));
        let effects = key(&mut m, KeyCode::Char(agent));
        let req = request(&effects);
        assert!(reply(&mut m, req, "menu-chosen-model").is_empty());
        assert_eq!(m.model_setup_step(), Some(ModelStep::Model));
        assert!(key(&mut m, KeyCode::Enter).is_empty());
        assert_eq!(m.model_setup_step(), Some(ModelStep::Effort));
        assert_eq!(m.model_setup_choice().unwrap().id(), "menu-chosen-model");
        assert_eq!(m.chat.models, before, "choosing the model must not save half the settings");
        key(&mut m, KeyCode::Home);
        for _ in 0..5 {
            key(&mut m, KeyCode::Down);
        }
        let effects = key(&mut m, KeyCode::Enter);
        let AppRequestPayload::UpdateModels(update) = request(&effects).payload() else {
            panic!("one atomic update")
        };
        assert_eq!(role.choice(update.models()).id(), "menu-chosen-model");
        assert_eq!(role.choice(update.models()).effort(), ProductModelEffort::XHigh);
        for other in [ModelRole::Writer, ModelRole::Reviewer, ModelRole::Fixer] {
            if other != role {
                assert_eq!(other.choice(update.models()), other.choice(&before));
            }
        }
        assert_eq!(m.chat.models, before, "wait for acknowledgement");
        assert!(m.model_setup_step().is_none());
        assert!(
            key(&mut m, KeyCode::Enter).is_empty(),
            "Enter cannot submit a duplicate settings update"
        );
    }
}

#[test]
fn menu_back_cancel_failed_discovery_and_retry_preserve_prior_settings() {
    let mut m = model();
    let before = m.chat.models.clone();
    submit(&mut m, "/model");
    key(&mut m, KeyCode::Down);
    assert_eq!(m.chat.model_role, ModelRole::Reviewer);
    let effects = key(&mut m, KeyCode::Enter);
    let req = request(&effects);
    let AppRequestPayload::QueryModels(q) = req.payload() else { panic!("reviewer catalog") };
    assert_eq!(q.profile(), ProviderProfileId::new([3; 16]).unwrap());
    // An error keeps the model page open with retry/back, never picks another model.
    m.handle_message(AppMessage::Response(AppResponseEnvelope::new(
        req.context(),
        req.request_id(),
        req.correlation_id(),
        AppResponsePayload::Error(AppProtocolError::new(AppErrorCode::InvalidIdentifier, None)),
    )));
    assert_eq!(m.model_setup_step(), Some(ModelStep::Model));
    assert!(m.chat.model_picker());
    assert!(key(&mut m, KeyCode::Enter).is_empty());
    let effects = key(&mut m, KeyCode::Char('r'));
    let req = request(&effects);
    reply(&mut m, req, "menu-chosen-model");
    key(&mut m, KeyCode::Enter);
    assert_eq!(m.model_setup_step(), Some(ModelStep::Effort));
    key(&mut m, KeyCode::Esc);
    assert_eq!(m.model_setup_step(), Some(ModelStep::Model));
    assert!(m.model_setup_choice().is_none());
    key(&mut m, KeyCode::Esc);
    assert_eq!(m.model_setup_step(), Some(ModelStep::Agent));
    key(&mut m, KeyCode::Esc);
    assert!(m.model_setup_step().is_none());
    assert_eq!(m.chat.models, before);
    assert!(!m.chat.model_picker());
    assert!(!m.chat.effort_picker());
}

#[test]
fn menu_never_saves_to_a_changed_destination_or_agent() {
    for fence in 0..5 {
        let mut m = model();
        let before = m.chat.models.clone();
        submit(&mut m, "/model");
        let effects = key(&mut m, KeyCode::Char('3'));
        reply(&mut m, request(&effects), "menu-chosen-model");
        key(&mut m, KeyCode::Enter);
        match fence {
            0 => m.chat.run_id = Some(RunId::new([9; 16]).unwrap()),
            1 => m.chat.buffer = "new draft".into(),
            2 => m.chat.model_role = ModelRole::Reviewer,
            3 => m.connection_generation += 1,
            _ => {
                m.interrupt_connection();
            }
        }
        assert!(key(&mut m, KeyCode::Enter).is_empty());
        assert_eq!(m.chat.models, before);
        assert!(m.model_setup_step().is_none());
        if fence == 1 {
            assert_eq!(m.chat.buffer, "new draft");
        }
    }
}

#[test]
fn refresh_requests_fresh_catalog_and_removed_model_cannot_be_saved() {
    let mut m = model();
    let before = m.chat.models.clone();
    let effects = submit(&mut m, "/model fixer refresh");
    let req = request(&effects);
    let AppRequestPayload::QueryModels(query) = req.payload() else { panic!("catalog request") };
    assert!(query.refresh());
    reply(&mut m, req, "removed-model");
    key(&mut m, KeyCode::Enter);
    assert_eq!(m.model_setup_step(), Some(ModelStep::Effort));
    m.chat.catalog = Some(
        ProductModelCatalog::new(
            query.profile(),
            "configured-exact".into(),
            vec![],
            1,
            false,
            "Catalog changed".into(),
        )
        .unwrap(),
    );
    assert!(key(&mut m, KeyCode::Enter).is_empty());
    assert_eq!(m.chat.models, before);
    assert_eq!(m.model_setup_step(), Some(ModelStep::Model));
}

#[test]
fn catalog_refresh_during_effort_selection_preserves_the_model_when_going_back() {
    let mut m = model();
    m.chat.run_id = Some(RunId::new([8; 16]).unwrap());
    let before = m.chat.models.clone();
    submit(&mut m, "/model");
    let initial = key(&mut m, KeyCode::Char('3'));
    let respond = |m: &mut AppModel, req: &AppRequestEnvelope, ids: &[&str]| {
        let AppRequestPayload::QueryModels(query) = req.payload() else {
            panic!("catalog query expected")
        };
        m.handle_message(AppMessage::Response(AppResponseEnvelope::new(
            req.context(),
            req.request_id(),
            req.correlation_id(),
            AppResponsePayload::Models(
                ProductModelCatalog::new(
                    query.profile(),
                    "configured-exact".into(),
                    ids.iter()
                        .map(|id| {
                            ProductModelInfo::new((*id).into(), (*id).into(), Some(true)).unwrap()
                        })
                        .collect(),
                    1,
                    false,
                    String::new(),
                )
                .unwrap(),
            ),
        )))
    };
    assert!(respond(&mut m, request(&initial), &["other", "selected"]).is_empty());
    key(&mut m, KeyCode::Down);
    let refresh = key(&mut m, KeyCode::Char('r'));
    assert!(key(&mut m, KeyCode::Enter).is_empty());
    assert_eq!(m.model_setup_choice().unwrap().id(), "selected");
    assert!(respond(&mut m, request(&refresh), &["selected", "other"]).is_empty());
    assert_eq!(m.model_setup_step(), Some(ModelStep::Effort));
    assert!(key(&mut m, KeyCode::Esc).is_empty());
    assert!(key(&mut m, KeyCode::Enter).is_empty());
    assert_eq!(m.model_setup_choice().unwrap().id(), "selected");
    assert_eq!(m.chat.models, before, "Back and re-entry must not publish settings");
    key(&mut m, KeyCode::Home);
    for _ in 0..5 {
        key(&mut m, KeyCode::Down);
    }
    let saved = key(&mut m, KeyCode::Enter);
    let AppRequestPayload::UpdateModels(update) = request(&saved).payload() else {
        panic!("one atomic update")
    };
    assert_eq!(update.models().fixer().id(), "selected");
    assert_eq!(update.models().fixer().effort(), ProductModelEffort::XHigh);
    assert_eq!(update.models().writer(), before.writer());
    assert_eq!(update.models().reviewer(), before.reviewer());
}

#[test]
fn standalone_effort_still_changes_only_effort_and_model_steps_render_staged_choice() {
    let mut m = model();
    let before = m.chat.models.clone();
    submit(&mut m, "/effort fixer");
    assert!(m.model_setup_step().is_none());
    key(&mut m, KeyCode::Home);
    for _ in 0..5 {
        key(&mut m, KeyCode::Down);
    }
    key(&mut m, KeyCode::Enter);
    assert_eq!(m.chat.models.fixer().id(), before.fixer().id());
    assert_eq!(m.chat.models.fixer().effort(), ProductModelEffort::XHigh);
    for (width, height) in [(80, 24), (120, 30)] {
        let mut m = model();
        submit(&mut m, "/model");
        let effects = key(&mut m, KeyCode::Char('3'));
        reply(&mut m, request(&effects), "menu-chosen-model");
        for (step, text) in
            [(ModelStep::Model, "Choose a model"), (ModelStep::Effort, "Choose reasoning effort")]
        {
            if step == ModelStep::Effort {
                key(&mut m, KeyCode::Enter);
            }
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
            terminal.draw(|frame| crate::render::draw(frame, &m)).unwrap();
            let output = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(ratatui::buffer::Cell::symbol)
                .collect::<String>();
            assert!(output.contains(text));
            assert!(output.contains("menu-chosen-model"));
            assert!(output.contains("Esc back"));
        }
    }
}
