use super::{MANIFEST, ManifestContext, ProofImpactDocument, authorization, validate_envelope};
use crate::error::{Diagnostic, XtaskError};

pub(in crate::trust) fn is_authorization_phase(
    context: &ManifestContext<'_>,
    document: &ProofImpactDocument,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<bool, XtaskError> {
    authorization::is_phase(context, document, diagnostics)
}

pub(in crate::trust) fn validate_authorization(
    context: &ManifestContext<'_>,
    document: &ProofImpactDocument,
    enforce_review_base: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), XtaskError> {
    validate_envelope(document, diagnostics);
    if !authorization::validate(context, document, enforce_review_base, diagnostics)? {
        diagnostics.push(Diagnostic::at(
            MANIFEST,
            "proof-impact authorization classification changed during validation",
            "restore one immutable authorization record and its exact referenced verdict and base",
        ));
    }
    Ok(())
}
