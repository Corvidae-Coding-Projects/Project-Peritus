//! Serialized receive loop and closed message dispatch.

use peritus_app_protocol::AppProtocolLimits;
use peritus_journal::SqliteJournal;
use tokio::sync::mpsc;

use super::{
    super::message::{AuthorityMessage, Response},
    error::{journal_error, owner_stopped, require_diagnostic, require_mutation},
    storage::reconcile_command,
};
use crate::{DaemonError, DaemonLifecycle, artifact::ArtifactAuthority, prompt::PromptBroker};

pub(super) async fn run(
    mut journal: SqliteJournal,
    mut lifecycle: DaemonLifecycle,
    mut artifacts: ArtifactAuthority,
    _prompts: PromptBroker,
    mut receiver: mpsc::Receiver<AuthorityMessage>,
) -> Result<(), DaemonError> {
    while let Some(message) = receiver.recv().await {
        match message {
            AuthorityMessage::Status { respond } => reply(
                respond,
                lifecycle.status(AppProtocolLimits::PRODUCTION.max_diagnostic_bytes()),
            ),
            AuthorityMessage::Principal { digest, respond } => {
                let result = require_diagnostic(&lifecycle)
                    .and_then(|()| journal.application_principal(digest).map_err(journal_error));
                reply(respond, result);
            }
            AuthorityMessage::Session { session_id, respond } => {
                let result = require_diagnostic(&lifecycle)
                    .and_then(|()| journal.application_session(session_id).map_err(journal_error));
                reply(respond, result);
            }
            AuthorityMessage::AuthorityEpoch { respond } => {
                let result = journal
                    .current_authority_epoch()
                    .map_err(journal_error)
                    .map(|epoch| epoch.map(|value| value.get()));
                reply(respond, result);
            }
            AuthorityMessage::Events { cursor, maximum, respond } => {
                let result = require_diagnostic(&lifecycle).and_then(|()| {
                    journal.global_events_after(cursor, maximum).map_err(journal_error)
                });
                reply(respond, result);
            }
            AuthorityMessage::ClaimOutbox { now, lease_until, respond } => {
                let result = require_mutation(&lifecycle)
                    .and_then(|()| journal.claim_outbox(now, lease_until).map_err(journal_error));
                reply(respond, result);
            }
            AuthorityMessage::AcknowledgeOutbox { id, fence, respond } => {
                reply(respond, journal.acknowledge_outbox(id, fence).map_err(journal_error));
            }
            AuthorityMessage::OpenArtifact {
                actor_id,
                session_id,
                request,
                maximum_chunk_bytes,
                respond,
            } => {
                let result = require_diagnostic(&lifecycle).and_then(|()| {
                    artifacts.open_download(
                        &journal,
                        actor_id,
                        session_id,
                        request.transfer_id(),
                        request.artifact_id(),
                        maximum_chunk_bytes,
                    )
                });
                reply(respond, result);
            }
            AuthorityMessage::PollArtifact {
                actor_id,
                session_id,
                transfer_id,
                maximum_chunk_bytes,
                respond,
            } => reply(
                respond,
                artifacts.poll_download(actor_id, session_id, transfer_id, maximum_chunk_bytes),
            ),
            AuthorityMessage::BeginArtifactUpload {
                actor_id,
                session_id,
                metadata,
                maximum_chunk_bytes,
                respond,
            } => {
                let result = require_mutation(&lifecycle).and_then(|()| {
                    artifacts.begin_upload(
                        &mut journal,
                        actor_id,
                        session_id,
                        metadata,
                        maximum_chunk_bytes,
                    )
                });
                reply(respond, result);
            }
            AuthorityMessage::UploadArtifactChunk { actor_id, session_id, chunk, respond } => {
                let result = require_mutation(&lifecycle)
                    .and_then(|()| artifacts.upload_chunk(actor_id, session_id, &chunk));
                reply(respond, result);
            }
            AuthorityMessage::CompleteArtifactUpload {
                actor_id,
                session_id,
                completion,
                respond,
            } => {
                let result = require_mutation(&lifecycle).and_then(|()| {
                    artifacts.complete_upload(&mut journal, actor_id, session_id, completion)
                });
                reply(respond, result);
            }
            AuthorityMessage::CancelArtifactTransfer {
                actor_id,
                session_id,
                cancellation,
                respond,
            } => reply(respond, artifacts.cancel(actor_id, session_id, cancellation)),
            AuthorityMessage::AbandonArtifactTransfers {
                actor_id,
                session_id,
                transfer_ids,
                respond,
            } => {
                artifacts.abandon(actor_id, session_id, &transfer_ids);
                reply(respond, Ok(()));
            }
            AuthorityMessage::BindPrincipal { principal, respond } => {
                let result = require_mutation(&lifecycle).and_then(|()| {
                    journal.bind_application_principal(principal).map_err(journal_error)
                });
                reply(respond, result);
            }
            AuthorityMessage::OpenSession { session, respond } => {
                let result = require_mutation(&lifecycle).and_then(|()| {
                    journal.open_application_session(session).map_err(journal_error)
                });
                reply(respond, result);
            }
            AuthorityMessage::ObserveSession {
                session_id,
                actor_id,
                protocol_id,
                version_major,
                version_minor,
                respond,
            } => {
                let result = require_mutation(&lifecycle).and_then(|()| {
                    journal
                        .observe_application_session_negotiation(
                            session_id,
                            actor_id,
                            protocol_id,
                            version_major,
                            version_minor,
                        )
                        .map_err(journal_error)
                });
                reply(respond, result);
            }
            AuthorityMessage::AdvanceSession { session_id, state, respond } => reply(
                respond,
                journal.advance_application_session(session_id, state).map_err(journal_error),
            ),
            AuthorityMessage::AdmitCommand { command, respond } => {
                let result = require_mutation(&lifecycle).and_then(|()| {
                    journal.admit_application_command(command).map_err(journal_error)
                });
                reply(respond, result);
            }
            AuthorityMessage::DispatchCommand { submission, respond } => {
                let result = require_mutation(&lifecycle)
                    .and_then(|()| crate::domain::dispatch(&mut journal, submission));
                reply(respond, result);
            }
            AuthorityMessage::SettleCommand { command_id, request_digest, settlement, respond } => {
                reply(
                    respond,
                    journal
                        .settle_application_command(command_id, request_digest, settlement)
                        .map_err(journal_error),
                )
            }
            AuthorityMessage::ReconcileCommand {
                command_id,
                request_digest,
                domain_command_digest,
                respond,
            } => reply(
                respond,
                reconcile_command(&mut journal, command_id, request_digest, domain_command_digest),
            ),
            AuthorityMessage::RecoverCommands { maximum, respond } => reply(
                respond,
                journal.unsettled_application_commands(maximum).map_err(journal_error),
            ),
            AuthorityMessage::BeginArtifact { artifact, respond } => {
                let result = require_mutation(&lifecycle).and_then(|()| {
                    journal.begin_application_artifact(artifact).map_err(journal_error)
                });
                reply(respond, result);
            }
            AuthorityMessage::CompleteArtifact { artifact_id, producing_position, respond } => {
                reply(
                    respond,
                    journal
                        .complete_application_artifact(artifact_id, producing_position)
                        .map_err(journal_error),
                );
            }
            AuthorityMessage::RegisterWorkspace { workspace, respond } => {
                let result = require_mutation(&lifecycle).and_then(|()| {
                    journal.register_application_workspace(workspace).map_err(journal_error)
                });
                reply(respond, result);
            }
            AuthorityMessage::SetWorkspaceState { workspace_id, state, respond } => reply(
                respond,
                journal.set_application_workspace_state(workspace_id, state).map_err(journal_error),
            ),
            AuthorityMessage::AdvanceStartup { phase, respond } => {
                reply(respond, lifecycle.advance(phase));
            }
            AuthorityMessage::ReadOnly { diagnostic, respond } => {
                lifecycle.read_only(diagnostic);
                reply(respond, Ok(()));
            }
            AuthorityMessage::BeginDraining { respond } => {
                lifecycle.begin_draining();
                reply(respond, Ok(()));
            }
            AuthorityMessage::Stop { respond } => {
                lifecycle.unavailable();
                reply(respond, Ok(()));
                return Ok(());
            }
        }
    }
    Err(owner_stopped())
}

fn reply<T>(respond: Response<T>, result: Result<T, DaemonError>) {
    let _ = respond.send(result);
}
