use super::*;
use crate::{Block, Expr, Statement, portable, resumable_c};
use std::borrow::Cow;

fn fixture(calls: usize, captured: bool, escaping: bool, observing: bool) -> Vec<u8> {
    let mut body = vec![
        Statement::Let {
            name: b"saved",
            value: Expr::Global(b"a"),
        },
        Statement::Let {
            name: b"closure",
            value: Expr::Block(Box::new(Block {
                params: vec![b"argument"],
                body: vec![
                    Statement::Let {
                        name: b"local",
                        value: Expr::Global(if captured { b"saved" } else { b"argument" }),
                    },
                    Statement::Expr(Expr::Global(b"local")),
                ],
            })),
        },
        Statement::Let {
            name: b"alias",
            value: Expr::Global(b"closure"),
        },
    ];
    for call in 0..calls {
        body.push(Statement::Let {
            name: b"alias",
            value: Expr::Global(b"alias"),
        });
        let argument = if observing && call == 2 {
            Expr::SmallInt(73)
        } else {
            Expr::Global(match call % 3 {
                0 => b"a",
                1 => b"b",
                _ => b"answer",
            })
        };
        body.push(Statement::Let {
            name: b"answer",
            value: Expr::Send {
                receiver: Box::new(Expr::Global(b"alias")),
                selector: Cow::Borrowed(b"value:"),
                args: vec![argument],
                source: b"",
            },
        });
    }
    body.push(Statement::Expr(Expr::Global(if escaping {
        b"alias"
    } else {
        b"answer"
    })));
    portable::encode_block(
        &Block {
            params: vec![b"a", b"b"],
            body,
        },
        &[],
    )
    .unwrap()
}

