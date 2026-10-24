//! Experimental strict-value probes, compiled ONLY by `cargo test`.
//!
//! Reuses production ObjectMemory, Symbols, dictionaries, bootstrap and dispatch.
//! Existing Operand/Outcome glue remains around semantic test sends; timings
//! isolate the proposed value representations using the same production algorithms.
//! This is not a migrated ABI, production immediate support, or a GC/image proof.
use super::*;
use crate::id::initial_objects::InitialObjects;
use std::{ffi::CStr, hint::black_box};

fn size_oop(bytes: usize) -> Option<Oop> {
    (bytes <= usize::MAX >> 1).then(|| core::ptr::without_provenance_mut((bytes << 1) | 1))
}
fn size_value(value: Oop) -> Option<usize> {
    (value.addr() & 1 == 1).then(|| value.addr() >> 1)
}

struct Probe {
    initial: InitialObjects,
    name_behavior: Oop,
    size_behavior: Oop,
    intern_selector: Oop,
    allocate_selector: Oop,
    identity_selector: Oop,
}
impl Probe {
    fn new() -> Box<Self> {
        let initial = InitialObjects::new();
        let p = initial.protocol.protocol;
        let intern = |name: &[u8]| unsafe { p.intern_name(name).unwrap() };
        let delegated = intern(b"delegated");
        let behavior = || unsafe {
            p.send(initial.object_vt, delegated, &[]).unwrap().as_object().unwrap()
        };
        let name_behavior = behavior();
        let size_behavior = behavior();
        let intern_selector = intern(b"probeIntern");
        let allocate_selector = intern(b"probeAllocate:");
        let identity_selector = intern(b"probeIdentity");
        let mut probe = Box::new(Self { initial, name_behavior, size_behavior,
            intern_selector, allocate_selector, identity_selector });
        let context = core::ptr::from_mut(&mut *probe).cast();
        probe.install(name_behavior, intern_selector, strict_intern, context);
        probe.install(probe.initial.object_vt, allocate_selector, strict_allocate, context);
        probe.install(size_behavior, identity_selector, identity, context);
        probe
    }
    fn state(&self) -> &State { &self.initial.protocol }
    fn context(&self) -> id_dispatch::Context {
        let mut c = self.state().dispatch_context();
        c.size_probe_behavior = self.size_behavior;
        c
    }
    fn install(&self, behavior: Oop, selector: Oop, method: native::Method, context: *mut c_void) {
        let p = self.state().protocol;
        let add = unsafe { p.intern_name(b"addMethod").unwrap() };
        unsafe { p.send(behavior, add, &[Operand::object(selector),
            Operand::entry(NativeEntry::new(method, context))]).unwrap(); }
    }
    fn send(&self, receiver: Oop, selector: Oop, args: &[Operand]) -> Result<Operand, Outcome> {
        unsafe { id_dispatch::send(self.context(), receiver, selector, args).into_result() }
    }
    // One real Id allocation containing immutable length + inline name bytes.
    // The byte/name owner creates and interprets its own representation. There
    // is no borrowed native pointer in the value sent to the interner.
    fn name(&self, bytes: &[u8]) -> Oop {
        let memory = unsafe { &mut *self.state().memory };
        let object = memory.allocate(size_of::<usize>() + bytes.len());
        assert!(memory.set_vtable(object, self.name_behavior));
        unsafe {
            object.cast::<usize>().write(bytes.len());
            core::ptr::copy_nonoverlapping(bytes.as_ptr(),
                object.cast::<u8>().add(size_of::<usize>()), bytes.len());
        }
        object
    }
    fn release(&self, object: Oop) {
        assert!(unsafe { (&mut *self.state().memory).detach(object.addr()) }.is_some());
    }
    #[inline(always)]
    fn allocation(&self, bytes: usize) -> Option<Oop> {
        if !ObjectMemory::valid_payload_size(bytes) { return None; }
        let memory = unsafe { &mut *self.state().memory };
        let object = memory.allocate(bytes);
        assert!(memory.set_vtable(object, self.initial.object_vt));
        Some(object)
    }
    #[inline(never)]
    fn allocate_raw(&self, bytes: usize) -> Option<Oop> { self.allocation(bytes) }
    #[inline(never)]
    fn allocate_object(&self, size: Oop) -> Option<Oop> {
        self.allocation(size_value(size)?)
    }
    #[inline(never)]
    fn intern_raw(&self, bytes: &[u8]) -> Option<Oop> {
        let s = self.state();
        unsafe { (&mut *s.symbols).intern(&mut *s.memory, s.symbol_vtable, bytes).ok() }
    }
    #[inline(never)]
    fn intern_cstr(&self, bytes: &CStr) -> Option<Oop> {
        // Model a raw const-char* interface, including its length scan.
        let bytes = unsafe { CStr::from_ptr(black_box(bytes.as_ptr())) };
        self.intern_raw(bytes.to_bytes())
    }
    #[inline(never)]
    fn intern_object(&self, name: Oop) -> Option<Oop> {
        let bytes = {
            let memory = unsafe { &*self.state().memory };
            // Shape validation prevents an ordinary reassociated short object
            // or a non-Name from being read as an unbounded native buffer.
            let allocation = memory.get(name.addr())?;
            if allocation.requested_size() < size_of::<usize>() ||
                unsafe { ObjectMemory::vtable_of_valid_object(name) } != self.name_behavior {
                return None;
            }
            let len = unsafe { name.cast::<usize>().read() };
            if len != allocation.requested_size() - size_of::<usize>() { return None; }
            unsafe { core::slice::from_raw_parts(name.cast::<u8>().add(size_of::<usize>()), len) }
        };
        // The Id allocator stores each payload separately; adding a Symbol
        // cannot move Name bytes. No registry borrow spans this operation,
        // and this baseline core interner does not invoke callbacks/automatic GC.
        self.intern_raw(bytes)
    }
}
unsafe extern "C" fn strict_allocate(c: *mut c_void, _: Oop, args: *const Operand, n: usize) -> Outcome {
    let p = unsafe { &*c.cast::<Probe>() };
    let result = (|| {
        let [arg] = (unsafe { crate::abi::borrowed_slice(args, n) })? else { return None };
        p.allocate_object(arg.as_object()?)
    })();
    answer(result)
}
unsafe extern "C" fn strict_intern(c: *mut c_void, receiver: Oop, _: *const Operand, n: usize) -> Outcome {
    answer((n == 0).then(|| unsafe { &*c.cast::<Probe>() }.intern_object(receiver)).flatten())
}
fn answer(value: Option<Oop>) -> Outcome {
    value.map(|o| Outcome::loaded(Operand::object(o)))
        .unwrap_or_else(|| Outcome::failed(native::INVALID_REQUEST, 0))
}
unsafe extern "C" fn identity(_: *mut c_void, r: Oop, _: *const Operand, n: usize) -> Outcome {
    answer((n == 0).then_some(r))
}
unsafe extern "C" fn replacement(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    answer(Some(c))
}

