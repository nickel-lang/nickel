//! Cross-evaluation incremental evaluation capabilities.

use super::{
    Cache,
    lazy::{CBNCache, Thunk, ThunkState},
};

use crate::eval::semantic_hash::SemanticHash;
use std::{
    collections::{HashMap, hash_map::Entry},
    io,
};

/// A thunk coming from a preivous evaluation. Can be lazily loaded as a normal thunk if needed.
#[derive(Clone)]
pub struct LoadableThunk(Thunk);

impl LoadableThunk {
    pub fn load(&self) -> Thunk {
        self.0.clone()
    }
}

#[derive(Clone)]
pub enum CacheEntry {
    /// The entry is coming from a previous evaluation round, but it hasn't been re-used yet.
    Loadable(LoadableThunk),
    /// The entry is coming from a previous evaluation round, and it has been re-used in the
    /// current one.
    Loaded(Thunk),
    /// The entry is a new thunk coming from the current evaluation round.
    Recorded(Thunk),
}

impl CacheEntry {
    pub fn get(&mut self) -> &Thunk {
        match self {
            CacheEntry::Loadable(loadable_thunk) => {
                *self = CacheEntry::Loaded(loadable_thunk.load());
                self.get()
            }
            CacheEntry::Loaded(thunk) | CacheEntry::Recorded(thunk) => thunk,
        }
    }
}

// Not clear yet if all hashable thunks should always be fetched from cache, or if we need a
// distinction between dependencies (intermediate thunks) whose hash is needed but that we don't
// necessarily want to put in the cache, and thunks of interest. Note that at the time of
// serialization, we'll need _some_ dependencies, but they might have changed (or be gone
// completely). If there is no distinction, then we don't need the whole on-demand computation for
// hashes: we need to pre-compute the hash anyway to see if the thunk is in the cache, so we can
// very much fill it right away at thunk creation. Concretely, the question is: should we have only
// one `add_cached` additional method, or a `add_hashed` and `add_or_get_from_cache`? Let's
// meditate.
//
// # After meditation
//
// I think there's an even better way: makes hashing/cache fetching lazy, and only try to fetch
// from the incremental cache when we first evaluate a (hashable/of interest) thunk. Doing so, any
// unused thunk will be left alone. We can compute hashes in a lazy way. This means we allocate a
// thunk unconditionally at first (even if it ends up being pulled from the incremental cache), but
// it's not a huge cost and is likely to be similar to the cost of pulling an existing loadable
// thunk from the incremental cache anyway, as we need to put the thunk data somewhere.
#[derive(Default, Clone)]
pub struct IncrementalCache {
    cbn_cache: CBNCache,
    thunks: HashMap<SemanticHash, CacheEntry>,
}

impl IncrementalCache {
    /// Serialize the cache to a persistent storage (typically a file), to be re-used in a
    /// subsequent evaluation. Only loaded and recorded [CacheEntry]s will be saved; loadable ones
    /// (recorded from the previous evaluation but not used in the current one) are dropped.
    pub fn persist(self, _out: impl io::Write) -> io::Result<()> {
        unimplemented!()
    }

    /// Loads a persisted cache.
    pub fn load(_src: impl io::Read) -> io::Result<Self> {
        unimplemented!()
    }
}

impl Cache for IncrementalCache {
    type UpdateIndex = <CBNCache as Cache>::UpdateIndex;

    fn get(&self, idx: super::CacheIndex) -> crate::eval::Closure {
        self.cbn_cache.get(idx)
    }

    fn get_update_index(
        &mut self,
        idx: &mut super::CacheIndex,
    ) -> Result<Option<Self::UpdateIndex>, super::BlackholedError> {
        // Incremental caching: if the thunk has a computable hash, and we find it in the table, we
        // use that value as well
        if let ThunkState::Suspended = idx.state()
            && idx.cui().is_some()
            && let Some(content_hash) = idx.semantic_hash()
            && let Some(cached) = self.thunks.get_mut(&content_hash)
        {
            idx.clone().update(cached.get().get_owned())
        }

        self.cbn_cache.get_update_index(idx)
    }

    fn add(
        &mut self,
        clos: crate::eval::Closure,
        bty: crate::term::BindingType,
    ) -> super::CacheIndex {
        self.cbn_cache.add(clos, bty)
    }

    fn patch<F: Fn(&mut crate::eval::Closure)>(&mut self, idx: super::CacheIndex, f: F) {
        self.cbn_cache.patch(idx, f)
    }

    fn get_then<T, F: FnOnce(&crate::eval::Closure) -> T>(
        &self,
        idx: super::CacheIndex,
        f: F,
    ) -> T {
        self.cbn_cache.get_then(idx, f)
    }

    fn update(&mut self, clos: crate::eval::Closure, idx: Self::UpdateIndex) {
        // If we update a thunk of interest, now that we have a value for it, we can try to compute its
        // hash and record it for future evaluations.
        if idx.cui().is_some()
            && let Some(content_hash) = idx.semantic_hash()
            && let Entry::Vacant(entry) = self.thunks.entry(content_hash)
        {
            entry.insert_entry(CacheEntry::Recorded(idx.clone()));
        }

        self.cbn_cache.update(clos, idx)
    }

    fn new() -> Self {
        Self::default()
    }

    fn reset_index_state(&mut self, idx: &mut Self::UpdateIndex) {
        self.cbn_cache.reset_index_state(idx)
    }

    fn build_cached(
        &mut self,
        idx: &mut super::CacheIndex,
        rec_env: &[(nickel_lang_parser::identifier::Ident, super::CacheIndex)],
    ) {
        self.cbn_cache.build_cached(idx, rec_env)
    }

    fn saturate<I: DoubleEndedIterator<Item = nickel_lang_parser::identifier::Ident> + Clone>(
        &mut self,
        idx: super::CacheIndex,
        fields: I,
    ) -> crate::eval::value::NickelValue {
        self.cbn_cache.saturate(idx, fields)
    }

    fn revert(&mut self, idx: &super::CacheIndex) -> super::CacheIndex {
        self.cbn_cache.revert(idx)
    }

    fn deps(&self, idx: &super::CacheIndex) -> Option<crate::term::record::FieldDeps> {
        self.cbn_cache.deps(idx)
    }

    fn make_update_index(
        &mut self,
        idx: &mut super::CacheIndex,
    ) -> Result<Self::UpdateIndex, super::BlackholedError> {
        self.cbn_cache.make_update_index(idx)
    }

    fn attach_cui(&mut self, idx: &super::CacheIndex, cui: SemanticHash) {
        idx.set_cui(cui);
    }
}
