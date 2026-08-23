use crate::error::Diagnostic;
use crate::model::RefinementReservation;
use std::collections::BTreeSet;

pub(super) fn validate_refinement_reservations(
    reservations: &[RefinementReservation],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut ids = BTreeSet::new();

    for reservation in reservations {
        if !ids.insert(reservation.id.as_str()) {
            diagnostics.push(Diagnostic::at(
                "architecture.toml",
                format!("refinement `{}` is reserved more than once", reservation.id),
                "retain one canonical reservation for each downstream refinement",
            ));
        }

        if !is_slice_id(&reservation.introduced_by) {
            diagnostics.push(Diagnostic::at(
                "architecture.toml",
                format!(
                    "refinement `{}` has invalid introducing slice `{}`",
                    reservation.id, reservation.introduced_by
                ),
                "use an uppercase slice identifier such as B1",
            ));
        }
        if !is_slice_id(&reservation.future_owner) {
            diagnostics.push(Diagnostic::at(
                "architecture.toml",
                format!(
                    "refinement `{}` has invalid future owner `{}`",
                    reservation.id, reservation.future_owner
                ),
                "use an uppercase slice identifier such as C0",
            ));
        }

        let prefix = format!("REF-{}-{}-", reservation.future_owner, reservation.introduced_by);
        let suffix = reservation.id.strip_prefix(&prefix);
        if suffix.is_none_or(|suffix| !is_refinement_suffix(suffix)) {
            diagnostics.push(Diagnostic::at(
                "architecture.toml",
                format!(
                    "refinement `{}` does not match its owner and introducing slice",
                    reservation.id
                ),
                format!("use `{prefix}<UPPERCASE-NAME>` with non-empty name segments"),
            ));
        }

        let statement = reservation.statement.trim();
        if statement.len() < 24 || statement != reservation.statement {
            diagnostics.push(Diagnostic::at(
                "architecture.toml",
                format!("refinement `{}` has no substantive canonical statement", reservation.id),
                "record a trimmed contract statement of at least 24 characters",
            ));
        }
    }
}

fn is_slice_id(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes.next().is_some_and(|byte| byte.is_ascii_uppercase())
        && bytes.clone().next().is_some()
        && bytes.all(|byte| byte.is_ascii_digit())
}

fn is_refinement_suffix(value: &str) -> bool {
    value.split('-').all(|segment| {
        !segment.is_empty()
            && segment.bytes().all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
    })
}

#[cfg(test)]
mod tests {
    use super::validate_refinement_reservations;
    use crate::model::{ArchitecturePolicy, RefinementReservation};

    fn reservation(id: &str, owner: &str, introduced_by: &str) -> RefinementReservation {
        RefinementReservation {
            id: id.to_owned(),
            introduced_by: introduced_by.to_owned(),
            future_owner: owner.to_owned(),
            statement: "The downstream slice preserves this exact integration contract.".to_owned(),
        }
    }

    fn diagnostics(reservations: &[RefinementReservation]) -> Vec<crate::error::Diagnostic> {
        let mut diagnostics = Vec::new();
        validate_refinement_reservations(reservations, &mut diagnostics);
        diagnostics
    }

    #[test]
    fn accepts_a_well_formed_reservation() {
        assert!(diagnostics(&[reservation("REF-C0-B1-COMMIT-ONCE", "C0", "B1")]).is_empty());
    }

    #[test]
    fn rejects_duplicates_and_identity_mismatches() {
        let reservations = [
            reservation("REF-C0-B1-COMMIT-ONCE", "C1", "B1"),
            reservation("REF-C0-B1-COMMIT-ONCE", "C1", "B1"),
        ];
        let diagnostics = diagnostics(&reservations);
        assert!(diagnostics.iter().any(|item| item.message().contains("reserved more than once")));
        assert!(diagnostics.iter().any(|item| item.message().contains("does not match its owner")));
    }

    #[test]
    fn rejects_malformed_slice_ids_and_empty_name_segments() {
        let diagnostics = diagnostics(&[reservation("REF-c0-B1-COMMIT--ONCE", "c0", "B1")]);
        assert!(diagnostics.iter().any(|item| item.message().contains("invalid future owner")));
        assert!(diagnostics.iter().any(|item| item.message().contains("does not match its owner")));
    }

    #[test]
    fn rejects_missing_contract_text() {
        let mut item = reservation("REF-C0-B1-COMMIT-ONCE", "C0", "B1");
        item.statement = "TBD".to_owned();
        assert!(
            diagnostics(&[item])
                .iter()
                .any(|diagnostic| diagnostic.message().contains("substantive canonical statement"))
        );
    }

    #[test]
    fn canonical_policy_reserves_every_declared_downstream_contract() {
        let policy: ArchitecturePolicy = toml::from_str(include_str!("../../../architecture.toml"))
            .expect("canonical architecture policy must parse");
        let actual: Vec<_> =
            policy.refinement_reservations.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(
            actual,
            [
                "REF-C2-B1-HOLDER-QUIESCENCE",
                "REF-C4-B1-OPERATION-CLASS",
                "REF-B0-B1-CURRENT-STATE-WITNESS",
                "REF-E0-B1-COMMIT-BEFORE-EFFECT",
                "REF-C2-B1-AUTHORITY-GATE",
                "REF-C4-B1-AUTHORITY-GATE",
                "REF-E0-B1-POLICY-ACTIVATION",
                "REF-B2-B1-CONTRACT-REVISION",
                "REF-B0-B2-CURRENT-EVIDENCE",
                "REF-D1-B2-GATE-OBSERVATION",
                "REF-D2-B2-REVIEW-OBSERVATION",
                "REF-B0-B1-REVISION-FRESHNESS",
                "REF-B0-B1-BUDGET-CEILING",
                "REF-G0-B1-STARTUP-FENCING",
                "REF-E0-B0-CURRENT-LIFECYCLE",
            ]
        );
    }
}
