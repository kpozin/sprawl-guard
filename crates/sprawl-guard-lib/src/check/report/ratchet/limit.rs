//! Current limits carried by measured ratchet diagnostics.

use crate::DirectFileLimit;
use crate::FiniteOrUnlimited;
use crate::ratchet::RatchetCount;

/// Current finite or unlimited limit carried by a measured ratchet diagnostic.
pub type RatchetLimit = FiniteOrUnlimited<RatchetCount>;

impl From<DirectFileLimit> for RatchetLimit {
    fn from(value: DirectFileLimit) -> Self {
        match value {
            DirectFileLimit::Finite(value) => Self::Finite(RatchetCount::from_direct_files(value)),
            DirectFileLimit::Unlimited => Self::Unlimited,
        }
    }
}
