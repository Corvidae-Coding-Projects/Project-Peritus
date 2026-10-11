//! Approval-first initialization fixtures retain exact preconditions and unverified command argv.

use super::{
    FixtureClass, GeneratedFixtureCase,
    values::{context, encoded, id, request},
};
use crate::{
    AppRequestPayload, AppResponseEnvelope, AppResponsePayload, ControlOperationId, ConversationId,
    InitCommand, InitCommandKind, InitCommandVerification, InitDiscoveryRequest, InitFileMode,
    InitProposal, InitSourceKind, InitSourceObservation, WorkbenchCommand, WorkbenchIntent,
    WorkbenchQuery,
};
use peritus_codec::{CodecError, CodecLimits};

pub(super) fn cases(limits: CodecLimits) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    let query =
        WorkbenchQuery::new(id(91, ConversationId::new), id(92, peritus_types::WorkspaceId::new));
    let discovery = InitDiscoveryRequest::new(query, 7).expect("discovery");
    let mut cases = vec![encoded(
        "minimal-init-discovery",
        FixtureClass::Minimal,
        &request(AppRequestPayload::DiscoverInit(discovery)),
        limits,
    )?];

    let original =
        "# Existing project instructions\n\nPreserve this guidance exactly.\n".to_owned();
    let sources = vec![
        InitSourceObservation::new(
            "AGENTS.md".to_owned(),
            InitSourceKind::Instructions,
            peritus_codec::sha256(original.as_bytes()),
            original.len() as u64,
        )
        .expect("instructions"),
        InitSourceObservation::new(
            "Cargo.toml".to_owned(),
            InitSourceKind::Manifest,
            peritus_codec::sha256(b"[package]\nname = \"sample\"\n"),
            26,
        )
        .expect("manifest"),
    ];
    let commands = vec![
        InitCommand::new(
            InitCommandKind::Build,
            "Cargo.toml".to_owned(),
            "cargo".to_owned(),
            vec!["build".to_owned()],
            InitCommandVerification::Unverified,
        )
        .expect("build"),
        InitCommand::new(
            InitCommandKind::Test,
            "Cargo.toml".to_owned(),
            "cargo".to_owned(),
            vec!["test".to_owned()],
            InitCommandVerification::Unverified,
        )
        .expect("test"),
    ];
    let proposal = InitProposal::from_discovery(
        discovery,
        peritus_types::Sha256Digest::new([93; 32]),
        sources,
        Some((original, InitFileMode::Regular)),
        commands,
    )
    .expect("proposal");
    let response = AppResponseEnvelope::new(
        context(),
        id(94, crate::RequestId::new),
        id(95, crate::CorrelationId::new),
        AppResponsePayload::InitProposal(proposal.clone()),
    );
    cases.push(encoded("realistic-init-proposal", FixtureClass::Realistic, &response, limits)?);
    let command = WorkbenchCommand::new(
        id(96, ControlOperationId::new),
        query,
        7,
        WorkbenchIntent::ApplyInitDiff(proposal),
    );
    cases.push(encoded(
        "realistic-init-apply",
        FixtureClass::Realistic,
        &request(AppRequestPayload::WorkbenchCommand(command)),
        limits,
    )?);
    artifact_cases(&mut cases, discovery, limits)?;
    Ok(cases)
}

fn artifact_cases(
    cases: &mut Vec<GeneratedFixtureCase>,
    discovery: InitDiscoveryRequest,
    limits: CodecLimits,
) -> Result<(), CodecError> {
    use crate::{
        InitArtifactDiscovery, InitArtifactPage, InitArtifactPageRequest, InitArtifactProposal,
        InitContentReference, InitSourceSelection,
    };
    let reference = InitContentReference::new(peritus_codec::sha256(b"manifest"), 8);
    let review = InitContentReference::new(peritus_codec::sha256(b"review"), 6);
    let proposal = InitArtifactProposal::new(discovery, reference, review);
    let selected = InitArtifactDiscovery::new(
        discovery,
        Some(reference),
        Some(
            InitSourceSelection::new("tools/package.json".to_owned(), InitSourceKind::Manifest)
                .expect("source"),
        ),
        None,
    )
    .expect("selection");
    let page_request = InitArtifactPageRequest::new(proposal, 0, 6).expect("page request");
    for (name, payload) in [
        ("realistic-init-artifact-discovery", AppRequestPayload::DiscoverInitArtifacts(selected)),
        (
            "minimal-init-artifact-page-request",
            AppRequestPayload::QueryInitArtifactPage(page_request),
        ),
        (
            "realistic-init-artifact-apply",
            AppRequestPayload::WorkbenchCommand(WorkbenchCommand::new(
                id(97, ControlOperationId::new),
                discovery.query(),
                discovery.revision(),
                WorkbenchIntent::ApplyInitArtifact(proposal),
            )),
        ),
    ] {
        cases.push(encoded(name, FixtureClass::Realistic, &request(payload), limits)?);
    }
    for (name, payload) in [
        ("minimal-init-artifact-proposal", AppResponsePayload::InitArtifactProposal(proposal)),
        (
            "minimal-init-artifact-page",
            AppResponsePayload::InitArtifactPage(
                InitArtifactPage::new(page_request, b"review".to_vec()).expect("page"),
            ),
        ),
    ] {
        let response = AppResponseEnvelope::new(
            context(),
            id(94, crate::RequestId::new),
            id(95, crate::CorrelationId::new),
            payload,
        );
        cases.push(encoded(name, FixtureClass::Realistic, &response, limits)?);
    }
    Ok(())
}
