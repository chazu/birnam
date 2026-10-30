//! Process-local Isolate objects communicate with other owners only through Id.
use super::*;
use birnam_abi::{
    self as abi, Oop, borrowed_slice,
    id_protocol::{Literal, MethodSpec, Module, Protocol},
};
use core::{
    cell::{Cell, RefCell},
    ptr,
};
mod transfer;
use transfer::{TransferError, capture, materialize};
type Answer = Result<Oop, Oop>;
use std::sync::atomic::AtomicPtr;
static API: AtomicPtr<Protocol> = AtomicPtr::new(ptr::null_mut());
static INSTANCE: AtomicPtr<State> = AtomicPtr::new(ptr::null_mut());
fn invalid() -> Oop {
    unsafe { abi::object_reference((*API.load(Ordering::Acquire)).invalid) }
}
fn object(v: Oop) -> Answer {
    if v.is_null() { Err(invalid()) } else { Ok(v) }
}
fn done(r: Answer) -> Oop {
    r.unwrap_or_else(abi::fault_reference)
}
unsafe fn state(c: Oop) -> Result<&'static State, Oop> {
    unsafe {
        let p = INSTANCE.load(Ordering::Acquire);
        if p.is_null() {
            return Err(invalid());
        }
        let s = &*p;
        if s.ns != c || s.thread != std::thread::current().id() {
            return Err(invalid());
        }
        Ok(s)
    }
}
struct State {
    pub(crate) byte_reader: abi::byte_transfer::Reader,
    id: Protocol,
    names: HashMap<&'static [u8], Oop>,
    scoped: RefCell<Vec<Oop>>,
    receiver: Oop,
    ready: Cell<bool>,
    ns: Oop,
    model: Oop,
    handle_vt: Oop,
    failure_vt: Oop,
    permanent: Vec<Oop>,
    handles: RefCell<HashMap<usize, u64>>,
    failures: RefCell<HashMap<usize, [Oop; 5]>>,
    thread: std::thread::ThreadId,
}
struct Scope<'a> {
    s: &'a State,
    start: usize,
}
impl<'a> Scope<'a> {
    fn new(s: &'a State) -> Self {
        Self {
            s,
            start: s.scoped.borrow().len(),
        }
    }
    unsafe fn keep(&self, o: Oop) -> Result<Oop, Oop> {
        self.s.scoped.borrow_mut().push(o);
        Ok(o)
    }
    unsafe fn call(&self, r: Oop, n: &[u8], a: &[Oop]) -> Answer {
        unsafe { self.keep(self.s.send(r, n, a)?) }
    }
    unsafe fn obj(&self, r: Oop, n: &[u8], a: &[Oop]) -> Answer {
        object(unsafe { self.call(r, n, a) }?)
    }
    unsafe fn named(&self, n: &[u8]) -> Answer {
        unsafe { self.obj(self.s.ns, b"at:", &[self.s.intern(n)?]) }
    }
    unsafe fn byte_value(&self, b: &[u8]) -> Answer {
        unsafe {
            let a: Vec<_> = b.iter().map(|b| abi::size(*b as usize).unwrap()).collect();
            self.call(self.s.id.root, b"sequence", &a)
        }
    }
    unsafe fn string(&self, b: &[u8]) -> Answer {
        unsafe {
            self.obj(
                self.named(b"String")?,
                b"fromBytes:",
                &[self.byte_value(b)?],
            )
        }
    }
    unsafe fn raw_word(&self, v: usize) -> Answer {
        unsafe {
            if let Some(v) = abi::size(v) {
                return Ok(v);
            }
            self.obj(
                self.named(b"Word")?,
                b"fromUnsignedLow:high:",
                &[
                    abi::size(v & 0xffff_ffff).unwrap(),
                    abi::size(v >> 32).unwrap(),
                ],
            )
        }
    }
    unsafe fn bits(&self, v: Oop) -> Result<usize, Oop> {
        unsafe {
            if let Some(v) = abi::as_size(v) {
                return Ok(v);
            }
            self.keep(v)?;
            let lo = abi::as_size(self.call(v, b"low32", &[])?).ok_or_else(invalid)?;
            let hi = abi::as_size(self.call(v, b"high32", &[])?).ok_or_else(invalid)?;
            Ok(lo | (hi << 32))
        }
    }
    unsafe fn int(&self, w: usize) -> Answer {
        unsafe { self.obj(self.named(b"Integer")?, b"fromWord:", &[self.raw_word(w)?]) }
    }
    unsafe fn word(&self, o: Oop) -> Result<Option<usize>, Oop> {
        unsafe {
            let v = self.call(self.named(b"Integer")?, b"wordOf:", &[o])?;
            if v.is_null() {
                Ok(None)
            } else {
                self.bits(v).map(Some)
            }
        }
    }
    unsafe fn buffer(&self, o: Oop) -> Result<Vec<u8>, Oop> {
        unsafe {
            let v = self.call(o, b"bytes", &[])?;
            Ok(self.s.byte_reader.read(v)?)
        }
    }
    unsafe fn bytes(&self, o: Oop) -> Result<Vec<u8>, Oop> {
        unsafe { self.buffer(o) }
    }
    unsafe fn symbol(&self, n: &[u8]) -> Answer {
        unsafe { self.keep(self.s.intern(n)?) }
    }
    unsafe fn nil(&self) -> Answer {
        unsafe { self.named(b"nil") }
    }
    unsafe fn boolean(&self, b: bool) -> Answer {
        unsafe { self.named(if b { b"true" } else { b"false" }) }
    }
    unsafe fn text(&self, o: Oop) -> Result<Option<Vec<u8>>, Oop> {
        unsafe {
            let behavior = self.obj(self.s.id.root, b"behaviorOf:", &[o])?;
            if behavior == self.named(b"SymbolVt")? {
                return Ok(Some(self.s.byte_reader.read(o)?));
            }
            if behavior == self.named(b"StringVt")? {
                self.bytes(o).map(Some)
            } else {
                Ok(None)
            }
        }
    }
    unsafe fn failure(&self, f: Failure) -> Answer {
        unsafe {
            let nil = self.named(b"nil")?;
            let fields = [
                self.symbol(f.class)?,
                self.symbol(&f.kind)?,
                self.string(&f.message)?,
                if let Some(s) = f.status {
                    self.int(s as usize)?
                } else {
                    nil
                },
                if let Some(i) = f.request {
                    self.int(i as usize)?
                } else {
                    nil
                },
            ];
            let o = self.obj(
                self.s.model,
                b"allocate:bytes:",
                &[self.s.failure_vt, self.raw_word(0)?],
            )?;
            self.s.failures.borrow_mut().insert(o.addr(), fields);
            Ok(o)
        }
    }
    unsafe fn transfer_failure(&self, e: TransferError) -> Answer {
        match e {
            TransferError::Native(e) => Err(e),
            TransferError::Codec(e) => unsafe {
                self.failure(Failure::new(
                    b"encode",
                    b"nonTransferable",
                    codec_error_name(e),
                ))
            },
        }
    }
    fn entry(&self, o: Oop) -> Option<Arc<Mutex<Entry>>> {
        self.s
            .handles
            .borrow()
            .get(&o.addr())
            .and_then(|t| lookup_entry(*t))
    }
    unsafe fn encode(&self, o: Oop) -> Result<Vec<u8>, TransferError> {
        let v = unsafe { capture(self, o) }?;
        Ok(encode_value(&v)?)
    }
    unsafe fn decode(&self, b: &[u8]) -> Result<Oop, TransferError> {
        let v = decode_value(b)?;
        unsafe { materialize(self, &v) }
    }
}
impl Drop for Scope<'_> {
    fn drop(&mut self) {
        self.s.scoped.borrow_mut().truncate(self.start);
    }
}
impl State {
    unsafe fn send(&self, r: Oop, n: &[u8], a: &[Oop]) -> Answer {
        unsafe { self.id.send(r, self.names[n], a).map_err(|v| v) }
    }
    unsafe fn intern(&self, n: &[u8]) -> Answer {
        unsafe {
            if let Some(v) = self.names.get(n) {
                return Ok(*v);
            }
            let a: Vec<_> = n.iter().map(|b| abi::size(*b as usize).unwrap()).collect();
            let name = self.send(self.id.root, b"nameWithBytes", &a)?;
            Ok(self.id.send(name, self.id.intern, &[])?)
        }
    }
}
struct Failure {
    class: &'static [u8],
    kind: Vec<u8>,
    message: Vec<u8>,
    status: Option<u32>,
    request: Option<u64>,
}
impl Failure {
    fn new(class: &'static [u8], kind: &[u8], message: &[u8]) -> Self {
        Self {
            class,
            kind: kind.to_vec(),
            message: message.to_vec(),
            status: None,
            request: None,
        }
    }
}
fn unknown() -> Failure {
    Failure::new(b"stopped", b"unknownHandle", b"unknown isolate handle")
}
fn busy() -> Failure {
    Failure::new(
        b"busy",
        b"notReady",
        b"one outstanding request is allowed per worker",
    )
}
fn record_parts(schema: &[u8], bytes: &[u8]) -> Result<(Vec<u8>, Vec<u8>), CodecError> {
    let mut d = Decoder {
        bytes,
        position: 0,
        nodes: 0,
    };
    if d.byte()? != TAG_RECORD {
        return Err(CodecError::InvalidRecord);
    }
    let len = u16::from_be_bytes(d.take(2)?.try_into().unwrap()) as usize;
    if d.take(len)? != schema {
        return Err(CodecError::UnknownRecordSchema);
    }
    if d.take(2)? != 2u16.to_be_bytes() {
        return Err(CodecError::InvalidRecord);
    }
    let Value::Symbol(kind) = d.value(0)? else {
        return Err(CodecError::InvalidRecord);
    };
    let Value::String(message) = d.value(0)? else {
        return Err(CodecError::InvalidRecord);
    };
    if d.position != bytes.len() {
        return Err(CodecError::TrailingBytes);
    }
    Ok((kind, message))
}
fn failure_record(class: &'static [u8], schema: &[u8], bytes: &[u8]) -> Failure {
    match record_parts(schema, bytes) {
        Ok((kind, message)) => Failure {
            class,
            kind,
            message,
            status: None,
            request: None,
        },
        Err(e) => Failure::new(b"protocol", b"invalidRecord", codec_error_name(e)),
    }
}
unsafe fn start(q: &Scope<'_>, a: &[Oop], service: bool) -> Answer {
    unsafe {
        if a.len() != if service { 2 } else { 3 } {
            return q.failure(Failure::new(
                b"encode",
                b"invalidArguments",
                b"invalid worker start arguments",
            ));
        }
        let Some(receiver) = q.text(a[0])? else {
            return q.failure(Failure::new(
                b"encode",
                b"invalidReceiver",
                b"receiver must be a Symbol or String",
            ));
        };
        let selector = if service {
            b"handle:".to_vec()
        } else {
            let Some(s) = q.text(a[1])? else {
                return q.failure(Failure::new(
                    b"encode",
                    b"invalidSelector",
                    b"selector must be a Symbol or String",
                ));
            };
            s
        };
        let Some(image) = q.text(a[if service { 1 } else { 2 }])? else {
            return q.failure(Failure::new(
                b"encode",
                b"invalidImage",
                b"image must be a String",
            ));
        };
        if receiver.contains(&0) || selector.contains(&0) || image.contains(&0) {
            return q.failure(Failure::new(
                b"encode",
                b"embeddedNul",
                b"worker names and image path may not contain NUL",
            ));
        }
        if !selector_arity_one(&selector) {
            return q.failure(Failure::new(
                b"boot",
                b"wrongArity",
                b"worker selector must have arity one",
            ));
        }
        let mut entry = match spawn_host_worker(&image, &receiver, &selector, service) {
            Ok(e) => e,
            Err(k) => return q.failure(Failure::new(b"boot", k.as_bytes(), k.as_bytes())),
        };
        let ready = entry
            .channel
            .as_mut()
            .ok_or(HostTransportError::Io)
            .and_then(|c| host_read_frame(c, 60_000));
        match ready {
            Ok(f) if f.header.kind == FrameKind::Ready => {}
            Ok(f) => {
                terminate_entry(&mut entry);
                return q.failure(if f.header.kind == FrameKind::BootError {
                    failure_record(b"boot", BOOT_ERROR_SCHEMA, &f.payload)
                } else {
                    Failure::new(b"boot", b"expectedReady", b"worker did not send ready")
                });
            }
            Err(e) => {
                terminate_entry(&mut entry);
                return q.failure(Failure::new(
                    b"boot",
                    match e {
                        HostTransportError::Timeout => b"handshakeTimeout",
                        HostTransportError::Protocol => b"invalidReady",
                        _ => b"handshakeFailed",
                    },
                    b"worker did not complete the ready handshake",
                ));
            }
        }
        entry.state = Lifecycle::Ready;
        let value = match q.obj(
            q.s.model,
            b"allocate:bytes:",
            &[q.s.handle_vt, q.raw_word(0)?],
        ) {
            Ok(o) => o,
            Err(e) => {
                terminate_entry(&mut entry);
                return Err(e);
            }
        };
        let token = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed);
        if token == 0 {
            terminate_entry(&mut entry);
            return Err(invalid());
        }
        q.s.handles.borrow_mut().insert(value.addr(), token);
        registry_lock().insert(token, Arc::new(Mutex::new(entry)));
        Ok(value)
    }
}
fn selector_arity_one(selector: &[u8]) -> bool {
    let colons = selector.iter().filter(|byte| **byte == b':').count();
    if colons != 0 {
        return colons == 1;
    }
    selector
        .first()
        .is_some_and(|byte| !byte.is_ascii_alphabetic())
}

