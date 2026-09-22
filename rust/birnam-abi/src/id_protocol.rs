//! Native operands and the common Id message/module boundary (Design 83).
//! Object references and native data are distinct. Bytes are borrowed for the
//! synchronous call; callers keep object and native entry owners live.
use crate::Oop;
use core::ffi::c_void;

pub const VERSION: u32 = 1;
pub const UNIT: u32 = 0;
pub const OBJECT: u32 = 1;
pub const WORD: u32 = 2;
pub const BYTES: u32 = 3;
pub const NATIVE_ENTRY: u32 = 4;
pub const PROTOCOL: u32 = 5;
pub const LOADED: u32 = 0;
pub const INVALID_REQUEST: u32 = 1;
pub const MAPPING_FAILED: u32 = 2;
pub const MISSING_ENTRY: u32 = 3;
pub const INCOMPATIBLE_ABI: u32 = 4;
pub const INITIALIZER_FAILED: u32 = 5;
pub const UNBOUND: u32 = 6;

pub type Method = unsafe extern "C" fn(*mut c_void, Oop, *const Operand, usize) -> Outcome;
pub type Message = unsafe extern "C" fn(*mut c_void, Oop, Oop, *const Operand, usize) -> Outcome;
pub type Initialize = unsafe extern "C" fn(*const Protocol, *const Operand, usize) -> Outcome;

