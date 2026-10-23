//! Scoped storage diagnostics over Id/Roots/reference-owner messages.
//! Observation roots protect the inventory without becoming declared graph roots.
use birnam_abi::{self as abi, Oop, borrowed_slice,
    id_protocol::{Literal, MethodSpec, Module, Protocol}};
use core::{
    cell::{Cell, RefCell},
    ptr,
};
use std::{
    collections::HashMap,
    sync::atomic::{AtomicBool, AtomicPtr, Ordering},
};

// Native stack roots are not inspected. Missing owner coverage is never guessed.
const NATIVE_ROOTS_UNOBSERVED: usize = 1;
const PAYLOAD_OWNER_MISSING: usize = 2;
#[derive(Clone, Copy)]
struct Allocation {
    object: Oop,
    bytes: usize,
    identity: usize,
}
#[derive(Default)]
struct Snapshot {
    allocations: Vec<Allocation>,
    indices: HashMap<usize, usize>,
    roots: Vec<Oop>,
    edges: Vec<(Oop, Oop)>,
    failure: Option<Oop>,
}
struct State {
    id: Protocol,
    names: HashMap<&'static [u8],Oop>,
    ready: Cell<bool>,
    invalid: Oop,
    nested: Oop,
    ns: Oop,
    roots: Oop,
    gc: Oop,
    receiver: Oop,
    sink_behavior: Oop,
    thread: std::thread::ThreadId,
    active: Cell<bool>,
    sink: Cell<Oop>,
    phase: Cell<usize>,
    source: Cell<Oop>,
    caller: Cell<Oop>,
    snapshot: RefCell<Snapshot>,
}
static INSTANCE: AtomicPtr<State> = AtomicPtr::new(ptr::null_mut());
static INITIALIZING: AtomicBool = AtomicBool::new(false);
static API: AtomicPtr<Protocol> = AtomicPtr::new(ptr::null_mut());
fn invalid() -> Oop {
    let raw=INSTANCE.load(Ordering::Acquire);
    if !raw.is_null() { return unsafe { (*raw).invalid }; }
    let api=API.load(Ordering::Acquire);
    if api.is_null() { std::process::abort(); }
    unsafe { (*api).invalid }
}
fn object(v: Oop) -> Result<Oop,Oop> {
    if v.is_null() { Err(invalid()) } else { Ok(v) }
}
unsafe fn state<'a>(c: Oop) -> Result<&'a State,Oop> {
    let raw=INSTANCE.load(Ordering::Acquire);
    if raw.is_null() { return Err(invalid()); }
    let s=unsafe { &*raw };
    if c==s.receiver && s.thread==std::thread::current().id() { Ok(s) } else { Err(invalid()) }
}
impl State {
    unsafe fn send(&self, r: Oop, name: &[u8], args: &[Oop]) -> Result<Oop, Oop> {
        unsafe { self.id.send(r, self.names[name], args) }
    }
    fn checked(&self) -> Result<(), Oop> {
        self.snapshot.borrow().failure.map_or(Ok(()), Err)
    }
}
struct Observation<'a>(&'a State);
impl Drop for Observation<'_> {
    fn drop(&mut self) {
        self.0.phase.set(0);
        self.0.sink.set(ptr::null_mut());
        self.0.caller.set(ptr::null_mut());
        *self.0.snapshot.borrow_mut() = Snapshot::default();
        self.0.active.set(false);
    }
}
unsafe extern "C" fn roots(_id: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let Some([v]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let v = object(*v)?;
        let mut values = vec![
            s.ns,
            s.receiver,
            s.sink_behavior,
            s.caller.get(),
            s.sink.get(),
            s.invalid,
            s.nested,
        ];
        // Only our diagnostic root sink omits the temporary inventory roots.
        // A collection (including one invoked by a provider) sees every object.
        if v != s.sink.get() {
            values.extend(s.snapshot.borrow().allocations.iter().map(|a| a.object));
        }
        values.extend(s.names.values().copied());
        for value in values.into_iter().filter(|o| !o.is_null()) {
            s.send(v, b"visit:", &[value])?;
        }
        Ok(ptr::null_mut())
    })()
    .unwrap_or_else(abi::fault_reference)
}
unsafe fn receive(
    c: Oop,
    r: Oop,
    a: *const Oop,
    n: usize,
    allocation: bool,
) -> Result<Oop, Oop> {
    unsafe {
        let s = state(c)?;
        let result = (|| {
            if r != s.sink.get() || !s.active.get() {
                return Err(invalid());
            }
            let args = borrowed_slice(a, n).ok_or_else(invalid)?;
            let mut data = s.snapshot.borrow_mut();
            if allocation {
                if s.phase.get() != 1 {
                    return Err(invalid());
                }
                let [value, bytes, identity] = args else {
                    return Err(invalid());
                };
                let allocation = Allocation {
                    object: object(*value)?,
                    bytes: abi::as_size(*bytes).ok_or_else(invalid)?,
                    identity: abi::as_size(*identity).filter(|n| *n != 0).ok_or_else(invalid)?,
                };
                let index = data.allocations.len();
                if data
                    .indices
                    .insert(allocation.object.addr(), index)
                    .is_some()
                {
                    return Err(invalid());
                }
                data.allocations.push(allocation);
            } else {
                let [value] = args else {
                    return Err(invalid());
                };
                let value = abi::object_reference(*value);
                if value.is_null() || abi::as_size(value).is_some() {
                    return Ok(ptr::null_mut());
                }
                if !data.indices.contains_key(&value.addr()) {
                    return Err(invalid());
                }
                match s.phase.get() {
                    2 => data.roots.push(value),
                    3 => data.edges.push((s.source.get(), value)),
                    _ => return Err(invalid()),
                }
            }
            Ok(ptr::null_mut())
        })();
        // A provider cannot hide a bad callback by ignoring its failure.
        if let Err(e) = result
            && r == s.sink.get()
        {
            s.snapshot.borrow_mut().failure.get_or_insert(e);
        }
        result
    }
}
unsafe extern "C" fn allocation(_id: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { receive(c, r, a, n, true) }.unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn visit(_id: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { receive(c, r, a, n, false) }.unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn observe(_id: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let Some([caller]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let caller = object(*caller)?;
        if s.active.replace(true) {
            return Err(s.nested);
        }
        let _guard = Observation(s);
        s.caller.set(caller);
        // A fresh receiver makes retained callback objects fail closed forever.
        let sink = object(s.send(s.sink_behavior, b"allocate", &[abi::size(0).unwrap()])?)?;
        s.sink.set(sink);
        let visitor = sink;
        s.phase.set(1);
        s.send(s.id.root, b"allocationsDo:", &[visitor])?;
        s.checked()?;
        let mut allocations = s.snapshot.borrow().allocations.clone();
        allocations.sort_unstable_by_key(|a| a.identity);
        if allocations.windows(2).any(|pair| pair[0].identity == pair[1].identity) {
            return Err(invalid());
        }
        s.phase.set(2);
        s.send(s.id.root, b"rootsDo:", &[visitor])?;
        s.send(s.roots, b"rootsDo:", &[visitor])?;
        s.checked()?;
        let mut gaps = NATIVE_ROOTS_UNOBSERVED;
        s.phase.set(3);
        for allocation in &allocations {
            let value = allocation.object;
            s.source.set(value);
            let answer=s.send(s.id.root,b"referencesOf:do:",&[value,visitor])?;
            if !answer.is_null() { return Err(invalid()); }
            let behavior=s.send(s.id.root,b"behaviorOf:",&[value])?;
            let owner=s.send(s.gc,b"referenceOwnerOf:",&[behavior])?;
            if !owner.is_null() {
                receive(c,sink,&owner,1,false)?;
                s.send(owner,b"referencesOf:do:",&[value,visitor])?;
            } else if allocation.bytes != 0 { gaps |= PAYLOAD_OWNER_MISSING; }
            s.checked()?;
        }
        s.phase.set(0);
        let (roots, edges) = {
            let mut data = s.snapshot.borrow_mut();
            // The retained allocation inventory protects all endpoints while
            // callbacks consume these finished lists. No second copy is needed.
            (core::mem::take(&mut data.roots), core::mem::take(&mut data.edges))
        };
        for a in allocations {
            s.send(
                caller,
                b"allocation:bytes:identity:",
                &[
                    a.object,
                    abi::size(a.bytes).ok_or_else(invalid)?,
                    abi::size(a.identity).ok_or_else(invalid)?,
                ],
            )?;
        }
        for root in roots {
            s.send(caller, b"root:", &[root])?;
        }
        for (from, to) in edges {
            s.send(
                caller,
                b"edgeFrom:to:",
                &[from, to],
            )?;
        }
        let result = s.send(caller, b"complete:", &[abi::size(gaps).unwrap()])?;
        s.checked()?;
        Ok(result)
    })()
    .unwrap_or_else(abi::fault_reference)
}
macro_rules! literals { ($($name:literal),* $(,)?) => {
    const NAMES: &[&[u8]] = &[$($name),*];
    static LITERALS: &[Literal] = &[$(Literal::new($name)),*];
}; }
literals![b"at:",b"at:put:",b"Roots",b"GC",b"Storage",b"objectBehavior",b"delegated",b"allocate",
    b"methodAt:",b"withContext:",b"addMethod",b"retain:",b"release:",b"register:",b"observe:",
    b"rootsDo:",b"allocation:bytes:identity:",b"visit:",b"allocationsDo:",b"referencesOf:do:",
    b"referenceOwnerOf:",b"root:",b"edgeFrom:to:",b"complete:",b"behaviorOf:",b"faultKind",b"fault:detail:"];
const fn index(name:&[u8])->usize {
    let mut i=0;while i<NAMES.len(){if NAMES[i].len()==name.len(){let mut j=0;
        while j<name.len() && NAMES[i][j]==name[j]{j+=1;}if j==name.len(){return i;}}i+=1;}
    panic!("missing literal")
}
static METHODS:&[MethodSpec]=&[
    MethodSpec{name:index(b"observe:"),entry:Some(observe)},
    MethodSpec{name:index(b"rootsDo:"),entry:Some(roots)},
    MethodSpec{name:index(b"allocation:bytes:identity:"),entry:Some(allocation)},
    MethodSpec{name:index(b"visit:"),entry:Some(visit)},
];
unsafe fn construct(id:Protocol,image:Oop,symbols:&[Oop],ns:Oop)->Result<Oop,Oop>{
    unsafe {
        let names:HashMap<_,_>=NAMES.iter().copied().zip(symbols.iter().copied()).collect();
        let send=|r,n:&[u8],a:&[Oop]|id.send(r,names[n],a);
        let roots=object(send(ns,b"at:",&[names[b"Roots".as_slice()]])?)?;
        let gc=object(send(ns,b"at:",&[names[b"GC".as_slice()]])?)?;
        let base=send(id.root,b"objectBehavior",&[])?;
        let behavior=send(base,b"delegated",&[])?;
        let receiver=send(behavior,b"allocate",&[abi::size(0).unwrap()])?;
        let receiver_token=send(roots,b"retain:",&[receiver])?;
        let sink_behavior=send(base,b"delegated",&[])?;
        let sink_token=send(roots,b"retain:",&[sink_behavior])?;
        let kind=send(id.invalid,b"faultKind",&[])?;
        let invalid=abi::fault_reference(send(id.root,b"fault:detail:",&[kind,abi::size(1).unwrap()]).err().ok_or(id.invalid)?);
        let error_token=send(roots,b"retain:",&[invalid])?;
        let nested=abi::fault_reference(send(id.root,b"fault:detail:",&[kind,abi::size(2).unwrap()]).err().ok_or(id.invalid)?);
        let nested_token=send(roots,b"retain:",&[nested])?;
        let s=Box::into_raw(Box::new(State {id,names,ready:Cell::new(false),invalid,nested,ns,roots,gc,receiver,sink_behavior,
            thread:std::thread::current().id(),active:Cell::new(false),sink:Cell::new(ptr::null_mut()),
            phase:Cell::new(0),source:Cell::new(ptr::null_mut()),caller:Cell::new(ptr::null_mut()),
            snapshot:RefCell::new(Snapshot::default())}));
        INSTANCE.store(s,Ordering::Release);
        let s=&*s;
        let result=(||{
            for spec in METHODS {
                let m=s.send(image,b"methodAt:",&[symbols[spec.name]])?;
                let m=s.send(m,b"withContext:",&[receiver])?;
                let target=if spec.name==index(b"observe:") || spec.name==index(b"rootsDo:"){behavior}else{sink_behavior};
                s.send(target,b"addMethod",&[symbols[spec.name],m])?;
            }
            s.send(roots,b"register:",&[receiver])?;
            s.send(ns,b"at:put:",&[s.names[b"Storage".as_slice()],receiver])?;
            s.ready.set(true);
            Ok(receiver)
        })();
        for token in [receiver_token,sink_token,error_token,nested_token]{s.send(roots,b"release:",&[token])?;}
        result
    }
}
unsafe extern "C" fn initialize(p:*const Protocol,image:Oop,l:*const Oop,n:usize,a:*const Oop,argc:usize)->Oop{
    API.store(p.cast_mut(),Ordering::Release);
    let id=unsafe{*p};
    let Some([ns])=(unsafe{borrowed_slice(a,argc)})else{return id.invalid;};
    let Some(symbols)=(unsafe{borrowed_slice(l,n)})else{return id.invalid;};
    if symbols.len()!=NAMES.len(){return id.invalid;}
    let old=INSTANCE.load(Ordering::Acquire);
    if !old.is_null(){return if unsafe{(*old).ready.get() && (*old).ns==*ns}{unsafe{(*old).receiver}}else{invalid()};}
    if INITIALIZING.swap(true,Ordering::AcqRel){return invalid();}
    let answer=unsafe{construct(id,image,symbols,*ns)};
    INITIALIZING.store(false,Ordering::Release);
    answer.unwrap_or_else(abi::fault_reference)
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v2:Module=Module{version:2,byte_size:size_of::<Module>() as u32,
    literals:LITERALS.as_ptr(),literal_count:LITERALS.len(),methods:METHODS.as_ptr(),method_count:METHODS.len(),initialize:Some(initialize)};
