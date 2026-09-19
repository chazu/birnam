//! Bounded, nonblocking process transport. Only the owner creates floor objects.
//! One outstanding request per worker; no background Oops or native activations.
use super::*;
use std::time::Instant;

pub(super) struct Pending {
    id: u64,
    outgoing: Vec<u8>,
    written: usize,
    incoming: Vec<u8>,
    expected: usize,
    deadline: Instant,
    result: Option<Result<Frame, HostTransportError>>,
    cancellation_queued: bool,
}

impl Pending {
    fn new(id: u64, payload: Vec<u8>, timeout: u32) -> Result<Self, HostTransportError> {
        let outgoing = Frame {
            header: Header {
                kind: FrameKind::Request,
                request_id: id,
                payload_len: 0,
            },
            payload,
        }
        .encode()
        .map_err(|_| HostTransportError::Protocol)?;
        Ok(Self {
            id,
            outgoing,
            written: 0,
            incoming: Vec::new(),
            expected: HEADER_LEN,
            deadline: Instant::now() + Duration::from_millis(timeout as u64),
            result: None,
            cancellation_queued: false,
        })
    }

    fn progress(&mut self, stream: &mut UnixStream) {
        if self.result.is_some() {
            return;
        }
        if Instant::now() >= self.deadline {
            self.result = Some(Err(HostTransportError::Timeout));
            return;
        }
        // A polling turn has bounded byte work, even for large frames. Socket
        // readiness is advisory; WouldBlock always yields to the owner again.
        let answer = (|| {
            let mut budget = 64 * 1024;
            while self.written < self.outgoing.len() && budget > 0 {
                let end = self.outgoing.len().min(self.written + budget);
                match stream.write(&self.outgoing[self.written..end]) {
                    Ok(0) => return Err(HostTransportError::Io),
                    Ok(n) => {
                        self.written += n;
                        budget -= n;
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(None),
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => return Ok(None),
                    Err(_) => return Err(HostTransportError::Io),
                }
            }
            if self.written != self.outgoing.len() {
                return Ok(None);
            }
            while budget > 0 {
                if self.incoming.len() == self.expected {
                    if self.expected == HEADER_LEN {
                        let h = decode_header(&self.incoming)
                            .map_err(|_| HostTransportError::Protocol)?;
                        self.expected = HEADER_LEN + h.payload_len as usize;
                    }
                    if self.incoming.len() == self.expected {
                        return Frame::decode(&self.incoming)
                            .map(Some)
                            .map_err(|_| HostTransportError::Protocol);
                    }
                }
                let mut buffer = [0u8; 8192];
                let count = buffer
                    .len()
                    .min(self.expected - self.incoming.len())
                    .min(budget);
                match stream.read(&mut buffer[..count]) {
                    Ok(0) => return Err(HostTransportError::Io),
                    Ok(n) => {
                        self.incoming.extend_from_slice(&buffer[..n]);
                        budget -= n;
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(None),
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => return Ok(None),
                    Err(_) => return Err(HostTransportError::Io),
                }
            }
            Ok(None)
        })();
        match answer {
            Ok(Some(frame)) => self.result = Some(Ok(frame)),
            Err(error) => self.result = Some(Err(error)),
            Ok(None) => {}
        }
    }
}

pub(super) unsafe extern "C" fn submit(handle: Oop, args: *const Oop, count: usize) -> Oop {
    let api = runtime_api();
    let Some(args) = (unsafe { birnam_abi::borrowed_oop_args(args, count) }) else {
        return api.nilObj;
    };
    if args.len() != 2
        || unsafe { (birnam_abi::integer::is_integer)(args[1]) } == 0
        || unsafe { (birnam_abi::integer::value)(args[1]) } < 0
    {
        return make_failure(
            api,
            b"encode",
            b"invalidArguments",
            b"submit expects a value and nonnegative timeout",
        );
    }
    // Copy before acquiring native registry state: encoders use ordinary sends.
    let payload = match encode_oop_value(api, args[0]) {
        Ok(payload) => payload,
        Err(error) => {
            return make_failure(api, b"encode", b"nonTransferable", codec_error_name(error));
        }
    };
    let Some(shared) = lookup_entry(handle_object(handle).token) else {
        return make_failure(api, b"stopped", b"unknownHandle", b"unknown isolate handle");
    };
    let mut entry = lock_entry(&shared);
    if entry.state != Lifecycle::Ready || entry.pending.is_some() {
        return make_failure(
            api,
            b"busy",
            b"notReady",
            b"one outstanding request is allowed per worker",
        );
    }
    let id = entry.next_request;
    if id == 0 || id > i64::MAX as u64 {
        return make_failure(
            api,
            b"protocol",
            b"requestIdExhausted",
            b"request id space exhausted",
        );
    }
    let timeout = (unsafe { (birnam_abi::integer::value)(args[1]) } as u64).min(u32::MAX as u64) as u32;
    let payload = if entry.task_service {
        match service::wrap_request(&payload) {
            Ok(payload) => payload,
            Err(error) => {
                return make_failure(api, b"encode", b"nonTransferable", codec_error_name(error));
            }
        }
    } else {
        payload
    };
    let pending = match Pending::new(id, payload, timeout) {
        Ok(pending) => pending,
        Err(_) => {
            return make_failure(
                api,
                b"encode",
                b"invalidRequest",
                b"request could not be encoded",
            );
        }
    };
    if entry
        .channel
        .as_ref()
        .is_none_or(|s| s.set_nonblocking(true).is_err())
    {
        return make_failure(
            api,
            b"transport",
            b"unavailable",
            b"worker channel is unavailable",
        );
    }
    entry.next_request += 1;
    entry.state = Lifecycle::Busy;
    entry.pending = Some(pending);
    unsafe { (birnam_abi::integer::make)(id as i64) }
}

pub(super) unsafe extern "C" fn poll(handle: Oop, args: *const Oop, count: usize) -> Oop {
    let api = runtime_api();
    let Some(args) = (unsafe { birnam_abi::borrowed_oop_args(args, count) }) else {
        return api.nilObj;
    };
    if args.len() != 1 || unsafe { (birnam_abi::integer::is_integer)(args[0]) } == 0 {
        return make_failure(
            api,
            b"protocol",
            b"invalidRequestId",
            b"poll expects a request id",
        );
    }
    let id = unsafe { (birnam_abi::integer::value)(args[0]) } as u64;
    let Some(shared) = lookup_entry(handle_object(handle).token) else {
        return make_failure(api, b"stopped", b"unknownHandle", b"unknown isolate handle");
    };
    let task_service;
    let result = {
        let mut entry = lock_entry(&shared);
        if entry.pending.as_ref().is_none_or(|p| p.id != id) {
            return make_failure(
                api,
                b"protocol",
                b"staleRequest",
                b"request is not outstanding on this worker",
            );
        }
        task_service = entry.task_service;
        let mut pending = entry.pending.take().expect("checked pending request");
        if let Some(channel) = entry.channel.as_mut() {
            pending.progress(channel);
        } else {
            pending.result = Some(Err(HostTransportError::Io));
        }
        let Some(result) = pending.result.take() else {
            entry.pending = Some(pending);
            return api.nilObj;
        };
        // Restore the original blocking API's socket contract before admission.
        if let Some(channel) = entry.channel.as_ref() {
            if channel.set_nonblocking(false).is_err() {
                terminate_entry(&mut entry);
            }
        }
        result
    };
    // No registry/entry borrow survives object construction or ordinary sends.
    let reply = match result {
        Ok(frame)
            if frame.header.request_id == id
                && matches!(
                    frame.header.kind,
                    FrameKind::ReplyOk | FrameKind::ReplyError
                ) =>
        {
            frame
        }
        other => {
            let (class, kind, message): (&[u8], &[u8], &[u8]) = match other {
                Err(HostTransportError::Timeout) => (
                    b"timeout",
                    b"deadline",
                    b"request deadline expired; execution may have occurred",
                ),
                Err(HostTransportError::Io) => (
                    b"transport",
                    b"uncertain",
                    b"connection lost; execution may have occurred",
                ),
                _ => (
                    b"protocol",
                    b"invalidReply",
                    b"worker returned an invalid reply",
                ),
            };
            let status = {
                let mut entry = lock_entry(&shared);
                terminate_entry(&mut entry);
                entry.exit_status
            };
            let failure = make_failure(api, class, kind, message);
            failure_object(failure).exit_status = status
                .map(|status| unsafe { (birnam_abi::integer::make)(status as i64) })
                .unwrap_or(api.nilObj);
            failure_object(failure).request_id = unsafe { (birnam_abi::integer::make)(id as i64) };
            return failure;
        }
    };
    let answer = if reply.header.kind == FrameKind::ReplyOk {
        // Build the envelope after decoding, so it does not consume a wire
        // nesting level or reduce the protocol's maximum payload size.
        decode_oop_value(api, &reply.payload).map(|value| unsafe {
            let vector = (api.import)(c"Vector".as_ptr());
            let new = (api.intern)(c"new".as_ptr());
            let empty = (api.send)(vector, new, ptr::null(), 0);
            let add = (api.intern)(c"add:".as_ptr());
            (api.send)(empty, add, &value, 1)
        })
    } else {
        decode_oop_error_record(api, APPLICATION_ERROR_SCHEMA, &reply.payload).map(
            |(kind, message)| {
                let class = unsafe {
                    (api.intern)(if task_service {
                        c"resultTransfer".as_ptr()
                    } else {
                        c"application".as_ptr()
                    })
                };
                make_failure_objects(api, class, kind, message, api.nilObj, unsafe {
                    (birnam_abi::integer::make)(id as i64)
                })
            },
        )
    };
    let mut entry = lock_entry(&shared);
    match answer {
        Ok(value) => {
            if entry.state == Lifecycle::Busy {
                entry.state = Lifecycle::Ready;
            }
            if entry.state == Lifecycle::Exited {
                entry.channel.take();
            }
            value
        }
        Err(error) => {
            terminate_entry(&mut entry);
            drop(entry);
            make_failure(api, b"protocol", b"invalidReply", codec_error_name(error))
        }
    }
}

// Explicit process termination, never presented as graceful task cleanup. This
// control path remains available while the application request is outstanding.
pub(super) unsafe extern "C" fn terminate(handle: Oop, _: *const Oop, _: usize) -> Oop {
    if let Some(entry) = lookup_entry(handle_object(handle).token) {
        terminate_entry(&mut lock_entry(&entry));
    }
    handle
}

pub(super) unsafe extern "C" fn cancel(handle: Oop, args: *const Oop, count: usize) -> Oop {
    let api = runtime_api();
    let Some(args) = (unsafe { birnam_abi::borrowed_oop_args(args, count) }) else {
        return api.falseObj;
    };
    if args.len() != 1 || unsafe { (birnam_abi::integer::is_integer)(args[0]) } == 0 {
        return api.falseObj;
    }
    let id = unsafe { (birnam_abi::integer::value)(args[0]) } as u64;
    let Some(shared) = lookup_entry(handle_object(handle).token) else {
        return api.falseObj;
    };
    let mut entry = lock_entry(&shared);
    if !entry.task_service {
        return api.falseObj;
    }
    let Some(pending) = entry.pending.as_mut().filter(|p| p.id == id) else {
        return api.falseObj;
    };
    if !pending.cancellation_queued {
        let frame = Frame {
            header: Header {
                kind: FrameKind::Request,
                request_id: id,
                payload_len: 0,
            },
            payload: service::cancellation_payload(),
        };
        let Ok(bytes) = frame.encode() else {
            return api.falseObj;
        };
        pending.outgoing.extend_from_slice(&bytes);
        pending.cancellation_queued = true;
    }
    // Local control admission is not acknowledgement of remote cancellation.
    api.trueObj
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragmented_reply_and_nil_are_distinct_from_pending() {
        let (mut host, mut worker) = UnixStream::pair().unwrap();
        host.set_nonblocking(true).unwrap();
        let mut pending = Pending::new(7, encode_value(&Value::Nil).unwrap(), 5_000).unwrap();
        pending.progress(&mut host);
        assert!(pending.result.is_none());
        assert_eq!(read_frame(&mut worker).unwrap().header.request_id, 7);
        let reply = Frame {
            header: Header {
                kind: FrameKind::ReplyOk,
                request_id: 7,
                payload_len: 0,
            },
            payload: encode_value(&Value::Nil).unwrap(),
        }
        .encode()
        .unwrap();
        for byte in &reply[..reply.len() - 1] {
            worker.write_all(&[*byte]).unwrap();
            pending.progress(&mut host);
            assert!(pending.result.is_none());
        }
        worker.write_all(&reply[reply.len() - 1..]).unwrap();
        pending.progress(&mut host);
        assert_eq!(
            decode_value(&pending.result.unwrap().unwrap().payload).unwrap(),
            Value::Nil
        );
    }

    #[test]
    fn polling_has_a_byte_budget_and_an_absolute_deadline() {
        let (mut host, _worker) = UnixStream::pair().unwrap();
        host.set_nonblocking(true).unwrap();
        let mut pending = Pending::new(
            1,
            encode_value(&Value::String(vec![b'x'; 200_000])).unwrap(),
            5_000,
        )
        .unwrap();
        pending.progress(&mut host);
        assert!(pending.written <= 64 * 1024);
        assert!(pending.written < pending.outgoing.len());
        assert!(pending.result.is_none());
        pending.deadline = Instant::now();
        pending.progress(&mut host);
        assert!(matches!(
            pending.result,
            Some(Err(HostTransportError::Timeout))
        ));
    }

    #[test]
    fn disconnect_does_not_replay_or_invent_a_reply() {
        let (mut host, worker) = UnixStream::pair().unwrap();
        host.set_nonblocking(true).unwrap();
        let mut pending = Pending::new(1, encode_value(&Value::Nil).unwrap(), 5_000).unwrap();
        drop(worker);
        pending.progress(&mut host);
        assert!(matches!(pending.result, Some(Err(HostTransportError::Io))));
    }
}
