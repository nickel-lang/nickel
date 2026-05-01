//! # Cross-evaluation Unique Identifier.
//!
//! The CUI is a semantic hash of expressions that makes it possible for the incremental evaluator
//! to identify and match expressions that haven't changed since the last evaluation, so that their
//! result can be re-used.
//!
//! # CUI of closures and open expressions
//!
//! We compute the CUI of an actual closure by combining the CUI of its core expression with the
//! CUI of its dependencies, hashing everything together. For example, if `CUI(x + 1) = A`, `x` is
//! bound to `0` in the environment, and `CUI(0) = B`, the CUI of the closure `{x + 1 | x <- 0 }`
//! is something akin to `hash((A,B))`.
//!
//! # CUI schemes
//!
//! There are a lot of possible CUI schemes. By scheme, we mean a specific implementation as a
//! function from expressions to CUI/hashes. The fundamental constraint we require a secheme is is
//! that if `CUI(e1) == CUI(e2)`, then `e1` and `e2` are beta-equivalent (to simplify, they either
//! both loop or evaluate to the same value: for example `1+1` and `2` are beta-equivalent).
//! Otherwise the incremental evaluator could change the result of a program by replacing `e1` with
//! a different, non-equivalent `e2` that happens to have the same CUI.
//!
//! Possible schemes are for example:
//!
//! - hashing the source expression
//! - hashing the AST
//! - hashing the AST modulo some rules or some normalization (for example hashing modulo alpha-conversion)
//! - unique and deterministic index based on hash-consing
//! - etc.
//!
//! The main trade-off for the scheme selection is between overhead and generality. A more general
//! CUI equalize more terms, or put differently, is invariant by more semantic-preserving
//! transformations. The more general, the better: the interpreter can identify more equivalent
//! expressions to reuse. However, it usually also means that the CUI is more expensive to compute,
//! which can nullify the benefits or heavily penalize cases with a lot of changes.
//!
//! The spectrum goes from the degenerate case of assigning a fresh, unique, random CUI to every
//! expression, which is fast and sound (probabilistically at least) but useless (no matching is
//! possible), to an ideal scheme verifying `e1` beta-equivalent to `e2` implies `CUI(e1)` ==
//! `CUI(e2)`. This would give the best reuse but isn't even computable because of the halting
//! problem.
//!
//! The interpreter usually chooses so-called _thunks of interest_, which are thunks that are worth
//! caching across evaluations (as hashing, recording and persisting has a cost). This is the
//! thunks for which we compute the CUI.

use super::value::NickelValue;

/// A semantic hash for re-using previous computations in the incremental evaluation mode.
#[derive(Copy, Debug, PartialEq, Eq, Clone, Hash)]
pub struct ContentHash(pub u64);

pub fn cui(v: &NickelValue) -> ContentHash {
    todo!()
}
