//! Immutable out-of-line product-finding bodies and their compact authenticated descriptors.

use peritus_codec::sha256;
use peritus_spec::FindingSeverity;
use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};

use super::{ProductFinding, ProductFindingCategory, ProductReviewError};

/// First ordinal reserved for immutable product-finding sources.
///
/// Existing attachment and improvement catalogs retain their lower-half ordinals unchanged.
pub const PRODUCT_FINDING_SOURCE_ORDINAL_BASE: u64 = 1_u64 << 63;

const PREVIEW_BYTES: usize = 160;
const BODY_HEADER: &[u8] = b"peritus-product-finding-body-v1\n";
const SUMMARY_HEADER: &[u8] = b"peritus-product-review-summary-v1\n";

/// Exact inline fields supplied by one reviewer before durable publication.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductFindingBody {
    title: String,
    description: String,
    location: String,
    provenance: String,
    reproduction: String,
    remediation: String,
}

impl ProductFindingBody {
    pub(super) fn new(
        title: String,
        description: String,
        location: String,
        provenance: String,
        reproduction: String,
        remediation: String,
    ) -> Self {
        Self { title, description, location, provenance, reproduction, remediation }
    }

    /// Exact title.
    #[must_use]
    pub fn title(&self) -> &str { &self.title }
    /// Exact description.
    #[must_use]
    pub fn description(&self) -> &str { &self.description }
    /// Exact source location.
    #[must_use]
    pub fn location(&self) -> &str { &self.location }
    /// Exact normalized location provenance.
    #[must_use]
    pub fn provenance(&self) -> &str { &self.provenance }
    /// Exact reproduction or evidence instructions.
    #[must_use]
    pub fn reproduction(&self) -> &str { &self.reproduction }
    /// Exact remediation.
    #[must_use]
    pub fn remediation(&self) -> &str { &self.remediation }
}

/// Authenticated byte range for one field in the canonical body artifact.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProductFindingFieldReference {
    offset: u64,
    bytes: u64,
    digest: Sha256Digest,
}

impl ProductFindingFieldReference {
    /// Restores one exact field range.
    #[must_use]
    pub const fn new(offset: u64, bytes: u64, digest: Sha256Digest) -> Self {
        Self { offset, bytes, digest }
    }

    /// First byte in the canonical body artifact.
    #[must_use]
    pub const fn offset(self) -> u64 { self.offset }
    /// Exact UTF-8 byte length.
    #[must_use]
    pub const fn bytes(self) -> u64 { self.bytes }
    /// SHA-256 of the exact field bytes.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest { self.digest }

    fn end(self) -> Option<u64> { self.offset.checked_add(self.bytes) }
}

/// Exact ranges for every reviewer-authored body field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProductFindingBodyFields {
    title: ProductFindingFieldReference,
    description: ProductFindingFieldReference,
    location: ProductFindingFieldReference,
    provenance: ProductFindingFieldReference,
    reproduction: ProductFindingFieldReference,
    remediation: ProductFindingFieldReference,
}

impl ProductFindingBodyFields {
    /// Restores a canonical ordered field table.
    ///
    /// # Errors
    /// Rejects overlapping, adjacent, or overflowing field ranges.
    #[allow(clippy::too_many_arguments, reason = "the six stable body fields remain explicit")]
    pub fn restore(
        title: ProductFindingFieldReference,
        description: ProductFindingFieldReference,
        location: ProductFindingFieldReference,
        provenance: ProductFindingFieldReference,
        reproduction: ProductFindingFieldReference,
        remediation: ProductFindingFieldReference,
    ) -> Result<Self, ProductReviewError> {
        let fields = Self { title, description, location, provenance, reproduction, remediation };
        if fields.ordered().windows(2).any(|pair| {
            pair[0].end().is_none_or(|end| end >= pair[1].offset())
        }) {
            return Err(ProductReviewError::new(
                "finding body field ranges are not canonically ordered",
            ));
        }
        Ok(fields)
    }

