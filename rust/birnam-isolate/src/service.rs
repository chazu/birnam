//! Opt-in task-service envelopes on the unchanged wire-v1 frame/value codec.
//! The owner polls cancellation; no helper thread holds an Oop or runs code.
use super::*;
use std::cell::RefCell;

pub(super) fn cancellation_payload() -> Vec<u8> {
    encode_value(&Value::Vector(vec![Value::Symbol(b"cancel".to_vec())])).unwrap()
}

pub(super) fn wrap_request(payload: &[u8]) -> Result<Vec<u8>, CodecError> {
    encode_value(&Value::Vector(vec![
        Value::Symbol(b"request".to_vec()),
        decode_value(payload)?,
    ]))
}

pub(super) fn request_payload(payload: &[u8]) -> Result<Option<Vec<u8>>, CodecError> {
    match decode_value(payload)? {
        Value::Vector(v) if v.len() == 1 && v[0] == Value::Symbol(b"cancel".to_vec()) => Ok(None),
        Value::Vector(v) if v.len() == 2 && v[0] == Value::Symbol(b"request".to_vec()) => {
            encode_value(&v[1]).map(Some)
        }
        _ => Err(CodecError::InvalidRecord),
    }
}

struct Control {
    stream: UnixStream,
    request: u64,
    bytes: Vec<u8>,
    expected: usize,
    lost: bool,
}

thread_local! { static CONTROL: RefCell<Option<Control>> = const { RefCell::new(None) }; }

pub(super) fn begin(fd: c_int, request: u64) -> io::Result<()> {
    if CONTROL.with(|slot| {
        let mut slot = slot.borrow_mut();
        if let Some(control) = slot.as_mut() {
            control.request = request;
            true
        } else {
            false
        }
    }) {
        return Ok(());
    }
    unsafe extern "C" {
        fn dup(fd: c_int) -> c_int;
    }
    let copy = unsafe { dup(fd) };
    if copy < 0 {
        return Err(io::Error::last_os_error());
    }
    let stream = unsafe { UnixStream::from_raw_fd(copy) };
    CONTROL.with(|slot| {
        *slot.borrow_mut() = Some(Control {
            stream,
            request,
            bytes: Vec::new(),
            expected: HEADER_LEN,
            lost: false,
        })
    });
    Ok(())
}

fn end() {
    CONTROL.with(|slot| {
        slot.borrow_mut().take();
    });
}

impl Control {
    fn poll(&mut self) -> io::Result<bool> {
        // Control messages have a fixed small shape. Never consume arbitrary
        // application-sized buffers on the worker's control path.
        for _ in 0..8 {
            if self.bytes.len() == self.expected {
                if self.expected == HEADER_LEN {
                    let header =
                        decode_header(&self.bytes).map_err(|_| io::ErrorKind::InvalidData)?;
                    if header.payload_len > 128 {
                        return Err(io::ErrorKind::InvalidData.into());
                    }
                    self.expected += header.payload_len as usize;
                }
                if self.bytes.len() == self.expected {
                    let frame =
                        Frame::decode(&self.bytes).map_err(|_| io::ErrorKind::InvalidData)?;
                    self.bytes.clear();
                    self.expected = HEADER_LEN;
                    if frame.header.kind != FrameKind::Request
                        || frame.payload != cancellation_payload()
                    {
                        return Err(io::ErrorKind::InvalidData.into());
                    }
                    if frame.header.request_id == self.request {
                        return Ok(true);
                    }
                    continue; // Old controls cannot cancel a newer request.
                }
            }
            let mut bytes = [0u8; HEADER_LEN + 128];
            let count = self.expected - self.bytes.len();
            match self.stream.read(&mut bytes[..count]) {
                Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
                Ok(n) => self.bytes.extend_from_slice(&bytes[..n]),
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) =>
                {
                    return Ok(false);
                }
                Err(e) => return Err(e),
            }
        }
        Ok(false)
    }
}

