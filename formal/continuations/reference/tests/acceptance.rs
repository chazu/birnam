#[path = "../src/scenarios.rs"]
mod scenarios;
use continuation_id::{Call, Fault, Id, NIL, Stop, Transfer};
use scenarios::{Machine, Scenario, finish, result};
use std::{cell::Cell, rc::Rc};

#[test]
fn suspending_metacircular_lookup_and_native_callback() {
    for asynchronous in [false, true] {
        let mut s = Scenario::new(asynchronous, true);
        let start = s.start();
        assert_eq!(finish(&mut s.id, start), Ok(s.answer));
        let c = s.counts.borrow();
        assert_eq!(c.helper_calls, 2);
        assert_eq!(c.native_returns, 1);
        assert!(c.pauses > 0);
        assert_eq!(c.overlapping_drives_refused, usize::from(!asynchronous));
        assert_eq!(c.denied > 0, !asynchronous);
        assert!(s.id.peak_entry_depth() <= if asynchronous { 1 } else { 2 });
    }
}
#[test]
fn selected_method_is_entered_despite_binding_and_method_behavior_changes() {
    let mut s = Scenario::new(true, false);
    let mut next = s.start();
    loop {
        if let Transfer::Enter { method, ref call } = next {
            if method == s.native && call.receiver == s.object {
                break;
            }
        }
        next = match s.id.run(next, 1).unwrap() {
            Stop::Budget(next) => next,
            other => panic!("{other:?}"),
        };
    }
    let replacement = s.id.declare(
        "replacement",
        NIL,
        NIL,
        Rc::new(|_, _| panic!("reselected")),
    );
    s.id.bind(s.table, s.work, replacement).unwrap();
    let poison = s.id.vtable(s.id.meta(), "poison");
    let fail = s.id.declare(
        "poison-lookup",
        NIL,
        NIL,
        Rc::new(|_, _| panic!("lookup on selected Method")),
    );
    s.id.bind(poison, s.id.lookup(), fail).unwrap();
    let meta = s.id.vtable(poison, "method-vtable");
    s.id.set_vtable(s.native, meta).unwrap();
    assert_eq!(finish(&mut s.id, next), Ok(s.answer));
}
#[test]
fn bootstrap_lookup_binding_is_live() {
    let mut s = Scenario::new(true, false);
    let old = s.id.default_lookup();
    let hit = Rc::new(Cell::new(0));
    let count = hit.clone();
    let selector = s.id.lookup();
    let replacement = s.id.declare(
        "lookup1",
        NIL,
        NIL,
        Rc::new(move |_, i| {
            count.set(count.get() + 1);
            Ok(Transfer::Enter {
                method: old,
                call: Call {
                    receiver: i.receiver,
                    selector,
                    arguments: i.arguments,
                    continuation: i.continuation,
                },
            })
        }),
    );
    s.id.bind(s.id.meta(), selector, replacement).unwrap();
    let start = s.start();
    assert_eq!(finish(&mut s.id, start), Ok(s.answer));
    assert!(hit.get() > 0);
}
#[test]
fn scheduler_owns_two_interleaved_computations_and_wakeup_is_not_replayed() {
    let mut s = Scenario::new(true, true);
    let mut machines = [Machine::new(s.start()), Machine::new(s.start())];
    let mut turns = 0;
    while machines.iter().any(|m| m.answer.is_none()) {
        for m in &mut machines {
            m.tick(&mut s.id).unwrap();
            if m.parked {
                assert!(m.wake());
                assert!(!m.wake());
            }
            turns += 1;
            assert!(turns < 1000);
        }
    }
    assert_eq!(machines[0].answer, Some(s.answer));
    assert_eq!(machines[1].answer, Some(s.answer));
    assert_eq!(s.counts.borrow().helper_calls, 4);
    assert_eq!(s.counts.borrow().native_returns, 2);
}
#[test]
fn long_tail_chain_uses_one_native_entry_activation() {
    let mut id = Id::bootstrap();
    let table = id.vtable(id.meta(), "tail-table");
    let receiver = id.allocate(table, "tail-object");
    let selector = id.intern("tail");
    let stop = result(&mut id);
    let count = Rc::new(Cell::new(10_000));
    let n = count.clone();
    let method = id.declare(
        "tail",
        NIL,
        NIL,
        Rc::new(move |id, i| {
            if n.get() == 0 {
                Ok(id.answer(i.continuation, i.receiver))
            } else {
                n.set(n.get() - 1);
                Ok(Transfer::Send(Call {
                    receiver: i.receiver,
                    selector,
                    arguments: vec![],
                    continuation: i.continuation,
                }))
            }
        }),
    );
    id.bind(table, selector, method).unwrap();
    assert_eq!(
        finish(
            &mut id,
            Transfer::Send(Call {
                receiver,
                selector,
                arguments: vec![],
                continuation: stop
            })
        ),
        Ok(receiver)
    );
    assert_eq!(count.get(), 0);
    assert_eq!(id.peak_entry_depth(), 1);
}
#[test]
fn dnu_keeps_original_selector_and_arguments() {
    let mut id = Id::bootstrap();
    let table = id.vtable(id.meta(), "table");
    let receiver = id.allocate(table, "object");
    let missing = id.intern("missing");
    let arg = id.intern("arg");
    let dnu = id.intern("doesNotUnderstand:");
    let stop = result(&mut id);
    let method = id.declare(
        "dnu",
        NIL,
        NIL,
        Rc::new(move |id, i| {
            assert_eq!(i.arguments, [missing, arg]);
            Ok(id.answer(i.continuation, arg))
        }),
    );
    id.bind(table, dnu, method).unwrap();
    assert_eq!(
        finish(
            &mut id,
            Transfer::Send(Call {
                receiver,
                selector: missing,
                arguments: vec![arg],
                continuation: stop
            })
        ),
        Ok(arg)
    );
}
#[test]
fn asynchronous_child_cannot_escape_a_synchronous_native_ancestor() {
    let mut id = Id::bootstrap();
    let stop = result(&mut id);
    let child = id.declare(
        "child",
        NIL,
        NIL,
        Rc::new(|_, _| Ok(Transfer::Park(Box::new(Transfer::Halt(NIL))))),
    );
    let outer = id.declare(
        "outer",
        NIL,
        NIL,
        Rc::new(move |id, i| {
            let call = Call {
                receiver: NIL,
                selector: NIL,
                arguments: vec![],
                continuation: stop,
            };
            assert!(matches!(
                id.native_callback(Transfer::Enter {
                    method: child,
                    call
                }),
                Err(Fault::NativeBarrier)
            ));
            Ok(id.answer(i.continuation, NIL))
        }),
    );
    assert_eq!(
        finish(
            &mut id,
            Transfer::Enter {
                method: outer,
                call: Call {
                    receiver: NIL,
                    selector: NIL,
                    arguments: vec![],
                    continuation: stop
                }
            }
        ),
        Ok(NIL)
    );
    assert!(id.park(Transfer::Halt(NIL)).is_ok());
}

