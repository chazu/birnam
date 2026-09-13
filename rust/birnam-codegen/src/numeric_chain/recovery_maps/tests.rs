use super::*;
use crate::{Block, Expr, Statement, portable};
use std::borrow::Cow;

fn send(left: Expr<'static>, right: Expr<'static>) -> Expr<'static> {
    Expr::Send {
        receiver: Box::new(left),
        selector: Cow::Borrowed(b"blend:"),
        args: vec![right],
        source: b"",
    }
}

fn derive(body: Vec<Statement<'static>>) -> (Vec<u8>, PhysicalLayout, RecoveryMaps) {
    let bytes = portable::encode_block(
        &Block {
            params: vec![b"a"],
            body,
        },
        &[],
    )
    .unwrap();
    let program = resumable::lower_portable_block(&bytes).unwrap();
    let layout = resumable::color_slots(&program).unwrap();
    let maps = RecoveryMaps::derive(&program, &layout).unwrap();
    (bytes, layout, maps)
}

#[test]
fn repeated_aliases_recover_the_same_result_in_locals_and_pending_operands() {
    let (bytes, layout, maps) = derive(vec![
        Statement::Let {
            name: b"first",
            value: send(Expr::Global(b"a"), Expr::SmallInt(7)),
        },
        Statement::Let {
            name: b"alias",
            value: Expr::Global(b"first"),
        },
        Statement::Let {
            name: b"second",
            value: send(Expr::Global(b"alias"), Expr::Global(b"first")),
        },
        Statement::Expr(Expr::Global(b"first")),
    ]);
    let program = resumable::lower_portable_block(&bytes).unwrap();
    assert_eq!(maps.result_count(), 2);
    assert_eq!(maps.steps().len(), 3);
    assert!(maps.steps()[0].at_action.iter().all(Option::is_none));
    let window = layout.call_window_start.unwrap() as usize;
    let second = &maps.steps()[1].at_action;
    assert_eq!(second[window], Some(0));
    assert_eq!(second[window + 1], None, "the selector is an ordinary Oop");
    assert_eq!(second[window + 2], Some(0));
    assert!(second.iter().filter(|&&v| v == Some(0)).count() >= 3);
    assert!(
        !second.contains(&Some(1)),
        "the pending call has no result yet"
    );
    let Terminator::Return(returned) = program.blocks[2].terminator else {
        panic!()
    };
    assert_eq!(
        maps.steps()[2].at_action[layout.value_slots[returned.0 as usize] as usize],
        Some(0)
    );
    assert!(
        maps.steps()[2].at_action.contains(&Some(1)),
        "a distinct later result keeps its own identity"
    );
    assert!(maps.steps()[2].after_transition.is_none());
}

#[test]
fn equal_computations_remain_distinct_virtual_identities() {
    let (_, layout, maps) = derive(vec![
        Statement::Let {
            name: b"first",
            value: send(Expr::Global(b"a"), Expr::SmallInt(7)),
        },
        Statement::Let {
            name: b"second",
            value: send(Expr::Global(b"a"), Expr::SmallInt(7)),
        },
        Statement::Expr(send(Expr::Global(b"first"), Expr::Global(b"second"))),
    ]);
    let window = layout.call_window_start.unwrap() as usize;
    assert_eq!(maps.result_count(), 3);
    assert_eq!(maps.steps()[2].at_action[window], Some(0));
    assert_eq!(maps.steps()[2].at_action[window + 2], Some(1));
    assert!(
        maps.steps()[2].after_transition.is_none(),
        "the tail result has no caller slot"
    );
}

#[test]
fn long_chains_clear_dead_results_and_preserve_the_final_alias() {
    for count in [2, 3, 16, 32] {
        let mut body = Vec::new();
        for index in 0..count {
            body.push(Statement::Let {
                name: b"answer",
                value: send(
                    Expr::Global(if index == 0 { b"a" } else { b"answer" }),
                    Expr::SmallInt(7),
                ),
            });
        }
        body.push(Statement::Expr(Expr::Global(b"answer")));
        let (_, layout, maps) = derive(body);
        let window = layout.call_window_start.unwrap() as usize;
        assert_eq!(maps.result_count(), count);
        for index in 0..count {
            let step = &maps.steps()[index];
            assert_eq!(
                step.at_action[window],
                index.checked_sub(1).map(|n| n as u32)
            );
            assert_eq!(
                step.at_action[window + 2],
                None,
                "literals retain their ordinary boxes"
            );
            assert!(
                step.at_action
                    .iter()
                    .flatten()
                    .all(|&n| (n as usize) < index)
            );
            let after = step.after_transition.as_ref().unwrap();
            assert!(after.contains(&Some(index as u32)));
            assert!(after.iter().flatten().all(|&n| n as usize <= index));
        }
        let last = &maps.steps()[count];
        assert!(last.at_action.contains(&Some(count as u32 - 1)));
        assert!(
            last.at_action
                .iter()
                .flatten()
                .all(|&n| n == count as u32 - 1)
        );
    }
}

#[test]
fn continue_boundaries_preserve_aliases_without_inventing_a_numeric_result() {
    let mut body = vec![Statement::Let {
        name: b"answer",
        value: send(Expr::Global(b"a"), Expr::SmallInt(7)),
    }];
    for _ in 0..110 {
        body.push(Statement::Let {
            name: b"answer",
            value: Expr::Global(b"answer"),
        });
    }
    body.push(Statement::Expr(send(
        Expr::Global(b"answer"),
        Expr::SmallInt(7),
    )));
    let (bytes, layout, maps) = derive(body);
    let program = resumable::lower_portable_block(&bytes).unwrap();
    let index = program
        .blocks
        .iter()
        .position(|b| matches!(b.terminator, Terminator::Continue(_)))
        .unwrap();
    let step = &maps.steps()[index];
    assert!(step.at_action.contains(&Some(0)));
    let after = step.after_transition.as_ref().unwrap();
    assert!(after.contains(&Some(0)));
    assert!(after.iter().flatten().all(|&n| n == 0));
    assert_eq!(maps.result_count(), 2);
    let window = layout.call_window_start.unwrap() as usize;
    assert_eq!(maps.steps().last().unwrap().at_action[window], Some(0));
}

#[test]
fn refuses_noncanonical_layouts_and_observing_code() {
    let (bytes, layout, _) = derive(vec![
        Statement::Let {
            name: b"first",
            value: send(Expr::Global(b"a"), Expr::SmallInt(7)),
        },
        Statement::Expr(send(Expr::Global(b"first"), Expr::SmallInt(7))),
    ]);
    let program = resumable::lower_portable_block(&bytes).unwrap();
    let mut invalid = layout.clone();
    invalid.edges[0].clear_bitmap_words.fill(0);
    assert_ne!(invalid, layout);
    assert!(RecoveryMaps::derive(&program, &invalid).is_none());
    let mut invalid = layout.clone();
    invalid.call_window_start = Some(layout.slot_count);
    assert!(RecoveryMaps::derive(&program, &invalid).is_none());
    let mut invalid = program.clone();
    invalid.blocks[0].operations.last_mut().unwrap().kind =
        OperationKind::GlobalLookup(b"observer");
    assert!(RecoveryMaps::derive(&invalid, &layout).is_none());
}