    /// Title range.
    #[must_use]
    pub const fn title(&self) -> ProductFindingFieldReference { self.title }
    /// Description range.
    #[must_use]
    pub const fn description(&self) -> ProductFindingFieldReference { self.description }
    /// Location range.
    #[must_use]
    pub const fn location(&self) -> ProductFindingFieldReference { self.location }
    /// Normalized provenance range.
    #[must_use]
    pub const fn provenance(&self) -> ProductFindingFieldReference { self.provenance }
    /// Reproduction range.
    #[must_use]
    pub const fn reproduction(&self) -> ProductFindingFieldReference { self.reproduction }
    /// Remediation range.
    #[must_use]
    pub const fn remediation(&self) -> ProductFindingFieldReference { self.remediation }

    fn ordered(&self) -> [ProductFindingFieldReference; 6] {
        [
            self.title,
            self.description,
            self.location,
            self.provenance,
            self.reproduction,
            self.remediation,
        ]
    }
}

/// Bounded catalog preview bound to its exact artifact and field descriptor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductFindingPreview {
    text: String,
    truncated: bool,
    binding: Sha256Digest,
}

impl ProductFindingPreview {
    /// Bounded UTF-8 prefix.
    #[must_use]
    pub fn text(&self) -> &str { &self.text }
    /// Whether exact field bytes continue after the prefix.
    #[must_use]
    pub const fn truncated(&self) -> bool { self.truncated }
    /// Binding of the prefix to the body digest and exact field range.
    #[must_use]
    pub const fn binding(&self) -> Sha256Digest { self.binding }
}

/// Compact durable authority for one immutable canonical finding body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductFindingBodyReference {
    finding_id: Sha256Digest,
    identity_version: u8,
    digest: Sha256Digest,
    bytes: u64,
    source_ordinal: u64,
    fields: ProductFindingBodyFields,
    title_preview: ProductFindingPreview,
    provenance_preview: ProductFindingPreview,
}

impl ProductFindingBodyReference {
    /// Restores and validates one durable compact body descriptor.
    ///
    /// # Errors
    /// Rejects an invalid source ordinal, field range, or preview binding.
    #[allow(clippy::too_many_arguments, reason = "durable descriptor fields remain explicit")]
    pub fn restore(
        finding_id: Sha256Digest,
        identity_version: u8,
        digest: Sha256Digest,
        bytes: u64,
        source_ordinal: u64,
        fields: ProductFindingBodyFields,
        title_preview: String,
        title_truncated: bool,
        title_preview_binding: Sha256Digest,
        provenance_preview: String,
        provenance_truncated: bool,
        provenance_preview_binding: Sha256Digest,
    ) -> Result<Self, ProductReviewError> {
        if bytes == 0
            || source_ordinal < PRODUCT_FINDING_SOURCE_ORDINAL_BASE
            || fields.ordered().iter().any(|field| {
                field.end().is_none_or(|end| end > bytes)
            })
        {
            return Err(ProductReviewError::new("finding body descriptor is outside its artifact"));
        }
        let title_preview = restore_preview(
            title_preview,
            title_truncated,
            title_preview_binding,
            digest,
            fields.title,
        )?;
        let provenance_preview = restore_preview(
            provenance_preview,
            provenance_truncated,
            provenance_preview_binding,
            digest,
            fields.provenance,
        )?;
        Ok(Self {
            finding_id,
            identity_version,
            digest,
            bytes,
            source_ordinal,
            fields,
            title_preview,
            provenance_preview,
        })
    }

