//! Optional untimed message census. No dispatch/lookup replacements.
use super::*;
use std::collections::BTreeMap;
struct Trace {
    parent: usize,
    receiver: usize,
    behavior: usize,
    selector: usize,
    key: Vec<u8>,
    argument: usize,
    result: usize,
    inclusive: usize,
    status: u32,
}
#[derive(Default)]
pub(super) struct Metrics {
    pub enabled: bool,
    pub counts: BTreeMap<usize, usize>,
    snapshot: Vec<u8>,
    next: usize,
    stack: Vec<usize>,
    frames: Vec<(usize, usize)>,
    edges: BTreeMap<(usize, usize, usize, usize), usize>,
    // Every send belongs to one nearest named ABI subtree, or the outside
    // bucket. Selector labels are diagnostic, never inferred foreign owners.
    regions: BTreeMap<(usize, usize), usize>,
    region_stack: Vec<usize>,
    subtrees: BTreeMap<(usize, usize, usize), (usize, usize)>,
    trace: Vec<Trace>,
    tokens: BTreeMap<(usize, usize), (usize, usize)>,
    live: BTreeMap<(usize, usize, usize), usize>,
    objects: std::collections::BTreeSet<usize>,
    total_roots: usize,
    same_call_duplicates: usize,
    other_call_duplicates: usize,
    max_live_tokens: usize,
}
pub(super) fn enter(s: &State, receiver: Oop, selector: Oop, args: &[Operand]) -> bool {
    let mut m = s.metrics.borrow_mut();
    if !m.enabled {
        return false;
    }
    *m.counts.entry(selector.addr()).or_default() += 1;
    let behavior = if unsafe { (&*s.memory).get(receiver.addr()) }.is_some() {
        unsafe { super::super::object_memory::ObjectMemory::vtable_of_valid_object(receiver) }
            .addr()
    } else {
        0
    };
    let (parent_behavior, parent_selector) = m.frames.last().copied().unwrap_or((0, 0));
    let argument = args.first().and_then(|v|v.as_object()).map_or(0, |v|v.addr());
    // Names supplied through the ABI or already owned by Id's Symbol table.
    // No foreign payload/context inspection and no diagnostic sends.
    let key = args.first().and_then(|value| {
        if let Some(object) = value.as_object() {
            unsafe { (&*s.symbols).bytes_of(object) }.map(|v|v.to_vec())
        } else if matches!(unsafe { (&*s.symbols).bytes_of(selector) }, Some(b"intern:" | b"symbolNamed:")) {
            let bytes=value.as_bytes()?;
            if bytes.length>256 {return None;}
            unsafe {crate::abi::borrowed_bytes(bytes.pointer,bytes.length)}.map(|v|v.to_vec())
        } else {None}
    }).unwrap_or_default();
    let parent = m.stack.last().copied().unwrap_or(0);
    m.trace.push(Trace {parent,receiver:receiver.addr(),behavior,selector:selector.addr(),key,
        argument,result:0,inclusive:0,status:0});
    *m.edges
        .entry((parent_behavior, parent_selector, behavior, selector.addr()))
        .or_default() += 1;
    m.frames.push((behavior, selector.addr()));
    let region = m.frames.iter().rev().find_map(|&(_, selector)| {
        let name = unsafe { (&*s.symbols).bytes_of(core::ptr::with_exposed_provenance_mut(selector)) }?;
        matches!(name,
            b"optionalWordResultOf:selector:argument:resultFactory:" | b"objectResultOf:selector:wordArgument:"
            | b"discardResultOf:selector:wordArgument:" | b"objectResultOf:selector:byteArgument:"
            | b"bytesResultOf:selector:resultFactory:"
            | b"wordResultOf:selector:" | b"bytesResultOf:selector:" | b"block:withNativeSignature:"
        ).then_some(selector)
    }).unwrap_or(0);
    *m.regions.entry((region, selector.addr())).or_default() += 1;
    m.region_stack.push(region);
    m.next += 1;
    let id = m.next;
    m.stack.push(id);
    true
}
pub(super) fn leave(
    s: &State,
    receiver: Oop,
    selector: Oop,
    args: &[Operand],
    result: &Outcome,
    tracked: bool,
) {
    if !tracked {
        return;
    }
    let name = unsafe { (&*s.symbols).bytes_of(selector) };
    let mut m = s.metrics.borrow_mut();
    let start = m.stack.pop().unwrap();
    m.frames.pop();
    let region = m.region_stack.pop().unwrap();
    let parent_selector = m.frames.last().map_or(0, |f|f.1);
    let inclusive = m.next-start+1;
    let row = &mut m.trace[start-1];
    row.inclusive = inclusive;
    row.status = result.status;
    row.result = result.value.as_object().map_or(0, |v|v.addr());
    let stats = m.subtrees.entry((region,parent_selector,selector.addr())).or_default();
    stats.0 += 1;
    stats.1 += inclusive;
    let parent = m.stack.last().copied().unwrap_or(0);
    if result.status != native::LOADED {
        return;
    }
    if name == Some(b"retain:".as_slice()) && args.len() == 1 {
        if let (Some(object), Some(token)) = (args[0].as_object(), result.value.as_word()) {
            track(&mut m, parent, receiver.addr(), object.addr(), token);
        }
    } else if name == Some(b"tryRetainMany:".as_slice()) && args.len() == 1 {
        if let (Some(input), Some(output)) = (args[0].as_bytes(), result.value.as_bytes()) {
            if let (Some(objects), Some(tokens)) = (
                unsafe { crate::abi::borrowed_bytes(input.pointer, input.length) },
                unsafe { crate::abi::borrowed_bytes(output.pointer, output.length) },
            ) {
                for (object, token) in objects
                    .chunks_exact(size_of::<usize>())
                    .zip(tokens.chunks_exact(size_of::<usize>()))
                {
                    track(
                        &mut m,
                        parent,
                        receiver.addr(),
                        usize::from_ne_bytes(object.try_into().unwrap()),
                        usize::from_ne_bytes(token.try_into().unwrap()),
                    );
                }
            }
        }
    } else if name == Some(b"release:".as_slice())
        && args.len() == 1
        && result.value.as_word() == Some(1)
    {
        if let Some(token) = args[0].as_word() {
            untrack(&mut m, receiver.addr(), token);
        }
    } else if (name == Some(b"tryReleaseMany:".as_slice())
        || name == Some(b"tryReleaseManyForward:".as_slice()))
        && args.len() == 1
        && result.value.as_word() == Some(1)
    {
        if let Some(bytes) = args[0]
            .as_bytes()
            .and_then(|b| unsafe { crate::abi::borrowed_bytes(b.pointer, b.length) })
        {
            for chunk in bytes.chunks_exact(size_of::<usize>()) {
                untrack(
                    &mut m,
                    receiver.addr(),
                    usize::from_ne_bytes(chunk.try_into().unwrap()),
                );
            }
        }
    }
}
fn track(m: &mut Metrics, parent: usize, owner: usize, object: usize, token: usize) {
    let key = (parent, owner, object);
    m.total_roots += 1;
    m.objects.insert(object);
    if m.live.get(&key).copied().unwrap_or(0) > 0 {
        m.same_call_duplicates += 1;
    } else if m
        .live
        .iter()
        .any(|(&(p, r, o), &n)| p != parent && r == owner && o == object && n > 0)
    {
        m.other_call_duplicates += 1;
    }
    *m.live.entry(key).or_default() += 1;
    m.tokens.insert((owner, token), (parent, object));
    m.max_live_tokens = m.max_live_tokens.max(m.tokens.len());
}
fn untrack(m: &mut Metrics, owner: usize, token: usize) {
    if let Some((parent, object)) = m.tokens.remove(&(owner, token)) {
        let key = (parent, owner, object);
        if let Some(n) = m.live.get_mut(&key) {
            *n -= 1;
            if *n == 0 {
                m.live.remove(&key);
            }
        }
    }
}
pub(super) unsafe fn install(s: &State, c: *mut c_void, add: Oop) {
    unsafe {
        for (name, method) in [
            (b"dispatchCountsBegin".as_slice(), begin as native::Method),
            (b"dispatchCountsEnd", end as native::Method),
            (b"dispatchRootSummary", root_summary as native::Method),
            (b"dispatchEdges", edges as native::Method),
            (b"dispatchRegions", regions as native::Method),
            (b"dispatchSubtrees", subtrees as native::Method),
            (b"dispatchTrace", trace as native::Method),
        ] {
            let selector = (&mut *s.symbols)
                .intern(&mut *s.memory, s.symbol_vtable, name)
                .unwrap();
            let result = id_dispatch::send(
                s.dispatch_context(),
                s.protocol.receiver,
                add,
                &[
                    Operand::object(selector),
                    Operand::entry(NativeEntry::new(method, c)),
                ],
            );
            assert_eq!(result.status, native::LOADED);
        }
    }
}
unsafe extern "C" fn begin(c: *mut c_void, _: Oop, _: *const Operand, n: usize) -> Outcome {
    if n != 0 {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    }
    let s = unsafe { &*c.cast::<State>() };
    let mut m = s.metrics.borrow_mut();
    *m = Metrics::default();
    m.enabled = true;
    Outcome::loaded(Operand::unit())
}
unsafe extern "C" fn end(c: *mut c_void, _: Oop, _: *const Operand, n: usize) -> Outcome {
    if n != 0 {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    }
    let s = unsafe { &*c.cast::<State>() };
    let mut m = s.metrics.borrow_mut();
    m.enabled = false;
    // Scalar selector ids belong to Id. Copy their owned spellings, with no sends.
    let counts = std::mem::take(&mut m.counts);
    m.snapshot.clear();
    for (address, count) in counts {
        let selector = core::ptr::with_exposed_provenance_mut(address);
        if let Some(name) = unsafe { (&*s.symbols).bytes_of(selector) } {
            m.snapshot.extend_from_slice(count.to_string().as_bytes());
            m.snapshot.push(b'\t');
            for byte in name {
                m.snapshot
                    .extend_from_slice(format!("{byte:02x}").as_bytes());
            }
            m.snapshot.push(b'\n');
        }
    }
    Outcome::loaded(Operand::bytes(&m.snapshot))
}

