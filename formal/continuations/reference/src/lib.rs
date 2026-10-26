//! Executable semantic reference, not the production Id or a frozen native ABI.
//! Runtime values are Oops. Transfer is explicit control, never an Oop payload
//! tag. The binary/test owners are separate Rust consumers of this public API.
use std::{collections::BTreeMap, rc::Rc};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Oop(usize);
pub const NIL: Oop = Oop(0);

#[derive(Clone, Debug)]
pub struct Call {
    pub receiver: Oop,
    pub selector: Oop,
    pub arguments: Vec<Oop>,
    pub continuation: Oop,
}
#[derive(Clone)]
pub struct Invocation {
    pub context: Oop,
    pub receiver: Oop,
    pub arguments: Vec<Oop>,
    pub continuation: Oop,
}
/// Rust notation for the transfer contract. It is not a second language value
/// representation, an object catalog, or a proposed C return packet.
#[derive(Clone, Debug)]
pub enum Transfer {
    Send(Call),
    Enter { method: Oop, call: Call },
    Park(Box<Transfer>),
    Halt(Oop),
}
#[derive(Debug, PartialEq, Eq)]
pub enum Fault {
    NotPublished,
    InvalidOop,
    NotMethod,
    MissingMethod,
    Arity,
    AlreadyRunning,
    NativeBarrier,
    NoNativeCaller,
    BudgetInNative,
    Consumed,
}
pub type Result<T> = std::result::Result<T, Fault>;
pub type Entry = Rc<dyn Fn(&mut Id, Invocation) -> Result<Transfer>>;
#[derive(Debug)]
pub enum Stop {
    Complete(Oop),
    Budget(Transfer),
    Parked(Transfer),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntryEvent {
    pub method: String,
    pub receiver: String,
    pub selector: String,
    pub arguments: Vec<String>,
}

// Generic object storage knows only the vtable. Owner state is deliberately
// elsewhere. This reference retains allocations; GC/address reuse is not modeled.
struct Object {
    vtable: Oop,
}
#[derive(Clone)]
struct Method {
    context: Oop,
    code_owner: Oop,
    entry: Entry,
}
#[derive(Default)]
struct Methods(BTreeMap<Oop, Method>);
impl Methods {
    fn selected(&self, value: Oop) -> Result<Method> {
        self.0.get(&value).cloned().ok_or(Fault::NotMethod)
    }
}
#[derive(Default)]
struct Tables(BTreeMap<Oop, BTreeMap<Oop, Oop>>);
impl Tables {
    fn lookup(&self, receiver: Oop, selector: Oop) -> Oop {
        self.0
            .get(&receiver)
            .and_then(|t| t.get(&selector))
            .copied()
            .unwrap_or(NIL)
    }
}

pub struct Id {
    objects: Vec<Object>,
    labels: BTreeMap<Oop, String>,
    symbols: BTreeMap<String, Oop>,
    tables: Tables,
    methods: Methods,
    meta: Oop,
    method_behavior: Oop,
    lookup: Oop,
    answer: Oop,
    dnu: Oop,
    published: bool,
    driving: bool,
    entry_depth: usize,
    native_depth: usize,
    peak_entry_depth: usize,
    trace: Vec<EntryEvent>,
}
impl Id {
    pub fn bootstrap() -> Self {
        Self::seed(true)
    }
    fn seed(publish: bool) -> Self {
        let mut id = Self {
            objects: vec![Object { vtable: NIL }],
            labels: BTreeMap::new(),
            symbols: BTreeMap::new(),
            tables: Tables::default(),
            methods: Methods::default(),
            meta: NIL,
            method_behavior: NIL,
            lookup: NIL,
            answer: NIL,
            dnu: NIL,
            published: false,
            driving: false,
            entry_depth: 0,
            native_depth: 0,
            peak_entry_depth: 0,
            trace: Vec::new(),
        };
        id.labels.insert(NIL, "nil".into());
        id.meta = id.allocate(NIL, "2");
        id.objects[id.meta.0].vtable = id.meta;
        id.tables.0.insert(id.meta, BTreeMap::new());
        id.method_behavior = id.vtable(id.meta, "method-behavior");
        id.lookup = id.intern("lookup");
        id.answer = id.intern("answer");
        id.dnu = id.intern("doesNotUnderstand:");
        let lookup = id.declare(
            "lookup0",
            NIL,
            NIL,
            Rc::new(|id, invocation| {
                let [selector] = invocation.arguments.as_slice() else {
                    return Err(Fault::Arity);
                };
                let selected = id.tables.lookup(invocation.receiver, *selector);
                Ok(id.answer(invocation.continuation, selected))
            }),
        );
        id.bind(id.meta, id.lookup, lookup).unwrap();
        id.published = publish;
        id
    }
    pub fn meta(&self) -> Oop {
        self.meta
    }
    pub fn lookup(&self) -> Oop {
        self.lookup
    }
    pub fn default_lookup(&self) -> Oop {
        self.tables.lookup(self.meta, self.lookup)
    }
    pub fn label(&self, value: Oop) -> &str {
        self.labels.get(&value).map(String::as_str).unwrap_or("?")
    }
    pub fn trace(&self) -> &[EntryEvent] {
        &self.trace
    }
    pub fn peak_entry_depth(&self) -> usize {
        self.peak_entry_depth
    }
    pub fn clear_trace(&mut self) {
        self.trace.clear();
    }
    pub fn valid(&self, value: Oop) -> bool {
        value.0 < self.objects.len()
    }
    // Construction of the native fixture graph. No foreign owner layout is exposed.
    pub fn allocate(&mut self, vtable: Oop, label: &str) -> Oop {
        let value = Oop(self.objects.len());
        self.objects.push(Object { vtable });
        self.labels.insert(value, label.into());
        value
    }
    pub fn vtable(&mut self, meta: Oop, label: &str) -> Oop {
        let value = self.allocate(meta, label);
        self.tables.0.insert(value, BTreeMap::new());
        value
    }
    pub fn intern(&mut self, name: &str) -> Oop {
        if let Some(value) = self.symbols.get(name) {
            return *value;
        }
        let value = self.allocate(self.meta, name);
        self.symbols.insert(name.into(), value);
        value
    }
    /// Native code declaration for this Rust reference. This is metadata supplied
    /// by the owner, not a code pointer transported in a language message.
    pub fn declare(&mut self, label: &str, context: Oop, code_owner: Oop, entry: Entry) -> Oop {
        let value = self.allocate(self.method_behavior, label);
        self.methods.0.insert(
            value,
            Method {
                context,
                code_owner,
                entry,
            },
        );
        value
    }
    pub fn bind(&mut self, table: Oop, selector: Oop, method: Oop) -> Result<()> {
        if !self.valid(selector) || !self.valid(method) {
            return Err(Fault::InvalidOop);
        }
        self.tables
            .0
            .get_mut(&table)
            .ok_or(Fault::InvalidOop)?
            .insert(selector, method);
        Ok(())
    }
    pub fn set_vtable(&mut self, value: Oop, table: Oop) -> Result<()> {
        if !self.valid(table) {
            return Err(Fault::InvalidOop);
        }
        self.objects
            .get_mut(value.0)
            .ok_or(Fault::InvalidOop)?
            .vtable = table;
        Ok(())
    }
    /// A continuation is an already captured Method Oop. Direct entry avoids
    /// recursively needing lookup in order to return the result of lookup.
    /// Its context is interpreted only by that Method's owner.
    pub fn answer(&self, continuation: Oop, value: Oop) -> Transfer {
        Transfer::Enter {
            method: continuation,
            call: Call {
                receiver: continuation,
                selector: self.answer,
                arguments: vec![value],
                continuation,
            },
        }
    }
    pub fn park(&self, next: Transfer) -> Result<Transfer> {
        if self.native_depth > 0 {
            Err(Fault::NativeBarrier)
        } else {
            Ok(Transfer::Park(Box::new(next)))
        }
    }
    pub fn run(&mut self, transfer: Transfer, budget: usize) -> Result<Stop> {
        if !self.published {
            return Err(Fault::NotPublished);
        }
        if self.driving {
            return Err(Fault::AlreadyRunning);
        }
        self.driving = true;
        let result = self.drive(transfer, budget);
        self.driving = false;
        result
    }
    /// Explicit synchronous native callback. It belongs to the running drive,
    /// and must settle before returning to the waiting native activation.
    pub fn native_callback(&mut self, transfer: Transfer) -> Result<Oop> {
        if !self.driving || self.entry_depth == 0 {
            return Err(Fault::NoNativeCaller);
        }
        self.native_depth += 1;
        let result = self.drive(transfer, usize::MAX);
        self.native_depth -= 1;
        match result? {
            Stop::Complete(v) => Ok(v),
            Stop::Parked(_) => Err(Fault::NativeBarrier),
            Stop::Budget(_) => Err(Fault::BudgetInNative),
        }
    }
    fn drive(&mut self, mut transfer: Transfer, budget: usize) -> Result<Stop> {
        for _ in 0..budget {
            transfer = match transfer {
                Transfer::Halt(value) => {
                    if !self.valid(value) {
                        return Err(Fault::InvalidOop);
                    }
                    return Ok(Stop::Complete(value));
                }
                Transfer::Park(next) => {
                    if self.native_depth > 0 {
                        return Err(Fault::NativeBarrier);
                    }
                    return Ok(Stop::Parked(*next));
                }
                Transfer::Send(call) => self.resolve(call)?,
                Transfer::Enter { method, call } => {
                    let selected = self.methods.selected(method)?;
                    for value in [
                        selected.context,
                        selected.code_owner,
                        call.receiver,
                        call.continuation,
                    ]
                    .into_iter()
                    .chain(call.arguments.iter().copied())
                    {
                        if !self.valid(value) {
                            return Err(Fault::InvalidOop);
                        }
                    }
                    self.trace.push(EntryEvent {
                        method: self.label(method).into(),
                        receiver: self.label(call.receiver).into(),
                        selector: self.label(call.selector).into(),
                        arguments: call
                            .arguments
                            .iter()
                            .map(|&v| self.label(v).into())
                            .collect(),
                    });
                    self.entry_depth += 1;
                    self.peak_entry_depth = self.peak_entry_depth.max(self.entry_depth);
                    let result = (selected.entry)(
                        self,
                        Invocation {
                            context: selected.context,
                            receiver: call.receiver,
                            arguments: call.arguments,
                            continuation: call.continuation,
                        },
                    );
                    self.entry_depth -= 1;
                    result?
                }
            };
        }
        Ok(Stop::Budget(transfer))
    }
    fn resolve(&mut self, original: Call) -> Result<Transfer> {
        let table = self
            .objects
            .get(original.receiver.0)
            .ok_or(Fault::InvalidOop)?
            .vtable;
        if original.receiver == self.meta && original.selector == self.lookup {
            let method = self.tables.lookup(table, self.lookup);
            if method == NIL {
                return Err(Fault::MissingMethod);
            }
            return Ok(Transfer::Enter {
                method,
                call: original,
            });
        }
        let receiver = table;
        let selector = original.selector;
        let used = std::cell::Cell::new(false);
        let return_to_send = self.declare(
            "lookup-return",
            NIL,
            NIL,
            Rc::new(move |id, invocation| {
                if used.replace(true) {
                    return Err(Fault::Consumed);
                }
                let [method] = invocation.arguments.as_slice() else {
                    return Err(Fault::Arity);
                };
                if *method != NIL {
                    Ok(Transfer::Enter {
                        method: *method,
                        call: original.clone(),
                    })
                } else if original.selector == id.dnu || original.selector == id.lookup {
                    Err(Fault::MissingMethod)
                } else {
                    let mut arguments = vec![original.selector];
                    arguments.extend_from_slice(&original.arguments);
                    Ok(Transfer::Send(Call {
                        receiver: original.receiver,
                        selector: id.dnu,
                        arguments,
                        continuation: original.continuation,
                    }))
                }
            }),
        );
        Ok(Transfer::Send(Call {
            receiver,
            selector: self.lookup,
            arguments: vec![selector],
            continuation: return_to_send,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unavailable_before_publication() {
        let mut id = Id::seed(false);
        assert!(matches!(
            id.run(Transfer::Halt(NIL), 10),
            Err(Fault::NotPublished)
        ));
    }
}