#[test]
fn size_boundary_and_real_dispatch_preserve_replacement() {
    let p = Probe::new();
    let count = unsafe { (&*p.state().memory).len() };
    // Include all representable unsigned-63-bit endpoints, without attempting
    // huge allocations. Check the actual allocator geometry independently.
    for n in [0, 1, 7, 8, 4096, usize::MAX >> 1] {
        let size = size_oop(n).unwrap();
        assert_eq!(size_value(size), Some(n));
        assert_eq!(p.send(size, p.identity_selector, &[]).unwrap().as_object(), Some(size));
    }
    assert_eq!(unsafe { (&*p.state().memory).len() }, count, "size values must not allocate");
    assert!(size_oop((usize::MAX >> 1) + 1).is_none());
    assert!(size_value(core::ptr::null_mut()).is_none());
    assert!(size_value(p.initial.object_prototype).is_none());
    assert!(p.allocate_object(size_oop(usize::MAX >> 1).unwrap()).is_none());
    // Id allocations have 16-byte alignment; Layout's rounded size must fit
    // isize::MAX, including the Oop header. No huge allocation is performed.
    let max_layout_payload = (isize::MAX as usize & !15) - size_of::<Oop>();
    assert!(ObjectMemory::valid_payload_size(max_layout_payload));
    assert!(size_oop(max_layout_payload).is_some());
    assert!(!ObjectMemory::valid_payload_size(max_layout_payload + 1));
    for n in [0, 1, 8, 31, 1024] {
        let object = p.send(p.initial.object_prototype, p.allocate_selector,
            &[Operand::object(size_oop(n).unwrap())]).unwrap().as_object().unwrap();
        assert_eq!(unsafe { (&*p.state().memory).get(object.addr()).unwrap().requested_size() }, n);
        p.release(object);
    }
    let sentinel = p.initial.object_prototype;
    p.install(p.size_behavior, p.identity_selector, replacement, sentinel);
    assert_eq!(p.send(size_oop(32).unwrap(), p.identity_selector, &[]).unwrap().as_object(), Some(sentinel));
    p.install(p.initial.object_vt, p.allocate_selector, replacement, sentinel);
    assert_eq!(p.send(sentinel, p.allocate_selector, &[Operand::object(size_oop(32).unwrap())])
        .unwrap().as_object(), Some(sentinel));
}

