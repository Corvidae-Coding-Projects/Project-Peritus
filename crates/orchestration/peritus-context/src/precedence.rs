//! Deterministic comparison rules shared by selection and rendering.

#![allow(
    clippy::redundant_pub_crate,
    reason = "sibling selection module consumes these private-module helpers"
)]

use crate::ContextNode;
use vstd::prelude::*;

verus! {

pub(super) fn optional_precedes(left: &ContextNode, right: &ContextNode) -> bool {
    if left.authority().precedence() != right.authority().precedence() {
        left.authority().precedence() > right.authority().precedence()
    } else if left.requirement().precedence() != right.requirement().precedence() {
        left.requirement().precedence() > right.requirement().precedence()
    } else if left.priority() != right.priority() {
        left.priority() > right.priority()
    } else if left.recency_sequence() != right.recency_sequence() {
        left.recency_sequence() > right.recency_sequence()
    } else {
        left.id() < right.id()
    }
}

pub(super) fn render_precedes(left: &ContextNode, right: &ContextNode) -> bool {
    if left.authority().precedence() != right.authority().precedence() {
        left.authority().precedence() > right.authority().precedence()
    } else if left.context_class() != right.context_class() {
        left.context_class() < right.context_class()
    } else if left.provenance().precedence() != right.provenance().precedence() {
        left.provenance().precedence() > right.provenance().precedence()
    } else {
        left.id() < right.id()
    }
}

} // verus!
