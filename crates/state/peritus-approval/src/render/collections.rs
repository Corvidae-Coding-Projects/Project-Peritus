//! Whole-item rendering and exact omission accounting for bounded collections.

use super::builder::Builder;
use super::encoding::hex;
use super::{MAX_RENDERED_PARTICIPANTS, MAX_RENDERED_PERMISSIONS};

#[derive(Clone, Copy)]
pub(super) struct CollectionCounts {
    pub(super) permission_total: usize,
    pub(super) permission_omitted: usize,
    pub(super) producing_total: usize,
    pub(super) producing_omitted: usize,
    pub(super) review_total: usize,
    pub(super) review_omitted: usize,
}
fn participants(
    builder: &mut Builder,
    field_name: &str,
    values: &[peritus_types::ActorId],
) -> Result<usize, crate::ApprovalError> {
    let attempted = values.len().min(MAX_RENDERED_PARTICIPANTS);
    let mut omitted = values.len() - attempted;
    let mut index = 0;
    while index < attempted {
        if !builder.field(field_name, &hex(values[index].as_bytes()))? {
            omitted += 1;
        }
        index += 1;
    }
    Ok(omitted)
}
fn permissions(
    builder: &mut Builder,
    values: &[peritus_policy::Permission],
) -> Result<usize, crate::ApprovalError> {
    let attempted = values.len().min(MAX_RENDERED_PERMISSIONS);
    let mut omitted = values.len() - attempted;
    let mut index = 0;
    while index < attempted {
        let mut value = hex(values[index].resource_id().as_bytes());
        value.push(':');
        value.push_str(values[index].capability_name().as_str());
        if !builder.field("permission", &value)? {
            omitted += 1;
        }
        index += 1;
    }
    Ok(omitted)
}
pub(super) fn render(
    builder: &mut Builder,
    request: &crate::ApprovalRequest,
) -> Result<CollectionCounts, crate::ApprovalError> {
    let producing = request.producing_participants().as_slice();
    let review = request.review_participants().as_slice();
    let permission_values = request.scope().permissions().as_slice();
    let producing_omitted = participants(builder, "producing-participant", producing)?;
    let review_omitted = participants(builder, "review-participant", review)?;
    let permission_omitted = permissions(builder, permission_values)?;
    if producing_omitted > 0 || review_omitted > 0 || permission_omitted > 0 {
        builder.truncated = true;
    }
    Ok(CollectionCounts {
        permission_total: permission_values.len(),
        permission_omitted,
        producing_total: producing.len(),
        producing_omitted,
        review_total: review.len(),
        review_omitted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use peritus_policy::{Permission, PermissionSet};
    use peritus_types::{ActorId, CapabilityName, ResourceId};
    use std::fmt::Write;

    fn actor(value: u8) -> ActorId {
        ActorId::new([value; 16]).expect("repeated bytes form an actor id")
    }

    fn permission(value: u8, capability: &str) -> Permission {
        Permission::new(
            ResourceId::new([value; 16]).expect("repeated bytes form a resource id"),
            CapabilityName::new(capability.to_owned()).expect("test capability is valid"),
        )
    }

    fn reference_hex(bytes: &[u8]) -> String {
        let mut output = String::new();
        for byte in bytes {
            write!(&mut output, "{byte:02x}").expect("writing to a string cannot fail");
        }
        output
    }

    #[test]
    fn participant_rendering_caps_complete_fields_and_counts_every_omission() {
        let values: Vec<_> = (1..=65).map(actor).collect();
        let mut builder = Builder::new();
        let omitted = participants(&mut builder, "producing-participant", &values)
            .expect("canonical actors render safely");

        assert_eq!(omitted, 1);
        let mut expected = String::new();
        for value in &values[..MAX_RENDERED_PARTICIPANTS] {
            write!(&mut expected, "producing-participant={};", reference_hex(value.as_bytes()))
                .expect("writing to a string cannot fail");
        }
        assert_eq!(builder.text, expected);
        assert!(!builder.truncated);
    }

    #[test]
    fn permission_rendering_can_skip_a_long_item_then_admit_a_shorter_item() {
        let values = vec![permission(1, &"a".repeat(128)), permission(2, "a")];
        let available = 64;
        let capacity = super::super::MAX_RENDERED_APPROVAL_BYTES
            - super::super::RENDER_TRUNCATION_SUFFIX_BYTES;
        let prefix = "x".repeat(capacity - available);
        let mut builder = Builder { text: prefix.clone(), truncated: false };
        let omitted = permissions(&mut builder, &values).expect("typed permissions render safely");

        assert_eq!(omitted, 1);
        assert!(builder.truncated);
        let short_value = reference_hex(values[1].resource_id().as_bytes());
        assert_eq!(builder.text, format!("{prefix}permission={short_value}:a;"));
    }

    #[test]
    fn permission_rendering_caps_at_sixty_four_whole_items() {
        let values: Vec<_> = (1..=65).map(|value| permission(value, "read")).collect();
        PermissionSet::new(
            values
                .iter()
                .map(|value| permission(value.resource_id().as_bytes()[0], "read"))
                .collect(),
        )
        .expect("fixture order is canonical");
        let mut builder = Builder::new();
        let omitted = permissions(&mut builder, &values).expect("typed permissions render safely");

        assert_eq!(omitted, 1);
        assert_eq!(builder.text.matches("permission=").count(), MAX_RENDERED_PERMISSIONS);
        assert!(!builder.truncated);
    }
}
