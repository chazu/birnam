//! Bulk byte copies through ordinary Oop messages. Producers own their snapshots;
//! consumers own scoped output buffers. No native pointer crosses Id.
use crate::{
    Oop, as_size, borrowed_slice, fault_reference, id_protocol::Protocol, object_reference, size,
};
use std::cell::RefCell;

pub const CHUNK_BYTES: usize = 8192;
pub const NAMES: &[&[u8]] = &[
    b"receiveByteValues",
    b"copyByteValuesFrom:count:to:",
    b"appendByteValues",
    b"sizeWord",
    b"objectBehavior",
    b"delegated",
    b"allocate",
    b"methodAt:",
    b"addMethod",
    b"retain:",
    b"releaseTokensProtecting",
];

pub struct Reader {
    id: Protocol,
    behavior: Oop,
    roots: Oop,
    token: Oop,
    retain: Oop,
    release: Oop,
    size: Oop,
    copy: Oop,
    allocate: Oop,
}
struct Read {
    root: Oop,
    sink: Oop,
    expected: usize,
    bytes: Vec<u8>,
    failed: bool,
}
#[derive(Default)]
struct Pending {
    reads: Vec<Read>,
    last: Option<Protocol>,
}
thread_local! { static PENDING: RefCell<Pending> = RefCell::new(Pending::default()); }