#[test]
fn repeated_closure_plans_match_physical_execution_and_baseline_indices() {
    const CLOSURE: usize = 90_001;
    for calls in [1, 2, 3, 32, 33] {
        for captured in [false, true] {
            for escaping in [false, true] {
                let bytes = fixture(calls, captured, escaping, false);
                let program = resumable::lower_portable_block(&bytes).unwrap();
                let layout = resumable::color_slots(&program).unwrap();
                let plan = Plan::derive(&program, &layout).unwrap();
                assert_eq!(plan.following.len() + 1, calls.min(MAX_CALLS));
                let source = String::from_utf8(
                    resumable_c::emit_step_method_with_portable_blocks(
                        0,
                        &program,
                        &layout,
                        1,
                        &[resumable_c::PortableBlockLiteralSpec {
                            node_index: plan.node_index,
                            disposition: resumable_c::BlockLiteralDisposition::Resumable {
                                manifest_entry_index: 7,
                            },
                            portable_program: plan.program.clone(),
                            retain_materialization_owner: true,
                        }],
                    )
                    .unwrap(),
                )
                .unwrap();
                let mut words = vec![0; layout.slot_count as usize];
                let mut planned = words.clone();
                let mut inputs = Vec::new();
                let mut captured_values = Vec::new();
                for call in 0..calls.min(MAX_CALLS) {
                    let block = &program.blocks[call];
                    let slot = |v: resumable::ValueId| layout.value_slots[v.0 as usize] as usize;
                    for op in &block.operations {
                        let value = match &op.kind {
                            OperationKind::EntryReceiver => 11,
                            OperationKind::EntryArgument { index, .. } => {
                                [101, 202][*index as usize]
                            }
                            OperationKind::Bind { source, .. } => words[slot(*source)],
                            OperationKind::PortableBlockLiteral { captures, .. } => {
                                captured_values =
                                    captures.iter().map(|v| words[slot(*v)]).collect();
                                inputs = words.clone();
                                planned.clone_from(&words);
                                CLOSURE
                            }
                            _ => panic!("unexpected operation"),
                        };
                        words[slot(op.result)] = value;
                    }
                    let (stores, arguments, returned, aliases, edge, after) = if call == 0 {
                        (
                            &plan.constructor_stores,
                            &plan.argument_sources,
                            plan.return_source,
                            &plan.invocation_aliases,
                            plan.continuation.as_ref().unwrap(),
                            plan.continuation_aliases.as_ref().unwrap(),
                        )
                    } else {
                        let step = &plan.following[call - 1];
                        assert_eq!(step.pc as usize, call);
                        let pc_text = source
                            .split(&format!("case {call}L:{{"))
                            .nth(1)
                            .unwrap()
                            .split(&format!("case {}L:{{", call + 1))
                            .next()
                            .unwrap();
                        let boxes: Vec<u32> = pc_text
                            .split("MKV(")
                            .skip(1)
                            .map(|s| s.split('L').next().unwrap().parse().unwrap())
                            .collect();
                        assert_eq!(step.boxed_indices, boxes);
                        (
                            &step.stores,
                            &step.argument_sources,
                            step.return_source,
                            &step.invocation_aliases,
                            &step.continuation,
                            &step.continuation_aliases,
                        )
                    };
                    for &(destination, value) in stores {
                        planned[destination as usize] = match value {
                            Value::Input(s) => inputs[s as usize],
                            Value::Closure => CLOSURE,
                            Value::Result(_) => panic!("entry-only fixture"),
                        };
                    }
                    assert_eq!(planned, words, "all prefix stores at call {call}");
                    let Terminator::Call {
                        receiver,
                        arguments: actual_arguments,
                        successor,
                        ..
                    } = &block.terminator
                    else {
                        panic!("call")
                    };
                    let actual: Vec<_> = actual_arguments.iter().map(|v| words[slot(*v)]).collect();
                    assert_eq!(
                        arguments
                            .iter()
                            .map(|s| inputs[*s as usize])
                            .collect::<Vec<_>>(),
                        actual
                    );
                    let result = if captured {
                        captured_values[0]
                    } else {
                        actual[0]
                    };
                    assert_eq!(inputs[returned.unwrap() as usize], result);
                    assert_eq!(words[slot(*receiver)], CLOSURE);
                    let window = plan.call_window_start as usize;
                    words[window] = CLOSURE;
                    words[window + 1] = 70_001;
                    for (index, value) in actual.into_iter().enumerate() {
                        words[window + 2 + index] = value;
                    }
                    assert_eq!(
                        *aliases,
                        words.iter().map(|v| *v == CLOSURE).collect::<Vec<_>>()
                    );
                    let original = layout
                        .edges
                        .iter()
                        .find(|e| e.source == block.pc && e.target == *successor)
                        .unwrap();
                    assert_eq!(edge, original);
                    let copies: Vec<_> = original
                        .copies
                        .iter()
                        .map(|c| words[c.source as usize])
                        .collect();
                    for (copy, value) in original.copies.iter().zip(copies) {
                        words[copy.destination as usize] = value;
                    }
                    words[original.result_initialization_slot.unwrap() as usize] = result;
                    for (slot, value) in words.iter_mut().enumerate() {
                        if original
                            .clear_bitmap_words
                            .get(slot / 64)
                            .is_some_and(|bits| bits & (1 << (slot % 64)) != 0)
                        {
                            *value = 0;
                        }
                    }
                    assert_eq!(
                        *after,
                        words.iter().map(|v| *v == CLOSURE).collect::<Vec<_>>()
                    );
                    planned.clone_from(&words);
                }
                let last = plan
                    .following
                    .last()
                    .map_or(plan.continuation_aliases.as_ref().unwrap(), |step| {
                        &step.continuation_aliases
                    });
                assert_eq!(last.iter().any(|v| *v), escaping || calls > MAX_CALLS);
            }
        }
    }
}

