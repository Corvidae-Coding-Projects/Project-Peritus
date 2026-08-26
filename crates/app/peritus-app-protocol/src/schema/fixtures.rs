//! Deterministic classified A3 compatibility frames.

use core::mem::size_of;

use crate::{APP_SCHEMA_V1, AppErrorCode};
use peritus_codec::{CodecError, CodecLimits, encode_frame};

mod values;

use values::generated_valid_cases;

/// A2 compatibility-fixture classification.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FixtureClass {
    /// Smallest representative valid value.
    Minimal,
    /// Representative multi-field production exchange.
    Realistic,
    /// Structurally damaged bytes.
    Corrupt,
    /// Well-framed input exercising a closed rejection boundary.
    Adversarial,
}

impl FixtureClass {
    /// Returns the exact A2 manifest spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Realistic => "realistic",
            Self::Corrupt => "corrupt",
            Self::Adversarial => "adversarial",
        }
    }
}

/// One complete generated compatibility case.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedFixtureCase {
    /// Validated A2 case path component.
    pub case: &'static str,
    /// Required fixture class.
    pub class: FixtureClass,
    /// Complete frame bytes.
    pub payload: Vec<u8>,
    /// Expected outer family when a full header exists.
    pub expected_family: Option<u16>,
    /// Whether the production A3 decoder accepts the bytes.
    pub accepted: bool,
    /// Stable rejection code for an invalid fixture.
    pub expected_error: Option<AppErrorCode>,
}

impl GeneratedFixtureCase {
    /// Renders protocol-specific expectations kept outside the strict A2 manifest envelope.
    #[must_use]
    pub fn render_expectation(&self) -> String {
        let mut output = format!("schema = 1\naccepted = {}\n", self.accepted);
        if let Some(family) = self.expected_family {
            output.push_str("expected_family = ");
            output.push_str(&family.to_string());
            output.push('\n');
        }
        if let Some(error) = self.expected_error {
            output.push_str("expected_error = \"");
            output.push_str(error.as_str());
            output.push_str("\"\n");
        }
        output
    }
}

/// Builds the stable minimal, realistic, corrupt, and adversarial version-one corpus.
///
/// # Errors
///
/// Returns a codec error only if a fixed source fixture exceeds production frame limits.
///
/// # Panics
///
/// Panics only if a source-code fixture constant violates a checked application or A1 invariant.
pub fn generated_fixture_cases() -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    let limits = CodecLimits::PRODUCTION;
    let mut cases = generated_valid_cases(limits)?;
    let mut unknown_request = cases
        .iter()
        .find(|case| case.case == "minimal-daemon-status-request")
        .expect("generated minimal request fixture")
        .payload
        .clone();
    let tag_offset = unknown_request.len() - size_of::<u16>();
    unknown_request[tag_offset..].copy_from_slice(&u16::MAX.to_be_bytes());
    cases.extend([
        GeneratedFixtureCase {
            case: "corrupt-truncated-header",
            class: FixtureClass::Corrupt,
            payload: b"PRTS\x01".to_vec(),
            expected_family: None,
            accepted: false,
            expected_error: Some(AppErrorCode::TruncatedFrame),
        },
        rejected(
            "adversarial-foreign-family",
            FixtureClass::Adversarial,
            93,
            encode_frame(93, APP_SCHEMA_V1, &[], limits)?,
            AppErrorCode::UnsupportedFamily,
        ),
        rejected(
            "adversarial-future-schema",
            FixtureClass::Adversarial,
            94,
            encode_frame(94, APP_SCHEMA_V1 + 1, &[0, 1], limits)?,
            AppErrorCode::UnsupportedSchema,
        ),
        rejected(
            "corrupt-unknown-request-tag",
            FixtureClass::Corrupt,
            96,
            unknown_request,
            AppErrorCode::UnknownTag,
        ),
    ]);
    cases.sort_by_key(|case| case.case);
    Ok(cases)
}

const fn rejected(
    case: &'static str,
    class: FixtureClass,
    family: u16,
    payload: Vec<u8>,
    error: AppErrorCode,
) -> GeneratedFixtureCase {
    GeneratedFixtureCase {
        case,
        class,
        payload,
        expected_family: Some(family),
        accepted: false,
        expected_error: Some(error),
    }
}