#[test]
fn names_preserve_canonical_identity_content_lifetime_and_validation() {
    let p = Probe::new();
    for bytes in [b"".as_slice(), b"drawFullMap", "m\u{00e9}thode:".as_bytes(), &[b'x'; 4096]] {
        let mut source = bytes.to_vec();
        let first = p.name(&source);
        source.fill(b'?');
        drop(source);
        let second = p.name(bytes);
        assert_ne!(first, second);
        let a = p.send(first, p.intern_selector, &[]).unwrap().as_object().unwrap();
        let b = p.send(second, p.intern_selector, &[]).unwrap().as_object().unwrap();
        assert_eq!(a, b);
        assert_eq!(a, p.intern_raw(bytes).unwrap());
        p.release(first);
        p.release(second);
        assert_eq!(unsafe { (&*p.state().symbols).bytes_of(a) }, Some(bytes));
        assert_eq!(a, p.intern_raw(bytes).unwrap());
    }
    let bad = p.name(b"not\0a-selector");
    assert!(p.intern_object(bad).is_none());
    p.release(bad);
    // A different behavior, a short object re-associated to Name, and a
    // corrupted declared length must all fail before unbounded byte access.
    assert!(p.intern_object(p.initial.object_prototype).is_none());
    let short = p.allocation(0).unwrap();
    assert!(unsafe { (&mut *p.state().memory).set_vtable(short, p.name_behavior) });
    assert!(p.intern_object(short).is_none());
    p.release(short);
    let malformed = p.name(b"x");
    unsafe { malformed.cast::<usize>().write(usize::MAX) };
    assert!(p.intern_object(malformed).is_none());
    p.release(malformed);
}