#[test]
fn repeated_closure_extension_stops_before_observation_and_changed_call_shapes() {
    let bytes = fixture(4, false, false, true);
    let program = resumable::lower_portable_block(&bytes).unwrap();
    let layout = resumable::color_slots(&program).unwrap();
    assert_eq!(Plan::derive(&program, &layout).unwrap().following.len(), 1);
    let bytes = fixture(4, false, false, false);
    let mut program = resumable::lower_portable_block(&bytes).unwrap();
    let Terminator::Call { selector, .. } = &mut program.blocks[1].terminator else {
        panic!("call")
    };
    *selector = b"anotherMeaning:";
    let plan = Plan::derive(&program, &resumable::color_slots(&program).unwrap()).unwrap();
    assert_eq!(plan.following[0].selector, b"anotherMeaning:");
    let Terminator::Call { arguments, .. } = &mut program.blocks[1].terminator else {
        panic!("call")
    };
    arguments.clear();
    assert!(
        Plan::derive(&program, &resumable::color_slots(&program).unwrap())
            .unwrap()
            .following
            .is_empty()
    );
}

#[test]
fn literal_closure_results_retain_distinct_origins_and_stop_before_dependent_arguments() {
    for dependent_argument in [false, true] {
        let send = |argument| Expr::Send {
            receiver: Box::new(Expr::Global(b"closure")),
            selector: Cow::Borrowed(b"value:"),
            args: vec![Expr::Global(argument)],
            source: b"",
        };
        let bytes = portable::encode_block(
            &Block {
                params: vec![b"input"],
                body: vec![
                    Statement::Let {
                        name: b"closure",
                        value: Expr::Block(Box::new(Block {
                            params: vec![b"unused"],
                            body: vec![Statement::Expr(Expr::SmallInt(42))],
                        })),
                    },
                    Statement::Let {
                        name: b"first",
                        value: send(b"input"),
                    },
                    Statement::Let {
                        name: b"second",
                        value: send(if dependent_argument {
                            b"first"
                        } else {
                            b"input"
                        }),
                    },
                    Statement::Let {
                        name: b"retained",
                        value: Expr::Global(b"first"),
                    },
                    Statement::Let {
                        name: b"third",
                        value: send(b"input"),
                    },
                    Statement::Expr(Expr::Global(b"retained")),
                ],
            },
            &[],
        )
        .unwrap();
        let program = resumable::lower_portable_block(&bytes).unwrap();
        let layout = resumable::color_slots(&program).unwrap();
        let plan = Plan::derive(&program, &layout).unwrap();
        assert_eq!(plan.return_source, None);
        assert!(plan.callee.return_allocation().is_some());
        if dependent_argument {
            assert!(
                plan.following.is_empty(),
                "future result must not read a stale original input"
            );
        } else {
            assert_eq!(plan.following.len(), 2);
            assert!(
                plan.following[0]
                    .stores
                    .iter()
                    .any(|(_, value)| *value == Value::Result(0))
            );
            assert!(
                plan.following[1]
                    .stores
                    .iter()
                    .any(|(_, value)| *value == Value::Result(1))
            );
            assert!(
                plan.following[1]
                    .stores
                    .iter()
                    .any(|(_, value)| *value == Value::Result(0)),
                "an alias of the first result stays distinct from the second"
            );
        }
    }
}

#[test]
fn closure_transition_domains_reject_noncanonical_edges_before_derivation() {
    let bytes = fixture(3, true, true, false);
    let program = resumable::lower_portable_block(&bytes).unwrap();
    let layout = resumable::color_slots(&program).unwrap();
    assert_eq!(Plan::derive(&program, &layout).unwrap().following.len(), 2);
    for edge_index in 0..layout.edges.len() {
        for mutation in 0..7 {
            let mut changed = layout.clone();
            let edge = &mut changed.edges[edge_index];
            match mutation {
                0 => edge.result_initialization_slot = None,
                1 => edge.result_initialization_slot = Some(layout.slot_count),
                2 => edge.clear_bitmap_words.clear(),
                3 => edge.clear_bitmap_words.push(0),
                4 => edge.clear_bitmap_words.push(1 << 63),
                5 => edge.copies.push(resumable::SlotCopy {
                    source: layout.slot_count, destination: 0,
                }),
                6 => edge.copies.push(resumable::SlotCopy {
                    source: 0, destination: layout.slot_count,
                }),
                _ => unreachable!(),
            }
            assert_eq!(Plan::derive(&program, &changed), Err(Refusal::InvalidLayout),
                       "edge {edge_index}, mutation {mutation}");
        }
    }
}