unsafe extern "C" fn root_summary(c: *mut c_void, _: Oop, _: *const Operand, n: usize) -> Outcome {
    if n != 0 {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    }
    let s = unsafe { &*c.cast::<State>() };
    let mut m = s.metrics.borrow_mut();
    m.snapshot=format!("{{\"retains\":{},\"distinct_object_addresses\":{},\"same_message_activation_live_duplicates\":{},\"other_activation_live_duplicates\":{},\"max_live_tokens_observed\":{},\"remaining_tokens_observed\":{}}}",m.total_roots,m.objects.len(),m.same_call_duplicates,m.other_call_duplicates,m.max_live_tokens,m.tokens.len()).into_bytes();
    Outcome::loaded(Operand::bytes(&m.snapshot))
}

unsafe extern "C" fn edges(c: *mut c_void, _: Oop, _: *const Operand, n: usize) -> Outcome {
    if n != 0 {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    }
    let s = unsafe { &*c.cast::<State>() };
    let mut m = s.metrics.borrow_mut();
    let mut bytes = Vec::new();
    for (&(parent_behavior, parent_selector, behavior, selector), count) in &m.edges {
        let names = [parent_selector, selector].map(|address| {
            if address == 0 {
                return "-".to_owned();
            }
            let symbol = core::ptr::with_exposed_provenance_mut(address);
            unsafe { (&*s.symbols).bytes_of(symbol) }
                .unwrap_or(b"?")
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        });
        bytes.extend_from_slice(
            format!(
                "{count}\t{parent_behavior:x}\t{}\t{behavior:x}\t{}\n",
                names[0], names[1]
            )
            .as_bytes(),
        );
    }
    m.snapshot = bytes;
    Outcome::loaded(Operand::bytes(&m.snapshot))
}

