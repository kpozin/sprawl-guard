//! Current limits carried by measured ratchet diagnostics.

use crate::FiniteOrUnlimited;
use crate::LeafFileLimit;
use crate::ratchet::RatchetCount;

/// Current finite or unlimited limit carried by a measured ratchet diagnostic.
pub type RatchetLimit = FiniteOrUnlimited<RatchetCount>;

impl From<LeafFileLimit> for RatchetLimit {
    fn from(value: LeafFileLimit) -> Self {
        match value {
            LeafFileLimit::Finite(value) => Self::Finite(RatchetCount::from_leaf_files(value)),
            LeafFileLimit::Unlimited => Self::Unlimited,
        }
    }
}
