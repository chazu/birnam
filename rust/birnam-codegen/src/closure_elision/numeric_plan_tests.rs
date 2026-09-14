use super::*;
use crate::{Block, Expr, Statement, portable};
use std::borrow::Cow;

fn send(
    receiver: Expr<'static>,
    selector: &'static [u8],
    argument: Expr<'static>,
) -> Expr<'static> {
    Expr::Send {
        receiver: Box::new(receiver),
        selector: Cow::Borrowed(selector),
        args: vec![argument],
        source: b"",
    }
}

fn fixture(calls: usize, returned: &'static [u8], tail: bool, dependent: bool) -> Vec<u8> {
    let mut inner = vec![Statement::Let {
        name: b"first",
        value: send(Expr::Global(b"saved"), b"blend:", Expr::Global(b"argument")),
    }];
    let last = send(Expr::Global(b"first"), b"blend:", Expr::SmallInt(7));
    if tail {
        inner.push(Statement::Expr(last));
    } else {
        inner.push(Statement::Let {
            name: b"second",
            value: last,
        });
        inner.push(Statement::Expr(Expr::Global(returned)));
    }
    let mut body = vec![
        Statement::Let {
            name: b"saved",
            value: Expr::Global(b"a"),
        },
        Statement::Let {
            name: b"closure",
            value: Expr::Block(Box::new(Block {
                params: vec![b"argument"],
                body: inner,
            })),
        },
        Statement::Let {
            name: b"alias",
            value: Expr::Global(b"closure"),
        },
    ];
    for index in 0..calls {
        body.push(Statement::Let {
            name: b"answer",
            value: send(
                Expr::Global(b"alias"),
                b"value:",
                Expr::Global(if index > 0 && dependent {
                    b"answer"
                } else {
                    b"b"
                }),
            ),
        });
    }
    // The closure identity remains live at the recovery successor.
    body.push(Statement::Expr(Expr::Global(b"alias")));
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
fn numeric_closure_regions_preserve_captures_returns_aliases_and_call_limits() {
    for calls in [1, 2, 3, 32, 33] {
        for tail in [false, true] {
            for returned in [b"argument".as_slice(), b"saved", b"first", b"second"] {
                let bytes = fixture(calls, returned, tail, false);
                let program = resumable::lower_portable_block(&bytes).unwrap();
                let layout = resumable::color_slots(&program).unwrap();
                assert_eq!(
                    Plan::derive(&program, &layout),
                    Err(Refusal::Callee),
                    "leaf admission must not silently broaden"
                );
                let plan = NumericPlan::derive_numeric(&program, &layout).unwrap();
                assert_eq!(plan.following.len() + 1, calls.min(MAX_CALLS));
                assert_eq!(plan.capture_sources.len(), 1);
                assert_eq!(plan.argument_sources.len(), 1);
                let nested = resumable::lower_portable_block(&plan.program).unwrap();
                let nested_layout = resumable::color_slots(&nested).unwrap();
                assert_eq!(
                    plan.callee,
                    numeric_body::NumericBody::derive(&nested, &nested_layout).unwrap()
                );
                let expected = if !tail && returned == b"argument" {
                    Some(plan.argument_sources[0])
                } else if !tail && returned == b"saved" {
                    Some(plan.capture_sources[0])
                } else {
                    None
                };
                assert_eq!(plan.return_source, expected);
                assert_ne!(plan.capture_sources[0], plan.argument_sources[0]);
                assert!(plan.invocation_aliases[plan.call_window_start as usize]);
                assert!(
                    plan.continuation_aliases
                        .as_ref()
                        .unwrap()
                        .iter()
                        .any(|v| *v)
                );
                for call in &plan.following {
                    assert_eq!(call.return_source, expected);
                    assert!(call.invocation_aliases[plan.call_window_start as usize]);
                    assert!(call.continuation_aliases.iter().any(|v| *v));
                }
                let mut malformed = layout.clone();
                malformed.entry_receiver_slot += 1;
                assert_eq!(
                    NumericPlan::derive_numeric(&program, &malformed),
                    Err(Refusal::InvalidLayout)
                );
            }
        }
    }
}

#[test]
fn numeric_closure_sequence_stops_before_a_result_dependent_invocation() {
    for returned in [b"argument".as_slice(), b"saved", b"first", b"second"] {
        let bytes = fixture(3, returned, false, true);
        let program = resumable::lower_portable_block(&bytes).unwrap();
        let layout = resumable::color_slots(&program).unwrap();
        let plan = NumericPlan::derive_numeric(&program, &layout).unwrap();
        // Entry aliases remain predictable before construction. Fresh numeric
        // results need a later consumer with dependency-aware admission.
        assert_eq!(
            plan.following.len(),
            if plan.return_source.is_some() { 2 } else { 0 }
        );
        assert!(plan.continuation_aliases.unwrap().iter().any(|v| *v));
    }
}