    /// Finding identity bound by the canonical body header.
    #[must_use]
    pub const fn finding_id(&self) -> Sha256Digest { self.finding_id }
    /// Identity derivation version bound by the canonical body header.
    #[must_use]
    pub const fn identity_version(&self) -> u8 { self.identity_version }
    /// Exact artifact digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest { self.digest }
    /// Exact artifact byte length.
    #[must_use]
    pub const fn bytes(&self) -> u64 { self.bytes }
    /// Stable context-source ordinal in the reserved upper namespace.
    #[must_use]
    pub const fn source_ordinal(&self) -> u64 { self.source_ordinal }
    /// Exact field ranges.
    #[must_use]
    pub const fn fields(&self) -> &ProductFindingBodyFields { &self.fields }
    /// Bounded title preview.
    #[must_use]
    pub const fn title_preview(&self) -> &ProductFindingPreview { &self.title_preview }
    /// Bounded normalized provenance preview.
    #[must_use]
    pub const fn provenance_preview(&self) -> &ProductFindingPreview {
        &self.provenance_preview
    }

    pub(super) fn validate_finding(
        &self,
        finding_id: Sha256Digest,
        identity_version: u8,
    ) -> Result<(), ProductReviewError> {
        if self.finding_id != finding_id || self.identity_version != identity_version {
            return Err(ProductReviewError::new(
                "finding body descriptor is bound to another identity",
            ));
        }
        Ok(())
    }
}

/// Compact durable authority for one exact reviewer summary artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductReviewSummaryReference {
    review_cycle: u32,
    digest: Sha256Digest,
    bytes: u64,
    source_ordinal: u64,
    summary: ProductFindingFieldReference,
    preview: ProductFindingPreview,
}

impl ProductReviewSummaryReference {
    /// Restores and validates one durable compact summary descriptor.
    ///
    /// # Errors
    /// Rejects an invalid review cycle, source ordinal, field range, or preview binding.
    #[allow(clippy::too_many_arguments, reason = "durable descriptor fields remain explicit")]
    pub fn restore(
        review_cycle: u32,
        digest: Sha256Digest,
        bytes: u64,
        source_ordinal: u64,
        summary: ProductFindingFieldReference,
        preview: String,
        preview_truncated: bool,
        preview_binding: Sha256Digest,
    ) -> Result<Self, ProductReviewError> {
        if review_cycle == 0
            || bytes == 0
            || source_ordinal < PRODUCT_FINDING_SOURCE_ORDINAL_BASE
            || summary.bytes() == 0
            || Some(summary.offset()) != summary_field_offset(review_cycle, summary.bytes())
            || summary
                .end()
                .and_then(|end| end.checked_add(1))
                != Some(bytes)
        {
            return Err(ProductReviewError::new(
                "review summary descriptor is outside its artifact",
            ));
        }
        let preview = restore_summary_preview(
            preview,
            preview_truncated,
            preview_binding,
            digest,
            summary,
            review_cycle,
        )?;
        Ok(Self { review_cycle, digest, bytes, source_ordinal, summary, preview })
    }

    /// Computes the canonical descriptor for exact accepted summary text.
    ///
    /// # Errors
    /// Rejects an invalid cycle, empty summary, invalid ordinal, or size overflow.
    pub fn measure(
        review_cycle: u32,
        summary: &str,
        source_ordinal: u64,
    ) -> Result<Self, ProductReviewError> {
        if review_cycle == 0 || summary.trim().is_empty() {
            return Err(ProductReviewError::new("review summary is empty"));
        }
        let mut hasher = Sha256::new();
        let mut writer = CanonicalWriter::new(|chunk| {
            hasher.update(chunk);
            Ok(())
        });
        let field = canonical_summary(review_cycle, summary, &mut writer)?;
        let bytes = writer.position();
        drop(writer);
        let digest = Sha256Digest::new(hasher.finalize().into());
        let (preview, truncated) = preview(summary);
        Self::restore(
            review_cycle,
            digest,
            bytes,
            source_ordinal,
            field,
            preview.clone(),
            truncated,
            summary_preview_binding(digest, field, &preview, truncated, review_cycle),
        )
    }

