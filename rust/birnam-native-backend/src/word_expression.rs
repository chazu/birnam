//! One expression tree with lexical inputs and nonescaping Word intermediates.
use birnam_artifact::portable_program::{BindingKind, NodeKind, Program, StatementKind};
use std::collections::{BTreeMap, BTreeSet};
pub(super) struct Plan {
    pub bytes: Vec<u8>,
    pub bindings: Vec<u32>,
    pub nodes: BTreeSet<usize>,
    pub aliases: BTreeMap<usize, usize>,
}
fn opcode(name: &[u8]) -> Option<u8> {
    Some(match name {
        b"=" => 2,
        b"bitNot" => 5,
        b"wrappingAdd:" => 6,
        b"wrappingSubtract:" => 7,
        b"wrappingMultiply:" => 8,
        b"unsignedDivide:" => 9,
        b"unsignedRemainder:" => 10,
        b"bitAnd:" => 11,
        b"bitOr:" => 12,
        b"bitXor:" => 13,
        b"shiftLeft:" => 14,
        b"shiftRight:" => 15,
        b"unsignedLessThan:" => 16,
        _ => return None,
    })
}
fn plan_min(
    p: &Program<'_>,
    root: usize,
    uses: &[usize],
    minimum: usize,
    shared_locals: bool,
) -> Option<Plan> {
    fn visit(
        p: &Program<'_>,
        i: usize,
        root: usize,
        uses: &[usize],
        bindings: &mut Vec<u32>,
        ops: &mut Vec<[u8; 3]>,
        nodes: &mut BTreeSet<usize>,
        depth: usize,
        locals: &[usize],
        aliases: &mut BTreeMap<usize, usize>,
        outputs: &mut BTreeMap<usize, (bool, usize)>,
        shared_locals: bool,
    ) -> Option<(bool, usize)> {
        if depth > 32 {
            return None;
        }
        if let Some(value) = outputs.get(&i) {
            return Some(*value);
        }
        let n = p.node(i)?;
        if n.semantic_kind() == Some(NodeKind::LexicalRead) {
            let b = n.auxiliary_0;
            let binding = p.binding(b as usize)?;
            if binding.kind == BindingKind::Let && (locals[b as usize] == 1 || shared_locals) {
                let source = p.statement(binding.defining_statement? as usize)?.root_node as usize;
                // A lexical alias is reconstructed only if its producer is included.
                let mut attempt_bindings = bindings.clone();
                let mut attempt_ops = ops.clone();
                let mut attempt_nodes = nodes.clone();
                let mut attempt_aliases = aliases.clone();
                let mut attempt_outputs = outputs.clone();
                if let Some(value) = visit(
                    p,
                    source,
                    root,
                    uses,
                    &mut attempt_bindings,
                    &mut attempt_ops,
                    &mut attempt_nodes,
                    depth + 1,
                    locals,
                    &mut attempt_aliases,
                    &mut attempt_outputs,
                    shared_locals,
                ) {
                    attempt_aliases.insert(i, source);
                    attempt_nodes.insert(i);
                    *bindings = attempt_bindings;
                    *ops = attempt_ops;
                    *nodes = attempt_nodes;
                    *aliases = attempt_aliases;
                    *outputs = attempt_outputs;
                    return Some(value);
                }
            }
            let index = bindings.iter().position(|v| *v == b).unwrap_or_else(|| {
                bindings.push(b);
                bindings.len() - 1
            });
            return Some((false, index));
        }
        if n.semantic_kind() != Some(NodeKind::Send) || (i != root && uses[i] != 1) {
            return None;
        }
        let op = opcode(n.data)?;
        if i != root && matches!(op, 2 | 16) {
            return None;
        }
        if n.operand_count != if op == 5 { 1 } else { 2 } {
            return None;
        }
        let a = visit(
            p,
            p.node_operand(i, 0)? as usize,
            root,
            uses,
            bindings,
            ops,
            nodes,
            depth + 1,
            locals,
            aliases,
            outputs,
            shared_locals,
        )?;
        let b = if op == 5 {
            a
        } else {
            visit(
                p,
                p.node_operand(i, 1)? as usize,
                root,
                uses,
                bindings,
                ops,
                nodes,
                depth + 1,
                locals,
                aliases,
                outputs,
                shared_locals,
            )?
        };
        // Initially encode operation references with the high bit set.
        let enc = |v: (bool, usize)| {
            u8::try_from(v.1)
                .ok()
                .filter(|v| *v < 32)
                .map(|x| if v.0 { x | 128 } else { x })
        };
        let index = ops.len();
        ops.push([op, enc(a)?, enc(b)?]);
        nodes.insert(i);
        outputs.insert(i, (true, index));
        Some((true, index))
    }
    let (mut bindings, mut ops, mut nodes) = (vec![], vec![], BTreeSet::new());
    let mut locals = vec![0; p.binding_count as usize];
    for i in 0..p.node_count as usize {
        let n = p.node(i)?;
        if n.semantic_kind() == Some(NodeKind::LexicalRead) {
            locals[n.auxiliary_0 as usize] += 1;
        }
        if n.semantic_kind() == Some(NodeKind::BlockLiteral) {
            // Block operands are binding indices, not node indices.
            for j in 0..n.operand_count as usize {
                locals[p.node_operand(i, j)? as usize] += 1;
            }
        }
    }
    let mut aliases = BTreeMap::new();
    let mut outputs = BTreeMap::new();
    visit(
        p,
        root,
        root,
        uses,
        &mut bindings,
        &mut ops,
        &mut nodes,
        0,
        &locals,
        &mut aliases,
        &mut outputs,
        shared_locals,
    )?;
    if !aliases.is_empty() {
        let first = *nodes.first()?;
        // Delay only a contiguous pure region. Calls and construction are barriers.
        for i in first..=root {
            if !nodes.contains(&i) && p.node(i)?.semantic_kind() != Some(NodeKind::LexicalRead) {
                return None;
            }
        }
        let sources: BTreeSet<_> = aliases.values().copied().collect();
        for &read in aliases.keys() {
            let binding = p.node(read)?.auxiliary_0;
            let included = aliases
                .keys()
                .filter(|&&i| p.node(i).unwrap().auxiliary_0 == binding)
                .count();
            if included != locals[binding as usize] {
                return None;
            }
        }
        for i in 0..p.statement_count as usize {
            let st = p.statement(i)?;
            let at = st.root_node as usize;
            if at >= first && at < root && (st.kind != StatementKind::Let || !sources.contains(&at))
            {
                return None;
            }
        }
    }
    if ops.len() < minimum || ops.len() > 32 || bindings.len() > 32 {
        return None;
    }
    let count = bindings.len();
    let mut bytes = vec![
        1,
        count as u8,
        ops.len() as u8,
        (count + ops.len() - 1) as u8,
    ];
    for mut op in ops {
        for index in &mut op[1..] {
            if *index & 128 != 0 {
                *index = (*index & 127) + count as u8;
            }
        }
        bytes.extend_from_slice(&op);
    }
    Some(Plan {
        bytes,
        bindings,
        nodes,
        aliases,
    })
}
pub(super) fn uses(p: &Program<'_>) -> Vec<usize> {
    let mut uses = vec![0; p.node_count as usize];
    for i in 0..p.node_count as usize {
        let n = p.node(i).unwrap();
        if n.semantic_kind() == Some(NodeKind::Send) {
            for j in 0..n.operand_count as usize {
                uses[p.node_operand(i, j).unwrap() as usize] += 1;
            }
        }
    }
    for i in 0..p.statement_count as usize {
        uses[p.statement(i).unwrap().root_node as usize] += 1;
    }
    uses
}
pub(super) fn plans(p: &Program<'_>) -> BTreeMap<usize, Plan> {
    let uses = uses(p);
    let mut result = BTreeMap::new();
    let mut claimed = BTreeSet::new();
    for i in (0..p.node_count as usize).rev() {
        if claimed.contains(&i) {
            continue;
        }
        if let Some(plan) = plan_min(p, i, &uses, 4, false) {
            claimed.extend(plan.nodes.iter().copied());
            result.insert(i, plan);
        }
    }
    result
}

