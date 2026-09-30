//! Owned diagnostic facts. No payload reads, collection, or retention policy.
use std::collections::{HashMap, HashSet};
#[derive(Default)]
pub(crate) struct Graph {
    pub allocations: Vec<(usize, usize)>, // opaque address, stable allocation identity
    pub roots: Vec<usize>,
    pub edges: Vec<(usize, usize)>,
}
impl Graph {
    pub fn partition(&self) -> Option<Vec<(usize, usize, bool)>> {
        let mut allocations = self.allocations.clone();
        allocations.sort_unstable_by_key(|a| a.1);
        let mut indices = HashMap::new();
        let mut identities = HashSet::new();
        for (index, &(address, identity)) in allocations.iter().enumerate() {
            if address == 0
                || identity == 0
                || indices.insert(address, index).is_some()
                || !identities.insert(identity)
            {
                return None;
            }
        }
        let mut pending = Vec::new();
        for root in self.roots.iter().filter(|&&root| root != 0) {
            pending.push(*indices.get(root)?);
        }
        let mut edges = vec![Vec::new(); allocations.len()];
        for (from, to) in &self.edges {
            edges[*indices.get(from)?].push(*indices.get(to)?);
        }
        let mut marked = vec![false; allocations.len()];
        while let Some(index) = pending.pop() {
            if !core::mem::replace(&mut marked[index], true) {
                pending.extend(&edges[index]);
            }
        }
        Some(
            allocations
                .into_iter()
                .zip(marked)
                .map(|((address, identity), reached)| (address, identity, reached))
                .collect(),
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_three_vertex_graph_and_root_set_matches_transitive_closure() {
        for bits in 0..512 {
            for roots in 0..8 {
                let mut g = Graph {
                    allocations: vec![(3, 30), (1, 10), (2, 20)],
                    ..Graph::default()
                };
                for i in 0..3 {
                    if roots & (1 << i) != 0 {
                        g.roots.push(i + 1);
                    }
                }
                for i in 0..3 {
                    for j in 0..3 {
                        if bits & (1 << (i * 3 + j)) != 0 {
                            g.edges.push((i + 1, j + 1));
                        }
                    }
                }
                let mut reached = roots;
                loop {
                    let before = reached;
                    for &(from, to) in &g.edges {
                        if reached & (1 << (from - 1)) != 0 {
                            reached |= 1 << (to - 1);
                        }
                    }
                    if reached == before {
                        break;
                    }
                }
                let expected = (1..=3)
                    .map(|i| (i, i * 10, reached & (1 << (i - 1)) != 0))
                    .collect::<Vec<_>>();
                assert_eq!(g.partition(), Some(expected));
            }
        }
    }
    #[test]
    fn rejects_foreign_edges_even_when_unreached_and_duplicate_identities() {
        let mut g = Graph {
            allocations: vec![(1, 10), (2, 20)],
            ..Graph::default()
        };
        g.edges.push((2, 3));
        assert!(g.partition().is_none());
        g.edges.clear();
        g.roots.push(3);
        assert!(g.partition().is_none());
        g.roots.clear();
        g.allocations[1] = (2, 10);
        assert!(g.partition().is_none());
        g.allocations[1] = (1, 20);
        assert!(g.partition().is_none());
        g.allocations[1] = (2, 0);
        assert!(g.partition().is_none());
    }
    #[test]
    fn duplicates_cycles_null_roots_and_empty_graph_are_valid() {
        assert_eq!(Graph::default().partition(), Some(vec![]));
        let g = Graph {
            allocations: vec![(1, 1), (2, 2), (3, 3)],
            roots: vec![0, 1, 1],
            edges: vec![(1, 2), (1, 2), (2, 1), (3, 3)],
        };
        assert_eq!(
            g.partition(),
            Some(vec![(1, 1, true), (2, 2, true), (3, 3, false)])
        );
    }
}