    /// Streams the exact canonical summary artifact without constructing a second summary.
    ///
    /// # Errors
    /// Rejects an invalid cycle, empty summary, size overflow, or writer failure.
    pub fn stream_canonical(
        review_cycle: u32,
        summary: &str,
        write: &mut dyn FnMut(&[u8]) -> Result<(), ProductReviewError>,
    ) -> Result<(), ProductReviewError> {
        if review_cycle == 0 || summary.trim().is_empty() {
            return Err(ProductReviewError::new("review summary is empty"));
        }
        let mut writer = CanonicalWriter::new(write);
        let _ = canonical_summary(review_cycle, summary, &mut writer)?;
        Ok(())
    }

    /// Review cycle that admitted the exact summary.
    #[must_use]
    pub const fn review_cycle(&self) -> u32 { self.review_cycle }
    /// Exact artifact digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest { self.digest }
    /// Exact artifact byte length.
    #[must_use]
    pub const fn bytes(&self) -> u64 { self.bytes }
    /// Stable context-source ordinal in the reserved upper namespace.
    #[must_use]
    pub const fn source_ordinal(&self) -> u64 { self.source_ordinal }
    /// Exact accepted-summary byte range in the canonical artifact.
    #[must_use]
    pub const fn summary(&self) -> ProductFindingFieldReference { self.summary }
    /// Bounded authenticated summary preview.
    #[must_use]
    pub const fn preview(&self) -> &ProductFindingPreview { &self.preview }

    pub(super) fn validate_cycle(&self, cycle: u32) -> Result<(), ProductReviewError> {
        if self.review_cycle != cycle {
            return Err(ProductReviewError::new(
                "review summary descriptor is bound to another cycle",
            ));
        }
        Ok(())
    }
}

/// Host-owned publication port for immutable finding bodies.
///
/// A successful return means the exact body bytes and their run-stable durable reference were
/// synchronized before the compact descriptor can enter the ledger.
pub trait ProductFindingBodyPublisher: Send + Sync {
    /// Publishes one inline finding body at its already-reserved stable source ordinal.
    fn publish(
        &self,
        finding: &ProductFinding,
        source_ordinal: u64,
    ) -> Result<ProductFindingBodyReference, ProductReviewError>;

    /// Publishes one exact reviewer summary at its already-reserved stable source ordinal.
    fn publish_summary(
        &self,
        _summary: &str,
        _review_cycle: u32,
        _source_ordinal: u64,
    ) -> Result<ProductReviewSummaryReference, ProductReviewError> {
        Err(ProductReviewError::new(
            "review summary publication is unavailable",
        ))
    }
}

pub(super) fn measure_body(
    finding_id: Sha256Digest,
    identity_version: u8,
    category: ProductFindingCategory,
    severity: FindingSeverity,
    body: &ProductFindingBody,
    source_ordinal: u64,
) -> Result<ProductFindingBodyReference, ProductReviewError> {
    let mut hasher = Sha256::new();
    let mut writer = CanonicalWriter::new(|chunk| {
        hasher.update(chunk);
        Ok(())
    });
    let fields = canonical_body(
        finding_id,
        identity_version,
        category,
        severity,
        body,
        &mut writer,
    )?;
    let bytes = writer.position();
    drop(writer);
    let digest = Sha256Digest::new(hasher.finalize().into());
    let (title_preview, title_truncated) = preview(body.title());
    let (provenance_preview, provenance_truncated) = preview(body.provenance());
    ProductFindingBodyReference::restore(
        finding_id,
        identity_version,
        digest,
        bytes,
        source_ordinal,
        fields,
        title_preview.clone(),
        title_truncated,
        preview_binding(digest, fields.title, &title_preview, title_truncated),
        provenance_preview.clone(),
        provenance_truncated,
        preview_binding(
            digest,
            fields.provenance,
            &provenance_preview,
            provenance_truncated,
        ),
    )
}

