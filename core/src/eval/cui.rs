//! Cross-evaluation Unique Identifier.
//!
//! The CUI is a semantic hash of expressions that makes it possible for the incremental evaluator
//! to identify and match expressions that haven't changed since the last evaluation, so that their
//! result can be re-used.
//!
//! We compute the CUI of an actual closure by combining the CUI of its core expression with the
//! CUI of its dependencies, hashing everything together. For example, if `CUI(x + 1) = A`, `x` is
//! bound to `0` in the environment, and `CUI(0) = B`, the CUI of the closure `{x + 1 | x <- 0 }`
//! is something akin to `hash((A,B))`.
//!
//! There are a lot of possibilities for CUI schemes. The fundamental constraint is that if
//! `CUI(e1) == CUI(e2)`, then `e1 == e2` (with free variables being equal to themselves based on
//! name alone). CUI can for example be the hash of the source expression, a hash of the AST, a
//! hash of the AST modulo some rules or some normalization (ex hash modulo alpha-conversion), it
//! could be a unique index based on hash-consed expression, etc. The main trade-off of the scheme
//! selection is between overhead and generality (in the sense of the CUIs being invariant by
//! beta-reduction and other semantic-preserving transformations), from the degenerate case of
//! assigning a fresh CUI to every expression, which is too restrictive and useless but the fastest
//! (for CUI computations) and still sound, to an ideal CUI verifying `e1` beta-equivalent to `e2`
//! => `CUI(e1)` == `CUI(e2)`, which would give the best reuse but isn't computable because of the
//! halting problem.
//!
//! The interpreter usually choose so-called _thunks of interest_, which are thunks that are worth
//! caching across evaluations (as hashing, recording and persisting has a cost). This is the
//! thunks for which we compute the CUI.

use super::value::NickelValue;

/// A semantic hash for re-using previous computations in the incremental evaluation mode.
#[derive(Copy, Debug, PartialEq, Eq, Clone, Hash)]
pub struct ContentHash(pub u64);

pub fn cui(v: &NickelValue) -> ContentHash {
    todo!()
}
