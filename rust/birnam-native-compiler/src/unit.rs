//! An owner for one private in-memory native unit.

use super::*;
use core::ptr::NonNull;
use std::ffi::CStr;
use std::sync::Mutex;

// libtcc has compiler-global scratch state. No lock survives a native entry.
static COMPILER: Mutex<()> = Mutex::new(());

pub struct Unit {
    state: NonNull<TccState>,
}

unsafe extern "C" fn discard_diagnostic(_: *mut c_void, _: *const c_char) {}

impl Unit {
    /// Compile trusted generated C and retain all relocated executable memory.
    ///
    /// # Safety
    /// Every supplied symbol must have the type declared by the generated
    /// source and remain valid until this Unit and all its native calls end.
    /// Executing a returned symbol requires the same proof for its arguments.
    pub unsafe fn compile(source: &CStr, symbols: &[(&CStr, *const c_void)]) -> Option<Self> {
        let _compiler = COMPILER.lock().unwrap_or_else(|error| error.into_inner());
        let state = NonNull::new(unsafe { tcc_new() })?;
        let raw = state.as_ptr();
        // SAFETY: this fresh compiler is exclusively held and all borrowed
        // strings and symbol names outlive these synchronous calls.
        let compiled = unsafe {
            tcc_set_error_func(raw, core::ptr::null_mut(), Some(discard_diagnostic));
            tcc_set_output_type(raw, 1) >= 0
                && symbols.iter().all(|(name, address)| {
                    !address.is_null() && tcc_add_symbol(raw, name.as_ptr(), *address) >= 0
                })
                && tcc_compile_string(raw, source.as_ptr()) >= 0
                && tcc_relocate(raw) >= 0
        };
        if !compiled {
            unsafe { tcc_delete(raw) };
            return None;
        }
        Some(Self { state })
    }

    /// The address is usable only while its Unit remains alive. Returning a
    /// raw address does not grant permission to invoke it or transfer ownership.
    pub fn symbol(&self, name: &CStr) -> Option<NonNull<c_void>> {
        let _compiler = COMPILER.lock().unwrap_or_else(|error| error.into_inner());
        // SAFETY: this owner retains the relocated compiler and its memory.
        NonNull::new(unsafe { tcc_get_symbol(self.state.as_ptr(), name.as_ptr()) })
    }
}

impl Drop for Unit {
    fn drop(&mut self) {
        let _compiler = COMPILER.lock().unwrap_or_else(|error| error.into_inner());
        // SAFETY: Unit owns this compiler. Callers must retain that owner for
        // every invocation of an address borrowed from symbol().
        unsafe { tcc_delete(self.state.as_ptr()) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    unsafe extern "C" fn twice(value: i64) -> i64 {
        value * 2
    }

    #[test]
    fn relocated_code_retains_its_owner_across_cache_eviction() {
        let owner = Rc::new(unsafe {
            Unit::compile(
                c"extern long twice(long); long answer(long n) { return twice(n) + 1; }",
                &[(c"twice", twice as *const c_void)],
            )
            .expect("compile and relocate")
        });
        let weak = Rc::downgrade(&owner);
        let entry: unsafe extern "C" fn(i64) -> i64 =
            unsafe { core::mem::transmute(owner.symbol(c"answer").unwrap().as_ptr()) };
        let selected_owner = owner.clone();
        drop(owner);
        assert_eq!(unsafe { entry(20) }, 41);
        assert!(weak.upgrade().is_some());
        drop(selected_owner);
        assert!(weak.upgrade().is_none());
        assert!(unsafe { Unit::compile(c"not valid C", &[]) }.is_none());
    }
}