struct Reentry { probe: *const Probe, name: Oop, selector: Oop, sentinel: Oop }
unsafe extern "C" fn reentrant_name(c: *mut c_void, receiver: Oop, _: *const Operand, _: usize) -> Outcome {
    let r = unsafe { &*c.cast::<Reentry>() };
    let p = unsafe { &*r.probe };
    // No interner/registry borrow may span this replacement and nested send.
    p.install(p.name_behavior, r.selector, replacement, r.sentinel);
    let nested = p.send(r.name, r.selector, &[]).unwrap().as_object().unwrap();
    assert_eq!(nested, r.sentinel);
    answer(p.intern_object(receiver))
}
#[test]
fn name_dispatch_observes_live_replacement_and_reentry() {
    let p = Probe::new();
    let name = p.name(b"live-name");
    let symbol = p.intern_raw(b"live-name").unwrap();
    let mut reentry = Reentry { probe: &*p, name, selector: p.intern_selector,
        sentinel: p.initial.object_prototype };
    p.install(p.name_behavior, p.intern_selector, reentrant_name,
        core::ptr::from_mut(&mut reentry).cast());
    assert_eq!(p.send(name, p.intern_selector, &[]).unwrap().as_object(), Some(symbol));
    assert_eq!(p.send(name, p.intern_selector, &[]).unwrap().as_object(), Some(reentry.sentinel));
    p.release(name);
}

struct LookupProbe {
    original: NativeEntry,
    selector: Oop,
    selected: NativeEntry,
    calls: Cell<usize>,
}
unsafe extern "C" fn custom_lookup(c: *mut c_void, receiver: Oop, a: *const Operand, n: usize) -> Outcome {
    let p = unsafe { &*c.cast::<LookupProbe>() };
    p.calls.set(p.calls.get() + 1);
    if n == 1 && unsafe { a.read() }.as_object() == Some(p.selector) {
        return Outcome::loaded(Operand::entry(p.selected));
    }
    unsafe { p.original.method.unwrap()(p.original.context, receiver, a, n) }
}
#[test]
fn name_custom_lookup_and_immediate_dnu_use_actual_id() {
    let p = Probe::new();
    let name = p.name(b"custom-lookup-probe");
    let sentinel = p.initial.object_prototype;
    let original = unsafe { (&*p.state().dictionaries).lookup(p.initial.vtable_vt, p.state().lookup) }.unwrap();
    let mut lookup = LookupProbe { original, selector: p.intern_selector,
        selected: NativeEntry::new(replacement, sentinel), calls: Cell::new(0) };
    p.install(p.initial.vtable_vt, p.state().lookup, custom_lookup,
        core::ptr::from_mut(&mut lookup).cast());
    assert_eq!(p.send(name, p.intern_selector, &[]).unwrap().as_object(), Some(sentinel));
    assert!(lookup.calls.get() > 0);
    p.install(p.size_behavior, p.state().does_not_understand, replacement, sentinel);
    let missing = unsafe { p.state().protocol.intern_name(b"unimplementedSizeMessage").unwrap() };
    assert_eq!(p.send(size_oop(48).unwrap(), missing, &[]).unwrap().as_object(), Some(sentinel));
    p.release(name);
}

#[test]
fn name_survives_symbol_retirement_and_reinterns_with_new_identity() {
    let p = Probe::new();
    let name = p.name(b"retire-probe-with-kept-name");
    let first = p.intern_object(name).unwrap();
    let old_identity = unsafe { (&*p.state().memory).get(first.addr()).unwrap().identity() };
    let mut dead = crate::raw::AddressMap::default();
    dead.insert(first.addr(), ());
    assert_eq!(unsafe { (&mut *p.state().symbols).remove_objects(&dead) }, 1);
    p.release(first);
    let second = p.send(name, p.intern_selector, &[]).unwrap().as_object().unwrap();
    let new_identity = unsafe { (&*p.state().memory).get(second.addr()).unwrap().identity() };
    assert_ne!(old_identity, new_identity, "allocator address reuse is not identity reuse");
    assert_eq!(p.intern_raw(b"retire-probe-with-kept-name"), Some(second));
    p.release(name);
}