impl Reader {
    /// `receiveByteValues` must declare `receive_entry` from this consumer's
    /// linked copy of the helper. With null Roots, collection must not yet be
    /// enabled; bootstrap attaches Roots before activating collection.
    pub unsafe fn new(
        id: Protocol,
        image: Oop,
        roots: Oop,
        name: impl Fn(&[u8]) -> Oop,
    ) -> Result<Self, Oop> {
        unsafe {
            let receive = id.send(image, name(b"methodAt:"), &[name(b"receiveByteValues")])?;
            Self::from_method(id, receive, roots, name)
        }
    }
    pub unsafe fn from_method(
        id: Protocol,
        receive: Oop,
        roots: Oop,
        name: impl Fn(&[u8]) -> Oop,
    ) -> Result<Self, Oop> {
        unsafe {
            PENDING.with(|p| p.borrow_mut().last = Some(id));
            let held = if roots.is_null() {
                std::ptr::null_mut()
            } else {
                id.send(roots, name(b"retain:"), &[receive])?
            };
            let result = (|| {
                let base = id.send(id.root, name(b"objectBehavior"), &[])?;
                let behavior = id.send(base, name(b"delegated"), &[])?;
                id.send(
                    behavior,
                    name(b"addMethod"),
                    &[name(b"appendByteValues"), receive],
                )?;
                let mut reader = Self {
                    id,
                    behavior,
                    roots: std::ptr::null_mut(),
                    token: std::ptr::null_mut(),
                    retain: name(b"retain:"),
                    release: name(b"releaseTokensProtecting"),
                    size: name(b"sizeWord"),
                    copy: name(b"copyByteValuesFrom:count:to:"),
                    allocate: name(b"allocate"),
                };
                if !roots.is_null() {
                    reader.attach_roots(roots)?;
                }
                Ok(reader)
            })();
            if !held.is_null() {
                let protected = match &result {
                    Ok(r) => r.behavior,
                    Err(e) => *e,
                };
                let cleanup = id.send(roots, name(b"releaseTokensProtecting"), &[protected, held]);
                return result.and_then(|r| cleanup.map(|_| r));
            }
            result
        }
    }
    pub unsafe fn attach_roots(&mut self, roots: Oop) -> Result<(), Oop> {
        self.token = unsafe { self.id.send(roots, self.retain, &[self.behavior]) }?;
        self.roots = roots;
        Ok(())
    }
    pub unsafe fn read(&self, provider: Oop) -> Result<Vec<u8>, Oop> {
        unsafe { self.read_limited(provider, usize::MAX) }
    }
    pub unsafe fn read_limited(&self, provider: Oop, limit: usize) -> Result<Vec<u8>, Oop> {
        unsafe {
            let invalid = object_reference(self.id.invalid);
            let count = as_size(self.id.send(provider, self.size, &[])?)
                .filter(|&n| n <= limit)
                .ok_or(invalid)?;
            self.read_count(provider, count)
        }
    }
    pub unsafe fn read_count(&self, provider: Oop, count: usize) -> Result<Vec<u8>, Oop> {
        unsafe { self.read_range(provider, 0, count) }
    }
    pub unsafe fn read_range(
        &self,
        provider: Oop,
        offset: usize,
        count: usize,
    ) -> Result<Vec<u8>, Oop> {
        unsafe {
            let invalid = object_reference(self.id.invalid);
            let from = size(offset).ok_or(invalid)?;
            let length = size(count).ok_or(invalid)?;
            offset.checked_add(count).ok_or(invalid)?;
            let mut bytes = Vec::new();
            bytes.try_reserve_exact(count).map_err(|_| invalid)?;
            if count == 0 {
                return Ok(bytes);
            }
            let token = if self.roots.is_null() {
                std::ptr::null_mut()
            } else {
                self.id.send(self.roots, self.retain, &[provider])?
            };
            let result = (|| {
                let sink = self
                    .id
                    .send(self.behavior, self.allocate, &[size(0).unwrap()])?;
                PENDING.with(|p| {
                    let mut p = p.borrow_mut();
                    p.last = Some(self.id);
                    p.reads.push(Read {
                        root: self.id.root,
                        sink,
                        expected: count,
                        bytes,
                        failed: false,
                    });
                });
                // Id roots both provider and sink during lookup and delivery.
                let answer = self.id.send(provider, self.copy, &[from, length, sink]);
                let read = PENDING.with(|p| {
                    let mut p = p.borrow_mut();
                    let i = p
                        .reads
                        .iter()
                        .position(|r| r.sink == sink && r.root == self.id.root)
                        .unwrap();
                    p.reads.remove(i)
                });
                answer?;
                if read.failed || read.bytes.len() != read.expected {
                    return Err(invalid);
                }
                Ok(read.bytes)
            })();
            if !token.is_null() {
                let protected = result
                    .as_ref()
                    .err()
                    .copied()
                    .unwrap_or(std::ptr::null_mut());
                let cleanup = self.id.send(self.roots, self.release, &[protected, token]);
                return result.and_then(|bytes| cleanup.map(|_| bytes));
            }
            result
        }
    }
}
impl Drop for Reader {
    fn drop(&mut self) {
        if !self.token.is_null() {
            let _ = unsafe {
                self.id.send(
                    self.roots,
                    self.release,
                    &[std::ptr::null_mut(), self.token],
                )
            };
        }
    }
}
pub unsafe extern "C" fn receive_entry(
    root: Oop,
    _: Oop,
    sink: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    PENDING.with(|p| {
        let mut p = p.borrow_mut();
        let error = p
            .last
            .filter(|id| id.root == root)
            .map_or(root, |id| object_reference(id.invalid));
        let Some(r) = p
            .reads
            .iter_mut()
            .find(|r| r.sink == sink && r.root == root)
        else {
            return fault_reference(error);
        };
        let values = unsafe { borrowed_slice(a, n) };
        if r.failed
            || r.bytes.len().checked_add(n).is_none_or(|n| n > r.expected)
            || values
                .is_none_or(|values| values.iter().any(|&v| as_size(v).is_none_or(|v| v > 255)))
        {
            r.failed = true;
            return fault_reference(error);
        }
        r.bytes
            .extend(values.unwrap().iter().map(|&v| as_size(v).unwrap() as u8));
        sink
    })
}
/// No borrow of mutable producer state may span delivery: the receiver may
/// collect, mutate the original source, or start another read.
pub unsafe fn deliver(id: &Protocol, sink: Oop, append: Oop, bytes: &[u8]) -> Result<(), Oop> {
    let mut values = Vec::with_capacity(CHUNK_BYTES.min(bytes.len()));
    for chunk in bytes.chunks(CHUNK_BYTES) {
        values.clear();
        values.extend(chunk.iter().map(|&b| size(b as usize).unwrap()));
        unsafe { id.send(sink, append, &values) }?;
    }
    Ok(())
}
