use std::rc::Rc;
use std::num::NonZeroU64;

enum NodeSlot<K, V, const N: usize> {
    Node { next: Rc<Node<K, V, N>> },
    Entry { key: K, value: V },
}

struct BitmapNode<K, V, const N: usize> {
    bitmap: usize,
    data: Vec<NodeSlot<K, V, N>>,
}

struct CollisionNode<K, V, const N: usize> {

}

struct Map<K, V, const N: usize> {
    count: usize,
    root: Node<K, V, N>,
}
