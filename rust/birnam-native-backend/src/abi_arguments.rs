//! Nonescaping Vector literals consumed only by the native ABI adapter.
use birnam_artifact::portable_program::{NodeKind, Program};
use std::collections::{BTreeMap, BTreeSet};
pub(super) struct Plan {
    pub receiver: u32,
    pub arguments: Vec<u32>,
    pub nodes: BTreeSet<usize>,
}
pub(super) fn plans(p: &Program<'_>) -> BTreeMap<usize, Plan> {
    let uses = super::word_expression::uses(p);
    let mut plans = BTreeMap::new();
    for root in 0..p.node_count as usize {
        let n = p.node(root).unwrap();
        if n.semantic_kind() != Some(NodeKind::Send)
            || n.data != b"sendNativeTo:selector:arguments:signature:resultFactory:"
            || n.operand_count != 6
        {
            continue;
        }
        let args: Vec<_> = (0..6).map(|i| p.node_operand(root, i).unwrap()).collect();
        if [0, 1, 4, 5].iter().any(|&i| {
            p.node(args[i] as usize).unwrap().semantic_kind() != Some(NodeKind::LexicalRead)
        }) || !matches!(
            p.node(args[2] as usize).unwrap().semantic_kind(),
            Some(NodeKind::Symbol | NodeKind::LexicalRead)
        ) {
            continue;
        }
        let mut at = args[3] as usize;
        let mut nodes = BTreeSet::new();
        let mut values = vec![];
        let vector = loop {
            let n = p.node(at).unwrap();
            if n.semantic_kind() != Some(NodeKind::Send) || uses[at] != 1 {
                break None;
            }
            nodes.insert(at);
            if n.data == b"add:" && n.operand_count == 2 && values.len() < 8 {
                let value = p.node_operand(at, 1).unwrap();
                if p.node(value as usize).unwrap().semantic_kind() != Some(NodeKind::LexicalRead) {
                    break None;
                }
                values.push(value);
                at = p.node_operand(at, 0).unwrap() as usize;
            } else if n.data == b"new" && n.operand_count == 1 {
                let v = p.node_operand(at, 0).unwrap();
                let factory = p.node(v as usize).unwrap();
                break (factory.semantic_kind() == Some(NodeKind::GlobalRead)
                    && factory.data == b"Vector")
                    .then_some(v);
            } else {
                break None;
            }
        };
        if let Some(vector) = vector {
            // No callbacks may occur between omitted construction and admission.
            let first = *nodes.first().unwrap();
            if (first..root).any(|i| {
                let kind = p.node(i).unwrap().semantic_kind();
                kind != Some(NodeKind::LexicalRead) && !nodes.contains(&i)
            }) {
                continue;
            }
            values.reverse();
            let mut arguments = vec![args[1], args[2], args[4], args[5], vector];
            arguments.extend(values);
            nodes.insert(root);
            plans.insert(
                root,
                Plan {
                    receiver: args[0],
                    arguments,
                    nodes,
                },
            );
        }
    }
    plans
}
