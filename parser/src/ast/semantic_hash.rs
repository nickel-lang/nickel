//! # Cross-evaluation Unique Identifier
//!
//! The CUI is a semantic hash of expressions that makes it possible for the incremental evaluator
//! to identify and match expressions that haven't changed since the last evaluation, so that their
//! result can be re-used.
use std::num::NonZeroU128;

/// A 128-bit semantic hash for incremental evaluation.
///
/// We reserve the zero value for the niche `Option<_>` optimization. If a hash of zero is
/// encountered, [Self::from_hash] will pick a fixed constant instead.
#[derive(Clone, Copy, Eq, PartialEq, Hash, Debug)]
pub struct SemanticHash(NonZeroU128);

impl SemanticHash {
    const ZERO_HASH: NonZeroU128 = NonZeroU128::new(184_505_291).unwrap();

    /// Wraps a u128 hash to [SemanticHash]. If the argument is zero, another constant is picked for
    /// the hash instead.
    pub fn from_hash(hash: u128) -> Self {
        Self(NonZeroU128::new(hash).unwrap_or(Self::ZERO_HASH))
    }
}
