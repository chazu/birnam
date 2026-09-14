use super::*;
use crate::{Block, Expr, Statement, portable, resumable, resumable_c};
use std::borrow::Cow;

fn send(left: Expr<'static>, right: Expr<'static>) -> Expr<'static> {
    Expr::Send {
        receiver: Box::new(left),
        selector: Cow::Borrowed(b"blend:"),
        args: vec![right],
        source: b"",
    }
}

fn body(tail: bool, returned: &'static [u8], captured: bool) -> Vec<u8> {
    let mut statements = vec![Statement::Let {
        name: b"first",
        value: send(
            Expr::Global(if captured { b"saved" } else { b"a" }),
            Expr::SmallInt(7),
        ),
    }];
    let second = send(Expr::Global(b"first"), Expr::SmallInt(7));
    if tail {
        statements.push(Statement::Expr(second));
    } else {
        statements.push(Statement::Let {
            name: b"second",
            value: second,
        });
        statements.push(Statement::Expr(Expr::Global(returned)));
    }
    portable::encode_block(
        &Block {
            params: vec![b"a"],
            body: statements,
        },
        &if captured {
            vec![b"saved".to_vec()]
        } else {
            vec![]
        },
    )
    .unwrap()
}

#[test]
fn numeric_body_preserves_allocation_order_and_return_identity() {
    for tail in [false, true] {
        for captured in [false, true] {
            let returns: &[&[u8]] = if captured {
                &[b"a", b"first", b"second", b"saved"]
            } else {
                &[b"a", b"first", b"second"]
            };
            for &returned in returns {
                let bytes = body(tail, returned, captured);
                let program = resumable::lower_portable_block(&bytes).unwrap();
                let layout = resumable::color_slots(&program).unwrap();
                assert!(crate::small_callee::compile(&program, &layout).is_err());
                let plan = NumericBody::derive(&program, &layout)
                    .expect("numeric closure body must be described");
                assert_eq!(plan.allocations.len(), plan.template.body_boxes() + 1);
                assert_eq!(plan.recovery.result_count(), 2);
                assert_eq!(plan.publications.len(), program.blocks.len());
                let result_allocations: Vec<_> = plan
                    .allocations
                    .iter()
                    .enumerate()
                    .filter_map(|(i, a)| matches!(a, Allocation::Result(_)).then_some(i as u32))
                    .collect();
                let expected = if tail || returned == b"second" {
                    Identity::Allocation(result_allocations[1])
                } else if returned == b"first" {
                    Identity::Allocation(result_allocations[0])
                } else if returned == b"saved" {
                    Identity::Entry(layout.entry_capture_slots[0])
                } else {
                    Identity::Entry(layout.entry_argument_start)
                };
                assert_eq!(plan.returned, expected);
                assert!(
                    plan.template
                        .sends()
                        .iter()
                        .all(|s| s.selector == b"blend:")
                );
                let literals: Vec<_> = plan
                    .template
                    .inputs()
                    .iter()
                    .filter(|i| matches!(i, crate::numeric_chain::Input::Literal { value: 7, .. }))
                    .collect();
                assert_eq!(literals.len(), 2);
                assert_ne!(literals[0], literals[1]);
                let c = String::from_utf8(
                    resumable_c::emit_step_method(19, &program, &layout, 1).unwrap(),
                )
                .unwrap();
                let emitted: Vec<i64> = c
                    .split("MKV(")
                    .skip(1)
                    .map(|s| {
                        s.split_once(')')
                            .unwrap()
                            .0
                            .trim_end_matches('L')
                            .parse()
                            .unwrap()
                    })
                    .collect();
                let described: Vec<_> = plan
                    .allocations
                    .iter()
                    .filter_map(|a| {
                        if let Allocation::Integer(v) = a {
                            Some(*v)
                        } else {
                            None
                        }
                    })
                    .collect();
                assert_eq!(
                    described, emitted,
                    "canonical C index/literal allocation order"
                );
                for publication in &plan.publications {
                    assert_eq!(
                        plan.allocations[publication.allocation_end - 1],
                        Allocation::Integer(match publication.action {
                            crate::frame_region::Exit::Call(v)
                            | crate::frame_region::Exit::TailCall(v)
                            | crate::frame_region::Exit::Continue(v)
                            | crate::frame_region::Exit::Return(v) => v as i64,
                        })
                    );
                }
            }
        }
    }
}