pub(super) fn poll_control() -> i32 {
    let status = CONTROL.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(control) = slot.as_mut() else {
            return 0;
        };
        if control.lost {
            return 2;
        }
        let result = control
            .stream
            .set_nonblocking(true)
            .and_then(|_| control.poll());
        let restored = control.stream.set_nonblocking(false);
        match (result, restored) {
            (Ok(cancel), Ok(())) => i32::from(cancel),
            _ => {
                control.lost = true;
                2
            }
        }
    });
    status
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn envelopes_preserve_nil_and_do_not_confuse_application_symbols_with_control() {
        for value in [Value::Nil, Value::Symbol(b"cancel".to_vec())] {
            let bytes = encode_value(&value).unwrap();
            assert_eq!(
                request_payload(&wrap_request(&bytes).unwrap()).unwrap(),
                Some(bytes)
            );
        }
        assert_eq!(request_payload(&cancellation_payload()).unwrap(), None);
    }
    #[test]
    fn controls_are_fragmented_and_bound_to_the_request() {
        let (stream, mut sender) = UnixStream::pair().unwrap();
        stream.set_nonblocking(true).unwrap();
        let mut reader = Control {
            stream,
            request: 2,
            bytes: vec![],
            expected: HEADER_LEN,
            lost: false,
        };
        for id in [1, 2] {
            let frame = Frame {
                header: Header {
                    kind: FrameKind::Request,
                    request_id: id,
                    payload_len: 0,
                },
                payload: cancellation_payload(),
            }
            .encode()
            .unwrap();
            for byte in &frame[..frame.len() - 1] {
                sender.write_all(&[*byte]).unwrap();
                assert!(!reader.poll().unwrap());
            }
            sender.write_all(&frame[frame.len() - 1..]).unwrap();
            assert_eq!(reader.poll().unwrap(), id == 2);
        }
        drop(sender);
        assert!(reader.poll().is_err());
    }

    #[test]
    fn partial_control_survives_request_publication() {
        use std::os::fd::AsRawFd;
        let (host, mut sender) = UnixStream::pair().unwrap();
        let mut channel = unsafe { File::from_raw_fd(host.try_clone().unwrap().into_raw_fd()) };
        begin(host.as_raw_fd(), 1).unwrap();
        let _guard = DriverGuard;
        let frame = Frame {
            header: Header {
                kind: FrameKind::Request,
                request_id: 1,
                payload_len: 0,
            },
            payload: cancellation_payload(),
        }
        .encode()
        .unwrap();
        sender.write_all(&frame[..5]).unwrap();
        CONTROL.with(|slot| {
            let mut slot = slot.borrow_mut();
            let control = slot.as_mut().unwrap();
            control.stream.set_nonblocking(true).unwrap();
            assert!(!control.poll().unwrap());
            control.stream.set_nonblocking(false).unwrap();
        });
        sender.write_all(&frame[5..]).unwrap();
        assert_eq!(
            read_next(&mut channel).unwrap().payload,
            cancellation_payload()
        );
        begin(host.as_raw_fd(), 2).unwrap();
        CONTROL.with(|slot| assert_eq!(slot.borrow().as_ref().unwrap().request, 2));
    }
}

// Preserve a partly read cancellation across terminal publication. Otherwise
// the next request could begin in the middle of a control frame.
pub(super) struct DriverGuard;
impl Drop for DriverGuard {
    fn drop(&mut self) {
        end();
    }
}

pub(super) fn read_next(channel: &mut File) -> io::Result<Frame> {
    let mut bytes = CONTROL.with(|slot| {
        let mut slot = slot.borrow_mut();
        slot.as_mut()
            .map(|control| {
                control.expected = HEADER_LEN;
                std::mem::take(&mut control.bytes)
            })
            .unwrap_or_default()
    });
    let have = bytes.len();
    if have < HEADER_LEN {
        bytes.resize(HEADER_LEN, 0);
        channel.read_exact(&mut bytes[have..])?;
    }
    let header = decode_header(&bytes[..HEADER_LEN]).map_err(|_| io::ErrorKind::InvalidData)?;
    let have = bytes.len();
    bytes.resize(HEADER_LEN + header.payload_len as usize, 0);
    channel.read_exact(&mut bytes[have..])?;
    Frame::decode(&bytes).map_err(|_| io::ErrorKind::InvalidData.into())
}
