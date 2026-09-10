//! Complete validated initialization proposals assembled from bounded observations.

use super::{
    AppProtocolError, INIT_INSTRUCTION_PATH, InitCommand, InitDiscoveryRequest, InitFileMode,
    InitInstructionPatch, InitSourceKind, InitSourceObservation, MAX_INIT_COMMANDS,
    MAX_INIT_SOURCES, Sha256Digest, WorkbenchQuery, invalid, managed_content, render_exact_diff,
};

/// Complete read-only discovery result and exact approval candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitProposal {
    query: WorkbenchQuery,
    revision: u64,
    folder_digest: Sha256Digest,
    sources: Vec<InitSourceObservation>,
    patch: InitInstructionPatch,
    commands: Vec<InitCommand>,
}

impl InitProposal {
    /// Validates a canonical proposal assembled from bounded local observations.
    ///
    /// # Errors
    /// Rejects invalid revision, ordering, source bindings, or managed-section content.
    pub fn new(
        query: WorkbenchQuery,
        revision: u64,
        folder_digest: Sha256Digest,
        sources: Vec<InitSourceObservation>,
        patch: InitInstructionPatch,
        commands: Vec<InitCommand>,
    ) -> Result<Self, AppProtocolError> {
        if revision == 0
            || sources.len() > MAX_INIT_SOURCES
            || commands.len() > MAX_INIT_COMMANDS
            || !strictly_sorted(&sources)
            || !strictly_sorted(&commands)
            || commands.iter().any(|command| {
                !sources.iter().any(|source| {
                    source.path() == command.source()
                        && matches!(
                            source.kind(),
                            InitSourceKind::Manifest | InitSourceKind::CommandConfig
                        )
                })
            })
            || !instruction_source_matches(&sources, &patch)
        {
            return Err(invalid());
        }
        let expected = managed_content(patch.original_content(), &commands)?;
        if patch.proposed_content() != expected {
            return Err(invalid());
        }
        Ok(Self { query, revision, folder_digest, sources, patch, commands })
    }

    /// Builds the sole canonical managed-section patch from discovery observations.
    ///
    /// # Errors
    /// Rejects malformed source, command, section, or size state.
    pub fn from_discovery(
        request: InitDiscoveryRequest,
        folder_digest: Sha256Digest,
        sources: Vec<InitSourceObservation>,
        original: Option<(String, InitFileMode)>,
        commands: Vec<InitCommand>,
    ) -> Result<Self, AppProtocolError> {
        let (original_content, mode) =
            original.map_or((None, InitFileMode::Regular), |(content, mode)| (Some(content), mode));
        let precondition_digest =
            original_content.as_ref().map(|content| peritus_codec::sha256(content.as_bytes()));
        let precondition_bytes = original_content
            .as_ref()
            .map_or(Ok(0), |content| u64::try_from(content.len()).map_err(|_| invalid()))?;
        let proposed_content = managed_content(original_content.as_deref(), &commands)?;
        let diff = render_exact_diff(
            INIT_INSTRUCTION_PATH,
            original_content.as_deref(),
            &proposed_content,
        );
        let patch = InitInstructionPatch::new(
            INIT_INSTRUCTION_PATH.to_owned(),
            original_content,
            precondition_digest,
            precondition_bytes,
            mode,
            proposed_content,
            diff,
        )?;
        Self::new(request.query(), request.revision(), folder_digest, sources, patch, commands)
    }

    /// Returns exact conversation and workspace scope.
    #[must_use]
    pub const fn query(&self) -> WorkbenchQuery {
        self.query
    }

    /// Returns the inspected conversation aggregate revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the selected root directory identity observed by discovery.
    #[must_use]
    pub const fn folder_digest(&self) -> Sha256Digest {
        self.folder_digest
    }

    /// Borrows canonical path-sorted source observations.
    #[must_use]
    pub fn sources(&self) -> &[InitSourceObservation] {
        &self.sources
    }

    /// Borrows the exact reviewed instruction-file patch.
    #[must_use]
    pub const fn patch(&self) -> &InitInstructionPatch {
        &self.patch
    }

    /// Borrows canonical structured commands, all explicitly unverified.
    #[must_use]
    pub fn commands(&self) -> &[InitCommand] {
        &self.commands
    }
}

fn instruction_source_matches(
    sources: &[InitSourceObservation],
    patch: &InitInstructionPatch,
) -> bool {
    let instructions =
        sources.iter().filter(|source| source.path() == INIT_INSTRUCTION_PATH).collect::<Vec<_>>();
    match patch.original_content() {
        Some(_) => {
            instructions.len() == 1
                && instructions[0].kind() == InitSourceKind::Instructions
                && Some(instructions[0].digest()) == patch.precondition_digest()
                && instructions[0].bytes() == patch.precondition_bytes()
        }
        None => instructions.is_empty(),
    }
}

fn strictly_sorted<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}