pub(super) fn stream_body(
    finding_id: Sha256Digest,
    identity_version: u8,
    category: ProductFindingCategory,
    severity: FindingSeverity,
    body: &ProductFindingBody,
    write: &mut dyn FnMut(&[u8]) -> Result<(), ProductReviewError>,
) -> Result<(), ProductReviewError> {
    let mut writer = CanonicalWriter::new(write);
    let _ = canonical_body(
        finding_id,
        identity_version,
        category,
        severity,
        body,
        &mut writer,
    )?;
    Ok(())
}

fn canonical_body<F>(
    finding_id: Sha256Digest,
    identity_version: u8,
    category: ProductFindingCategory,
    severity: FindingSeverity,
    body: &ProductFindingBody,
    writer: &mut CanonicalWriter<F>,
) -> Result<ProductFindingBodyFields, ProductReviewError>
where
    F: FnMut(&[u8]) -> Result<(), ProductReviewError>,
{
    writer.write(BODY_HEADER)?;
    writer.write(format!("identity:{}\n", hex_digest(finding_id)).as_bytes())?;
    writer.write(format!("identity-version:{identity_version}\n").as_bytes())?;
    writer.write(format!("category:{}\n", category.as_str()).as_bytes())?;
    writer.write(format!("severity:{}\n", severity_text(severity)).as_bytes())?;
    let title = writer.field("title", body.title())?;
    let description = writer.field("description", body.description())?;
    let location = writer.field("location", body.location())?;
    let provenance = writer.field("provenance", body.provenance())?;
    let reproduction = writer.field("reproduction", body.reproduction())?;
    let remediation = writer.field("remediation", body.remediation())?;
    ProductFindingBodyFields::restore(
        title,
        description,
        location,
        provenance,
        reproduction,
        remediation,
    )
}

fn canonical_summary<F>(
    review_cycle: u32,
    summary: &str,
    writer: &mut CanonicalWriter<F>,
) -> Result<ProductFindingFieldReference, ProductReviewError>
where
    F: FnMut(&[u8]) -> Result<(), ProductReviewError>,
{
    writer.write(SUMMARY_HEADER)?;
    writer.write(format!("review-cycle:{review_cycle}\n").as_bytes())?;
    writer.field("summary", summary)
}

fn summary_field_offset(review_cycle: u32, summary_bytes: u64) -> Option<u64> {
    let fields = format!(
        "review-cycle:{review_cycle}\nsummary-bytes:{summary_bytes}\nsummary:\n"
    );
    u64::try_from(SUMMARY_HEADER.len())
        .ok()?
        .checked_add(u64::try_from(fields.len()).ok()?)
}

struct CanonicalWriter<F> {
    write: F,
    position: u64,
}

impl<F> CanonicalWriter<F>
where
    F: FnMut(&[u8]) -> Result<(), ProductReviewError>,
{
    const fn new(write: F) -> Self { Self { write, position: 0 } }

    const fn position(&self) -> u64 { self.position }

    fn write(&mut self, bytes: &[u8]) -> Result<(), ProductReviewError> {
        let count = u64::try_from(bytes.len()).map_err(|_| {
            ProductReviewError::new("finding body length exceeds its representation")
        })?;
        let next = self.position.checked_add(count).ok_or_else(|| {
            ProductReviewError::new("finding body length exceeds its representation")
        })?;
        (self.write)(bytes)?;
        self.position = next;
        Ok(())
    }

    fn field(
        &mut self,
        name: &'static str,
        value: &str,
    ) -> Result<ProductFindingFieldReference, ProductReviewError> {
        self.write(format!("{name}-bytes:{}\n{name}:\n", value.len()).as_bytes())?;
        let offset = self.position;
        self.write(value.as_bytes())?;
        let bytes = u64::try_from(value.len()).map_err(|_| {
            ProductReviewError::new("finding body length exceeds its representation")
        })?;
        self.write(b"\n")?;
        Ok(ProductFindingFieldReference::new(offset, bytes, sha256(value.as_bytes())))
    }
}