#[cfg(target_os = "macos")]
unsafe extern "C" { fn clock_gettime_nsec_np(clock: i32) -> u64; }
#[cfg(target_os = "macos")]
fn time() -> u64 { unsafe { clock_gettime_nsec_np(16) } }
#[cfg(target_os = "macos")]
fn record(case: &str, lane: &str, round: usize, n: usize, start: u64) {
    let elapsed = time() - start;
    println!("OOP_PROBE={{\"case\":\"{case}\",\"lane\":\"{lane}\",\"round\":{round},\"iterations\":{n},\"cpu_ns\":{elapsed},\"cpu_ns_per_op\":{}}}", elapsed as f64 / n as f64);
}
fn names(n: usize, length: usize, round: usize) -> Vec<Vec<u8>> {
    (0..n).map(|i| {
        let mut s = format!("{round:03x}{i:05x}").into_bytes();
        s.resize(length.max(8), b'x'); s
    }).collect()
}

#[test]
#[ignore = "serial release-only timing experiment; not a correctness gate"]
#[cfg(target_os = "macos")]
fn timed_strict_boundary_designs() {
    assert!(!cfg!(debug_assertions), "run this experiment with --release");
    let p = Probe::new();
    let sizes = [0, 8, 24, 64, 256, 1024, 4096];
    let tagged = sizes.map(|n| size_oop(n).unwrap());
    for round in 0..10 {
        for lane in if round % 2 == 0 { ["raw", "oop"] } else { ["oop", "raw"] } {
            let n = 16_384;
            let mut objects = Vec::with_capacity(n);
            let start = time();
            for i in 0..n {
                let object = if lane == "raw" { p.allocate_raw(black_box(sizes[i % sizes.len()])) }
                    else { p.allocate_object(black_box(tagged[i % tagged.len()])) }.unwrap();
                objects.push(black_box(object));
            }
            record("allocation-burst", lane, round, n, start);
            for object in objects { p.release(object); }
        }
    }
    for length in [8, 32, 128, 1024] {
        let data = names(128, length, 0);
        let cstrings: Vec<_> = data.iter().map(|b| std::ffi::CString::new(b.clone()).unwrap()).collect();
        let objects: Vec<_> = data.iter().map(|b| p.name(b)).collect();
        for bytes in &data { p.intern_raw(bytes).unwrap(); }
        for round in 0..10 {
            let orders = [["raw", "oop", "cstr"], ["cstr", "oop", "raw"], ["oop", "raw", "cstr"]];
            for lane in orders[round % orders.len()] {
                let n = 250_000;
                let start = time();
                for i in 0..n {
                    let at = i % data.len();
                    black_box(match lane {
                        "raw" => p.intern_raw(black_box(&data[at])),
                        "cstr" => p.intern_cstr(black_box(&cstrings[at])),
                        _ => p.intern_object(black_box(objects[at])),
                    }.unwrap());
                }
                record(&format!("intern-hit-{length}"), lane, round, n, start);
            }
        }
        for object in objects { p.release(object); }
    }
    // Matched fresh floors eliminate hits and keep inventory sizes comparable.
    // Both prepared lanes own source Name objects, isolating consumption cost.
    for case in ["intern-miss-prepared", "intern-hit-fresh-name", "intern-miss-fresh-name"] {
        for round in 0..10 {
            for lane in if round % 2 == 0 { ["raw", "oop"] } else { ["oop", "raw"] } {
                let p = Probe::new();
                let n = 8192;
                let data = names(n, 32, round + 1);
                let objects: Vec<_> = if case == "intern-miss-prepared" {
                    data.iter().map(|b| p.name(b)).collect()
                } else { Vec::new() };
                if case == "intern-hit-fresh-name" {
                    for bytes in &data { p.intern_raw(bytes).unwrap(); }
                }
                let start = time();
                for (i, bytes) in data.iter().enumerate() {
                    if lane == "raw" { black_box(p.intern_raw(black_box(bytes)).unwrap()); }
                    else if case == "intern-miss-prepared" {
                        black_box(p.intern_object(black_box(objects[i])).unwrap());
                    } else {
                        let name = p.name(black_box(bytes));
                        black_box(p.intern_object(name).unwrap());
                        p.release(name);
                    }
                }
                record(case, lane, round, n, start);
                for object in objects { p.release(object); }
            }
        }
    }
}