#[test]
fn continuation_oop_can_be_returned_as_ordinary_data() {
    let mut id = Id::bootstrap();
    let stop = result(&mut id);
    let next = id.answer(stop, stop);
    assert_eq!(finish(&mut id, next), Ok(stop));
}
#[test]
fn completion_ownership_and_consumption_belong_to_the_owner() {
    let mut id = Id::bootstrap();
    let owner = id.intern("owner");
    let foreign = id.intern("foreign");
    let answer = id.intern("answer");
    let used = Cell::new(false);
    let ticket = id.declare(
        "completion-ticket",
        owner,
        NIL,
        Rc::new(move |_, i| {
            if i.arguments[0] != i.context {
                return Err(Fault::InvalidOop);
            }
            if used.replace(true) {
                return Err(Fault::Consumed);
            }
            Ok(Transfer::Halt(i.arguments[1]))
        }),
    );
    let invoke = |caller| Transfer::Enter {
        method: ticket,
        call: Call {
            receiver: ticket,
            selector: NIL,
            arguments: vec![caller, answer],
            continuation: ticket,
        },
    };
    assert_eq!(finish(&mut id, invoke(foreign)), Err(Fault::InvalidOop));
    assert_eq!(finish(&mut id, invoke(owner)), Ok(answer));
    assert_eq!(finish(&mut id, invoke(owner)), Err(Fault::Consumed));
}
#[test]
fn later_sends_observe_replacement_without_rewriting_captured_entry() {
    let mut id = Id::bootstrap();
    let table = id.vtable(id.meta(), "table");
    let receiver = id.allocate(table, "object");
    let selector = id.intern("value");
    let old_value = id.intern("old");
    let new_value = id.intern("new");
    let stop = result(&mut id);
    let old = id.declare(
        "old-method",
        NIL,
        NIL,
        Rc::new(move |id, i| Ok(id.answer(i.continuation, old_value))),
    );
    let new = id.declare(
        "new-method",
        NIL,
        NIL,
        Rc::new(move |id, i| Ok(id.answer(i.continuation, new_value))),
    );
    let call = Call {
        receiver,
        selector,
        arguments: vec![],
        continuation: stop,
    };
    id.bind(table, selector, old).unwrap();
    assert_eq!(finish(&mut id, Transfer::Send(call.clone())), Ok(old_value));
    id.bind(table, selector, new).unwrap();
    assert_eq!(finish(&mut id, Transfer::Send(call.clone())), Ok(new_value));
    assert_eq!(
        finish(&mut id, Transfer::Enter { method: old, call }),
        Ok(old_value)
    );
}

#[test]
fn captured_continuation_can_choose_live_object_behavior_when_resumed() {
    let mut id = Id::bootstrap();
    let table = id.vtable(id.meta(), "continuation-owner");
    let receiver = id.allocate(table, "continuation-object");
    let selector = id.intern("resume:");
    let answer = id.intern("new-answer");
    let stop = result(&mut id);
    let old = id.declare(
        "old-resume",
        NIL,
        NIL,
        Rc::new(|_, _| panic!("frozen continuation behavior")),
    );
    id.bind(table, selector, old).unwrap();
    let continuation = id.declare(
        "forward-resume",
        receiver,
        NIL,
        Rc::new(move |_, i| {
            Ok(Transfer::Send(Call {
                receiver: i.context,
                selector,
                arguments: i.arguments,
                continuation: stop,
            }))
        }),
    );
    let replacement = id.declare(
        "new-resume",
        NIL,
        NIL,
        Rc::new(move |id, i| Ok(id.answer(i.continuation, answer))),
    );
    id.bind(table, selector, replacement).unwrap();
    let next = id.answer(continuation, NIL);
    assert_eq!(finish(&mut id, next), Ok(answer));
}
