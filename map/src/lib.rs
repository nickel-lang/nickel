use std::rc::Rc;
use std::num::NonZeroU64;
use std::hash::{Hash, Hasher, DefaultHasher};

enum Node<K, V, const N: usize> {
    Bitmap(Rc<BitmapNode<K, V, N>>),
    Collision(Rc<CollisionNode<K, V, N>>),
    Array(ArrayNode<K, V, N>),
}

enum NodeSlot<K, V, const N: usize> {
    Node(Node<K, V, N>),
    Entry(Entry<K, V>),
}

struct Entry<K, V> {
    key: K,
    value: V,
}

struct BitmapNode<K, V, const N: usize> {
    bitmap: usize,
    data: Vec<NodeSlot<K, V, N>>,
}

struct CollisionNode<K, V, const N: usize> {
    data: Vec<Entry<K, V>>,
}

struct ArrayNode<K, V, const N: usize>(Rc<ArrayNodeData>);

struct ArrayNodeData<K, V, const N: usize> {
    count: usize,
    data: [Option<Node<K, V, N>>; N]
}

pub struct Map<K, V, const N: usize> {
    len: usize,
    root: Option<Node<K, V, N>>,
}

const HASH_MASK: u64 = 31;

/// The result of a mutation (either insertion or deletion). `T` is instantiated for differen types
/// of nodes or node data.
struct MutationResult<T> {
    /// Whether a new element was added or an element was removed. `false` indicates an in-place
    /// modification for insertion or a failed deletion (the key was missing).
    count_changed: bool,
    /// The new copy of the node if one was created, or `None` if it didn't need to change (a failed
    /// deletion).
    modified: Option<T>,
}

impl<K: Hash + Eq, V> Map<K, V, 32> {
    pub fn new() -> Self {
        Map {
            len: 0,
            root: None,
        }
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

        let result = root.insert(key, value, hash);

        Map {
            len: self.len + result.count_changed as usize,
            root: Some(result.modified.unwrap()),
        }
    }

    pub fn len(&self) -> usize {
        self.len
    } 

    pub fn is_empty(&self) -> bool {
        self.len == 0 
    }
}

impl<K: Hash + Eq, V> Node<K, V, 32> {
    pub fn get(&self, key: &K, hash: u64) -> Option<&V> {
        match self {
            Node::Bitmap(bitmap_node) => bitmap_node.get(key, hash),
            Node::Collision(collision_node) => collision_node.get(key),
            Node::Array(array_node) => array_node.get(key, hash),
        }
    }

    pub fn insert(&self, key: K, value: V, hash: u64) -> MutationResult<Self> {
        match self {
            Node::Bitmap(bitmap_node) => todo!(),
            Node::Collision(collision_node) => todo!(),
            Node::Array(array_node) => {
                Node::Array(array_node.insert(key, value, hash))
            },
        }
    }
}

impl<K: Hash + Eq, V> CollisionNode<K, V, 32> {
    pub fn get(&self, key: &K) -> Option<&V> {
        self.data.iter().find_map(|Entry { key: k, value }| (key == k).then_some(value))
    }

    pub fn insert(&self, key: K, value: V, hash: u64) -> (bool, Self) {
        todo!()
    }
}

impl<K: Hash + Eq, V> ArrayNode<K, V, 32> {
    pub fn get(&self, key: &K, hash: u64) -> Option<&V> {
        let index = (hash & HASH_MASK) as usize;
        self.0.data[index].as_ref()?.get(key, hash >> 5)
    }


    pub fn insert(&self, key: K, value: V, hash: u64) -> (bool, Self) {
}

impl<K: Hash + Eq, V> BitmapNode<K, V, 32> {
    pub fn get(&self, key: &K, hash: u64) -> Option<&V> {
        let index = (hash & HASH_MASK) as usize;

        if self.bitmap & (1 << index) == 0 {
            return None;
        }

        match &self.data[self.dense_index(index)] {
            NodeSlot::Node(node) => node.get(key, hash >> 5),
            NodeSlot::Entry(Entry {key: k, value}) => (k == key).then_some(value),
        }
    }

    fn dense_index(&self, full_index: usize) -> usize {
        let masked = self.bitmap & ((1 << full_index) - 1);
        masked.count_ones() as usize
    }
}
