//! Bounded, nonblocking process transport. Only the owner creates floor objects.
//! One outstanding request per worker; no background Oops or native activations.
use super::*;
use std::time::Instant;

pub(super) struct Pending {
    pub(super) id: u64,
    pub(super) outgoing: Vec<u8>,
    written: usize,
    incoming: Vec<u8>,
    expected: usize,
    deadline: Instant,
    pub(super) result: Option<Result<Frame, HostTransportError>>,
    pub(super) cancellation_queued: bool,
}

impl Pending {
    pub(super) fn new(id: u64, payload: Vec<u8>, timeout: u32) -> Result<Self, HostTransportError> {
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

    pub(super) fn progress(&mut self, stream: &mut UnixStream) {
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