fn restore_preview(
    text: String,
    truncated: bool,
    binding: Sha256Digest,
    body_digest: Sha256Digest,
    field: ProductFindingFieldReference,
) -> Result<ProductFindingPreview, ProductReviewError> {
    if text.len() > PREVIEW_BYTES
        || (!truncated && u64::try_from(text.len()).ok() != Some(field.bytes()))
        || (truncated && u64::try_from(text.len()).ok().is_none_or(|len| len >= field.bytes()))
        || preview_binding(body_digest, field, &text, truncated) != binding
    {
        return Err(ProductReviewError::new("finding body preview binding is invalid"));
    }
    Ok(ProductFindingPreview { text, truncated, binding })
}

fn restore_summary_preview(
    text: String,
    truncated: bool,
    binding: Sha256Digest,
    body_digest: Sha256Digest,
    field: ProductFindingFieldReference,
    review_cycle: u32,
) -> Result<ProductFindingPreview, ProductReviewError> {
    if text.len() > PREVIEW_BYTES
        || (!truncated && u64::try_from(text.len()).ok() != Some(field.bytes()))
        || (truncated && u64::try_from(text.len()).ok().is_none_or(|len| len >= field.bytes()))
        || (!truncated && sha256(text.as_bytes()) != field.digest())
        || summary_preview_binding(body_digest, field, &text, truncated, review_cycle) != binding
    {
        return Err(ProductReviewError::new("review summary preview binding is invalid"));
    }
    Ok(ProductFindingPreview { text, truncated, binding })
}

fn preview(value: &str) -> (String, bool) {
    if value.len() <= PREVIEW_BYTES {
        return (value.to_owned(), false);
    }
    let end = value.floor_char_boundary(PREVIEW_BYTES);
    (value[..end].to_owned(), true)
}

fn preview_binding(
    body_digest: Sha256Digest,
    field: ProductFindingFieldReference,
    text: &str,
    truncated: bool,
) -> Sha256Digest {
    let mut bytes = b"peritus.product-finding-preview.v1\0".to_vec();
    bytes.extend_from_slice(body_digest.as_bytes());
    bytes.extend_from_slice(&field.offset().to_le_bytes());
    bytes.extend_from_slice(&field.bytes().to_le_bytes());
    bytes.extend_from_slice(field.digest().as_bytes());
    bytes.push(u8::from(truncated));
    bytes.extend_from_slice(text.as_bytes());
    sha256(&bytes)
}

fn summary_preview_binding(
    body_digest: Sha256Digest,
    field: ProductFindingFieldReference,
    text: &str,
    truncated: bool,
    review_cycle: u32,
) -> Sha256Digest {
    let mut bytes = b"peritus.product-review-summary-preview.v1\0".to_vec();
    bytes.extend_from_slice(&review_cycle.to_le_bytes());
    bytes.extend_from_slice(body_digest.as_bytes());
    bytes.extend_from_slice(&field.offset().to_le_bytes());
    bytes.extend_from_slice(&field.bytes().to_le_bytes());
    bytes.extend_from_slice(field.digest().as_bytes());
    bytes.push(u8::from(truncated));
    bytes.extend_from_slice(text.as_bytes());
    sha256(&bytes)
}

fn hex_digest(value: Sha256Digest) -> String {
    let mut output = String::with_capacity(Sha256Digest::LENGTH.saturating_mul(2));
    for byte in value.as_bytes() {
        use core::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

const fn severity_text(value: FindingSeverity) -> &'static str {
    match value {
        FindingSeverity::Advisory => "advisory",
        FindingSeverity::Low => "low",
        FindingSeverity::Medium => "medium",
        FindingSeverity::High => "high",
        FindingSeverity::Critical => "critical",
    }
}
