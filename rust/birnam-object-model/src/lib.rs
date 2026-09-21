//! Private-behavior ownership and construction above Id. The native entry owns
//! associations; upper collection follows the same owner records.
use birnam_abi::Oop;
use core::ffi::c_char;

pub mod enumeration;
mod native;

/// Rust adapter for the existing Id operations; no new native table or state.
/// The production adapter uses IdApi and ordinary Id messages. Floor unit
/// tests use the same algorithm with their independently constructed runtime.
pub trait BehaviorAccess {
    fn prepare(&self, object: Oop) -> Oop;
    fn metavtable(&self) -> Oop;
    fn owner(&self, behavior: Oop) -> Oop;
    fn delegate(&self, parent: Oop) -> Oop;
    fn set_owner(&self, behavior: Oop, object: Oop);
    fn attach(&self, object: Oop, behavior: Oop);
}

pub fn ensure_private_vtable(id: &impl BehaviorAccess, object: Oop) -> Oop {
    let current = id.prepare(object);
    if current == id.metavtable() {
        return object;
    }
    if id.owner(current) == object {
        return current;
    }
    let private = id.delegate(current);
    id.set_owner(private, object);
    id.attach(object, private);
    private
}

#[unsafe(no_mangle)]
pub extern "C" fn birnam_object_model_rust_identity() -> *const c_char {
    c"rust-object-model-v1".as_ptr()
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> core::ffi::c_int {
    1
}
