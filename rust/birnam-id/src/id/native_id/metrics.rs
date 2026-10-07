//! Optional untimed message census. No dispatch/lookup replacements.
use super::*;
use std::collections::BTreeMap;
#[derive(Default)]
pub(super) struct Metrics {
    pub enabled: bool,
    pub counts: BTreeMap<usize, usize>,
    snapshot: Vec<u8>,
    next: usize,
    stack: Vec<usize>,
    tokens: BTreeMap<(usize, usize), (usize, usize)>,
    live: BTreeMap<(usize, usize, usize), usize>,
    objects: std::collections::BTreeSet<usize>,
    total_roots: usize,
    same_call_duplicates: usize,
    other_call_duplicates: usize,
    max_live_tokens: usize,
}
pub(super) fn enter(s: &State, selector: Oop) -> bool {
    let mut m = s.metrics.borrow_mut();
    if !m.enabled {
        return false;
    }
    *m.counts.entry(selector.addr()).or_default() += 1;
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
    m.stack.pop();
    let parent = m.stack.last().copied().unwrap_or(0);
    if result.status != native::LOADED {
        return;
    }
    if name == Some(b"retain:".as_slice()) && args.len() == 1 {
        if let (Some(object), Some(token)) = (args[0].as_object(), result.value.as_word()) {
            let owner = receiver.addr();
            let object = object.addr();
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
    } else if name == Some(b"release:".as_slice())
        && args.len() == 1
        && result.value.as_word() == Some(1)
    {
        if let Some(token) = args[0].as_word() {
            untrack(&mut m, receiver.addr(), token);
        }
    } else if name == Some(b"tryReleaseMany:".as_slice()) && args.len() == 1 && result.value.as_word() == Some(1) {
        if let Some(bytes) = args[0].as_bytes().and_then(|b| unsafe { crate::abi::borrowed_bytes(b.pointer, b.length) }) {
            for chunk in bytes.chunks_exact(size_of::<usize>()) {
                untrack(&mut m, receiver.addr(), usize::from_ne_bytes(chunk.try_into().unwrap()));
            }
        }
    }
}
fn untrack(m: &mut Metrics, owner: usize, token: usize) {
    if let Some((parent, object)) = m.tokens.remove(&(owner, token)) {
        let key = (parent, owner, object);
        if let Some(n) = m.live.get_mut(&key) {
            *n -= 1;
            if *n == 0 { m.live.remove(&key); }
        }
    }
}
pub(super) unsafe fn install(s: &State, c: *mut c_void, add: Oop) {
    unsafe {
        for (name, method) in [
            (b"dispatchCountsBegin".as_slice(), begin as native::Method),
            (b"dispatchCountsEnd", end as native::Method),
            (b"dispatchRootSummary", root_summary as native::Method),
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