/// Whole synchronous helper body, derived from its authored PortableProgram.
/// No calls, literals, escaping locals or resumable boundaries are admitted.
pub(super) fn helper(p: &Program<'_>) -> Option<Plan> {
    let last = p.statement_count.checked_sub(1)? as usize;
    let root = p.statement(last)?.root_node as usize;
    let plan = plan_min(p, root, &uses(p), 4, true)?;
    if p.parameter_count == 0
        || p.parameter_count > 17
        || p.capture_count > 32
        || plan
            .bindings
            .iter()
            .any(|b| *b >= p.parameter_count + p.capture_count)
    {
        return None;
    }
    for i in 0..p.node_count as usize {
        if !plan.nodes.contains(&i) && p.node(i)?.semantic_kind() != Some(NodeKind::LexicalRead) {
            return None;
        }
    }
    let sources: BTreeSet<_> = plan.aliases.values().copied().collect();
    for i in 0..last {
        let st = p.statement(i)?;
        if st.kind != StatementKind::Let || !sources.contains(&(st.root_node as usize)) {
            return None;
        }
    }
    Some(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Block, Expr, Statement};
    use std::borrow::Cow;
    fn send(a: Expr<'static>, name: &'static [u8], b: Expr<'static>) -> Expr<'static> {
        Expr::Send {
            receiver: Box::new(a),
            selector: Cow::Borrowed(name),
            args: vec![b],
            source: b"",
        }
    }
    fn encoded(effectful: bool) -> Vec<u8> {
        let a = Expr::Global(b"a");
        let b = Expr::Global(b"b");
        let inner = if effectful {
            send(a, b"unknown:", b)
        } else {
            send(a, b"bitXor:", b)
        };
        let expr = send(
            send(
                send(inner, b"wrappingAdd:", Expr::Global(b"a")),
                b"bitAnd:",
                Expr::Global(b"b"),
            ),
            b"wrappingAdd:",
            Expr::Global(b"a"),
        );
        crate::portable::encode_block(
            &Block {
                params: vec![b"a", b"b"],
                body: vec![Statement::Expr(expr)],
            },
            &[],
        )
        .unwrap()
    }
    #[test]
    fn locals_fuse_only_when_single_use_and_contiguous() {
        for (escape, barrier) in [(false, false), (true, false), (false, true)] {
            let mut body = vec![Statement::Let {
                name: b"x",
                value: send(Expr::Global(b"a"), b"bitXor:", Expr::Global(b"b")),
            }];
            if barrier {
                body.push(Statement::Expr(send(
                    Expr::Global(b"a"),
                    b"observe:",
                    Expr::Global(b"b"),
                )));
            }
            if escape {
                body.push(Statement::Expr(Expr::Block(Box::new(Block {
                    params: vec![],
                    body: vec![Statement::Expr(Expr::Global(b"x"))],
                }))));
            }
            body.push(Statement::Expr(send(
                send(
                    send(Expr::Global(b"x"), b"wrappingAdd:", Expr::Global(b"a")),
                    b"bitAnd:",
                    Expr::Global(b"b"),
                ),
                b"wrappingAdd:",
                Expr::Global(b"a"),
            )));
            let bytes = crate::portable::encode_block(
                &Block {
                    params: vec![b"a", b"b"],
                    body,
                },
                &[],
            )
            .unwrap();
            let p = birnam_artifact::portable_program::inspect(&bytes).unwrap();
            let plans = plans(&p);
            assert_eq!(plans.len(), usize::from(!escape && !barrier));
            if !escape && !barrier {
                assert_eq!(plans.values().next().unwrap().aliases.len(), 1);
            }
        }
    }
    #[test]
    fn only_nonescaping_primitive_trees_form_a_plan() {
        let bytes = encoded(false);
        let p = birnam_artifact::portable_program::inspect(&bytes).unwrap();
        let plans = plans(&p);
        assert_eq!(plans.len(), 1);
        let plan = plans.values().next().unwrap();
        assert_eq!(plan.bindings, vec![0, 1]);
        assert_eq!(
            plan.bytes,
            vec![1, 2, 4, 5, 13, 0, 1, 6, 2, 0, 11, 3, 1, 6, 4, 0]
        );
        let bytes = encoded(true);
        let p = birnam_artifact::portable_program::inspect(&bytes).unwrap();
        assert!(
            super::plans(&p).is_empty(),
            "an unknown/effectful send cannot be omitted"
        );
    }
}

#[cfg(test)]
mod option_tests {
    use super::super::{Optimizations, compile_portable_with_optimizations};
    use crate::{Block, Expr, Statement};
    use std::borrow::Cow;
    #[test]
    fn explicit_options_enable_only_the_requested_passes() {
        fn send(a: Expr<'static>, name: &'static [u8], b: Expr<'static>) -> Expr<'static> {
            Expr::Send {
                receiver: Box::new(a),
                selector: Cow::Borrowed(name),
                args: vec![b],
                source: b"",
            }
        }
        let expr = send(
            send(
                send(
                    send(Expr::Global(b"a"), b"bitXor:", Expr::Global(b"b")),
                    b"wrappingAdd:",
                    Expr::Global(b"a"),
                ),
                b"bitAnd:",
                Expr::Global(b"b"),
            ),
            b"wrappingAdd:",
            Expr::Global(b"a"),
        );
        let bytes = crate::portable::encode_block(
            &Block {
                params: vec![b"a", b"b"],
                body: vec![Statement::Expr(expr)],
            },
            &[],
        )
        .unwrap();
        for mask in 0..128 {
            if mask & 2 != 0 {assert!(Optimizations::from_bits(mask).is_none());continue;}
            let source = compile_portable_with_optimizations(
                &bytes,
                b"",
                b"<test>",
                false,
                true,
                Optimizations::from_bits(mask).unwrap(),
            )
            .unwrap();
            let source = String::from_utf8(source).unwrap();
            assert_eq!(source.contains("tryPrimitiveExpression:"), mask & 1 != 0);
            assert!(!source.contains("R fast=try_frame_call"), "synchronous code keeps its ABI");
            let resumable = String::from_utf8(compile_portable_with_optimizations(
                &bytes, b"", b"<test>", false, false, Optimizations::from_bits(mask).unwrap(),
            ).unwrap()).unwrap();
            assert_eq!(resumable.contains("R fast=try_frame_call"), mask & 32 != 0);

            assert_eq!(
                source.contains("withWordExpression:bindings:parameters:"),
                mask & 4 != 0
            );
        }
        assert!(Optimizations::from_bits(128).is_none());
    }
}
