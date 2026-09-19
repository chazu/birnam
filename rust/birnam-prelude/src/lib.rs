//! Prelude coordinates printing defaults and owns Block and System primitives.

#![allow(clippy::missing_safety_doc)]

use birnam_abi::{IdApi, ModuleInitFn, Oop, borrowed_oop_args};
use core::ffi::{c_char, c_void};
use core::ptr;
use std::ffi::CString;
use std::fs::{self, File};
use std::io::{self, BufRead, Read, Write};
use std::path::Path;
use std::sync::OnceLock;
use std::{env, str};

const RUST_PRELUDE_IDENTITY: &[u8] = b"rust-prelude-n7.1-core\0";

#[derive(Clone, Copy)]
struct Runtime {
    api: usize,
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

fn abort(message: &str) -> ! {
    eprintln!("birnam Rust Prelude: {message}");
    std::process::abort();
}

fn runtime() -> &'static Runtime {
    RUNTIME
        .get()
        .unwrap_or_else(|| abort("module_init was not called"))
}

unsafe fn api_ref() -> &'static IdApi {
    let runtime = runtime();
    // SAFETY: module_init records the process-owned IdApi address and the host
    // retains the selected floor for the process lifetime.
    unsafe { &*(runtime.api as *const IdApi) }
}

unsafe fn intern(api: &IdApi, name: &[u8]) -> Oop {
    // SAFETY: all callers pass static NUL-terminated selector names.
    unsafe { (api.intern)(name.as_ptr().cast::<c_char>()) }
}

unsafe fn argument(args: *const Oop, count: usize) -> Option<Oop> {
    // SAFETY: the host owns the argument buffer for the duration of the call.
    unsafe { borrowed_oop_args(args, count)?.first().copied() }
}

unsafe fn send0(api: &IdApi, receiver: Oop, selector: &[u8]) -> Oop {
    let selector = unsafe { intern(api, selector) };
    // SAFETY: a zero-count send may carry a null argument pointer under IdApi.
    unsafe { (api.send)(receiver, selector, ptr::null(), 0) }
}

unsafe fn send_args(api: &IdApi, receiver: Oop, selector: &[u8], args: &[Oop]) -> Oop {
    let selector = unsafe { intern(api, selector) };
    let arguments = if args.is_empty() {
        ptr::null()
    } else {
        args.as_ptr()
    };
    // SAFETY: callers keep the synchronous stack-backed argument slice alive
    // until this send returns; IdApi does not retain the pointer.
    unsafe { (api.send)(receiver, selector, arguments, args.len()) }
}

unsafe fn block_loop(
    self_object: Oop,
    args: *const Oop,
    count: usize,
    continue_on_true: bool,
) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(body) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    loop {
        let condition = unsafe { send0(api, self_object, b"value\0") };
        if unsafe { is_error(api, condition) } {
            return condition;
        }
        let is_true = condition == api.trueObj;
        let is_false = condition == api.falseObj;
        if !is_true && !is_false {
            return unsafe {
                numeric_error(
                    api,
                    b"mustBeBoolean\0",
                    b"while loop condition must answer true or false",
                )
            };
        }
        if is_true != continue_on_true {
            return api.nilObj;
        }
        let result = unsafe { send0(api, body, b"value\0") };
        if unsafe { is_error(api, result) } {
            return result;
        }
    }
}

unsafe fn is_error(api: &IdApi, value: Oop) -> bool {
    if value.is_null() || unsafe { (api.isExported)(c"ErrorVt".as_ptr()) } == 0 {
        return false;
    }
    let error_vt = unsafe { (api.import)(c"ErrorVt".as_ptr()) };
    !error_vt.is_null() && unsafe { (api.vtableOf)(value) } == error_vt
}

unsafe extern "C" fn block_while_true(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { block_loop(self_object, args, count, true) }
}

unsafe extern "C" fn block_while_false(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { block_loop(self_object, args, count, false) }
}