/// The context is native implementation data, not an implicitly traced object.
/// Its owner, like the function's image, must remain live through invocation.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct NativeEntry {
    pub method: Option<Method>,
    pub context: *mut c_void,
}
impl PartialEq for NativeEntry {
    fn eq(&self, other: &Self) -> bool {
        self.method.map(|method| method as usize) == other.method.map(|method| method as usize)
            && self.context == other.context
    }
}
impl Eq for NativeEntry {}
impl NativeEntry {
    pub const fn new(method: Method, context: *mut c_void) -> Self {
        Self {
            method: Some(method),
            context,
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Bytes {
    pub pointer: *const u8,
    pub length: usize,
}
#[repr(C)]
#[derive(Clone, Copy)]
union Data {
    pub object: Oop,
    pub word: usize,
    pub bytes: Bytes,
    pub entry: NativeEntry,
    pub protocol: *const Protocol,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Operand {
    kind: u32,
    reserved: u32,
    data: Data,
}
impl Operand {
    pub const fn kind(self) -> u32 {
        self.kind
    }
    pub const fn unit() -> Self {
        Self::word_kind(UNIT, 0)
    }
    pub const fn word(value: usize) -> Self {
        Self::word_kind(WORD, value)
    }
    const fn word_kind(kind: u32, value: usize) -> Self {
        Self {
            kind,
            reserved: 0,
            data: Data { word: value },
        }
    }
    pub const fn object(value: Oop) -> Self {
        Self {
            kind: OBJECT,
            reserved: 0,
            data: Data { object: value },
        }
    }
    pub const fn entry(value: NativeEntry) -> Self {
        Self {
            kind: NATIVE_ENTRY,
            reserved: 0,
            data: Data { entry: value },
        }
    }
    pub fn bytes(value: &[u8]) -> Self {
        Self {
            kind: BYTES,
            reserved: 0,
            data: Data {
                bytes: Bytes {
                    pointer: value.as_ptr(),
                    length: value.len(),
                },
            },
        }
    }
    pub const fn protocol(value: *const Protocol) -> Self {
        Self {
            kind: PROTOCOL,
            reserved: 0,
            data: Data { protocol: value },
        }
    }
    pub fn as_bytes(self) -> Option<Bytes> {
        (self.kind == BYTES).then(|| unsafe { self.data.bytes })
    }
    pub fn as_protocol(self) -> Option<*const Protocol> {
        (self.kind == PROTOCOL).then(|| unsafe { self.data.protocol })
    }
    pub fn as_object(self) -> Option<Oop> {
        (self.kind == OBJECT).then(|| unsafe { self.data.object })
    }
    pub fn as_word(self) -> Option<usize> {
        (self.kind == WORD).then(|| unsafe { self.data.word })
    }
    pub fn as_entry(self) -> Option<NativeEntry> {
        (self.kind == NATIVE_ENTRY).then(|| unsafe { self.data.entry })
    }
}
impl PartialEq for Operand {
    fn eq(&self, other: &Self) -> bool {
        if self.kind != other.kind {
            return false;
        }
        match self.kind {
            UNIT => true,
            OBJECT => self.as_object() == other.as_object(),
            WORD => self.as_word() == other.as_word(),
            NATIVE_ENTRY => self.as_entry() == other.as_entry(),
            PROTOCOL => self.as_protocol() == other.as_protocol(),
            BYTES => {
                let left = self.as_bytes().unwrap();
                let right = other.as_bytes().unwrap();
                left.pointer == right.pointer && left.length == right.length
            }
            _ => false,
        }
    }
}
impl Eq for Operand {}
impl core::fmt::Debug for Operand {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Operand")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Outcome {
    pub status: u32,
    pub reserved: u32,
    pub detail: usize,
    pub value: Operand,
}
impl Outcome {
    pub const fn loaded(value: Operand) -> Self {
        Self {
            status: LOADED,
            reserved: 0,
            detail: 0,
            value,
        }
    }
    pub const fn failed(status: u32, detail: usize) -> Self {
        Self {
            status,
            reserved: 0,
            detail,
            value: Operand::unit(),
        }
    }
    pub fn into_result(self) -> Result<Operand, Self> {
        if self.status == LOADED {
            Ok(self.value)
        } else {
            Err(self)
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Protocol {
    pub version: u32,
    pub byte_size: u32,
    pub message: Option<Message>,
    pub context: *mut c_void,
    pub receiver: Oop,
    pub intern: Oop,
    pub load: Oop,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Module {
    pub version: u32,
    pub byte_size: u32,
    pub initialize: Option<Initialize>,
}
impl Protocol {
    /// Send through this Id. No selected method is cached between calls.
    /// # Safety
    /// The protocol, objects, arguments and their owners must remain live on
    /// the Id owner thread. Borrowed results retain their method's contract.
    pub unsafe fn send(
        &self,
        receiver: Oop,
        selector: Oop,
        arguments: &[Operand],
    ) -> Result<Operand, Outcome> {
        let message = self
            .message
            .ok_or_else(|| Outcome::failed(INVALID_REQUEST, 0))?;
        unsafe {
            message(
                self.context,
                receiver,
                selector,
                arguments.as_ptr(),
                arguments.len(),
            )
        }
        .into_result()
    }
    /// Intern a selector using the current Id method.
    /// # Safety
    /// The same owner-thread and lifetime rules as `send` apply.
    pub unsafe fn intern_name(&self, name: &[u8]) -> Result<Oop, Outcome> {
        unsafe { self.send(self.receiver, self.intern, &[Operand::bytes(name)]) }?
            .as_object()
            .filter(|object| !object.is_null())
            .ok_or_else(|| Outcome::failed(INVALID_REQUEST, 0))
    }
    /// Intern a name, then perform a fresh message send.
    /// # Safety
    /// Arguments must stay live across interning and the subsequent send.
    pub unsafe fn send_named(
        &self,
        receiver: Oop,
        name: &[u8],
        arguments: &[Operand],
    ) -> Result<Operand, Outcome> {
        let selector = unsafe { self.intern_name(name) }?;
        unsafe { self.send(receiver, selector, arguments) }
    }
    pub fn same_contract(&self, other: &Self) -> bool {
        self.version == other.version
            && self.byte_size == other.byte_size
            && self.message.map(|f| f as usize) == other.message.map(|f| f as usize)
            && self.context == other.context
            && self.receiver == other.receiver
            && self.intern == other.intern
            && self.load == other.load
    }
}
/// Validate a completed carrier without calling any of its operations.
/// # Safety
/// The pointer names a readable header, and the declared extent if admitted.
pub unsafe fn valid_protocol(pointer: *const Protocol) -> bool {
    if pointer.is_null() || !pointer.addr().is_multiple_of(align_of::<Protocol>()) {
        return false;
    }
    unsafe {
        if core::ptr::addr_of!((*pointer).version).read() != VERSION
            || core::ptr::addr_of!((*pointer).byte_size).read() as usize != size_of::<Protocol>()
        {
            return false;
        }
        let protocol = *pointer;
        protocol.message.is_some()
            && !protocol.receiver.is_null()
            && !protocol.intern.is_null()
            && !protocol.load.is_null()
    }
}

const _: () = {
    assert!(size_of::<NativeEntry>() == 16);
    assert!(size_of::<Operand>() == 24);
    assert!(size_of::<Outcome>() == 40);
    assert!(size_of::<Protocol>() == 48);
    assert!(size_of::<Module>() == 16);
    assert!(core::mem::offset_of!(NativeEntry, context) == 8);
    assert!(core::mem::offset_of!(Operand, data) == 8);
    assert!(core::mem::offset_of!(Outcome, detail) == 8);
    assert!(core::mem::offset_of!(Outcome, value) == 16);
    assert!(core::mem::offset_of!(Protocol, message) == 8);
    assert!(core::mem::offset_of!(Protocol, context) == 16);
    assert!(core::mem::offset_of!(Protocol, receiver) == 24);
    assert!(core::mem::offset_of!(Protocol, intern) == 32);
    assert!(core::mem::offset_of!(Protocol, load) == 40);
    assert!(core::mem::offset_of!(Module, initialize) == 8);
};

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn carrier_validation_is_inert_and_checks_the_header_before_fields() {
        #[repr(C)]
        struct Header {
            version: u32,
            byte_size: u32,
        }
        #[repr(align(8))]
        struct Aligned(Header);
        let short = Aligned(Header {
            version: VERSION,
            byte_size: 8,
        });
        assert!(!unsafe { valid_protocol((&raw const short).cast()) });
        assert!(!unsafe { valid_protocol(core::ptr::null()) });
        unsafe extern "C" fn message(
            _: *mut c_void,
            _: Oop,
            _: Oop,
            _: *const Operand,
            _: usize,
        ) -> Outcome {
            panic!("validation must not invoke the carrier")
        }
        let mut protocol = Protocol {
            version: VERSION,
            byte_size: size_of::<Protocol>() as u32,
            message: Some(message),
            context: core::ptr::null_mut(),
            receiver: core::ptr::without_provenance_mut(1),
            intern: core::ptr::without_provenance_mut(2),
            load: core::ptr::without_provenance_mut(3),
        };
        assert!(unsafe { valid_protocol(&protocol) });
        protocol.message = None;
        assert!(!unsafe { valid_protocol(&protocol) });
        protocol.message = Some(message);
        protocol.load = core::ptr::null_mut();
        assert!(!unsafe { valid_protocol(&protocol) });
    }
}
