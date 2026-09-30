//! Process-local Isolate objects communicate with other owners only through Id.
use super::*;
use birnam_abi::{
    Oop, borrowed_bytes, borrowed_slice,
    id_protocol::{self as native, Method, NativeEntry, Operand, Outcome, Protocol},
};
use core::{cell::RefCell, ffi::c_void, ptr};
mod transfer;
use transfer::{TransferError, capture, materialize};
type Answer = Result<Operand, Outcome>;
fn invalid() -> Outcome {
    Outcome::failed(native::INVALID_REQUEST, 0)
}
fn object(a: Operand) -> Result<Oop, Outcome> {
    a.as_object().filter(|o| !o.is_null()).ok_or_else(invalid)
}
fn done(r: Answer) -> Outcome {
    r.map_or_else(|e| e, Outcome::loaded)
}
struct State {
    id: Protocol,
    ns: Oop,
    roots: Oop,
    gc: Oop,
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
    tokens: RefCell<Vec<usize>>,
}
impl Scope<'_> {
    unsafe fn keep(&self, o: Oop) -> Result<Oop, Outcome> {
        unsafe {
            let t = self
                .s
                .id
                .send_named(self.s.roots, b"retain:", &[Operand::object(o)])?
                .as_word()
                .ok_or_else(invalid)?;
            self.tokens.borrow_mut().push(t);
            Ok(o)
        }
    }
    unsafe fn call(&self, r: Oop, n: &[u8], a: &[Operand]) -> Answer {
        unsafe {
            let v = self.s.id.send_named(r, n, a)?;
            if let Some(o) = v.as_object() {
                self.keep(o)?;
            }
            Ok(v)
        }
    }
    unsafe fn obj(&self, r: Oop, n: &[u8], a: &[Operand]) -> Result<Oop, Outcome> {
        object(unsafe { self.call(r, n, a) }?)
    }
    unsafe fn symbol(&self, n: &[u8]) -> Result<Oop, Outcome> {
        unsafe { self.keep(self.s.id.intern_name(n)?) }
    }
    unsafe fn named(&self, n: &[u8]) -> Result<Oop, Outcome> {
        unsafe { self.obj(self.s.ns, b"at:", &[Operand::object(self.symbol(n)?)]) }
    }
    unsafe fn int(&self, w: usize) -> Result<Oop, Outcome> {
        unsafe { self.obj(self.named(b"Integer")?, b"fromWord:", &[Operand::word(w)]) }
    }
    unsafe fn word(&self, o: Oop) -> Result<Option<usize>, Outcome> {
        unsafe {
            Ok(self
                .call(self.named(b"Integer")?, b"wordOf:", &[Operand::object(o)])?
                .as_word())
        }
    }
    unsafe fn string(&self, b: &[u8]) -> Result<Oop, Outcome> {
        unsafe { self.obj(self.named(b"String")?, b"fromBytes:", &[Operand::bytes(b)]) }
    }
    unsafe fn nil(&self) -> Answer {
        unsafe { Ok(Operand::object(self.named(b"nil")?)) }
    }
    unsafe fn boolean(&self, b: bool) -> Answer {
        unsafe {
            Ok(Operand::object(self.named(if b {
                b"true"
            } else {
                b"false"
            })?))
        }
    }
    unsafe fn bytes(&self, o: Oop) -> Result<Vec<u8>, Outcome> {
        unsafe {
            let b = self
                .call(o, b"bytes", &[])?
                .as_bytes()
                .ok_or_else(invalid)?;
            Ok(borrowed_bytes(b.pointer, b.length)
                .ok_or_else(invalid)?
                .to_vec())
        }
    }
    unsafe fn text(&self, o: Oop) -> Result<Option<Vec<u8>>, Outcome> {
        unsafe {
            let symbol = self.call(self.s.id.receiver, b"symbolBytesOf:", &[Operand::object(o)])?;
            if let Some(b) = symbol.as_bytes() {
                return Ok(Some(
                    borrowed_bytes(b.pointer, b.length)
                        .ok_or_else(invalid)?
                        .to_vec(),
                ));
            }
            if self.obj(self.s.id.receiver, b"behaviorOf:", &[Operand::object(o)])?
                == self.named(b"StringVt")?
            {
                Ok(Some(self.bytes(o)?))
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
                &[Operand::object(self.s.failure_vt), Operand::word(0)],
            )?;
            self.s.failures.borrow_mut().insert(o.addr(), fields);
            Ok(Operand::object(o))
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
        for t in self.tokens.get_mut().drain(..).rev() {
            unsafe {
                self.s
                    .id
                    .send_named(self.s.roots, b"release:", &[Operand::word(t)])
                    .expect("release Isolate root");
            }
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
            .and_then(|c| host_read_frame(c, 10_000));
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
            &[Operand::object(q.s.handle_vt), Operand::word(0)],
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
        Ok(Operand::object(value))
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
                            q.call(vector, b"add:", &[Operand::object(v)])
                        })()
                    } else {
                        Ok(Operand::object(v))
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
            Ok((id, None)) => Ok(Operand::object(q.int(id as usize)?)),
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
            return Ok(Operand::object(r));
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
            Ok(()) => Ok(Operand::object(r)),
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
struct EntryContext {
    state: *const State,
    op: Op,
}
unsafe extern "C" fn method(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let context = &*c.cast::<EntryContext>();
        let s = &*context.state;
        if s.thread != std::thread::current().id() {
            return Err(invalid());
        }
        let q = Scope {
            s,
            tokens: RefCell::new(Vec::new()),
        };
        q.keep(r)?;
        let operands = borrowed_slice(a, n).ok_or_else(invalid)?;
        // Worker/boot-error take native Words; every object input remains retained.
        for v in operands {
            if let Some(o) = v.as_object() {
                q.keep(o)?;
            }
        }
        if matches!(context.op, Op::Worker) {
            return worker(&q, operands);
        }
        if matches!(context.op, Op::BootError) {
            return boot_error(&q, operands);
        }
        let args = operands
            .iter()
            .map(|v| object(*v))
            .collect::<Result<Vec<_>, _>>()?;
        match context.op {
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
                Ok(Operand::object(q.symbol(state)?))
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
                Ok(Operand::object(value))
            }
            Op::Crash => std::process::abort(),
            Op::PollControl => match service::poll_control() {
                1 => q.boolean(true),
                2 => Ok(Operand::object(q.symbol(b"ownerLost")?)),
                _ => q.nil(),
            },
            Op::Drain | Op::Shutdown => {
                drain(matches!(context.op, Op::Shutdown));
                Ok(Operand::unit())
            }
            Op::IsError => q.boolean(true),
            Op::Class | Op::Kind | Op::Message | Op::ExitStatus | Op::RequestId => {
                let i = match context.op {
                    Op::Class => 0,
                    Op::Kind => 1,
                    Op::Message => 2,
                    Op::ExitStatus => 3,
                    _ => 4,
                };
                let o = s.failures.borrow().get(&r.addr()).ok_or_else(invalid)?[i];
                Ok(Operand::object(o))
            }
            Op::Encode => {
                let [v] = args.as_slice() else {
                    return Err(invalid());
                };
                match q.encode(*v) {
                    Ok(b) => q.call(q.named(b"Bytes")?, b"fromBytes:", &[Operand::bytes(&b)]),
                    Err(e) => q.transfer_failure(e),
                }
            }
            Op::Decode => {
                let [v] = args.as_slice() else {
                    return Err(invalid());
                };
                let b = q.bytes(*v)?;
                match q.decode(&b) {
                    Ok(v) => Ok(Operand::object(v)),
                    Err(e) => q.transfer_failure(e),
                }
            }
            _ => Err(invalid()),
        }
    })())
}
unsafe extern "C" fn roots(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s = &*c.cast::<State>();
        let Some([v]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        for &o in &s.permanent {
            s.id.send_named(object(*v)?, b"visit:", &[Operand::object(o)])?;
        }
        Ok(Operand::unit())
    })())
}
unsafe extern "C" fn references(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s = &*c.cast::<State>();
        let Some([o, v]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let fields = s.failures.borrow().get(&object(*o)?.addr()).copied();
        if let Some(fields) = fields {
            for o in fields {
                s.id.send_named(object(*v)?, b"visit:", &[Operand::object(o)])?;
            }
        }
        Ok(Operand::unit())
    })())
}
unsafe extern "C" fn retire(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s = &*c.cast::<State>();
        let Some([v]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let b = v.as_bytes().ok_or_else(invalid)?;
        let b = borrowed_bytes(b.pointer, b.length).ok_or_else(invalid)?;
        if b.len() % size_of::<usize>() != 0 {
            return Err(invalid());
        }
        for b in b.chunks_exact(size_of::<usize>()) {
            let address = usize::from_ne_bytes(b.try_into().unwrap());
            s.failures.borrow_mut().remove(&address);
            if let Some(t) = s.handles.borrow_mut().remove(&address) {
                if let Some(e) = lookup_entry(t) {
                    lock_entry(&e).orphaned = true;
                }
            }
        }
        Ok(Operand::unit())
    })())
}
unsafe fn construct(id: Protocol, ns: Oop) -> Answer {
    unsafe {
        let named =
            |n: &[u8]| object(id.send_named(ns, b"at:", &[Operand::object(id.intern_name(n)?)])?);
        let existing =
            id.send_named(ns, b"at:", &[Operand::object(id.intern_name(b"Isolate")?)])?;
        if existing.as_object().is_some() {
            return Ok(existing);
        }
        let mut s = Box::new(State {
            id,
            ns,
            roots: named(b"Roots")?,
            gc: named(b"GC")?,
            model: named(b"ObjectModel")?,
            handle_vt: ptr::null_mut(),
            failure_vt: ptr::null_mut(),
            permanent: Vec::new(),
            handles: RefCell::new(HashMap::new()),
            failures: RefCell::new(HashMap::new()),
            thread: std::thread::current().id(),
        });
        let q = Scope {
            s: &s,
            tokens: RefCell::new(Vec::new()),
        };
        let vt = q.obj(named(b"ObjectVt")?, b"delegated", &[])?;
        let hv = q.obj(named(b"ObjectVt")?, b"delegated", &[])?;
        let fv = q.obj(named(b"ErrorVt")?, b"delegated", &[])?;
        let receiver = q.obj(
            s.model,
            b"allocate:bytes:",
            &[Operand::object(vt), Operand::word(0)],
        )?;
        let tokens = q.tokens.take();
        drop(q);
        s.handle_vt = hv;
        s.failure_vt = fv;
        s.permanent = vec![ns, s.roots, s.gc, s.model, vt, hv, fv, receiver];
        let s = Box::into_raw(s);
        for (target, name, op) in [
            (vt, b"start:selector:image:".as_slice(), Op::Start),
            (vt, b"startService:image:", Op::StartService),
            (vt, b"idleFor:", Op::Sleep),
            (vt, b"_isolateSleep:", Op::Sleep),
            (vt, b"_isolateCrash:", Op::Crash),
            (vt, b"pollServiceControl", Op::PollControl),
            (vt, b"checkpoint", Op::Drain),
            (vt, b"shutdown", Op::Shutdown),
            (vt, b"encode:", Op::Encode),
            (vt, b"decode:", Op::Decode),
            (vt, b"runWorker:receiver:selector:", Op::Worker),
            (vt, b"workerBootError:kind:message:", Op::BootError),
            (hv, b"request:", Op::Request),
            (hv, b"request:timeout:", Op::Request),
            (hv, b"submit:timeout:", Op::Submit),
            (hv, b"pollRequest:", Op::Poll),
            (hv, b"cancelRequest:", Op::Cancel),
            (hv, b"stop", Op::Stop),
            (hv, b"terminate", Op::Terminate),
            (hv, b"state", Op::State),
            (fv, b"isError", Op::IsError),
            (fv, b"class", Op::Class),
            (fv, b"kind", Op::Kind),
            (fv, b"message", Op::Message),
            (fv, b"exitStatus", Op::ExitStatus),
            (fv, b"requestId", Op::RequestId),
        ] {
            let c = Box::into_raw(Box::new(EntryContext { state: s, op }));
            id.send_named(
                target,
                b"addMethod",
                &[
                    Operand::object(id.intern_name(name)?),
                    Operand::entry(NativeEntry::new(method, c.cast())),
                ],
            )?;
        }
        for (name, f) in [
            (b"rootsDo:".as_slice(), roots as Method),
            (b"referencesOf:do:", references),
            (b"retire:", retire),
        ] {
            id.send_named(
                vt,
                b"addMethod",
                &[
                    Operand::object(id.intern_name(name)?),
                    Operand::entry(NativeEntry::new(f, s.cast())),
                ],
            )?;
        }
        id.send_named((*s).roots, b"register:", &[Operand::object(receiver)])?;
        id.send_named(
            (*s).gc,
            b"registerRetirement:",
            &[Operand::object(receiver)],
        )?;
        id.send_named(
            (*s).gc,
            b"trace:with:",
            &[Operand::object(fv), Operand::object(receiver)],
        )?;
        for (n, o) in [
            (b"Isolate".as_slice(), receiver),
            (b"IsolateVt", vt),
            (b"IsolateHandleVt", hv),
            (b"IsolateFailureVt", fv),
        ] {
            id.send_named(
                ns,
                b"at:put:",
                &[Operand::object(id.intern_name(n)?), Operand::object(o)],
            )?;
        }
        id.send_named(
            id.receiver,
            b"ownerOf:put:",
            &[Operand::object(vt), Operand::object(receiver)],
        )?;
        for t in tokens {
            id.send_named((*s).roots, b"release:", &[Operand::word(t)])?;
        }
        Ok(Operand::object(receiver))
    }
}
unsafe extern "C" fn initialize(id: *const Protocol, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        if !native::valid_protocol(id) {
            return Err(invalid());
        }
        let Some([ns]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        construct(*id, object(*ns)?)
    })())
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: native::Module = native::Module {
    version: 1,
    byte_size: size_of::<native::Module>() as u32,
    initialize: Some(initialize),
};
unsafe fn boot_error(q: &Scope<'_>, a: &[Operand]) -> Answer {
    unsafe {
        let [fd, kind, message] = a else {
            return Err(invalid());
        };
        let fd = fd
            .as_word()
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
        Ok(Operand::unit())
    }
}
unsafe fn worker(q: &Scope<'_>, a: &[Operand]) -> Answer {
    unsafe {
        let [fd, receiver, selector] = a else {
            return Err(invalid());
        };
        let fd = fd
            .as_word()
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
            return Ok(Operand::word(1));
        }
        loop {
            let frame = match if task_service {
                service::read_next(channel).map_err(|_| CodecError::InvalidRecord)
            } else {
                read_frame(channel)
            } {
                Ok(f) => f,
                Err(_) => return Ok(Operand::word(1)),
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
                return Ok(Operand::word(usize::from(
                    write_frame(channel, &stopped).is_err(),
                )));
            }
            if frame.header.kind != FrameKind::Request {
                return Ok(Operand::word(1));
            }
            let request = Scope {
                s: q.s,
                tokens: RefCell::new(Vec::new()),
            };
            let payload = if task_service {
                match service::request_payload(&frame.payload) {
                    Ok(Some(p)) => p,
                    Ok(None) => continue,
                    Err(_) => return Ok(Operand::word(1)),
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
                    return Ok(Operand::word(1));
                }
            };
            let result = if task_service {
                if service::begin(fd, frame.header.request_id).is_err() {
                    return Ok(Operand::word(1));
                }
                let policy = request.obj(
                    request.named(b"Globals")?,
                    b"at:",
                    &[Operand::object(request.symbol(b"TaskService")?)],
                )?;
                request.obj(
                    policy,
                    b"serve:request:",
                    &[Operand::object(receiver), Operand::object(argument)],
                )
            } else {
                let result =
                    q.s.id
                        .send(receiver, selector, &[Operand::object(argument)]);
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
                    if request.call(result, b"isError", &[])?.as_object()
                        == Some(request.named(b"true")?)
                    {
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
                return Ok(Operand::word(1));
            }
            drop(request);
            drain(false);
        }
    }
}