unsafe extern "C" fn regions(c: *mut c_void, _: Oop, _: *const Operand, n: usize) -> Outcome {
    if n != 0 { return Outcome::failed(native::INVALID_REQUEST, 0); }
    let s = unsafe { &*c.cast::<State>() };
    let mut m = s.metrics.borrow_mut();
    let mut bytes = Vec::new();
    for (&(region, selector), count) in &m.regions {
        let names = [region, selector].map(|address| {
            if address == 0 { return "-".to_owned(); }
            unsafe { (&*s.symbols).bytes_of(core::ptr::with_exposed_provenance_mut(address)) }
                .unwrap_or(b"?").iter().map(|b| format!("{b:02x}")).collect::<String>()
        });
        bytes.extend_from_slice(format!("{count}\t{}\t{}\n", names[0], names[1]).as_bytes());
    }
    m.snapshot = bytes;
    Outcome::loaded(Operand::bytes(&m.snapshot))
}

unsafe extern "C" fn subtrees(c: *mut c_void, _: Oop, _: *const Operand, n: usize) -> Outcome {
    if n != 0 { return Outcome::failed(native::INVALID_REQUEST, 0); }
    let s = unsafe { &*c.cast::<State>() };
    let mut m = s.metrics.borrow_mut();
    let mut bytes = Vec::new();
    for (&(region, parent, selector), &(calls, inclusive)) in &m.subtrees {
        let names = [region,parent,selector].map(|address| {
            if address == 0 { return "-".to_owned(); }
            unsafe { (&*s.symbols).bytes_of(core::ptr::with_exposed_provenance_mut(address)) }
                .unwrap_or(b"?").iter().map(|b| format!("{b:02x}")).collect::<String>()
        });
        bytes.extend_from_slice(format!("{calls}\t{inclusive}\t{}\t{}\t{}\n", names[0],names[1],names[2]).as_bytes());
    }
    m.snapshot = bytes;
    Outcome::loaded(Operand::bytes(&m.snapshot))
}

unsafe extern "C" fn trace(c: *mut c_void, _: Oop, _: *const Operand, n: usize) -> Outcome {
    if n != 0 { return Outcome::failed(native::INVALID_REQUEST, 0); }
    let s = unsafe { &*c.cast::<State>() };
    let mut m = s.metrics.borrow_mut();
    let mut bytes = Vec::new();
    for (i,row) in m.trace.iter().enumerate() {
        let name=unsafe{(&*s.symbols).bytes_of(core::ptr::with_exposed_provenance_mut(row.selector))}.unwrap_or(b"?");
        let hex=|v:&[u8]| v.iter().map(|b|format!("{b:02x}")).collect::<String>();
        bytes.extend_from_slice(format!("{}\t{}\t{:x}\t{:x}\t{}\t{}\t{:x}\t{:x}\t{}\t{}\n",
            i+1,row.parent,row.receiver,row.behavior,hex(name),if row.key.is_empty(){"-".into()}else{hex(&row.key)},
            row.argument,row.result,row.inclusive,row.status).as_bytes());
    }
    m.snapshot=bytes;
    Outcome::loaded(Operand::bytes(&m.snapshot))
}
