//! Deterministic bounded ASCII rendering of typed approval facts.

mod builder;
mod collections;
mod encoding;
mod facts;

use self::builder::{Builder, safe_ascii};
use self::encoding::decimal_usize;

#[doc = " Exact maximum rendered approval size in bytes."]
pub const MAX_RENDERED_APPROVAL_BYTES: usize = 16_384;
#[doc = " Maximum permission summaries rendered before whole-item truncation."]
pub const MAX_RENDERED_PERMISSIONS: usize = 64;
#[doc = " Maximum actor summaries rendered from each provenance participant set."]
pub const MAX_RENDERED_PARTICIPANTS: usize = 64;
#[doc = " Maximum one complete rendered field size."]
pub const MAX_RENDERED_FIELD_BYTES: usize = 256;
#[doc = " Bytes reserved for mandatory truncation metadata."]
pub const RENDER_TRUNCATION_SUFFIX_BYTES: usize = 96;
#[doc = " Bounded deterministic safe approval projection."]
#[derive(Debug, Eq, PartialEq)]
pub struct RenderedApproval {
    text: String,
    was_truncated: bool,
    omitted_permissions: usize,
    omitted_producing_participants: usize,
    omitted_review_participants: usize,
}
impl RenderedApproval {
    #[doc = " Borrows the valid printable-ASCII UTF-8 projection."]
    #[must_use]
    pub const fn as_str(&self) -> &str {
        self.text.as_str()
    }
    #[doc = " Returns whether any whole field or permission was omitted."]
    #[must_use]
    pub const fn was_truncated(&self) -> bool {
        self.was_truncated
    }
    #[doc = " Returns the exact number of permission summaries omitted."]
    #[must_use]
    pub const fn omitted_permissions(&self) -> usize {
        self.omitted_permissions
    }
    #[doc = " Returns the exact number of producing-attempt participant summaries omitted."]
    #[must_use]
    pub const fn omitted_producing_participants(&self) -> usize {
        self.omitted_producing_participants
    }
    #[doc = " Returns the exact number of review participant summaries omitted."]
    #[must_use]
    pub const fn omitted_review_participants(&self) -> usize {
        self.omitted_review_participants
    }
}
#[doc = " Renders one approval aggregate without raw payload, reason, metadata, secret, or debug values."]
#[doc = ""]
#[doc = " # Errors"]
#[doc = ""]
#[doc = " Returns `UnsafeRenderingInput` if a typed field violates printable ASCII or field bounds."]
#[allow(
    clippy::needless_as_bytes,
    clippy::redundant_as_str,
    reason = "pinned Verus supports byte lengths through explicit str and byte-slice views"
)]
pub fn render_approval(
    aggregate: &crate::ApprovalAggregate,
) -> Result<RenderedApproval, crate::ApprovalError> {
    let request = aggregate.request();
    let mut builder = Builder::new();
    facts::render(&mut builder, aggregate)?;
    let counts = collections::render(&mut builder, request)?;
    let mut suffix = String::new();
    suffix.push_str("permission-count=");
    suffix.push_str(&decimal_usize(counts.permission_total));
    suffix.push('/');
    suffix.push_str(&decimal_usize(counts.permission_omitted));
    suffix.push_str(";producing-count=");
    suffix.push_str(&decimal_usize(counts.producing_total));
    suffix.push('/');
    suffix.push_str(&decimal_usize(counts.producing_omitted));
    suffix.push_str(";review-count=");
    suffix.push_str(&decimal_usize(counts.review_total));
    suffix.push('/');
    suffix.push_str(&decimal_usize(counts.review_omitted));
    suffix.push_str(";truncated=");
    suffix.push_str(if builder.truncated { "true" } else { "false" });
    suffix.push(';');
    let suffix_length = suffix.as_str().as_bytes().len();
    if suffix_length > RENDER_TRUNCATION_SUFFIX_BYTES
        || builder.text.as_str().as_bytes().len().saturating_add(suffix_length)
            > MAX_RENDERED_APPROVAL_BYTES
        || !safe_ascii(suffix.as_str().as_bytes())
    {
        return Err(crate::ApprovalError::UnsafeRenderingInput);
    }
    builder.text.push_str(&suffix);
    Ok(RenderedApproval {
        text: builder.text,
        was_truncated: builder.truncated,
        omitted_permissions: counts.permission_omitted,
        omitted_producing_participants: counts.producing_omitted,
        omitted_review_participants: counts.review_omitted,
    })
}
