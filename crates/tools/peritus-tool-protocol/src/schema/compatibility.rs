//! Same-major schema compatibility decisions.

use super::{Schema, SchemaCompatibility, SchemaKind};

pub(super) fn classify(current: &Schema, successor: &Schema) -> SchemaCompatibility {
    if current == successor {
        return SchemaCompatibility::Equal;
    }
    if additive(current, successor) {
        SchemaCompatibility::Additive
    } else {
        SchemaCompatibility::Breaking
    }
}

fn additive(current: &Schema, successor: &Schema) -> bool {
    if current.enum_values != successor.enum_values {
        return false;
    }
    match (&current.kind, &successor.kind) {
        (
            SchemaKind::Object { properties: before, additional_properties: before_extra },
            SchemaKind::Object { properties: after, additional_properties: after_extra },
        ) if before_extra == after_extra => {
            if before.len() >= after.len() {
                return false;
            }
            before.iter().all(|property| {
                after
                    .binary_search_by(|candidate| candidate.name.cmp(&property.name))
                    .ok()
                    .is_some_and(|index| after[index] == *property)
            }) && after
                .iter()
                .filter(|property| {
                    before.binary_search_by(|candidate| candidate.name.cmp(&property.name)).is_err()
                })
                .all(|property| !property.required)
        }
        _ => false,
    }
}