#[test]
fn returned_literal_names_its_own_allocation_occurrence() {
    for value in [7, i64::MIN, i64::MAX] {
        let bytes = portable::encode_block(
            &Block {
                params: vec![b"a"],
                body: vec![
                    Statement::Let {
                        name: b"first",
                        value: send(Expr::Global(b"a"), Expr::SmallInt(value)),
                    },
                    Statement::Let {
                        name: b"second",
                        value: send(Expr::Global(b"first"), Expr::SmallInt(value)),
                    },
                    Statement::Expr(Expr::SmallInt(value)),
                ],
            },
            &[],
        )
        .unwrap();
        let program = resumable::lower_portable_block(&bytes).unwrap();
        let layout = resumable::color_slots(&program).unwrap();
        let plan = NumericBody::derive(&program, &layout).unwrap();
        let Identity::Allocation(returned) = plan.returned else {
            panic!("fresh literal result")
        };
        assert_eq!(
            plan.allocations[returned as usize],
            Allocation::Integer(value)
        );
        assert_eq!(
            returned as usize,
            plan.allocations.len() - 2,
            "the final publication index follows the returned occurrence"
        );
        assert!(
            plan.allocations[..returned as usize]
                .iter()
                .filter(|&&a| a == Allocation::Integer(value))
                .count()
                >= 2
        );
        assert_eq!(plan.allocations.len(), plan.template.body_boxes() + 1);
    }
}

#[test]
fn numeric_body_refuses_private_receiver_exposure_and_noncanonical_inputs() {
    use crate::resumable::{OperationKind, Terminator};
    let bytes = body(false, b"second", false);
    let program = resumable::lower_portable_block(&bytes).unwrap();
    let layout = resumable::color_slots(&program).unwrap();
    let receiver = program.blocks[0]
        .operations
        .iter()
        .find(|o| matches!(o.kind, OperationKind::EntryReceiver))
        .unwrap()
        .result;

    let mut exposed = program.clone();
    exposed.blocks.last_mut().unwrap().terminator = Terminator::Return(receiver);
    let changed_layout = resumable::color_slots(&exposed).unwrap();
    assert!(
        Template::derive(&exposed, &changed_layout).is_some(),
        "numeric dataflow alone does not forbid receiver return"
    );
    assert!(NumericBody::derive(&exposed, &changed_layout).is_none());

    let mut operand = program.clone();
    let Terminator::Call {
        receiver: input, ..
    } = &mut operand.blocks[0].terminator
    else {
        panic!()
    };
    *input = receiver;
    let changed_layout = resumable::color_slots(&operand).unwrap();
    assert!(Template::derive(&operand, &changed_layout).is_some());
    assert!(NumericBody::derive(&operand, &changed_layout).is_none());

    let mut invalid = layout.clone();
    invalid.entry_receiver_slot = layout.entry_argument_start;
    assert!(NumericBody::derive(&program, &invalid).is_none());

    let mut observing = program.clone();
    let binding = observing.blocks[0]
        .operations
        .iter_mut()
        .find(|o| matches!(o.kind, OperationKind::Literal(_)))
        .unwrap();
    binding.kind = OperationKind::GlobalLookup(b"effect");
    let changed_layout = resumable::color_slots(&observing).unwrap();
    assert!(NumericBody::derive(&observing, &changed_layout).is_none());

    let mut cycle = program.clone();
    let Terminator::Call { successor, .. } = &mut cycle.blocks[0].terminator else {
        panic!()
    };
    successor.0 = 0;
    assert!(NumericBody::derive(&cycle, &layout).is_none());
}
