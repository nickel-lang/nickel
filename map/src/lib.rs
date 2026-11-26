use std::hash::{DefaultHasher, Hash, Hasher};
use std::num::NonZeroU64;
use std::rc::Rc;

#[derive(Clone)]
enum Node<K: Clone, V: Clone, const N: usize> {
    Bitmap(BitmapNode<K, V, N>),
    Collision(CollisionNode<K, V, N>),
    Array(ArrayNode<K, V, N>),
}

#[derive(Clone)]
enum NodeSlot<K: Clone, V: Clone, const N: usize> {
    Node(Node<K, V, N>),
    Entry(Entry<K, V>),
}

#[derive(Clone)]
struct Entry<K: Clone, V: Clone> {
    key: K,
    value: V,
}

struct BitmapNode<K: Clone, V: Clone, const N: usize>(Rc<BitmapNodeData<K, V, N>>);

impl<K: Clone, V: Clone, const N: usize> Clone for BitmapNode<K, V, N> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<K: Clone, V: Clone, const N: usize> From<BitmapNodeData<K, V, N>> for BitmapNode<K, V, N> {
    fn from(data: BitmapNodeData<K, V, N>) -> Self {
        BitmapNode(Rc::new(data))
    }
}

#[derive(Clone)]
struct BitmapNodeData<K: Clone, V: Clone, const N: usize> {
    bitmap: usize,
    data: Vec<NodeSlot<K, V, N>>,
}

#[derive(Clone)]
struct CollisionNode<K: Clone, V: Clone, const N: usize>(Rc<CollisionNodeData<K, V, N>>);

#[derive(Clone)]
struct CollisionNodeData<K: Clone, V: Clone, const N: usize> {
    data: Vec<Entry<K, V>>,
}

#[derive(Clone)]
struct ArrayNode<K: Clone, V: Clone, const N: usize>(Rc<ArrayNodeData<K, V, N>>);

#[derive(Clone)]
struct ArrayNodeData<K: Clone, V: Clone, const N: usize> {
    count: usize,
    data: [Option<Node<K, V, N>>; N],
}

#[derive(Clone)]
pub struct Map<K: Clone, V: Clone, const N: usize> {
    len: usize,
    root: Option<Node<K, V, N>>,
}

const HASH_MASK: u64 = 31;

/// The result of a mutation (either insertion or deletion). `T` is instantiated for different types
/// of nodes or node data.
enum Update<T> {
    /// The map wasn't mutated (deletion of an element that isn't in the map, insertion of a value
    /// that already exists with the same value)
    Unchanged,
    Updated {
        /// Whether a new element was added or an element was removed. `false` indicates an in-place
        /// modification for insertion or a failed deletion (the key was missing).
        count_changed: bool,
        /// The new copy of the node if one was created, or `None` if it didn't need to change (a failed
        /// deletion).
        updated: T,
    },
}

impl<T> Update<T> {
    fn map<U>(self, f: impl FnOnce(T) -> U) -> Update<U> {
        match self {
            Update::Unchanged => Update::Unchanged,
            Update::Updated {
                count_changed,
                updated,
            } => Update::Updated {
                count_changed,
                updated: f(updated),
            },
        }
    }
}

impl<K: Hash + Eq + Clone, V: Eq + Clone> Map<K, V, 32> {
    pub fn new() -> Self {
        Map { len: 0, root: None }
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let hash = hasher.finish();

        self.root.as_ref()?.get(key, hash)
    }

