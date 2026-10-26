//! Independent consumers of the public reference Id, compiled in the binary and
//! integration-test crates. Execution state is never accessible through Id.
#![allow(dead_code)] // Shared executable/test fixture; each consumer uses a subset.
use continuation_id::{Call, Fault, Id, NIL, Oop, Result, Stop, Transfer};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

pub fn result(id: &mut Id) -> Oop {
    id.declare(
        "result",
        NIL,
        NIL,
        Rc::new(|_, invocation| {
            let [value] = invocation.arguments.as_slice() else {
                return Err(Fault::Arity);
            };
            Ok(Transfer::Halt(*value))
        }),
    )
}
pub fn finish(id: &mut Id, mut next: Transfer) -> Result<Oop> {
    loop {
        match id.run(next, 1)? {
            Stop::Complete(value) => return Ok(value),
            Stop::Budget(work) | Stop::Parked(work) => next = work,
        }
    }
}
fn enter(
    method: Oop,
    receiver: Oop,
    arguments: Vec<Oop>,
    continuation: Oop,
    selector: Oop,
) -> Transfer {
    Transfer::Enter {
        method,
        call: Call {
            receiver,
            selector,
            arguments,
            continuation,
        },
    }
}
#[derive(Default)]
pub struct Counters {
    pub helper_calls: usize,
    pub lookup_effects: usize,
    pub pauses: usize,
    pub denied: usize,
    pub native_returns: usize,
    pub overlapping_drives_refused: usize,
}
pub struct Scenario {
    pub id: Id,
    pub object: Oop,
    pub table: Oop,
    pub work: Oop,
    pub argument: Oop,
    pub answer: Oop,
    pub destination: Oop,
    pub native: Oop,
    pub counts: Rc<RefCell<Counters>>,
}
impl Scenario {
    pub fn new(asynchronous: bool, suspend_lookup: bool) -> Self {
        let mut id = Id::bootstrap();
        let meta = id.meta();
        let lookup_selector = id.lookup();
        let original_lookup = id.default_lookup();
        let table = id.vtable(meta, "1");
        let object = id.allocate(table, "0");
        let work = id.intern("work");
        let helper_selector = id.intern("helper");
        let argument = id.intern("argument");
        let answer = id.intern("answer-value");
        let destination = result(&mut id);
        let counts = Rc::new(RefCell::new(Counters::default()));
        let helper_counts = counts.clone();
        let helper = id.declare(
            "helperBody",
            NIL,
            NIL,
            Rc::new(move |id, i| {
                helper_counts.borrow_mut().helper_calls += 1;
                Ok(id.answer(i.continuation, answer))
            }),
        );
        id.bind(table, helper_selector, helper).unwrap();
        // Custom lookup calls source on the eventual target receiver with the
        // same argument count, then resumes the captured lookup implementation.
        let lookup_counts = counts.clone();
        let custom = id.declare(
            "custom-lookup",
            NIL,
            NIL,
            Rc::new(move |id, i| {
                lookup_counts.borrow_mut().lookup_effects += 1;
                let pending = i.clone();
                let stats = lookup_counts.clone();
                let read = id.declare(
                    "lookup-read",
                    NIL,
                    NIL,
                    Rc::new(move |_, _| {
                        Ok(enter(
                            original_lookup,
                            pending.receiver,
                            pending.arguments.clone(),
                            pending.continuation,
                            lookup_selector,
                        ))
                    }),
                );
                let next = id.answer(read, NIL);
                let resume = id.declare(
                    "after-lookup-helper",
                    NIL,
                    NIL,
                    Rc::new(move |id, _| {
                        if suspend_lookup {
                            match id.park(next.clone()) {
                                Ok(park) => {
                                    stats.borrow_mut().pauses += 1;
                                    Ok(park)
                                }
                                Err(Fault::NativeBarrier) => {
                                    stats.borrow_mut().denied += 1;
                                    Ok(next.clone())
                                }
                                Err(error) => Err(error),
                            }
                        } else {
                            Ok(next.clone())
                        }
                    }),
                );
                if i.receiver == table && i.arguments == [work] {
                    Ok(Transfer::Send(Call {
                        receiver: object,
                        selector: helper_selector,
                        arguments: vec![argument],
                        continuation: resume,
                    }))
                } else {
                    Ok(id.answer(resume, NIL))
                }
            }),
        );
        id.bind(meta, lookup_selector, custom).unwrap();
        let native_counts = counts.clone();
        let native = id.declare(
            "native0",
            NIL,
            NIL,
            Rc::new(move |id, i| {
                if i.arguments != [argument] {
                    return Err(Fault::Arity);
                }
                let destination = i.continuation;
                if asynchronous {
                    // This native activation returns before the loop parks and
                    // before the later source callback is entered.
                    native_counts.borrow_mut().native_returns += 1;
                    let used = Cell::new(false);
                    let complete = id.declare(
                        "async-complete",
                        NIL,
                        NIL,
                        Rc::new(move |id, i| {
                            if used.replace(true) {
                                return Err(Fault::Consumed);
                            }
                            Ok(id.answer(destination, i.arguments[0]))
                        }),
                    );
                    id.park(Transfer::Send(Call {
                        receiver: object,
                        selector: helper_selector,
                        arguments: vec![],
                        continuation: complete,
                    }))
                } else {
                    if matches!(id.run(Transfer::Halt(NIL), 1), Err(Fault::AlreadyRunning)) {
                        native_counts.borrow_mut().overlapping_drives_refused += 1;
                    }
                    let stop = result(id);
                    let value = id.native_callback(Transfer::Send(Call {
                        receiver: object,
                        selector: helper_selector,
                        arguments: vec![],
                        continuation: stop,
                    }))?;
                    native_counts.borrow_mut().native_returns += 1;
                    Ok(id.answer(destination, value))
                }
            }),
        );
        id.bind(table, work, native).unwrap();
        Self {
            id,
            object,
            table,
            work,
            argument,
            answer,
            destination,
            native,
            counts,
        }
    }
    pub fn start(&self) -> Transfer {
        Transfer::Send(Call {
            receiver: self.object,
            selector: self.work,
            arguments: vec![self.argument],
            continuation: self.destination,
        })
    }
}

// Execution chooses how to hold a pending computation; Scheduler chooses which
// machine to drive. The Id library knows neither of these representations.
pub struct Machine {
    next: Option<Transfer>,
    pub answer: Option<Oop>,
    pub parked: bool,
}
impl Machine {
    pub fn new(next: Transfer) -> Self {
        Self {
            next: Some(next),
            answer: None,
            parked: false,
        }
    }
    pub fn tick(&mut self, id: &mut Id) -> Result<()> {
        if self.parked {
            return Ok(());
        }
        let Some(next) = self.next.take() else {
            return Ok(());
        };
        match id.run(next, 1)? {
            Stop::Complete(value) => self.answer = Some(value),
            Stop::Budget(next) => self.next = Some(next),
            Stop::Parked(next) => {
                self.next = Some(next);
                self.parked = true;
            }
        }
        Ok(())
    }
    pub fn wake(&mut self) -> bool {
        if !self.parked {
            false
        } else {
            self.parked = false;
            true
        }
    }
}
