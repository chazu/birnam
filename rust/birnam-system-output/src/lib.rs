//! System's existing stdout methods, installed by Prelude (81AL).
#![allow(clippy::missing_safety_doc)]
use birnam_abi::system_output::ApiV1;
use birnam_abi::text_view::printable_string;
use birnam_abi::{IdApi, Oop, borrowed_oop_args};
use core::ffi::c_void;
use core::ptr;
use std::io::{self, Write};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

static READY: AtomicBool = AtomicBool::new(false);
// Prelude supplies its original selection, even when registration reenters
// with another table before the first output installation. The host pins it.
static EXECUTION_API: OnceLock<usize> = OnceLock::new();

unsafe fn api_ref() -> &'static IdApi {
    let pointer = *EXECUTION_API
        .get()
        .expect("System output methods were not installed");
    unsafe { &*(pointer as *const IdApi) }
}
unsafe fn argument(args: *const Oop, count: usize) -> Option<Oop> {
    // SAFETY: the host owns the argument buffer for the duration of the call.
    unsafe { borrowed_oop_args(args, count)?.first().copied() }
}

fn write_system_output(bytes: &[u8], newline: bool) {
    let mut stdout = io::stdout().lock();
    let _ = write_system_output_to(&mut stdout, bytes, newline);
}

fn write_system_output_to<W: Write>(writer: &mut W, bytes: &[u8], newline: bool) -> io::Result<()> {
    writer.write_all(bytes)?;
    if newline {
        writer.write_all(b"\n")?;
    }
    writer.flush()
}

unsafe fn system_output(self_object: Oop, args: *const Oop, count: usize, newline: bool) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(value) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    let Some(bytes) = (unsafe { printable_string(api, value) }) else {
        return api.nilObj;
    };
    write_system_output(bytes, newline);
    self_object
}

unsafe extern "C" fn system_print(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { system_output(self_object, args, count, false) }
}

unsafe extern "C" fn system_println(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { system_output(self_object, args, count, true) }
}

unsafe extern "C" fn install(
    registration: *const IdApi,
    execution: *const IdApi,
    system_vt: Oop,
    print: Oop,
    println: Oop,
) {
    assert!(
        READY.load(Ordering::Acquire) && !registration.is_null() && !execution.is_null(),
        "invalid System output installation"
    );
    EXECUTION_API.get_or_init(|| execution as usize);
    let api = unsafe { &*registration };
    unsafe {
        (api.method)(system_vt, print, system_print);
        (api.method)(system_vt, println, system_println);
    }
}
static API: ApiV1 = ApiV1 {
    version: 1,
    byte_size: size_of::<ApiV1>(),
    install: Some(install),
};
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api: *mut IdApi, _: *mut c_void, _presence: *const birnam_abi::namespace_presence::ApiV1, _imports: *const birnam_abi::namespace_import::ApiV1, _exports: *const birnam_abi::namespace_export::ApiV1, _vtables: *const birnam_abi::vtable_enumeration::ApiV2, _observation: *const birnam_abi::method_observation::ApiV1, _roots: *const birnam_abi::root_registry::ApiV1) -> *mut c_void {
    assert!(!api.is_null(), "System output received a null IdApi");
    READY.store(true, Ordering::Release);
    ptr::null_mut()
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_system_output_provider_v1() -> *const ApiV1 {
    if READY.load(Ordering::Acquire) {
        &API
    } else {
        ptr::null()
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}
birnam_abi::text_view_dependency!();

#[cfg(test)]
mod tests {
    use super::write_system_output_to;
    use std::io::{self, Write};

    struct FlushProbe {
        bytes: Vec<u8>,
        flushed: bool,
    }

    impl Write for FlushProbe {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            self.flushed = true;
            Ok(())
        }
    }

    #[test]
    fn system_output_flushes_a_newline_free_prompt() {
        let mut output = FlushProbe {
            bytes: Vec::new(),
            flushed: false,
        };

        write_system_output_to(&mut output, b"birnam> ", false).expect("prompt write");

        assert_eq!(output.bytes, b"birnam> ");
        assert!(
            output.flushed,
            "a prompt must be visible before stdin blocks"
        );
    }
}
