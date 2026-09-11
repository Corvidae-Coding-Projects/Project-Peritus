//! Provider-bound local preview. Confirmation revalidates this exact immutable observation.

use super::{
    AppProtocolError, ProductModelChoice, WorkbenchImageMetadata, WorkbenchImageRequest, invalid,
};

/// Complete preview to display before the user explicitly confirms image inclusion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchImagePreview {
    request: WorkbenchImageRequest,
    image: WorkbenchImageMetadata,
    provider_revision: u64,
    resolved_model: String,
}
impl WorkbenchImagePreview {
    /// Binds detected metadata to the exact current provider observation.
    ///
    /// # Errors
    /// Rejects zero provider revision or invalid resolved model identifiers.
    pub fn new(
        request: WorkbenchImageRequest,
        image: WorkbenchImageMetadata,
        provider_revision: u64,
        resolved_model: String,
    ) -> Result<Self, AppProtocolError> {
        if provider_revision == 0 {
            return Err(invalid());
        }
        ProductModelChoice::new(resolved_model.clone(), false).map_err(|_| invalid())?;
        Ok(Self { request, image, provider_revision, resolved_model })
    }
    /// Borrows exact original preview selection and visibility scope.
    #[must_use]
    pub const fn request(&self) -> &WorkbenchImageRequest {
        &self.request
    }
    /// Returns original encoded metadata and decoded dimensions/frame count.
    #[must_use]
    pub const fn image(&self) -> WorkbenchImageMetadata {
        self.image
    }
    /// Returns observed configured-provider revision.
    #[must_use]
    pub const fn provider_revision(&self) -> u64 {
        self.provider_revision
    }
    /// Borrows the actual resolved model name, not a default/fallback label.
    #[must_use]
    pub fn resolved_model(&self) -> &str {
        &self.resolved_model
    }
}
