//! Compare authored method semantics with admitted portable code, without
//! executing source or reopening compile-time assets.
use birnam_artifact::portable_program::{NodeKind, Program, inspect};

/// Exact source semantics, allowing the compiler's literal embedFile: rewrite.
/// Embedded payload bytes are compiled inputs, not recoverable from source text;
/// their integrity belongs to the containing artifact/image. No other source
/// literals, bindings, statement order, or message operands may differ.
pub fn matches_source(source: &[u8], actual: &[u8]) -> bool {
    compare(source, actual).unwrap_or(false)
}

fn compare(source: &[u8], actual: &[u8]) -> Option<bool> {
    enum Work<'a> {
        Program(&'a [u8], &'a [u8]),
        Node(Program<'a>, usize, Program<'a>, usize),
    }
    let mut pending = vec![Work::Program(source, actual)];
    while let Some(work) = pending.pop() {
        match work {
            Work::Program(source, actual) => {
                let s = inspect(source).ok()?;
                let a = inspect(actual).ok()?;
                if source == actual {
                    continue;
                }
                if (
                    s.parameter_count,
                    s.capture_count,
                    s.binding_count,
                    s.statement_count,
                ) != (
                    a.parameter_count,
                    a.capture_count,
                    a.binding_count,
                    a.statement_count,
                ) {
                    return Some(false);
                }
                for i in 0..s.binding_count as usize {
                    if s.binding(i)? != a.binding(i)? {
                        return Some(false);
                    }
                }
                for i in 0..s.statement_count as usize {
                    let ss = s.statement(i)?;
                    let aa = a.statement(i)?;
                    if (ss.kind, ss.binding_index) != (aa.kind, aa.binding_index) {
                        return Some(false);
                    }
                    pending.push(Work::Node(
                        s,
                        ss.root_node as usize,
                        a,
                        aa.root_node as usize,
                    ));
                }
            }
            Work::Node(s, si, a, ai) => {
                let sn = s.node(si)?;
                let an = a.node(ai)?;
                let kind = sn.semantic_kind()?;
                if kind != an.semantic_kind()? {
                    return Some(false);
                }
                // Embedding is restricted to a literal path on global System.
                // A lexical receiver (including a captured System) cannot match.
                if kind == NodeKind::Send
                    && sn.data == b"embedFile:"
                    && sn.operand_count == 2
                    && an.data == b"asBytes"
                    && an.operand_count == 1
                {
                    let receiver = s.node(s.node_operand(si, 0)? as usize)?;
                    let path = s.node(s.node_operand(si, 1)? as usize)?;
                    let payload = a.node(a.node_operand(ai, 0)? as usize)?;
                    if receiver.semantic_kind()? == NodeKind::GlobalRead
                        && receiver.data == b"System"
                        && path.semantic_kind()? == NodeKind::String
                        && valid_path(path.data)
                        && payload.semantic_kind()? == NodeKind::String
                        && payload.data.len() <= 16 * 1024 * 1024
                    {
                        continue;
                    }
                    return Some(false);
                }
                if sn.operand_count != an.operand_count
                    || sn.payload != an.payload
                    || sn.auxiliary_0 != an.auxiliary_0
                    || sn.auxiliary_1 != an.auxiliary_1
                {
                    return Some(false);
                }
                if kind == NodeKind::BlockLiteral {
                    pending.push(Work::Program(sn.data, an.data));
                    for i in 0..sn.operand_count as usize {
                        // Block operands are lexical binding indices, not nodes.
                        if s.node_operand(si, i)? != a.node_operand(ai, i)? {
                            return Some(false);
                        }
                    }
                } else {
                    if sn.data != an.data {
                        return Some(false);
                    }
                    for i in 0..sn.operand_count as usize {
                        pending.push(Work::Node(
                            s,
                            s.node_operand(si, i)? as usize,
                            a,
                            a.node_operand(ai, i)? as usize,
                        ));
                    }
                }
            }
        }
    }
    Some(true)
}

fn valid_path(path: &[u8]) -> bool {
    !path.is_empty()
        && !path.starts_with(b"/")
        && !path.contains(&0)
        && std::str::from_utf8(path).is_ok()
        && !path.split(|&b| b == b'/').any(|p| p == b"." || p == b"..")
}

#[cfg(test)]
mod tests {
    use super::*;
    use birnam_artifact::portable_program::{NodeSpec, ProgramSpec, StatementSpec, encode};

    fn program(nodes: &[NodeSpec<'_>], captures: &[&[u8]]) -> Vec<u8> {
        encode(ProgramSpec {
            parameters: &[b"self"],
            captures,
            lets: &[],
            statements: &[StatementSpec::Expression {
                root_node: nodes.len() as u32 - 1,
            }],
            nodes,
        })
        .unwrap()
    }

    #[test]
    fn compares_bodies_and_capture_identity() {
        let source = program(&[NodeSpec::Integer(17)], &[]);
        assert!(matches_source(&source, &source));
        assert!(!matches_source(
            &source,
            &program(&[NodeSpec::Integer(99)], &[])
        ));
        assert!(!matches_source(b"invalid", b"invalid"));
        let source = program(&[NodeSpec::LexicalRead(1)], &[b"helper"]);
        assert!(matches_source(&source, &source));
        assert!(!matches_source(
            &source,
            &program(&[NodeSpec::LexicalRead(0)], &[b"helper"])
        ));
        assert!(!matches_source(
            &source,
            &program(&[NodeSpec::LexicalRead(1)], &[b"other"])
        ));
    }

    #[test]
    fn accepts_only_the_literal_embedding_rewrite_including_nested_blocks() {
        let authored = program(
            &[
                NodeSpec::GlobalRead(b"System"),
                NodeSpec::String(b"asset.bin"),
                NodeSpec::Send {
                    selector: b"embedFile:",
                    operands: &[0, 1],
                },
            ],
            &[],
        );
        let compiled = program(
            &[
                NodeSpec::String(b"\x00\xffpayload"),
                NodeSpec::Send {
                    selector: b"asBytes",
                    operands: &[0],
                },
            ],
            &[],
        );
        assert!(matches_source(&authored, &compiled));
        let wrap = |bytes: &[u8]| {
            program(
                &[NodeSpec::BlockLiteral {
                    program: bytes,
                    captures: &[],
                }],
                &[],
            )
        };
        assert!(matches_source(&wrap(&authored), &wrap(&compiled)));
        assert!(!matches_source(
            &authored,
            &program(
                &[
                    NodeSpec::String(b"payload"),
                    NodeSpec::Send {
                        selector: b"printString",
                        operands: &[0]
                    },
                ],
                &[]
            )
        ));
        for receiver in [NodeSpec::GlobalRead(b"Other"), NodeSpec::LexicalRead(0)] {
            let shadowed = program(
                &[
                    receiver,
                    NodeSpec::String(b"asset.bin"),
                    NodeSpec::Send {
                        selector: b"embedFile:",
                        operands: &[0, 1],
                    },
                ],
                &[],
            );
            assert!(!matches_source(&shadowed, &compiled));
        }
        let captured = program(
            &[
                NodeSpec::LexicalRead(1),
                NodeSpec::String(b"asset.bin"),
                NodeSpec::Send {
                    selector: b"embedFile:",
                    operands: &[0, 1],
                },
            ],
            &[b"System"],
        );
        assert!(!matches_source(&captured, &compiled));
        let changed_literal = program(&[NodeSpec::String(b"new")], &[]);
        assert!(!matches_source(
            &program(&[NodeSpec::String(b"old")], &[]),
            &changed_literal
        ));
    }
}
