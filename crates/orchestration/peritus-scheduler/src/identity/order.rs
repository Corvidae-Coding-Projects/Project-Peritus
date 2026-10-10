//! Exact identity equality and strict canonical byte-order laws.

use super::{DispatchId, WorkId, WorkerId};

impl WorkId {
    #[doc = " Compares exact canonical work identities for verified lookup."]
    pub(crate) const fn same(&self, other: &Self) -> bool {
        let mut index = 0;
        while index < 16 {
            if self.0[index] != other.0[index] {
                return false;
            }
            index += 1;
        }
        true
    }
}
impl WorkerId {
    #[doc = " Compares exact canonical worker identities for verified lookup."]
    pub(crate) const fn same(&self, other: &Self) -> bool {
        let mut index = 0;
        while index < 16 {
            if self.0[index] != other.0[index] {
                return false;
            }
            index += 1;
        }
        true
    }
}
impl DispatchId {
    #[doc = " Compares exact dispatch identities for verified history membership."]
    pub(crate) const fn same(&self, other: &Self) -> bool {
        let mut index = 0;
        while index < 16 {
            if self.0[index] != other.0[index] {
                return false;
            }
            index += 1;
        }
        true
    }
}
