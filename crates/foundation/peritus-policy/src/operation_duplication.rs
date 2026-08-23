//! Exact immutable duplication of the policy-bound operation registry.

use crate::{OperationDescriptor, OperationRegistry, RiskSet};
use vstd::prelude::*;

verus! {

impl RiskSet {
    fn duplicate(&self) -> (duplicate: Self)
        ensures duplicate.spec_values() == self.spec_values(),
    {
        proof {
            use_type_invariant(self);
            self.expose_values();
        }
        let mut values = Vec::new();
        let mut index = 0;
        while index < self.values.len()
            invariant
                0 <= index <= self.values.len(),
                values@ == self.values@.subrange(0, index as int),
            decreases self.values.len() - index,
        {
            values.push(self.values[index]);
            index += 1;
        }
        proof {
            assert(values@ == self.spec_values());
            assert(RiskSet::spec_values_are_valid(values@));
        }
        Self::from_derived_values(values)
    }
}

impl OperationDescriptor {
    /// Returns whether two descriptors bind the exact same operation and risk classification.
    pub closed spec fn spec_same_as(&self, original: &Self) -> bool {
        self.name.spec_bytes() == original.name.spec_bytes()
            && self.operation_class == original.operation_class
            && self.risks.spec_values() == original.risks.spec_values()
    }

    fn duplicate(&self) -> (duplicate: Self)
        ensures duplicate.spec_same_as(self),
    {
        proof {
            use_type_invariant(self);
        }
        let risks = self.risks.duplicate();
        proof {
            risks.same_values_preserve_membership(
                &self.risks,
                self.operation_class.spec_mandatory_risk(),
            );
            assert(risks.spec_contains(self.operation_class.spec_mandatory_risk()));
        }
        let duplicate = Self {
            name: self.name.clone(),
            operation_class: self.operation_class,
            risks,
        };
        reveal(OperationDescriptor::spec_same_as);
        duplicate
    }
}

impl OperationRegistry {
    pub(crate) open spec fn spec_duplication_descriptors(&self) -> Seq<OperationDescriptor> {
        self.descriptors@
    }

    /// Returns whether two registries contain the exact same authenticated descriptors.
    pub closed spec fn spec_same_as(&self, original: &Self) -> bool {
        self.spec_duplication_descriptors().len()
                == original.spec_duplication_descriptors().len()
            && forall |index: int| 0 <= index < self.spec_duplication_descriptors().len() ==>
                #[trigger] self.spec_duplication_descriptors()[index].spec_same_as(
                    &original.spec_duplication_descriptors()[index],
                )
    }

    pub(crate) fn duplicate(&self) -> (duplicate: Self)
        ensures duplicate.spec_same_as(self),
    {
        let mut descriptors: Vec<OperationDescriptor> = Vec::new();
        let mut index = 0;
        while index < self.descriptors.len()
            invariant
                0 <= index <= self.descriptors.len(),
                descriptors@.len() == index,
                forall |prior: int| 0 <= prior < index ==>
                    #[trigger] descriptors@[prior].spec_same_as(
                        &self.descriptors@[prior],
                    ),
            decreases self.descriptors.len() - index,
        {
            descriptors.push(self.descriptors[index].duplicate());
            index += 1;
        }
        assert(descriptors@.len() == self.descriptors@.len());
        assert(forall |prior: int| 0 <= prior < descriptors@.len() ==>
            #[trigger] descriptors@[prior].spec_same_as(&self.descriptors@[prior]));
        let duplicate = Self { descriptors };
        reveal(OperationRegistry::spec_same_as);
        duplicate
    }
}

} // verus!
