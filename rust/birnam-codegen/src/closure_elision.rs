//! Source-created closure provenance, without runtime authority or Oop storage.
//!
//! This plan describes the first literal and immediate invocation in canonical
//! PortableProgram lowering. It does not authorize omitting construction or a
//! send: admission must validate the current constructor, carrier, code owner,
//! actual invocation Method, Frame controls and allocation/observation budget.
use crate::resumable::{
    self, ColoredEdge, LogicalProgram, OperationKind, PhysicalLayout, Terminator,
};
use crate::small_callee;
pub mod numeric_body;
mod sequence;
mod transition;
pub use sequence::{Invocation, MAX_CALLS};

pub const MAX_SLOTS: usize = 128;
pub const MAX_PROGRAM_BYTES: usize = 65_536;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    RegionShape,
    Limit,
    InvalidLayout,
    ObservingOperation,
    IdentityExposure,
    Callee,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Value {
    Input(u32),
    Closure,
    /// Fresh result of a preceding invocation in this region.
    Result(u32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan<C = small_callee::Plan> {
    /// Occurrence identity, independent of identical nested program bytes.
    pub node_index: u32,
    pub literal_value: u32,
    pub literal_slot: u32,
    pub program: Vec<u8>,
    /// Original caller entry slots, in payload order. Repeated inputs stay
    /// repeated references; neither Cells nor other captures are copied.
    pub capture_sources: Vec<u32>,
    pub argument_sources: Vec<u32>,
    pub return_source: Option<u32>,
    pub selector: Vec<u8>,
    pub call_site: u32,
    pub callee: C,
    /// Complete physical aliases at call publication, including staging slots.
    pub invocation_aliases: Vec<bool>,
    /// Ordered stores after the constructor, including its result but excluding
    /// call staging. Inputs name original caller entry slots.
    pub constructor_stores: Vec<(u32, Value)>,
    pub call_window_start: u32,
    /// The original V2 parallel copies, result destination and clear bitmap.
    /// None is a tail invocation, which has no caller successor.
    pub continuation: Option<ColoredEdge>,
    pub continuation_aliases: Option<Vec<bool>>,
    /// Original integer index boxes, separated at construction. These exclude
    /// constructor work, callee entry/return driving and the initial PC token.
    pub before_constructor_boxes: Vec<u32>,
    pub after_constructor_boxes: Vec<u32>,
    /// Further immediate calls sharing this literal's virtual identity.
    pub following: Vec<Invocation>,
}

impl Plan {
    /// Inputs must be canonical lowering and coloring. Selector spelling gives
    /// no authority: even `value` must resolve to the admitted body at runtime.
    pub fn derive(program: &LogicalProgram<'_>, layout: &PhysicalLayout) -> Result<Self, Refusal> {
        Self::derive_with(
            program,
            layout,
            |program, layout| small_callee::compile(program, layout).map_err(|_| Refusal::Callee),
            small_callee::Plan::return_entry_slot,
        )
    }
}

/// Numeric closure provenance uses the same caller identity and V2 analysis.
/// Existing leaf consumers retain `Plan::derive` and cannot execute this type.
pub type NumericPlan = Plan<numeric_body::NumericBody>;

impl NumericPlan {
    pub fn derive_numeric(
        program: &LogicalProgram<'_>,
        layout: &PhysicalLayout,
    ) -> Result<Self, Refusal> {
        Self::derive_with(
            program,
            layout,
            |program, layout| {
                numeric_body::NumericBody::derive(program, layout).ok_or(Refusal::Callee)
            },
            |body| match body.returned {
                numeric_body::Identity::Entry(slot) => Some(slot as usize),
                numeric_body::Identity::Allocation(_) => None,
            },
        )
    }
}

impl<C> Plan<C> {
    /// Change only the owned callee description; caller provenance stays intact.
    pub fn map_callee<D>(self, map: impl FnOnce(C) -> D) -> Plan<D> {
        Plan {
            node_index: self.node_index,
            literal_value: self.literal_value,
            literal_slot: self.literal_slot,
            program: self.program,
            capture_sources: self.capture_sources,
            argument_sources: self.argument_sources,
            return_source: self.return_source,
            selector: self.selector,
            call_site: self.call_site,
            callee: map(self.callee),
            invocation_aliases: self.invocation_aliases,
            constructor_stores: self.constructor_stores,
            call_window_start: self.call_window_start,
            continuation: self.continuation,
            continuation_aliases: self.continuation_aliases,
            before_constructor_boxes: self.before_constructor_boxes,
            after_constructor_boxes: self.after_constructor_boxes,
            following: self.following,
        }
    }

    fn derive_with(
        program: &LogicalProgram<'_>,
        layout: &PhysicalLayout,
        compile: impl Fn(&LogicalProgram<'_>, &PhysicalLayout) -> Result<C, Refusal>,
        return_entry_slot: impl Fn(&C) -> Option<usize>,
    ) -> Result<Self, Refusal> {
        if program.entry.0 != 0 || program.blocks.is_empty() {
            return Err(Refusal::RegionShape);
        }
        let block = &program.blocks[0];
        if program.blocks.len() > 128
            || program.value_count > 4096
            || block.operations.len() > 128
            || layout.slot_count as usize > MAX_SLOTS
        {
            return Err(Refusal::Limit);
        }
        if resumable::color_slots(program).as_ref() != Ok(layout) {
            return Err(Refusal::InvalidLayout);
        }
        let slot = |value: resumable::ValueId| {
            layout
                .value_slots
                .get(value.0 as usize)
                .copied()
                .ok_or(Refusal::InvalidLayout)
        };
        let mut values = vec![None; program.value_count as usize];
        let mut aliases = vec![false; layout.slot_count as usize];
        let mut literal = None;
        let mut before_boxes = Vec::new();
        let mut after_boxes = Vec::new();
        let mut constructor_stores = Vec::new();
        for operation in &block.operations {
            let destination = slot(operation.result)?;
            let boxes = if literal.is_some() {
                &mut after_boxes
            } else {
                &mut before_boxes
            };
            let value = match &operation.kind {
                OperationKind::EntryReceiver
                | OperationKind::EntryArgument { .. }
                | OperationKind::EntryCapture { .. } => Value::Input(destination),
                OperationKind::Bind { source, .. } => {
                    boxes.extend([destination, slot(*source)?]);
                    values
                        .get(source.0 as usize)
                        .copied()
                        .flatten()
                        .ok_or(Refusal::InvalidLayout)?
                }
                OperationKind::PortableBlockLiteral {
                    node_index,
                    program: nested,
                    captures,
                } => {
                    if literal.is_some() {
                        return Err(Refusal::RegionShape);
                    }
                    if nested.len() > MAX_PROGRAM_BYTES {
                        return Err(Refusal::Limit);
                    }
                    let nested_program =
                        resumable::lower_portable_block(nested).map_err(|_| Refusal::Callee)?;
                    if nested_program.capture_count as usize != captures.len() {
                        return Err(Refusal::Callee);
                    }
                    let nested_layout =
                        resumable::color_slots(&nested_program).map_err(|_| Refusal::Callee)?;
                    let callee = compile(&nested_program, &nested_layout)?;
                    if return_entry_slot(&callee)
                        == Some(nested_layout.entry_receiver_slot as usize)
                    {
                        return Err(Refusal::IdentityExposure);
                    }
                    let mut capture_sources = Vec::new();
                    // Baseline stores the destination index, resolves CodeObject
                    // and vtable, reads each capture, then calls the constructor.
                    boxes.push(destination);
                    for capture in captures {
                        let Some(Value::Input(source)) =
                            values.get(capture.0 as usize).copied().flatten()
                        else {
                            return Err(Refusal::IdentityExposure);
                        };
                        capture_sources.push(source);
                        boxes.push(slot(*capture)?);
                    }
                    literal = Some((
                        *node_index,
                        operation.result.0,
                        destination,
                        nested.to_vec(),
                        capture_sources,
                        nested_layout,
                        callee,
                    ));
                    Value::Closure
                }
                _ => return Err(Refusal::ObservingOperation),
            };
            aliases[destination as usize] = value == Value::Closure;
            if literal.is_some() {
                constructor_stores.push((destination, value));
            }
            if values[operation.result.0 as usize].replace(value).is_some() {
                return Err(Refusal::InvalidLayout);
            }
        }
        let (
            node_index,
            literal_value,
            literal_slot,
            nested,
            capture_sources,
            nested_layout,
            callee,
        ) = literal.ok_or(Refusal::RegionShape)?;
        let (call_site, receiver, selector, arguments, continuation) = match &block.terminator {
            Terminator::Call {
                call_site,
                receiver,
                selector,
                arguments,
                successor,
                ..
            } => {
                let edge = layout
                    .edges
                    .iter()
                    .find(|e| e.source == block.pc && e.target == *successor)
                    .ok_or(Refusal::InvalidLayout)?;
                (
                    *call_site,
                    *receiver,
                    *selector,
                    arguments,
                    Some(edge.clone()),
                )
            }
            Terminator::TailCall {
                call_site,
                receiver,
                selector,
                arguments,
                ..
            } => (*call_site, *receiver, *selector, arguments, None),
            _ => return Err(Refusal::RegionShape),
        };
        if values.get(receiver.0 as usize).copied().flatten() != Some(Value::Closure) {
            return Err(Refusal::RegionShape);
        }
        if arguments.len() != nested_layout.entry_argument_count as usize || selector.len() > 4096 {
            return Err(Refusal::Callee);
        }
        let mut argument_sources = Vec::new();
        for argument in arguments {
            let Some(Value::Input(source)) = values.get(argument.0 as usize).copied().flatten()
            else {
                return Err(Refusal::IdentityExposure);
            };
            argument_sources.push(source);
        }
        let return_source = if let Some(returned) = return_entry_slot(&callee) {
            let returned = returned as u32;
            Some(
                if let Some(index) = nested_layout
                    .entry_capture_slots
                    .iter()
                    .position(|s| *s == returned)
                {
                    capture_sources[index]
                } else {
                    let index = returned
                        .checked_sub(nested_layout.entry_argument_start)
                        .ok_or(Refusal::Callee)? as usize;
                    *argument_sources.get(index).ok_or(Refusal::Callee)?
                },
            )
        } else {
            None
        };
        let window = layout.call_window_start.ok_or(Refusal::InvalidLayout)?;
        after_boxes.extend([window, slot(receiver)?, window + 1]);
        aliases[window as usize] = true;
        aliases[window as usize + 1] = false;
        for (index, argument) in arguments.iter().enumerate() {
            let destination = window + 2 + index as u32;
            after_boxes.extend([destination, slot(*argument)?]);
            aliases[destination as usize] = false;
        }
        after_boxes.push(call_site.0);
        let continuation_aliases = if let Some(edge) = &continuation {
            Some(transition::continuation_aliases(&aliases, edge)
                .ok_or(Refusal::InvalidLayout)?)
        } else {
            None
        };
        let mut plan = Self {
            node_index,
            literal_value,
            literal_slot,
            program: nested,
            capture_sources,
            argument_sources,
            return_source,
            selector: selector.to_vec(),
            call_site: call_site.0,
            callee,
            invocation_aliases: aliases,
            constructor_stores,
            call_window_start: window,
            continuation,
            continuation_aliases,
            before_constructor_boxes: before_boxes,
            after_constructor_boxes: after_boxes,
            following: Vec::new(),
        };
        plan.following = sequence::derive(
            program,
            layout,
            &plan,
            &values,
            &nested_layout,
            return_entry_slot(&plan.callee),
        );
        Ok(plan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Block, Expr, Statement, portable, resumable_c};
    use std::borrow::Cow;

    fn send(receiver: Expr<'static>, args: Vec<Expr<'static>>) -> Expr<'static> {
        Expr::Send {
            receiver: Box::new(receiver),
            selector: Cow::Borrowed(if args.is_empty() { b"value" } else { b"value:" }),
            args,
            source: b"",
        }
    }

    fn fixture(captured: bool, escape_after: bool) -> Vec<u8> {
        let inner = Block {
            params: vec![b"argument"],
            body: vec![
                Statement::Let {
                    name: b"local",
                    value: Expr::Global(if captured { b"saved" } else { b"argument" }),
                },
                Statement::Expr(Expr::Global(b"local")),
            ],
        };
        portable::encode_block(
            &Block {
                params: vec![b"input", b"argumentInput"],
                body: vec![
                    Statement::Let {
                        name: b"saved",
                        value: Expr::Global(b"input"),
                    },
                    Statement::Let {
                        name: b"closure",
                        value: Expr::Block(Box::new(inner)),
                    },
                    Statement::Let {
                        name: b"alias",
                        value: Expr::Global(b"closure"),
                    },
                    Statement::Let {
                        name: b"answer",
                        value: send(Expr::Global(b"alias"), vec![Expr::Global(b"argumentInput")]),
                    },
                    Statement::Expr(Expr::Global(if escape_after {
                        b"alias"
                    } else {
                        b"answer"
                    })),
                ],
            },
            &[],
        )
        .unwrap()
    }

    #[test]
    fn source_closure_substitution_and_recovery_match_canonical_frame_execution() {
        for captured in [false, true] {
            for escape_after in [false, true] {
                let bytes = fixture(captured, escape_after);
                let program = resumable::lower_portable_block(&bytes).unwrap();
                let layout = resumable::color_slots(&program).unwrap();
                let plan = Plan::derive(&program, &layout).unwrap();
                assert_eq!(plan.capture_sources.len(), usize::from(captured));
                assert_eq!(
                    plan.return_source,
                    Some(layout.entry_argument_start + u32::from(!captured))
                );
                assert_eq!(plan.argument_sources, [layout.entry_argument_start + 1]);
                assert_eq!(plan.callee.alias_count(), 1);

                // An independent physical execution of canonical operations and
                // V2 commit must identify precisely the slots needing recovery.
                // Distinct tokens represent original object identities.
                const CLOSURE: usize = 10_001;
                let mut words: Vec<_> = (1..=layout.slot_count as usize).collect();
                let inputs = words.clone();
                let mut constructor_entry = None;
                let input = words[plan.return_source.unwrap() as usize];
                for op in &program.blocks[0].operations {
                    let dest = layout.value_slots[op.result.0 as usize] as usize;
                    match &op.kind {
                        OperationKind::Bind { source, .. } => {
                            words[dest] = words[layout.value_slots[source.0 as usize] as usize]
                        }
                        OperationKind::PortableBlockLiteral { .. } => {
                            constructor_entry = Some(words.clone());
                            words[dest] = CLOSURE;
                        }
                        _ => {}
                    }
                }
                let mut substituted = constructor_entry.unwrap();
                for &(slot, value) in &plan.constructor_stores {
                    substituted[slot as usize] = match value {
                        Value::Input(s) => inputs[s as usize],
                        Value::Closure => CLOSURE,
                        Value::Result(_) => panic!("entry-only fixture"),
                    };
                }
                assert_eq!(
                    substituted, words,
                    "constructor continuation must perform every baseline store"
                );
                let Terminator::Call {
                    receiver,
                    arguments,
                    successor,
                    ..
                } = &program.blocks[0].terminator
                else {
                    panic!("call")
                };
                let window = layout.call_window_start.unwrap() as usize;
                words[window] = words[layout.value_slots[receiver.0 as usize] as usize];
                words[window + 1] = 20_001;
                for (index, argument) in arguments.iter().enumerate() {
                    words[window + 2 + index] =
                        words[layout.value_slots[argument.0 as usize] as usize];
                }
                assert_eq!(
                    plan.invocation_aliases,
                    words.iter().map(|v| *v == CLOSURE).collect::<Vec<_>>()
                );
                let edge = layout
                    .edges
                    .iter()
                    .find(|e| e.target == *successor)
                    .unwrap();
                let copied: Vec<_> = edge
                    .copies
                    .iter()
                    .map(|c| words[c.source as usize])
                    .collect();
                for (copy, value) in edge.copies.iter().zip(copied) {
                    words[copy.destination as usize] = value;
                }
                words[edge.result_initialization_slot.unwrap() as usize] = input;
                for (slot, value) in words.iter_mut().enumerate() {
                    if edge
                        .clear_bitmap_words
                        .get(slot / 64)
                        .is_some_and(|bits| bits & (1 << (slot % 64)) != 0)
                    {
                        *value = 0;
                    }
                }
                assert_eq!(
                    plan.continuation_aliases.as_ref().unwrap(),
                    &words.iter().map(|v| *v == CLOSURE).collect::<Vec<_>>()
                );
                assert_eq!(plan.continuation.as_ref(), Some(edge));
                if escape_after {
                    assert!(
                        plan.continuation_aliases
                            .as_ref()
                            .unwrap()
                            .iter()
                            .any(|v| *v)
                    );
                }

                // Compare allocation order against actual baseline C emission,
                // not a second hand-written index-count formula.
                for owner in [false, true] {
                    let source = resumable_c::emit_step_method_with_portable_blocks(
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
                            retain_materialization_owner: owner,
                        }],
                    )
                    .unwrap();
                    let source = String::from_utf8(source).unwrap();
                    let pc = source
                        .split("case 0L:{")
                        .nth(1)
                        .unwrap()
                        .split("case 1L:{")
                        .next()
                        .unwrap();
                    let (before, after) = pc
                        .split_once(if owner {
                            "=birnam_portable_materialized_block_new_v2("
                        } else {
                            "=birnam_portable_block_new_v1("
                        })
                        .unwrap();
                    let indices = |text: &str| {
                        text.split("MKV(")
                            .skip(1)
                            .map(|s| s.split('L').next().unwrap().parse::<u32>().unwrap())
                            .collect::<Vec<_>>()
                    };
                    assert_eq!(plan.before_constructor_boxes, indices(before));
                    assert_eq!(plan.after_constructor_boxes, indices(after));
                }
            }
        }
    }

    #[test]
    fn captured_aliases_retain_shared_input_and_distinct_literal_occurrences() {
        let mut plans = Vec::new();
        for extra_alias in [false, true] {
            let mut body = vec![
                Statement::Let {
                    name: b"a",
                    value: Expr::Global(b"input"),
                },
                Statement::Let {
                    name: b"b",
                    value: Expr::Global(b"input"),
                },
            ];
            if extra_alias {
                body.push(Statement::Let {
                    name: b"extra",
                    value: Expr::Global(b"input"),
                });
            }
            body.push(Statement::Expr(send(
                Expr::Block(Box::new(Block {
                    params: vec![],
                    body: vec![
                        Statement::Let {
                            name: b"first",
                            value: Expr::Global(b"a"),
                        },
                        Statement::Expr(Expr::Global(b"b")),
                    ],
                })),
                vec![],
            )));
            let bytes = portable::encode_block(
                &Block {
                    params: vec![b"input"],
                    body,
                },
                &[],
            )
            .unwrap();
            let program = resumable::lower_portable_block(&bytes).unwrap();
            let layout = resumable::color_slots(&program).unwrap();
            let plan = Plan::derive(&program, &layout).unwrap();
            assert_eq!(plan.capture_sources, [layout.entry_argument_start; 2]);
            plans.push(plan);
        }
        assert_eq!(plans[0].program, plans[1].program);
        assert_ne!(plans[0].node_index, plans[1].node_index);
    }

    #[test]
    fn source_closure_analysis_refuses_observation_and_identity_arguments() {
        let observer = send(Expr::Global(b"saved"), vec![]);
        for inner in [
            observer,
            Expr::Nil,
            Expr::Block(Box::new(Block {
                params: vec![],
                body: vec![Statement::Expr(Expr::Nil)],
            })),
        ] {
            let bytes = portable::encode_block(
                &Block {
                    params: vec![b"saved"],
                    body: vec![Statement::Expr(send(
                        Expr::Block(Box::new(Block {
                            params: vec![],
                            body: vec![Statement::Expr(inner)],
                        })),
                        vec![],
                    ))],
                },
                &[],
            )
            .unwrap();
            let program = resumable::lower_portable_block(&bytes).unwrap();
            assert_eq!(
                Plan::derive(&program, &resumable::color_slots(&program).unwrap()),
                Err(Refusal::Callee)
            );
        }
        let bytes = portable::encode_block(
            &Block {
                params: vec![],
                body: vec![
                    Statement::Let {
                        name: b"closure",
                        value: Expr::Block(Box::new(Block {
                            params: vec![b"arg"],
                            body: vec![Statement::Expr(Expr::Global(b"arg"))],
                        })),
                    },
                    Statement::Expr(send(
                        Expr::Global(b"closure"),
                        vec![Expr::Global(b"closure")],
                    )),
                ],
            },
            &[],
        )
        .unwrap();
        let program = resumable::lower_portable_block(&bytes).unwrap();
        assert_eq!(
            Plan::derive(&program, &resumable::color_slots(&program).unwrap()),
            Err(Refusal::IdentityExposure)
        );
        let bytes = fixture(true, false);
        let program = resumable::lower_portable_block(&bytes).unwrap();
        let mut layout = resumable::color_slots(&program).unwrap();
        layout.edges[0].clear_bitmap_words[0] ^= 1;
        assert_eq!(Plan::derive(&program, &layout), Err(Refusal::InvalidLayout));
    }

    #[test]
    fn invocation_spelling_has_no_authority_and_wrong_arity_declines() {
        let bytes = fixture(true, false);
        let mut program = resumable::lower_portable_block(&bytes).unwrap();
        let Terminator::Call { selector, .. } = &mut program.blocks[0].terminator else {
            panic!("call")
        };
        *selector = b"anotherMeaning:";
        let plan = Plan::derive(&program, &resumable::color_slots(&program).unwrap()).unwrap();
        assert_eq!(plan.selector, b"anotherMeaning:");
        let Terminator::Call { arguments, .. } = &mut program.blocks[0].terminator else {
            panic!("call")
        };
        arguments.clear();
        assert_eq!(
            Plan::derive(&program, &resumable::color_slots(&program).unwrap()),
            Err(Refusal::Callee)
        );
    }

    #[test]
    #[cfg(feature = "dev-pc-specialization")]
    fn literal_pc_specialization_retains_baseline_construction_and_validates_specs() {
        let bytes = fixture(true, true);
        let program = resumable::lower_portable_block(&bytes).unwrap();
        let layout = resumable::color_slots(&program).unwrap();
        let plan = Plan::derive(&program, &layout).unwrap();
        for owner in [false, true] {
            let spec = resumable_c::PortableBlockLiteralSpec {
                node_index: plan.node_index,
                disposition: resumable_c::BlockLiteralDisposition::Resumable {
                    manifest_entry_index: 7,
                },
                portable_program: plan.program.clone(),
                retain_materialization_owner: owner,
            };
            let all = resumable_c::emit_step_method_with_portable_blocks(
                5,
                &program,
                &layout,
                1,
                std::slice::from_ref(&spec),
            )
            .unwrap();
            let all = String::from_utf8(all).unwrap();
            for pc in 0..program.blocks.len() {
                let one = resumable_c::emit_pc_specialized_step_with_portable_blocks(
                    5,
                    &program,
                    &layout,
                    pc as u32,
                    std::slice::from_ref(&spec),
                )
                .unwrap();
                let one = String::from_utf8(one).unwrap();
                let body = one
                    .split_once("(void)args;\n{\n")
                    .unwrap()
                    .1
                    .strip_suffix("}\nreturn birnam_nil();\n}\n")
                    .unwrap();
                let original = all.split_once(&format!("case {pc}L:{{\n")).unwrap().1;
                assert!(
                    original.starts_with(body),
                    "PC {pc} must retain original body bytes"
                );
            }
            assert!(
                resumable_c::emit_pc_specialized_step_with_portable_blocks(
                    5,
                    &program,
                    &layout,
                    0,
                    &[]
                )
                .is_err()
            );
            let mut wrong = spec.clone();
            wrong.node_index += 1;
            assert!(
                resumable_c::emit_pc_specialized_step_with_portable_blocks(
                    5,
                    &program,
                    &layout,
                    0,
                    &[wrong]
                )
                .is_err()
            );
            let mut wrong = spec.clone();
            wrong.portable_program.push(0);
            assert!(
                resumable_c::emit_pc_specialized_step_with_portable_blocks(
                    5,
                    &program,
                    &layout,
                    0,
                    &[wrong]
                )
                .is_err()
            );
            assert!(
                resumable_c::emit_pc_specialized_step_with_portable_blocks(
                    5,
                    &program,
                    &layout,
                    0,
                    &[spec.clone(), spec]
                )
                .is_err()
            );
        }
    }
}

#[cfg(test)]
mod numeric_plan_tests;