    pub fn insert(&self, key: K, value: V) -> Self {
        let Some(root) = self.root.as_ref() else {
            return Map {
                len: self.len,
                root: None,
            };
        };

        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let hash = hasher.finish();

        match root.insert(key, value, hash) {
            Update::Unchanged => (*self).clone(),
            Update::Updated {
                count_changed,
                updated,
            } => Map {
                len: self.len + count_changed as usize,
                root: Some(updated),
            },
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl<K: Hash + Eq + Clone, V: Eq + Clone> Node<K, V, 32> {
    pub fn get(&self, key: &K, hash: u64) -> Option<&V> {
        match self {
            Node::Bitmap(bitmap_node) => bitmap_node.get(key, hash),
            Node::Collision(collision_node) => collision_node.get(key),
            Node::Array(array_node) => array_node.get(key, hash),
        }
    }

    pub fn insert(&self, key: K, value: V, hash: u64) -> Update<Self> {
        match self {
            Node::Bitmap(bitmap_node) => bitmap_node.insert(key, value, hash).map(Node::Bitmap),
            Node::Collision(collision_node) => {
                collision_node.insert(key, value, hash).map(Node::Collision)
            }
            Node::Array(array_node) => array_node.insert(key, value, hash).map(Node::Array),
        }
    }
}

impl<K: Hash + Eq + Clone, V: Eq + Clone> CollisionNode<K, V, 32> {
    pub fn get(&self, key: &K) -> Option<&V> {
        self.0
            .data
            .iter()
            .find_map(|Entry { key: k, value }| (key == k).then_some(value))
    }

    pub fn insert(&self, key: K, value: V, hash: u64) -> Update<Self> {
        todo!()
    }
}

impl<K: Hash + Eq + Clone, V: Eq + Clone> ArrayNode<K, V, 32> {
    pub fn get(&self, key: &K, hash: u64) -> Option<&V> {
        let index = (hash & HASH_MASK) as usize;
        self.0.data[index].as_ref()?.get(key, hash >> 5)
    }

    pub fn insert(&self, key: K, value: V, hash: u64) -> Update<Self> {
        todo!()
    }
}

impl<K: Hash + Eq + Clone, V: Eq + Clone> BitmapNode<K, V, 32> {
    pub fn singleton(key: K, value: V, hash: usize) -> Self {
        todo!("use the hash, bozo");
        BitmapNodeData {
            bitmap: 1,
            data: vec![NodeSlot::Entry(Entry { key, value })],
        }
        .into()
    }

    pub fn from_entriies(entries: impl IntoIterator<Item = Entry<K,V>>) -> Self {
        todo!("need to put them at the right place with the hash and all")
    }

    pub fn get(&self, key: &K, hash: u64) -> Option<&V> {
        let index = (hash & HASH_MASK) as usize;

        if self.is_slot_empty(index) {
            return None;
        }

        match &self.0.data[self.dense_index(index)] {
            NodeSlot::Node(node) => node.get(key, hash >> 5),
            NodeSlot::Entry(Entry { key: k, value }) => (k == key).then_some(value),
        }
    }

    fn dense_index(&self, full_index: usize) -> usize {
        let masked = self.0.bitmap & ((1 << full_index) - 1);
        masked.count_ones() as usize
    }

    fn is_slot_empty(&self, index: usize) -> bool {
        self.0.bitmap & (1 << index) == 0
    }

    pub fn insert(&self, key: K, value: V, hash: u64) -> Update<Self> {
        let index = (hash & HASH_MASK) as usize;

        if self.is_slot_empty(index) {
            return Update::Updated {
                count_changed: true,
                updated: self.copy_with_child_at(key, value, index),
            };
        }

        match &self.0.data[self.dense_index(index)] {
            NodeSlot::Node(node) => node
                .insert(key, value, hash >> 5)
                .map(|upd| self.copy_with_child_upd(NodeSlot::Node(upd), index)),
            NodeSlot::Entry(Entry { key: k, value: v }) if k != &key => {
                Update::Updated {
                    count_changed: true,
                    updated: self.copy_with_child_upd(todo!("generate a proper node with two entry slot, the new and the old"), index)
                } 
            },
            NodeSlot::Entry(Entry { key: k, value: v }) if v == &value => {
                Update::Unchanged 
            }
            NodeSlot::Entry(Entry { key : k, value :v }) => {
                todo!("key are equals, we need a collision node")
            }
        }
    }

    fn copy_with_child_at(&self, key: K, value: V, index: usize) -> Self {
        let dense_index = self.dense_index(index);

        BitmapNodeData {
            bitmap: self.0.bitmap | (1 << index),
            data: self.0.data[..dense_index]
                .iter()
                .cloned()
                .chain(std::iter::once(NodeSlot::Entry(Entry { key, value })))
                .chain(self.0.data[dense_index..].iter().cloned())
                .collect(),
        }
        .into()
    }

    fn copy_with_child_upd(&self, slot: NodeSlot<K, V, 32>, index: usize) -> Self {
        let dense_index = self.dense_index(index);

        BitmapNodeData {
            bitmap: self.0.bitmap | (1 << index),
            data: self.0.data[..dense_index]
                .iter()
                .cloned()
                .chain(std::iter::once(slot))
                .chain(self.0.data[dense_index + 1..].iter().cloned())
                .collect(),
        }
        .into()
    }
}