unsafe fn make_error(api: &IdApi, kind: Oop, message: Oop) -> Oop {
    // Error owns its representation and Object protocol in the standalone
    // Error rung. Prelude faults are intentionally lazy: before that rung is
    // loaded, the floor has no catchable Error value and answers nil, matching
    // the native prelude's `Error` import behavior.
    if unsafe { (api.isExported)(c"Error".as_ptr()) } == 0 {
        return api.nilObj;
    }
    let factory = unsafe { (api.import)(c"Error".as_ptr()) };
    if factory.is_null() {
        return api.nilObj;
    }
    let selector = unsafe { intern(api, b"kind:message:\0") };
    let args = [kind, message];
    // SAFETY: the standalone Error factory owns this selector and the stack
    // argument array remains live for the duration of the synchronous send.
    unsafe { (api.send)(factory, selector, args.as_ptr(), args.len()) }
}

unsafe fn numeric_error(api: &IdApi, kind: &[u8], message: &[u8]) -> Oop {
    let kind = unsafe { intern(api, kind) };
    let message = unsafe { (birnam_abi::string::make)(message.as_ptr(), message.len()) };
    unsafe { make_error(api, kind, message) }
}

unsafe fn string_bytes(api: &IdApi, value: Oop) -> Option<&[u8]> {
    if value.is_null() || unsafe { (api.vtableOf)(value) } != unsafe { birnam_abi::string::behavior() } {
        return None;
    }
    let mut length = 0usize;
    let bytes = unsafe { (birnam_abi::string::bytes)(value, &mut length) };
    if length == 0 {
        return Some(&[]);
    }
    if bytes.is_null() {
        return None;
    }
    // SAFETY: stringBytes returns the floor-owned String payload for the
    // duration of this callback; the write completes before the borrow ends.
    Some(unsafe { core::slice::from_raw_parts(bytes, length) })
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

fn path_from_bytes(bytes: &[u8]) -> Option<&Path> {
    std::str::from_utf8(bytes).ok().map(Path::new)
}

unsafe extern "C" fn system_read_file(_self_object: Oop, args: *const Oop, count: usize) -> Oop {
    const MAX_FILE_BYTES: u64 = 1 << 30;
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(path_object) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    let Some(path_bytes) = (unsafe { string_bytes(api, path_object) }) else {
        return api.nilObj;
    };
    let Some(path) = path_from_bytes(path_bytes) else {
        return unsafe { numeric_error(api, b"ioError\0", b"could not read file") };
    };
    let Ok(file) = File::open(path) else {
        return unsafe { numeric_error(api, b"ioError\0", b"could not read file") };
    };
    let mut bytes = Vec::new();
    if file
        .take(MAX_FILE_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .is_err()
        || bytes.len() > MAX_FILE_BYTES as usize
    {
        return unsafe { numeric_error(api, b"ioError\0", b"could not read file") };
    }
    unsafe { (birnam_abi::string::make)(bytes.as_ptr(), bytes.len()) }
}

unsafe extern "C" fn system_write_file(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 2 {
        return api.nilObj;
    }
    let Some(values) = (unsafe { borrowed_oop_args(args, count) }) else {
        return api.nilObj;
    };
    let Some(path_bytes) = (unsafe { string_bytes(api, values[0]) }) else {
        return api.nilObj;
    };
    let Some(contents) = (unsafe { string_bytes(api, values[1]) }) else {
        return api.nilObj;
    };
    let Some(path) = path_from_bytes(path_bytes) else {
        return unsafe { numeric_error(api, b"ioError\0", b"could not write file") };
    };
    if fs::write(path, contents).is_err() {
        return unsafe { numeric_error(api, b"ioError\0", b"could not write file") };
    }
    self_object
}

unsafe extern "C" fn system_get_env(_self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(name_object) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    let Some(name_bytes) = (unsafe { string_bytes(api, name_object) }) else {
        return api.nilObj;
    };
    let Ok(name) = str::from_utf8(name_bytes) else {
        return unsafe { numeric_error(api, b"ioError\0", b"could not read environment") };
    };
    match env::var(name) {
        Ok(value) => unsafe { (birnam_abi::string::make)(value.as_ptr(), value.len()) },
        Err(env::VarError::NotPresent) => api.nilObj,
        Err(env::VarError::NotUnicode(_)) => unsafe {
            numeric_error(api, b"ioError\0", b"could not read environment")
        },
    }
}

unsafe extern "C" fn system_load(_self_object: Oop, args: *const Oop, count: usize) -> Oop {
    const MAX_SOURCE_BYTES: u64 = 1 << 30;
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(path_object) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    let Some(path_bytes) = (unsafe { string_bytes(api, path_object) }) else {
        return api.nilObj;
    };
    let Some(path) = path_from_bytes(path_bytes) else {
        return unsafe { numeric_error(api, b"ioError\0", b"could not read source file") };
    };
    let Ok(file) = File::open(path) else {
        return unsafe { numeric_error(api, b"ioError\0", b"could not read source file") };
    };
    let mut source = Vec::new();
    if file
        .take(MAX_SOURCE_BYTES.saturating_add(1))
        .read_to_end(&mut source)
        .is_err()
        || source.len() > MAX_SOURCE_BYTES as usize
    {
        return unsafe { numeric_error(api, b"ioError\0", b"could not read source file") };
    }
    if unsafe { (api.isExported)(c"Codegen".as_ptr()) } == 0 {
        return unsafe { numeric_error(api, b"noCodegen\0", b"Codegen module is not loaded") };
    }
    let codegen = unsafe { (api.import)(c"Codegen".as_ptr()) };
    if codegen.is_null() {
        return unsafe { numeric_error(api, b"noCodegen\0", b"Codegen module is not loaded") };
    }
    let source_object = unsafe { (birnam_abi::string::make)(source.as_ptr(), source.len()) };

    // Preserve the module-source staging protocol used by the native prelude:
    // inspect a declaration first, record it by declared identity, then run
    // the source-file-aware persisting evaluator.
    let revision = unsafe { send_args(api, codegen, b"sourceRevisionOf:\0", &[source_object]) };
    let revision_is_error = unsafe { is_error(api, revision) };
    let mut declared_name = api.nilObj;
    if !revision_is_error && revision != api.nilObj {
        let name = unsafe { send_args(api, revision, b"at:\0", &[(birnam_abi::integer::make)(1)]) };
        declared_name = name;
        let recorded = unsafe {
            send_args(
                api,
                codegen,
                b"recordModuleSource:named:\0",
                &[source_object, name],
            )
        };
        if unsafe { is_error(api, recorded) } {
            return recorded;
        }
    }

    let result = unsafe {
        send_args(
            api,
            codegen,
            b"evalPersisting:from:\0",
            &[source_object, path_object],
        )
    };
    if result == api.nilObj {
        let last_error = unsafe { send0(api, codegen, b"lastError\0") };
        if last_error != api.nilObj {
            if let Some(bytes) = unsafe { string_bytes(api, last_error) } {
                eprintln!(
                    "birnam: load {} failed:\n{}",
                    path.display(),
                    String::from_utf8_lossy(bytes)
                );
            }
            return unsafe {
                numeric_error(api, b"syntaxError\0", b"source file failed to compile")
            };
        }
        return result;
    }
    if unsafe { is_error(api, result) } {
        if let Some(rendered) = unsafe { printable_string(api, result) } {
            eprintln!(
                "birnam: load {}: {}",
                path.display(),
                String::from_utf8_lossy(rendered)
            );
        }
    } else if declared_name != api.nilObj && unsafe { (api.isExported)(c"Module".as_ptr()) } != 0 {
        let module = unsafe { (api.import)(c"Module".as_ptr()) };
        if !module.is_null() {
            let _ = unsafe { send_args(api, module, b"markLoaded:\0", &[declared_name]) };
        }
    }
    result
}

unsafe extern "C" fn system_load_module(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    const RTLD_LAZY: i32 = 1;
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(path_object) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    let Some(path_bytes) = (unsafe { string_bytes(api, path_object) }) else {
        return api.nilObj;
    };
    let Ok(path) = CString::new(path_bytes) else {
        return unsafe { numeric_error(api, b"ioError\0", b"could not load module dylib") };
    };
    let handle = unsafe { (api.dlopen)(path.as_ptr(), RTLD_LAZY) };
    if handle.is_null() {
        return unsafe { numeric_error(api, b"ioError\0", b"could not load module dylib") };
    }
    let symbol = unsafe { (api.dlsym)(handle, c"module_init".as_ptr()) };
    if symbol.is_null() {
        return unsafe { numeric_error(api, b"notAModule\0", b"dylib has no module_init") };
    }
    // SAFETY: dlsym returned the module_init symbol from a live handle; all
    // Birnam module initializers share this frozen C ABI and receive the
    // process-owned IdApi with no state payload for this loader path.
    let init: ModuleInitFn = unsafe { core::mem::transmute(symbol) };
    unsafe { init(runtime().api as *mut IdApi, ptr::null_mut()) };
    self_object
}

unsafe extern "C" fn system_read_line(_self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    let stdin = io::stdin();
    let mut stdin = stdin.lock();
    let mut line = Vec::new();
    match stdin.read_until(b'\n', &mut line) {
        Ok(0) => api.nilObj,
        Ok(_) => {
            if line.last() == Some(&b'\n') {
                line.pop();
            }
            unsafe { (birnam_abi::string::make)(line.as_ptr(), line.len()) }
        }
        Err(_) => unsafe { numeric_error(api, b"ioError\0", b"stdin read failed") },
    }
}

unsafe extern "C" fn system_gc(_self_object: Oop, _args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    unsafe { (birnam_abi::integer::make)((api.gcCollect)() as i64) }
}

unsafe extern "C" fn system_live_objects(
    _self_object: Oop,
    _args: *const Oop,
    count: usize,
) -> Oop {
    let api = unsafe { api_ref() };
    if count != 0 {
        return api.nilObj;
    }
    unsafe { (birnam_abi::integer::make)((api.gcLiveCount)() as i64) }
}

unsafe extern "C" fn system_auto_gc(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(value) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    if value != api.trueObj && value != api.falseObj {
        return api.nilObj;
    }
    unsafe { (api.gcSetAuto)(if value == api.trueObj { 1 } else { 0 }) };
    self_object
}

unsafe extern "C" fn system_precise_gc(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    let api = unsafe { api_ref() };
    if count != 1 {
        return api.nilObj;
    }
    let Some(value) = (unsafe { argument(args, count) }) else {
        return api.nilObj;
    };
    if value != api.trueObj && value != api.falseObj {
        return api.nilObj;
    }
    unsafe { (api.gcSetPrecise)(if value == api.trueObj { 1 } else { 0 }) };
    self_object
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

unsafe fn printable_string(api: &IdApi, value: Oop) -> Option<&[u8]> {
    if let Some(bytes) = unsafe { string_bytes(api, value) } {
        return Some(bytes);
    }
    if value.is_null() {
        return None;
    }
    let rendered = unsafe { send0(api, value, b"printString\0") };
    unsafe { string_bytes(api, rendered) }
}

unsafe extern "C" fn system_print(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { system_output(self_object, args, count, false) }
}

unsafe extern "C" fn system_println(self_object: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { system_output(self_object, args, count, true) }
}

unsafe fn install_system_protocol(api: &IdApi) {
    let (system_vt, system_object) = if unsafe { (api.isExported)(c"System".as_ptr()) } != 0 {
        // SAFETY: an exported System family is process-owned and remains live
        // across reloads; extend its vtable instead of minting a duplicate.
        unsafe {
            (
                (api.import)(c"SystemVt".as_ptr()),
                (api.import)(c"System".as_ptr()),
            )
        }
    } else {
        let system_vt = unsafe { (api.proto)(api.object_vt) };
        let system_object = unsafe { (api.alloc)(system_vt, 0) };
        unsafe { birnam_abi::set_vtable_owner(api, system_vt, system_object) };
        unsafe { (api.exportObj)(c"SystemVt".as_ptr(), system_vt) };
        unsafe { (api.exportObj)(c"System".as_ptr(), system_object) };
        (system_vt, system_object)
    };
    let print = unsafe { intern(api, b"print:\0") };
    let println = unsafe { intern(api, b"println:\0") };
    let read_file = unsafe { intern(api, b"readFile:\0") };
    let write_file = unsafe { intern(api, b"writeFile:contents:\0") };
    let get_env = unsafe { intern(api, b"getEnv:\0") };
    let load = unsafe { intern(api, b"load:\0") };
    let load_module = unsafe { intern(api, b"loadModule:\0") };
    let read_line = unsafe { intern(api, b"readLine\0") };
    let gc = unsafe { intern(api, b"gc\0") };
    let live_objects = unsafe { intern(api, b"liveObjects\0") };
    let auto_gc = unsafe { intern(api, b"autoGc:\0") };
    let precise_gc = unsafe { intern(api, b"preciseGc:\0") };
    // SAFETY: the System vtable and callback addresses are process-lifetime
    // values. The singleton is retained by the exported family.
    unsafe {
        (api.method)(system_vt, print, system_print);
        (api.method)(system_vt, println, system_println);
        (api.method)(system_vt, read_file, system_read_file);
        (api.method)(system_vt, write_file, system_write_file);
        (api.method)(system_vt, get_env, system_get_env);
        (api.method)(system_vt, load, system_load);
        (api.method)(system_vt, load_module, system_load_module);
        (api.method)(system_vt, read_line, system_read_line);
        (api.method)(system_vt, gc, system_gc);
        (api.method)(system_vt, live_objects, system_live_objects);
        (api.method)(system_vt, auto_gc, system_auto_gc);
        (api.method)(system_vt, precise_gc, system_precise_gc);
    }
    let _ = system_object;
}

unsafe fn install_block_protocol(api: &IdApi) {
    let (block_vt, block_proto) = if unsafe { (api.isExported)(c"Block".as_ptr()) } != 0 {
        let block_vt = unsafe { (api.import)(c"BlockVt".as_ptr()) };
        let block_proto = unsafe { (api.import)(c"Block".as_ptr()) };
        (block_vt, block_proto)
    } else {
        let block_vt = unsafe { (api.proto)(api.object_vt) };
        // SAFETY: a Block payload consists only of captured Oops, and this
        // first native loop rung does not inspect the payload itself.
        unsafe { (api.setLayout)(block_vt, ptr::null(), 0, birnam_abi::SLOT_OOP) };
        let block_proto = unsafe { (api.alloc)(block_vt, 0) };
        unsafe { birnam_abi::set_vtable_owner(api, block_vt, block_proto) };
        unsafe { (api.exportObj)(c"Block".as_ptr(), block_proto) };
        unsafe { (api.exportObj)(c"BlockVt".as_ptr(), block_vt) };
        (block_vt, block_proto)
    };
    let _ = block_proto;
    let while_true = unsafe { intern(api, b"whileTrue:\0") };
    let while_false = unsafe { intern(api, b"whileFalse:\0") };
    // SAFETY: the Block vtable is process-owned and remains live with the image.
    unsafe {
        (api.method)(block_vt, while_true, block_while_true);
        (api.method)(block_vt, while_false, block_while_false);
    }
}

unsafe fn bring_up(api_pointer: *mut IdApi) {
    if api_pointer.is_null() {
        abort("module_init received a null IdApi");
    }
    let api = unsafe { &*api_pointer };
    let _ = RUNTIME.get_or_init(|| Runtime {
        api: api_pointer as usize,
    });
    unsafe { birnam_abi::printing::install(api_pointer, runtime().api as *const IdApi) };
    unsafe { install_block_protocol(api) };
    unsafe { install_system_protocol(api) };
}

/// Identifies the original Rust Prelude rung; identity remains stable.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_prelude_rust_identity() -> *const c_char {
    RUST_PRELUDE_IDENTITY.as_ptr().cast()
}

/// This module copies or synchronously consumes every borrowed Symbol name.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> core::ffi::c_int {
    1
}

/// Initialize the Rust prelude against the selected floor's IdApi.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api: *mut IdApi, _state: *mut c_void) -> *mut c_void {
    // SAFETY: bring_up enforces the null boundary and only retains the
    // process-owned IdApi address.
    unsafe { bring_up(api) };
    ptr::null_mut()
}

#[cfg(test)]
mod tests {
    use super::{RUST_PRELUDE_IDENTITY, birnam_prelude_rust_identity, write_system_output_to};
    use std::ffi::CStr;
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
    fn identity_is_a_terminated_static_string() {
        // SAFETY: the export points to immutable process-lifetime storage.
        let identity = unsafe { CStr::from_ptr(birnam_prelude_rust_identity()) };
        assert_eq!(identity.to_bytes_with_nul(), RUST_PRELUDE_IDENTITY);
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

birnam_abi::integer_dependency!();

birnam_abi::string_dependency!();

birnam_abi::printing_dependency!();
