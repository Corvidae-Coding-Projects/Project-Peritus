//! Exact immutable duplication of the policy-bound operation registry.

use crate::{OperationDescriptor, OperationRegistry, RiskSet};

impl RiskSet {
    fn duplicate(&self) -> Self {
        let mut values = Vec::new();
        let mut index = 0;
        while index < self.values.len() {
            values.push(self.values[index]);
            index += 1;
        }
        Self::from_derived_values(values)
    }
}
impl OperationDescriptor {
    fn duplicate(&self) -> Self {
        let risks = self.risks.duplicate();
        let duplicate =
            Self { name: self.name.clone(), operation_class: self.operation_class, risks };
        duplicate
    }
}
impl OperationRegistry {
    pub(crate) fn duplicate(&self) -> Self {
        let mut descriptors: Vec<OperationDescriptor> = Vec::new();
        let mut index = 0;
        while index < self.descriptors.len() {
            descriptors.push(self.descriptors[index].duplicate());
            index += 1;
        }
        let duplicate = Self { descriptors };
        duplicate
    }
}
