use super::*;
use crate::{Block, Expr, Statement, portable, resumable};
use std::borrow::Cow;

fn send(left: Expr<'static>, right: Expr<'static>) -> Expr<'static> {
    Expr::Send {
        receiver: Box::new(left),
        selector: Cow::Borrowed(b"blend:"),
        args: vec![right],
        source: b"",
    }
}

fn program() -> Vec<u8> {
    portable::encode_block(
        &Block {
            params: vec![b"a"],
            body: vec![
                Statement::Let {
                    name: b"first",
                    value: send(Expr::Global(b"a"), Expr::SmallInt(7)),
                },
                Statement::Let {
                    name: b"alias",
                    value: Expr::Global(b"first"),
                },
                Statement::Expr(send(Expr::Global(b"alias"), Expr::SmallInt(7))),
            ],
        },
        &[],
    )
    .unwrap()
}

#[test]
fn canonical_lowering_retains_raw_dependencies_and_distinct_literal_occurrences() {
    let bytes = program();
    let logical = resumable::lower_portable_block(&bytes).unwrap();
    let layout = resumable::color_slots(&logical).unwrap();
    let template = Template::derive(&logical, &layout).unwrap();
    assert_eq!(
        template.inputs().len(),
        3,
        "unused Block receiver is not a numeric input"
    );
    assert_eq!(
        template.inputs()[0],
        Input::Frame(layout.entry_argument_start)
    );
    assert!(matches!(
        template.inputs()[1],
        Input::Literal { value: 7, .. }
    ));
    assert!(matches!(
        template.inputs()[2],
        Input::Literal { value: 7, .. }
    ));
    assert_ne!(template.inputs()[1], template.inputs()[2]);
    assert_eq!(
        template.sends()[1].left,
        Value::Result(0),
        "local aliases retain raw dataflow"
    );
    assert!(
        template
            .sends()
            .iter()
            .all(|send| send.selector == b"blend:")
    );
    let plan = template
        .instantiate(&[Operator::Add, Operator::Multiply])
        .unwrap();
    assert_eq!(plan.operations()[1].operator, Operator::Multiply);
    assert!(template.instantiate(&[Operator::Add]).is_none());
    assert!(template.remaining_boxes() > template.sends().len());
}

#[test]
fn refuses_observing_bodies_backedges_invalid_sources_and_oversized_metadata() {
    let bytes = program();
    let logical = resumable::lower_portable_block(&bytes).unwrap();
    let layout = resumable::color_slots(&logical).unwrap();
    let mut invalid = logical.clone();
    invalid.value_count = u32::MAX;
    assert!(Template::derive(&invalid, &layout).is_none());
    let mut invalid = logical.clone();
    invalid.blocks[0].operations.last_mut().unwrap().kind = OperationKind::GlobalLookup(b"a");
    assert!(Template::derive(&invalid, &layout).is_none());
    let mut invalid = logical.clone();
    if let Terminator::Call { successor, .. } = &mut invalid.blocks[0].terminator {
        successor.0 = 0;
    } else {
        panic!("first ordinary call");
    }
    assert!(Template::derive(&invalid, &layout).is_none());
    let mut invalid = logical.clone();
    if let Terminator::Call { receiver, .. } = &mut invalid.blocks[0].terminator {
        receiver.0 = logical.value_count;
    }
    assert!(Template::derive(&invalid, &layout).is_none());
    let mut invalid = layout.clone();
    invalid.value_slots.fill(invalid.slot_count);
    assert!(Template::derive(&logical, &invalid).is_none());
}
