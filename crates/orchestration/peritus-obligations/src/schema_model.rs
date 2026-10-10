//! Exact logical views of directional schema requirements and observations.

use super::{SchemaDirection, SchemaEvidence, SchemaField, SchemaRequirement};

impl SchemaDirection {
    #[doc = " Exact executable equality for schema directions."]
    pub(super) const fn same(self, other: Self) -> bool {
        matches!((self, other), (Self::Request, Self::Request) | (Self::Response, Self::Response))
    }
}
impl SchemaField {}
impl SchemaRequirement {}
impl SchemaEvidence {}