fn transport_failure(entry: &mut Entry, error: HostTransportError, id: u64) -> Failure {
    terminate_entry(entry);
    let mut f = match error {
        HostTransportError::Timeout => Failure::new(
            b"timeout",
            b"deadline",
            b"request deadline expired; execution may have occurred",
        ),
        HostTransportError::Io => Failure::new(
            b"transport",
            b"uncertain",
            b"connection lost; execution may have occurred",
        ),
        HostTransportError::Protocol => Failure::new(
            b"protocol",
            b"invalidReply",
            b"worker returned an invalid reply",
        ),
    };
    f.status = entry.exit_status;
    f.request = Some(id);
    f
}
unsafe fn reply(
    q: &Scope<'_>,
    shared: &Arc<Mutex<Entry>>,
    frame: Frame,
    id: u64,
    envelope: bool,
) -> Answer {
    unsafe {
        if frame.header.request_id != id
            || !matches!(
                frame.header.kind,
                FrameKind::ReplyOk | FrameKind::ReplyError
            )
        {
            let f =
                { transport_failure(&mut lock_entry(shared), HostTransportError::Protocol, id) };
            return q.failure(f);
        }
        // Decoding and Failure construction can collect or re-enter this handle.
        // No registry or Entry borrow may survive those owner messages.
        let service = lock_entry(shared).task_service;
        let result = if frame.header.kind == FrameKind::ReplyError {
            let mut f = failure_record(
                if service {
                    b"resultTransfer"
                } else {
                    b"application"
                },
                APPLICATION_ERROR_SCHEMA,
                &frame.payload,
            );
            f.request = Some(id);
            if f.class == b"protocol" {
                terminate_entry(&mut lock_entry(shared));
            }
            q.failure(f)
        } else {
            match q.decode(&frame.payload) {
                Ok(v) => {
                    if envelope {
                        (|| {
                            let vector = q.obj(q.named(b"Vector")?, b"new", &[])?;
                            q.call(vector, b"add:", &[v])
                        })()
                    } else {
                        Ok(v)
                    }
                }
                Err(TransferError::Native(e)) => {
                    terminate_entry(&mut lock_entry(shared));
                    Err(e)
                }
                Err(TransferError::Codec(e)) => {
                    terminate_entry(&mut lock_entry(shared));
                    q.failure(Failure::new(
                        b"protocol",
                        b"invalidReply",
                        codec_error_name(e),
                    ))
                }
            }
        };
        {
            let mut e = lock_entry(shared);
            if result.is_err() {
                terminate_entry(&mut e);
            } else if e.state == Lifecycle::Busy {
                e.state = Lifecycle::Ready;
            }
        }
        result
    }
}
unsafe fn request(q: &Scope<'_>, r: Oop, a: &[Oop], asynchronous: bool) -> Answer {
    unsafe {
        if a.len() != 2 && !(a.len() == 1 && !asynchronous) {
            return q.failure(Failure::new(
                b"encode",
                b"invalidArguments",
                b"request expects a value and timeout",
            ));
        }
        let timeout = if a.len() == 1 {
            5000
        } else {
            let Some(w) = q.word(a[1])? else {
                return q.failure(Failure::new(
                    b"encode",
                    b"invalidTimeout",
                    b"timeout must be a nonnegative Integer",
                ));
            };
            if (w as isize) < 0 {
                return q.failure(Failure::new(
                    b"encode",
                    b"invalidTimeout",
                    b"timeout must be a nonnegative Integer",
                ));
            };
            w.min(u32::MAX as usize) as u32
        };
        // Copy through live language sends before borrowing process transport state.
        let payload = match q.encode(a[0]) {
            Ok(p) => p,
            Err(e) => return q.transfer_failure(e),
        };
        let Some(shared) = q.entry(r) else {
            return q.failure(unknown());
        };
        let response = (|| {
            let mut e = lock_entry(&shared);
            if e.state != Lifecycle::Ready || e.pending.is_some() {
                return Err(busy());
            }
            if e.task_service && !asynchronous {
                return Err(Failure::new(
                    b"encode",
                    b"asyncRequired",
                    b"task services use submit/poll or Worker request:",
                ));
            }
            // No bytes can be sent with an already-expired budget. In
            // particular, socket timeouts reject Duration::ZERO as an I/O
            // error; reaching that layer would unnecessarily kill the worker.
            if timeout == 0 {
                return Err(Failure::new(
                    b"timeout",
                    b"deadline",
                    b"request deadline expired before transmission",
                ));
            }
            let id = e.next_request;
            if id == 0 || id > i64::MAX as u64 {
                return Err(Failure::new(
                    b"protocol",
                    b"requestIdExhausted",
                    b"request id space exhausted",
                ));
            }
            let payload = if e.task_service {
                service::wrap_request(&payload).map_err(|err| {
                    Failure::new(b"encode", b"nonTransferable", codec_error_name(err))
                })?
            } else {
                payload
            };
            if asynchronous {
                let pending = asynchronous::Pending::new(id, payload, timeout).map_err(|_| {
                    Failure::new(
                        b"encode",
                        b"invalidRequest",
                        b"request could not be encoded",
                    )
                })?;
                if e.channel
                    .as_ref()
                    .is_none_or(|c| c.set_nonblocking(true).is_err())
                {
                    return Err(Failure::new(
                        b"transport",
                        b"unavailable",
                        b"worker channel is unavailable",
                    ));
                }
                e.next_request += 1;
                e.state = Lifecycle::Busy;
                e.pending = Some(pending);
                Ok((id, None))
            } else {
                e.next_request += 1;
                e.state = Lifecycle::Busy;
                let f = Frame {
                    header: Header {
                        kind: FrameKind::Request,
                        request_id: id,
                        payload_len: 0,
                    },
                    payload,
                };
                let result = (|| {
                    let c = e.channel.as_mut().ok_or(HostTransportError::Io)?;
                    host_write_frame(c, &f, timeout)?;
                    host_read_frame(c, timeout)
                })();
                result.map(|f| (id, Some(f))).map_err(|err| {
                    let mut failure = transport_failure(&mut e, err, id);
                    match err {
                        HostTransportError::Io => {
                            failure.class = if failure.status.is_some() {
                                b"exit"
                            } else {
                                b"transport"
                            };
                            failure.kind = b"transport".to_vec();
                            failure.message = b"request transport failed".to_vec();
                        }
                        HostTransportError::Timeout => {
                            failure.message =
                                b"request deadline expired before a reply arrived".to_vec()
                        }
                        _ => {}
                    }
                    failure
                })
            }
        })();
        match response {
            Err(f) => q.failure(f),
            Ok((id, None)) => Ok(q.int(id as usize)?),
            Ok((id, Some(frame))) => reply(q, &shared, frame, id, false),
        }
    }
}
unsafe fn poll(q: &Scope<'_>, r: Oop, a: &[Oop]) -> Answer {
    unsafe {
        if a.len() != 1 {
            return Err(invalid());
        }
        let Some(id) = q.word(a[0])? else {
            return q.failure(Failure::new(
                b"protocol",
                b"invalidRequestId",
                b"poll expects a request id",
            ));
        };
        let id = id as u64;
        let Some(shared) = q.entry(r) else {
            return q.failure(unknown());
        };
        let answer = (|| {
            let mut e = lock_entry(&shared);
            if e.pending.as_ref().is_none_or(|p| p.id != id) {
                return Err(Failure::new(
                    b"protocol",
                    b"staleRequest",
                    b"request is not outstanding on this worker",
                ));
            }
            let mut p = e.pending.take().unwrap();
            if let Some(c) = e.channel.as_mut() {
                p.progress(c)
            } else {
                p.result = Some(Err(HostTransportError::Io))
            }
            let Some(result) = p.result.take() else {
                e.pending = Some(p);
                return Ok(None);
            };
            if e.channel
                .as_ref()
                .is_some_and(|c| c.set_nonblocking(false).is_err())
            {
                return Err(transport_failure(&mut e, HostTransportError::Io, id));
            }
            result
                .map(Some)
                .map_err(|err| transport_failure(&mut e, err, id))
        })();
        match answer {
            Ok(None) => q.nil(),
            Ok(Some(f)) => reply(q, &shared, f, id, true),
            Err(f) => q.failure(f),
        }
    }
}
unsafe fn stop(q: &Scope<'_>, r: Oop, force: bool) -> Answer {
    unsafe {
        let Some(shared) = q.entry(r) else {
            return Ok(r);
        };
        let result = (|| {
            let mut e = lock_entry(&shared);
            if force {
                terminate_entry(&mut e);
                return Ok(());
            }
            if e.state == Lifecycle::Exited {
                return Ok(());
            }
            if e.state != Lifecycle::Ready {
                return Err(busy());
            }
            let id = e.next_request;
            e.next_request += 1;
            e.state = Lifecycle::Stopping;
            let stop = Frame {
                header: Header {
                    kind: FrameKind::Stop,
                    request_id: id,
                    payload_len: 0,
                },
                payload: Vec::new(),
            };
            let answer = if let Some(c) = e.channel.as_mut() {
                let _ = host_write_frame(c, &stop, 2000);
                host_read_frame(c, 2000).ok()
            } else {
                None
            };
            terminate_entry(&mut e);
            if answer
                .is_some_and(|f| f.header.kind != FrameKind::Stopped || f.header.request_id != id)
            {
                return Err(Failure::new(
                    b"protocol",
                    b"invalidStopReply",
                    b"invalid stopped reply",
                ));
            }
            Ok(())
        })();
        match result {
            Ok(()) => Ok(r),
            Err(f) => q.failure(f),
        }
    }
}
unsafe fn cancel(q: &Scope<'_>, r: Oop, a: &[Oop]) -> Answer {
    unsafe {
        if a.len() != 1 {
            return q.boolean(false);
        }
        let Some(id) = q.word(a[0])? else {
            return q.boolean(false);
        };
        let Some(shared) = q.entry(r) else {
            return q.boolean(false);
        };
        let admitted = (|| {
            let mut e = lock_entry(&shared);
            if !e.task_service {
                return false;
            }
            let Some(p) = e.pending.as_mut().filter(|p| p.id == id as u64) else {
                return false;
            };
            if !p.cancellation_queued {
                let f = Frame {
                    header: Header {
                        kind: FrameKind::Request,
                        request_id: id as u64,
                        payload_len: 0,
                    },
                    payload: service::cancellation_payload(),
                };
                let Ok(b) = f.encode() else { return false };
                p.outgoing.extend_from_slice(&b);
                p.cancellation_queued = true;
            }
            true
        })();
        q.boolean(admitted)
    }
}
// Process termination belongs to an owner checkpoint, never a GC retirement callback.
fn drain(shutdown: bool) {
    let entries = registry_lock()
        .iter()
        .map(|(&id, e)| (id, e.clone()))
        .collect::<Vec<_>>();
    let mut remove = Vec::new();
    for (id, e) in entries {
        let mut e = lock_entry(&e);
        if shutdown || e.orphaned {
            terminate_entry(&mut e)
        } else {
            status_of(&mut e);
        }
        if e.orphaned && e.state == Lifecycle::Exited {
            remove.push(id)
        }
    }
    let mut r = registry_lock();
    for id in remove {
        r.remove(&id);
    }
}
#[derive(Clone, Copy)]
enum Op {
    Start,
    StartService,
    Request,
    Submit,
    Poll,
    Cancel,
    Stop,
    Terminate,
    State,
    Sleep,
    Crash,
    PollControl,
    Drain,
    Shutdown,
    IsError,
    Class,
    Kind,
    Message,
    ExitStatus,
    RequestId,
    Encode,
    Decode,
    Worker,
    BootError,
}
unsafe fn method(c: Oop, r: Oop, a: *const Oop, n: usize, op: Op) -> Oop {
    done((|| unsafe {
        let s = state(c)?;
        let q = Scope::new(s);
        q.keep(r)?;
        let operands = borrowed_slice(a, n).ok_or_else(invalid)?;
        // Worker/boot-error take native Words; every object input remains retained.
        for v in operands {
            q.keep(*v)?;
        }
        if matches!(op, Op::Worker) {
            return worker(&q, operands);
        }
        if matches!(op, Op::BootError) {
            return boot_error(&q, operands);
        }
        let args = operands
            .iter()
            .map(|v| object(*v))
            .collect::<Result<Vec<_>, _>>()?;
        match op {
            Op::Start => start(&q, &args, false),
            Op::StartService => start(&q, &args, true),
            Op::Request => request(&q, r, &args, false),
            Op::Submit => request(&q, r, &args, true),
            Op::Poll => poll(&q, r, &args),
            Op::Cancel => cancel(&q, r, &args),
            Op::Stop => stop(&q, r, false),
            Op::Terminate => stop(&q, r, true),
            Op::State => {
                let state = q
                    .entry(r)
                    .map(|e| {
                        let mut e = lock_entry(&e);
                        status_of(&mut e);
                        match e.state {
                            Lifecycle::Starting => b"starting".as_slice(),
                            Lifecycle::Ready => b"ready",
                            Lifecycle::Busy => b"busy",
                            Lifecycle::Stopping => b"stopping",
                            Lifecycle::ReapPending => b"reapPending",
                            Lifecycle::Exited => b"exited",
                        }
                    })
                    .unwrap_or(b"stopped");
                Ok(q.symbol(state)?)
            }
            Op::Sleep => {
                let Some(&value) = args.first() else {
                    return q.nil();
                };
                if let Some(ms) = q.word(value)?
                    && (ms as isize) > 0
                {
                    std::thread::sleep(Duration::from_millis(ms as u64));
                }
                drain(false);
                Ok(value)
            }
            Op::Crash => std::process::abort(),
            Op::PollControl => match service::poll_control() {
                1 => q.boolean(true),
                2 => Ok(q.symbol(b"ownerLost")?),
                _ => q.nil(),
            },
            Op::Drain | Op::Shutdown => {
                drain(matches!(op, Op::Shutdown));
                Ok(ptr::null_mut())
            }
            Op::IsError => q.boolean(true),
            Op::Class | Op::Kind | Op::Message | Op::ExitStatus | Op::RequestId => {
                let i = match op {
                    Op::Class => 0,
                    Op::Kind => 1,
                    Op::Message => 2,
                    Op::ExitStatus => 3,
                    _ => 4,
                };
                let o = s.failures.borrow().get(&r.addr()).ok_or_else(invalid)?[i];
                Ok(o)
            }
            Op::Encode => {
                let [v] = args.as_slice() else {
                    return Err(invalid());
                };
                match q.encode(*v) {
                    Ok(b) => q.call(q.named(b"Bytes")?, b"fromBytes:", &[q.byte_value(&b)?]),
                    Err(e) => q.transfer_failure(e),
                }
            }
            Op::Decode => {
                let [v] = args.as_slice() else {
                    return Err(invalid());
                };
                let b = q.bytes(*v)?;
                match q.decode(&b) {
                    Ok(v) => Ok(v),
                    Err(e) => q.transfer_failure(e),
                }
            }
            _ => Err(invalid()),
        }
    })())
}
unsafe extern "C" fn roots(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    done((|| unsafe {
        let s = state(c)?;
        let Some([v]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let mut values = s.permanent.clone();
        values.extend(s.names.values().copied());
        values.extend(s.scoped.borrow().iter().copied());
        for o in values {
            s.send(object(*v)?, b"visit:", &[o])?;
        }
        Ok(ptr::null_mut())
    })())
}
unsafe extern "C" fn references(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    done((|| unsafe {
        let s = state(c)?;
        let Some([o, v]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let fields = s.failures.borrow().get(&object(*o)?.addr()).copied();
        if let Some(fields) = fields {
            for o in fields {
                s.send(object(*v)?, b"visit:", &[o])?;
            }
        }
        Ok(ptr::null_mut())
    })())
}
unsafe extern "C" fn retire(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    done((|| unsafe {
        let s = state(c)?;
        for value in borrowed_slice(a, n).ok_or_else(invalid)? {
            let address = value.addr();
            s.failures.borrow_mut().remove(&address);
            if let Some(t) = s.handles.borrow_mut().remove(&address) {
                if let Some(e) = lookup_entry(t) {
                    lock_entry(&e).orphaned = true;
                }
            }
        }
        Ok(ptr::null_mut())
    })())
}
unsafe fn boot_error(q: &Scope<'_>, a: &[Oop]) -> Answer {
    unsafe {
        let [fd, kind, message] = a else {
            return Err(invalid());
        };
        let fd = abi::as_size(*fd)
            .filter(|fd| *fd <= c_int::MAX as usize)
            .ok_or_else(invalid)? as c_int;
        let kind = q.text(object(*kind)?)?.ok_or_else(invalid)?;
        let message = q.bytes(object(*message)?)?;
        let payload = error_record(BOOT_ERROR_SCHEMA, &kind, &message).map_err(|_| invalid())?;
        let frame = Frame {
            header: Header {
                kind: FrameKind::BootError,
                request_id: 0,
                payload_len: 0,
            },
            payload,
        };
        let mut file = ManuallyDrop::new(File::from_raw_fd(fd));
        write_frame(borrowed_file(&mut file), &frame).map_err(|_| invalid())?;
        Ok(ptr::null_mut())
    }
}
unsafe fn worker(q: &Scope<'_>, a: &[Oop]) -> Answer {
    unsafe {
        let [fd, receiver, selector] = a else {
            return Err(invalid());
        };
        let fd = abi::as_size(*fd)
            .filter(|fd| *fd <= c_int::MAX as usize)
            .ok_or_else(invalid)? as c_int;
        let receiver = object(*receiver)?;
        let selector = object(*selector)?;
        let selector_bytes = q.text(selector)?.ok_or_else(invalid)?;
        if !selector_arity_one(&selector_bytes) {
            return Err(invalid());
        }
        let mut file = ManuallyDrop::new(File::from_raw_fd(fd));
        let channel = borrowed_file(&mut file);
        let task_service = std::env::var("BIRNAM_TASK_SERVICE").is_ok_and(|v| v == "1");
        let _guard = service::DriverGuard;
        let ready = Frame {
            header: Header {
                kind: FrameKind::Ready,
                request_id: 0,
                payload_len: 0,
            },
            payload: Vec::new(),
        };
        if write_frame(channel, &ready).is_err() {
            return Ok(q.raw_word(1)?);
        }
        loop {
            let frame = match if task_service {
                service::read_next(channel).map_err(|_| CodecError::InvalidRecord)
            } else {
                read_frame(channel)
            } {
                Ok(f) => f,
                Err(_) => return Ok(q.raw_word(1)?),
            };
            if frame.header.kind == FrameKind::Stop {
                let stopped = Frame {
                    header: Header {
                        kind: FrameKind::Stopped,
                        request_id: frame.header.request_id,
                        payload_len: 0,
                    },
                    payload: Vec::new(),
                };
                return Ok(q.raw_word(usize::from(write_frame(channel, &stopped).is_err()))?);
            }
            if frame.header.kind != FrameKind::Request {
                return Ok(q.raw_word(1)?);
            }
            let request = Scope::new(q.s);
            let payload = if task_service {
                match service::request_payload(&frame.payload) {
                    Ok(Some(p)) => p,
                    Ok(None) => continue,
                    Err(_) => return Ok(q.raw_word(1)?),
                }
            } else {
                frame.payload
            };
            let argument = match request.decode(&payload) {
                Ok(o) => o,
                Err(e) => {
                    let message = match e {
                        TransferError::Codec(e) => codec_error_name(e),
                        TransferError::Native(_) => b"native decode failure",
                    };
                    let payload = error_record(PROTOCOL_ERROR_SCHEMA, b"invalidPayload", message)
                        .map_err(|_| invalid())?;
                    let _ = write_frame(
                        channel,
                        &Frame {
                            header: Header {
                                kind: FrameKind::ProtocolError,
                                request_id: frame.header.request_id,
                                payload_len: 0,
                            },
                            payload,
                        },
                    );
                    return Ok(q.raw_word(1)?);
                }
            };
            let result = if task_service {
                if service::begin(fd, frame.header.request_id).is_err() {
                    return Ok(q.raw_word(1)?);
                }
                let policy = request.obj(
                    request.named(b"Globals")?,
                    b"at:",
                    &[request.symbol(b"TaskService")?],
                )?;
                request.obj(policy, b"serve:request:", &[receiver, argument])
            } else {
                let result = q.s.id.send(receiver, selector, &[argument]);
                result.and_then(object).and_then(|o| request.keep(o))
            };
            let (kind, payload) = match result {
                Err(_) => (
                    FrameKind::ReplyError,
                    error_record(
                        APPLICATION_ERROR_SCHEMA,
                        b"nativeFailure",
                        b"worker dispatch failed",
                    )
                    .map_err(|_| invalid())?,
                ),
                Ok(result) => {
                    if request.call(result, b"isError", &[])? == request.named(b"true")? {
                        let kind = request.obj(result, b"kind", &[])?;
                        let message = request.obj(result, b"message", &[])?;
                        let kind = request.text(kind)?.ok_or_else(invalid)?;
                        let message = if message == request.named(b"nil")? {
                            Vec::new()
                        } else {
                            request.bytes(message)?
                        };
                        (
                            FrameKind::ReplyError,
                            error_record(APPLICATION_ERROR_SCHEMA, &kind, &message)
                                .map_err(|_| invalid())?,
                        )
                    } else {
                        match request.encode(result) {
                            Ok(p) => (FrameKind::ReplyOk, p),
                            Err(e) => {
                                let message = match e {
                                    TransferError::Codec(e) => codec_error_name(e),
                                    TransferError::Native(_) => b"native encode failure",
                                };
                                (
                                    FrameKind::ReplyError,
                                    error_record(
                                        APPLICATION_ERROR_SCHEMA,
                                        b"encodeFailed",
                                        message,
                                    )
                                    .map_err(|_| invalid())?,
                                )
                            }
                        }
                    }
                }
            };
            let reply = Frame {
                header: Header {
                    kind,
                    request_id: frame.header.request_id,
                    payload_len: 0,
                },
                payload,
            };
            if write_frame(channel, &reply).is_err() {
                return Ok(q.raw_word(1)?);
            }
            drop(request);
            drain(false);
        }
    }
}
unsafe extern "C" fn op_start(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { method(c, r, a, n, Op::Start) }
}
unsafe extern "C" fn op_startservice(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { method(c, r, a, n, Op::StartService) }
}
unsafe extern "C" fn op_sleep(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { method(c, r, a, n, Op::Sleep) }
}
unsafe extern "C" fn op_crash(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { method(c, r, a, n, Op::Crash) }
}
unsafe extern "C" fn op_pollcontrol(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { method(c, r, a, n, Op::PollControl) }
}
unsafe extern "C" fn op_drain(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { method(c, r, a, n, Op::Drain) }
}
unsafe extern "C" fn op_shutdown(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { method(c, r, a, n, Op::Shutdown) }
}
unsafe extern "C" fn op_encode(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { method(c, r, a, n, Op::Encode) }
}
unsafe extern "C" fn op_decode(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { method(c, r, a, n, Op::Decode) }
}
unsafe extern "C" fn op_worker(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { method(c, r, a, n, Op::Worker) }
}
unsafe extern "C" fn op_booterror(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { method(c, r, a, n, Op::BootError) }
}
unsafe extern "C" fn op_request(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { method(c, r, a, n, Op::Request) }
}
unsafe extern "C" fn op_submit(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { method(c, r, a, n, Op::Submit) }
}
unsafe extern "C" fn op_poll(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { method(c, r, a, n, Op::Poll) }
}
unsafe extern "C" fn op_cancel(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { method(c, r, a, n, Op::Cancel) }
}
unsafe extern "C" fn op_stop(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { method(c, r, a, n, Op::Stop) }
}
unsafe extern "C" fn op_terminate(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { method(c, r, a, n, Op::Terminate) }
}
unsafe extern "C" fn op_state(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { method(c, r, a, n, Op::State) }
}
unsafe extern "C" fn op_iserror(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { method(c, r, a, n, Op::IsError) }
}
unsafe extern "C" fn op_class(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { method(c, r, a, n, Op::Class) }
}
unsafe extern "C" fn op_kind(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { method(c, r, a, n, Op::Kind) }
}
unsafe extern "C" fn op_message(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { method(c, r, a, n, Op::Message) }
}
unsafe extern "C" fn op_exitstatus(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { method(c, r, a, n, Op::ExitStatus) }
}
unsafe extern "C" fn op_requestid(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    unsafe { method(c, r, a, n, Op::RequestId) }
}
unsafe extern "C" fn initialize(
    p: *const Protocol,
    image: Oop,
    l: *const Oop,
    n: usize,
    a: *const Oop,
    count: usize,
) -> Oop {
    API.store(p.cast_mut(), Ordering::Release);
    done((|| unsafe {
        let id = *p;
        let [ns] = borrowed_slice(a, count).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let symbols = borrowed_slice(l, n)
            .filter(|v| v.len() == NAMES.len())
            .ok_or_else(invalid)?;
        let old = INSTANCE.load(Ordering::Acquire);
        if !old.is_null() {
            let s = &*old;
            return if s.ready.get() && s.ns == *ns && s.id.root == id.root {
                Ok(s.receiver)
            } else {
                Err(invalid())
            };
        }
        let names: HashMap<_, _> = NAMES.iter().copied().zip(symbols.iter().copied()).collect();
        let send = |r, n: &[u8], a: &[Oop]| id.send(r, names[n], a);
        let named = |n: &[u8]| send(*ns, b"at:", &[names[n]]);
        let roots = named(b"Roots")?;
        let model = named(b"ObjectModel")?;
        let gc = named(b"GC")?;
        let mut tokens = Vec::new();
        let mut keep = |v| -> Result<Oop, Oop> {
            tokens.push(send(roots, b"retain:", &[v])?);
            Ok(v)
        };
        let vt = keep(send(named(b"ObjectVt")?, b"delegated", &[])?)?;
        let hv = keep(send(named(b"ObjectVt")?, b"delegated", &[])?)?;
        let fv = keep(send(named(b"ErrorVt")?, b"delegated", &[])?)?;
        let receiver = keep(send(vt, b"allocate", &[abi::size(0).unwrap()])?)?;
        let s = Box::leak(Box::new(State {
                byte_reader: abi::byte_transfer::Reader::new(id, image, roots, |n| names[n])?,
            id,
            names,
            ns: *ns,
            model,
            receiver,
            handle_vt: hv,
            failure_vt: fv,
            ready: Cell::new(false),
            scoped: RefCell::new(Vec::new()),
            permanent: vec![*ns, roots, gc, model, vt, hv, fv, receiver],
            handles: RefCell::new(HashMap::new()),
            failures: RefCell::new(HashMap::new()),
            thread: std::thread::current().id(),
        }));
        INSTANCE.store(s, Ordering::Release);
        for (i, (target, name)) in INSTALL.iter().enumerate() {
            let m = s.send(image, b"newMethodAt:", &[abi::size(i).unwrap()])?;
            let m = s.send(m, b"withContext:", &[*ns])?;
            s.send([vt, hv, fv][*target], b"addMethod", &[s.intern(name)?, m])?;
        }
        s.send(roots, b"register:", &[receiver])?;
        s.send(gc, b"registerRetirement:", &[receiver])?;
        s.send(gc, b"trace:with:", &[fv, receiver])?;
        for (name, value) in [
            (b"Isolate".as_slice(), receiver),
            (b"IsolateVt", vt),
            (b"IsolateHandleVt", hv),
            (b"IsolateFailureVt", fv),
        ] {
            s.send(*ns, b"at:put:", &[s.intern(name)?, value])?;
        }
        s.send(id.root, b"ownerOf:put:", &[vt, receiver])?;
        for t in tokens {
            s.send(roots, b"release:", &[t])?;
        }
        s.ready.set(true);
        Ok(receiver)
    })())
}
static INSTALL: &[(usize, &[u8])] = &[
    (0, b"start:selector:image:"),
    (0, b"startService:image:"),
    (0, b"idleFor:"),
    (0, b"_isolateSleep:"),
    (0, b"_isolateCrash:"),
    (0, b"pollServiceControl"),
    (0, b"checkpoint"),
    (0, b"shutdown"),
    (0, b"encode:"),
    (0, b"decode:"),
    (0, b"runWorker:receiver:selector:"),
    (0, b"workerBootError:kind:message:"),
    (1, b"request:"),
    (1, b"request:timeout:"),
    (1, b"submit:timeout:"),
    (1, b"pollRequest:"),
    (1, b"cancelRequest:"),
    (1, b"stop"),
    (1, b"terminate"),
    (1, b"state"),
    (2, b"isError"),
    (2, b"class"),
    (2, b"kind"),
    (2, b"message"),
    (2, b"exitStatus"),
    (2, b"requestId"),
    (0, b"rootsDo:"),
    (0, b"referencesOf:do:"),
    (0, b"retire:"),
];
macro_rules! literals { ($($v:literal),*$(,)?) => {
 const NAMES:&[&[u8]]=&[$($v),*];
 static LITERALS:&[Literal]=&[$(Literal::new($v)),*];
}; }
literals![
    b"at:",
    b"sequence",
    b"String",
    b"fromBytes:",
    b"Word",
    b"fromUnsignedLow:high:",
    b"low32",
    b"high32",
    b"Integer",
    b"fromWord:",
    b"wordOf:",
    b"bytes",
    b"sizeWord",
    b"atWord:",
    b"nil",
    b"true",
    b"false",
    b"SymbolVt",
    b"behaviorOf:",
    b"StringVt",
    b"allocate:bytes:",
    b"encode",
    b"nonTransferable",
    b"nameWithBytes",
    b"stopped",
    b"unknownHandle",
    b"busy",
    b"notReady",
    b"protocol",
    b"invalidRecord",
    b"invalidArguments",
    b"invalidReceiver",
    b"handle:",
    b"invalidSelector",
    b"invalidImage",
    b"embeddedNul",
    b"boot",
    b"wrongArity",
    b"expectedReady",
    b"handshakeTimeout",
    b"invalidReady",
    b"handshakeFailed",
    b"timeout",
    b"deadline",
    b"transport",
    b"uncertain",
    b"invalidReply",
    b"resultTransfer",
    b"application",
    b"Vector",
    b"new",
    b"add:",
    b"invalidTimeout",
    b"asyncRequired",
    b"requestIdExhausted",
    b"invalidRequest",
    b"unavailable",
    b"exit",
    b"invalidRequestId",
    b"staleRequest",
    b"invalidStopReply",
    b"starting",
    b"ready",
    b"stopping",
    b"reapPending",
    b"exited",
    b"ownerLost",
    b"Bytes",
    b"visit:",
    b"invalidPayload",
    b"Globals",
    b"TaskService",
    b"serve:request:",
    b"nativeFailure",
    b"isError",
    b"kind",
    b"message",
    b"encodeFailed",
    b"Roots",
    b"ObjectModel",
    b"GC",
    b"retain:",
    b"ObjectVt",
    b"delegated",
    b"ErrorVt",
    b"allocate",
    b"newMethodAt:",
    b"withContext:",
    b"addMethod",
    b"register:",
    b"registerRetirement:",
    b"trace:with:",
    b"Isolate",
    b"IsolateVt",
    b"IsolateHandleVt",
    b"IsolateFailureVt",
    b"at:put:",
    b"ownerOf:put:",
    b"release:",
    b"start:selector:image:",
    b"startService:image:",
    b"idleFor:",
    b"_isolateSleep:",
    b"_isolateCrash:",
    b"pollServiceControl",
    b"checkpoint",
    b"shutdown",
    b"encode:",
    b"decode:",
    b"runWorker:receiver:selector:",
    b"workerBootError:kind:message:",
    b"request:",
    b"request:timeout:",
    b"submit:timeout:",
    b"pollRequest:",
    b"cancelRequest:",
    b"stop",
    b"terminate",
    b"state",
    b"class",
    b"exitStatus",
    b"requestId",
    b"rootsDo:",
    b"referencesOf:do:",
    b"retire:",
    b"size",
    b"IntegerVt",
    b"FloatVt",
    b"Float",
    b"bitsOf:",
    b"VectorVt",
    b"MapVt",
    b"keys",
    b"fromBits:",
    b"Map",
    b"includesKey:",
    b"copyByteValuesFrom:count:to:",
    b"appendByteValues",
    b"objectBehavior",
    b"methodAt:",
    b"releaseTokensProtecting",
    b"receiveByteValues",
];
static METHODS: &[MethodSpec] = &[
    MethodSpec {
        name: 99,
        entry: Some(op_start),
    },
    MethodSpec {
        name: 100,
        entry: Some(op_startservice),
    },
    MethodSpec {
        name: 101,
        entry: Some(op_sleep),
    },
    MethodSpec {
        name: 102,
        entry: Some(op_sleep),
    },
    MethodSpec {
        name: 103,
        entry: Some(op_crash),
    },
    MethodSpec {
        name: 104,
        entry: Some(op_pollcontrol),
    },
    MethodSpec {
        name: 105,
        entry: Some(op_drain),
    },
    MethodSpec {
        name: 106,
        entry: Some(op_shutdown),
    },
    MethodSpec {
        name: 107,
        entry: Some(op_encode),
    },
    MethodSpec {
        name: 108,
        entry: Some(op_decode),
    },
    MethodSpec {
        name: 109,
        entry: Some(op_worker),
    },
    MethodSpec {
        name: 110,
        entry: Some(op_booterror),
    },
    MethodSpec {
        name: 111,
        entry: Some(op_request),
    },
    MethodSpec {
        name: 112,
        entry: Some(op_request),
    },
    MethodSpec {
        name: 113,
        entry: Some(op_submit),
    },
    MethodSpec {
        name: 114,
        entry: Some(op_poll),
    },
    MethodSpec {
        name: 115,
        entry: Some(op_cancel),
    },
    MethodSpec {
        name: 116,
        entry: Some(op_stop),
    },
    MethodSpec {
        name: 117,
        entry: Some(op_terminate),
    },
    MethodSpec {
        name: 118,
        entry: Some(op_state),
    },
    MethodSpec {
        name: 74,
        entry: Some(op_iserror),
    },
    MethodSpec {
        name: 119,
        entry: Some(op_class),
    },
    MethodSpec {
        name: 75,
        entry: Some(op_kind),
    },
    MethodSpec {
        name: 76,
        entry: Some(op_message),
    },
    MethodSpec {
        name: 120,
        entry: Some(op_exitstatus),
    },
    MethodSpec {
        name: 121,
        entry: Some(op_requestid),
    },
    MethodSpec {
        name: 122,
        entry: Some(roots),
    },
    MethodSpec {
        name: 123,
        entry: Some(references),
    },
    MethodSpec {
        name: 124,
        entry: Some(retire),
    },

    MethodSpec { name: NAMES.len()-1, entry: Some(abi::byte_transfer::receive_entry) },
];
#[unsafe(no_mangle)]
pub static birnam_native_module_v2: Module = Module {
    version: 2,
    byte_size: size_of::<Module>() as u32,
    literals: LITERALS.as_ptr(),
    literal_count: LITERALS.len(),
    methods: METHODS.as_ptr(),
    method_count: METHODS.len(),
    initialize: Some(initialize),
};
