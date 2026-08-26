//! Exact opaque component-byte verification used before revision construction.

use crate::domain::{
    CheckedHarnessGraph, ComponentDeclaration, ComponentId, HarnessDomainError,
    HarnessDomainErrorKind,
};

/// Opaque bytes verified against one exact declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedComponentContent {
    component_id: ComponentId,
    bytes: Vec<u8>,
}

impl VerifiedComponentContent {
    /// Verifies exact byte length and SHA-256 against a declaration.
    ///
    /// # Errors
    ///
    /// Rejects a length or content-digest mismatch.
    pub fn new(
        declaration: &ComponentDeclaration,
        bytes: Vec<u8>,
    ) -> Result<Self, HarnessDomainError> {
        let actual_length = u64::try_from(bytes.len()).map_err(|_| {
            HarnessDomainError::component(
                HarnessDomainErrorKind::ArithmeticOverflow,
                declaration.id().clone(),
            )
        })?;
        if actual_length != declaration.byte_length() {
            return Err(HarnessDomainError::component_numbers(
                HarnessDomainErrorKind::ContentLengthMismatch,
                declaration.id().clone(),
                declaration.byte_length(),
                actual_length,
            ));
        }
        if peritus_codec::sha256(&bytes) != declaration.content_digest() {
            return Err(HarnessDomainError::component(
                HarnessDomainErrorKind::ContentDigestMismatch,
                declaration.id().clone(),
            ));
        }
        Ok(Self { component_id: declaration.id().clone(), bytes })
    }

    /// Returns the component identity bound by these bytes.
    #[must_use]
    pub const fn component_id(&self) -> &ComponentId {
        &self.component_id
    }
    /// Borrows exact opaque content bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Complete component bytes for one checked graph, in canonical component-ID order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComponentContents {
    contents: Vec<VerifiedComponentContent>,
}

impl ComponentContents {
    /// Validates exact one-to-one coverage of the graph's declaration set.
    ///
    /// # Errors
    ///
    /// Rejects duplicate, missing, unexpected, or stale content.
    pub fn new(
        graph: &CheckedHarnessGraph,
        mut contents: Vec<VerifiedComponentContent>,
    ) -> Result<Self, HarnessDomainError> {
        contents.sort_by(|left, right| left.component_id.cmp(&right.component_id));
        for pair in contents.windows(2) {
            if pair[0].component_id == pair[1].component_id {
                return Err(HarnessDomainError::component(
                    HarnessDomainErrorKind::DuplicateContent,
                    pair[1].component_id.clone(),
                ));
            }
        }
        for content in &contents {
            let declaration = graph.declaration(&content.component_id).ok_or_else(|| {
                HarnessDomainError::component(
                    HarnessDomainErrorKind::UnexpectedContent,
                    content.component_id.clone(),
                )
            })?;
            verify_again(declaration, content)?;
        }
        for declaration in graph.declarations() {
            if contents.binary_search_by(|item| item.component_id.cmp(declaration.id())).is_err() {
                return Err(HarnessDomainError::component(
                    HarnessDomainErrorKind::MissingContent,
                    declaration.id().clone(),
                ));
            }
        }
        Ok(Self { contents })
    }

    /// Borrows verified contents in canonical component-ID order.
    #[must_use]
    pub fn entries(&self) -> &[VerifiedComponentContent] {
        &self.contents
    }

    /// Looks up exact verified bytes by stable component identity.
    #[must_use]
    pub fn content(&self, id: &ComponentId) -> Option<&VerifiedComponentContent> {
        self.contents
            .binary_search_by(|item| item.component_id.cmp(id))
            .ok()
            .map(|index| &self.contents[index])
    }

    pub(crate) fn matches_graph(&self, graph: &CheckedHarnessGraph) -> bool {
        self.contents.len() == graph.declarations().len()
            && graph.declarations().iter().zip(&self.contents).all(|(declaration, content)| {
                declaration.id() == content.component_id()
                    && u64::try_from(content.bytes.len()).ok() == Some(declaration.byte_length())
                    && peritus_codec::sha256(&content.bytes) == declaration.content_digest()
            })
    }
}

fn verify_again(
    declaration: &ComponentDeclaration,
    content: &VerifiedComponentContent,
) -> Result<(), HarnessDomainError> {
    let actual_length = u64::try_from(content.bytes.len()).map_err(|_| {
        HarnessDomainError::component(
            HarnessDomainErrorKind::ArithmeticOverflow,
            content.component_id.clone(),
        )
    })?;
    if actual_length != declaration.byte_length() {
        return Err(HarnessDomainError::component_numbers(
            HarnessDomainErrorKind::ContentLengthMismatch,
            content.component_id.clone(),
            declaration.byte_length(),
            actual_length,
        ));
    }
    if peritus_codec::sha256(&content.bytes) != declaration.content_digest() {
        return Err(HarnessDomainError::component(
            HarnessDomainErrorKind::ContentDigestMismatch,
            content.component_id.clone(),
        ));
    }
    Ok(())
}
