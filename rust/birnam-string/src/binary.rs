//! Text-to-byte conversion crosses the storage owner through current Id lookup.
use super::*;
pub(super) unsafe extern "C" fn from_string(c:*mut c_void,r:Oop,a:*const Operand,n:usize)->Outcome {
    done((||unsafe {
        let t=state(c)?; if !borrowed_slice(a,n).ok_or_else(invalid)?.is_empty() { return Err(invalid()); }
        t.rooted(&[r],|| {let bytes=t.copy(r,Kind::String)?;t.id.send_named(t.byte_factory()?,b"fromBytes:",&[Operand::bytes(&bytes)])})
    })())
}
