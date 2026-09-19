//! Independent Arc-backed Vector kernel and its original tests.
use std::array;
use std::sync::Arc;

const BITS: usize = 5;
const WIDTH: usize = 1 << BITS;
const MASK: usize = WIDTH - 1;

type Leaf<T> = [Option<T>; WIDTH];
type Branch<T> = [Option<Arc<Node<T>>>; WIDTH];

#[derive(Clone)]
enum Node<T> {
    Leaf(Leaf<T>),
    Branch(Box<Branch<T>>),
}

fn empty_leaf<T>() -> Leaf<T> {
    array::from_fn(|_| None)
}

fn empty_branch<T>() -> Branch<T> {
    array::from_fn(|_| None)
}

fn new_path<T>(level: usize, node: Arc<Node<T>>) -> Arc<Node<T>> {
    if level == 0 {
        return node;
    }
    let mut branch = empty_branch();
    branch[0] = Some(new_path(level - BITS, node));
    Arc::new(Node::Branch(Box::new(branch)))
}

fn copy_branch<T: Clone>(node: &Arc<Node<T>>) -> Branch<T> {
    match node.as_ref() {
        Node::Branch(branch) => branch.as_ref().clone(),
        Node::Leaf(_) => panic!("collection trie expected a branch node"),
    }
}

fn push_tail<T: Clone>(
    old_count: usize,
    level: usize,
    parent: &Arc<Node<T>>,
    tail: Arc<Node<T>>,
) -> Arc<Node<T>> {
    let subindex = ((old_count - 1) >> level) & MASK;
    let mut branch = copy_branch(parent);
    let child = if level == BITS {
        tail
    } else {
        match &branch[subindex] {
            Some(existing) => push_tail(old_count, level - BITS, existing, tail),
            None => new_path(level - BITS, tail),
        }
    };
    branch[subindex] = Some(child);
    Arc::new(Node::Branch(Box::new(branch)))
}

fn array_for<T>(vector: &PersistentVector<T>, index: usize) -> Option<&Leaf<T>> {
    if index >= vector.tail_offset() {
        return Some(vector.tail.as_ref());
    }
    let mut node = vector.root.as_ref();
    let mut level = vector.shift;
    while level > 0 {
        let child = match node {
            Node::Branch(branch) => branch[(index >> level) & MASK].as_deref()?,
            Node::Leaf(_) => return None,
        };
        node = child;
        level -= BITS;
    }
    match node {
        Node::Leaf(leaf) => Some(leaf),
        Node::Branch(_) => None,
    }
}

/// An immutable, persistent vector with a 32-way radix trie and a short tail.
///
/// Values are cloned only along the path copied by `push`; all untouched trie
/// nodes are shared by `Arc`.  This makes old versions safe to retain while a
/// newer version is built, matching Birnam's immutable collection contract.
#[derive(Clone)]
pub struct PersistentVector<T> {
    count: usize,
    shift: usize,
    root: Arc<Node<T>>,
    tail: Arc<Leaf<T>>,
    tail_len: usize,
}

impl<T> Default for PersistentVector<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> PersistentVector<T> {
    /// Construct an empty vector.
    pub fn new() -> Self {
        Self {
            count: 0,
            shift: BITS,
            root: Arc::new(Node::Branch(Box::new(empty_branch()))),
            tail: Arc::new(empty_leaf()),
            tail_len: 0,
        }
    }

    pub const fn len(&self) -> usize {
        self.count
    }

    pub const fn is_empty(&self) -> bool {
        self.count == 0
    }

    fn tail_offset(&self) -> usize {
        self.count - self.tail_len
    }

    /// Return the value at a zero-based index, or `None` when it is outside
    /// the vector's bounds.
    pub fn get(&self, index: usize) -> Option<&T> {
        if index >= self.count {
            return None;
        }
        let leaf = array_for(self, index)?;
        leaf[index & MASK].as_ref()
    }
}

impl<T: Clone> PersistentVector<T> {
    /// Append one value, returning a new vector and preserving this version.
    pub fn push(&self, value: T) -> Self {
        let mut next_tail = (*self.tail).clone();
        if self.tail_len < WIDTH {
            next_tail[self.tail_len] = Some(value);
            return Self {
                count: self.count + 1,
                shift: self.shift,
                root: Arc::clone(&self.root),
                tail: Arc::new(next_tail),
                tail_len: self.tail_len + 1,
            };
        }

        let tail_node = Arc::new(Node::Leaf((*self.tail).clone()));
        let (root, shift) = if (self.count >> BITS) > (1usize << self.shift) {
            let mut new_root = empty_branch();
            new_root[0] = Some(Arc::clone(&self.root));
            new_root[1] = Some(new_path(self.shift, tail_node));
            (
                Arc::new(Node::Branch(Box::new(new_root))),
                self.shift + BITS,
            )
        } else {
            (
                push_tail(self.count, self.shift, &self.root, tail_node),
                self.shift,
            )
        };

        next_tail = empty_leaf();
        next_tail[0] = Some(value);
        Self {
            count: self.count + 1,
            shift,
            root,
            tail: Arc::new(next_tail),
            tail_len: 1,
        }
    }

    /// Materialize a copy in index order for protocol adapters and tests.
    pub fn to_vec(&self) -> Vec<T> {
        (0..self.count)
            .filter_map(|index| self.get(index).cloned())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{PersistentVector, WIDTH};
    use std::sync::Arc;

    #[test]
    fn empty_and_bounds_are_safe() {
        let vector = PersistentVector::<usize>::new();
        assert!(vector.is_empty());
        assert_eq!(vector.get(0), None);
        assert_eq!(vector.to_vec(), Vec::<usize>::new());
    }

    #[test]
    fn append_is_persistent_and_shares_the_root_while_tail_has_room() {
        let first = PersistentVector::new().push(10).push(20);
        let second = first.push(30);
        assert_eq!(first.to_vec(), vec![10, 20]);
        assert_eq!(second.to_vec(), vec![10, 20, 30]);
        assert_eq!(first.get(1), Some(&20));
        assert!(Arc::ptr_eq(&first.root, &second.root));
    }

    #[test]
    fn full_tail_moves_into_the_trie_without_mutating_the_old_version() {
        let mut first = PersistentVector::new();
        for value in 0..WIDTH {
            first = first.push(value);
        }
        let second = first.push(WIDTH);
        assert_eq!(first.len(), WIDTH);
        assert_eq!(first.get(WIDTH), None);
        assert_eq!(second.len(), WIDTH + 1);
        assert_eq!(second.get(0), Some(&0));
        assert_eq!(second.get(WIDTH), Some(&WIDTH));
        assert_eq!(first.tail_len, WIDTH);
        assert_ne!(first.tail.as_ptr(), second.tail.as_ptr());
    }

    #[test]
    fn deep_growth_preserves_every_index() {
        let mut vector = PersistentVector::new();
        for value in 0..1_057usize {
            vector = vector.push(value);
        }
        assert_eq!(vector.len(), 1_057);
        for index in [0, 1, 31, 32, 33, 1_023, 1_024, 1_056] {
            assert_eq!(vector.get(index), Some(&index));
        }
        assert_eq!(vector.to_vec().len(), 1_057);
    }
}
