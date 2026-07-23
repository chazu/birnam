//! Rust PEG pattern representation.
//!
//! N3.2b ports the owned pattern graph and constructor/combinator protocol. The
//! matcher, Grammar, and CST remain on the Zig oracle until later slices, so
//! this explicit dylib must not be selected by the normal language stack yet.

use birnam_abi::{IdApi, Oop};
use core::ffi::{c_char, c_void};
use core::mem::size_of;
use core::ptr;
use std::slice;
use std::sync::OnceLock;

const RUST_PEG_IDENTITY: &[u8] = b"rust-peg-n3.2-patterns\0";
const PEG_NAME: &[u8] = b"Peg\0";
const PEG_VT_NAME: &[u8] = b"PegFacadeVt\0";
const PATTERN_VT_NAME: &[u8] = b"PegPatternVt\0";
const GRAMMAR_VT_NAME: &[u8] = b"PegGrammarVt\0";

#[repr(u8)]
#[derive(Clone, Copy)]
enum Kind {
    Literal = 0,
    Any = 1,
    Eof = 2,
    Charset = 3,
    Seq = 4,
    Choice = 5,
    Star = 6,
    Plus = 7,
    Opt = 8,
    And = 9,
    Not = 10,
    RuleRef = 11,
}

#[repr(C)]
struct Range {
    lo: u8,
    hi: u8,
}

#[repr(C)]
struct PatternState {
    kind: Kind,
    literal: Vec<u8>,
    children: Vec<Oop>,
    ranges: Vec<Range>,
    rule_name: Oop,
}

impl PatternState {
    fn new(kind: Kind) -> Self {
        Self {
            kind,
            literal: Vec::new(),
            children: Vec::new(),
            ranges: Vec::new(),
            rule_name: ptr::null_mut(),
        }
    }
}

#[derive(Clone, Copy)]
struct Runtime {
    api: usize,
    pattern_vt: usize,
    peg_vt: usize,
    peg: usize,
    grammar_vt: usize,
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

fn abort(message: &str) -> ! {
    eprintln!("birnam Rust PEG: {message}");
    std::process::abort();
}

fn runtime() -> &'static Runtime {
    RUNTIME
        .get()
        .unwrap_or_else(|| abort("module_init was not called"))
}

unsafe fn api(runtime: &Runtime) -> &'static IdApi {
    // SAFETY: module_init records the process-owned IdApi table and the host
    // keeps the selected floor loaded for the process lifetime.
    unsafe { &*(runtime.api as *const IdApi) }
}

fn as_oop(value: usize) -> Oop {
    value as Oop
}

fn oop_address(value: Oop) -> usize {
    value as usize
}

unsafe fn intern(api: &IdApi, name: &[u8]) -> Oop {
    debug_assert_eq!(name.last(), Some(&0));
    // SAFETY: all selectors and export names are static NUL-terminated bytes.
    unsafe { (api.intern)(name.as_ptr().cast::<c_char>()) }
}

unsafe fn import(api: &IdApi, name: &[u8]) -> Oop {
    // SAFETY: all import names are static NUL-terminated bytes.
    unsafe { (api.import)(name.as_ptr().cast::<c_char>()) }
}

unsafe fn state_mut(object: Oop) -> &'static mut PatternState {
    if object.is_null() {
        abort("Pattern object was null");
    }
    // SAFETY: every Rust Pattern object is allocated with this exact payload.
    unsafe { &mut *(object.cast::<PatternState>()) }
}

unsafe extern "C" fn trace_pattern(object: Oop, mark: birnam_abi::MarkFn) {
    let state = unsafe { state_mut(object) };
    for child in state.children.iter().copied() {
        // SAFETY: children are Oop carriers retained by this Pattern.
        unsafe { mark(child) };
    }
    if !state.rule_name.is_null() {
        // SAFETY: rule_name is an interned Symbol Oop.
        unsafe { mark(state.rule_name) };
    }
}

unsafe extern "C" fn finalize_pattern(object: Oop) {
    if !object.is_null() {
        // SAFETY: finalization runs once for a live Pattern payload.
        unsafe { ptr::drop_in_place(object.cast::<PatternState>()) };
    }
}

unsafe fn new_pattern(kind: Kind) -> Oop {
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    let object = unsafe { (api.alloc)(as_oop(runtime.pattern_vt), size_of::<PatternState>()) };
    if object.is_null() {
        abort("could not allocate Pattern object");
    }
    // SAFETY: the IdApi allocation has exactly the requested payload size.
    unsafe { ptr::write(object.cast::<PatternState>(), PatternState::new(kind)) };
    unsafe { (api.setFinalizer)(object, finalize_pattern) };
    object
}

unsafe fn string_bytes(api: &IdApi, object: Oop) -> Vec<u8> {
    let mut length = 0;
    // SAFETY: the object is expected to satisfy the existing String protocol.
    let pointer = unsafe { (api.stringBytes)(object, &mut length) };
    if pointer.is_null() && length != 0 {
        abort("String protocol returned null bytes");
    }
    if length == 0 {
        return Vec::new();
    }
    // SAFETY: IdApi owns the immutable String storage for this call.
    unsafe { slice::from_raw_parts(pointer, length) }.to_vec()
}

unsafe fn args<'a>(pointer: *const Oop, count: usize) -> &'a [Oop] {
    // SAFETY: the Method caller supplies the frozen argument-buffer contract.
    unsafe { birnam_abi::borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("invalid PEG method argument buffer"))
}

unsafe extern "C" fn peg_literal(_self_object: Oop, pointer: *const Oop, count: usize) -> Oop {
    let values = unsafe { args(pointer, count) };
    if values.len() != 1 {
        abort("literal: expects one argument");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    let object = unsafe { new_pattern(Kind::Literal) };
    let literal = unsafe { string_bytes(api, values[0]) };
    unsafe { state_mut(object).literal = literal };
    object
}

unsafe extern "C" fn peg_nullary(kind: Kind) -> Oop {
    unsafe { new_pattern(kind) }
}

unsafe extern "C" fn peg_any(_self_object: Oop, pointer: *const Oop, count: usize) -> Oop {
    if !unsafe { args(pointer, count) }.is_empty() {
        abort("any expects no arguments");
    }
    unsafe { peg_nullary(Kind::Any) }
}

unsafe extern "C" fn peg_eof(_self_object: Oop, pointer: *const Oop, count: usize) -> Oop {
    if !unsafe { args(pointer, count) }.is_empty() {
        abort("eof expects no arguments");
    }
    unsafe { peg_nullary(Kind::Eof) }
}

unsafe extern "C" fn peg_charset(_self_object: Oop, pointer: *const Oop, count: usize) -> Oop {
    if !unsafe { args(pointer, count) }.is_empty() {
        abort("charset expects no arguments");
    }
    unsafe { peg_nullary(Kind::Charset) }
}

unsafe extern "C" fn peg_rule_ref(_self_object: Oop, pointer: *const Oop, count: usize) -> Oop {
    let values = unsafe { args(pointer, count) };
    if values.len() != 1 {
        abort("ruleRef: expects one argument");
    }
    let object = unsafe { new_pattern(Kind::RuleRef) };
    unsafe { state_mut(object).rule_name = values[0] };
    object
}

unsafe extern "C" fn peg_seq(_self_object: Oop, pointer: *const Oop, count: usize) -> Oop {
    if !unsafe { args(pointer, count) }.is_empty() {
        abort("seq expects no arguments");
    }
    unsafe { peg_nullary(Kind::Seq) }
}

unsafe extern "C" fn peg_choice(_self_object: Oop, pointer: *const Oop, count: usize) -> Oop {
    if !unsafe { args(pointer, count) }.is_empty() {
        abort("choice expects no arguments");
    }
    unsafe { peg_nullary(Kind::Choice) }
}

unsafe extern "C" fn peg_unary(kind: Kind, self_object: Oop) -> Oop {
    let object = unsafe { new_pattern(kind) };
    unsafe { state_mut(object).children.push(self_object) };
    object
}

unsafe extern "C" fn pattern_star(self_object: Oop, pointer: *const Oop, count: usize) -> Oop {
    if !unsafe { args(pointer, count) }.is_empty() {
        abort("star expects no arguments");
    }
    unsafe { peg_unary(Kind::Star, self_object) }
}

unsafe extern "C" fn pattern_plus(self_object: Oop, pointer: *const Oop, count: usize) -> Oop {
    if !unsafe { args(pointer, count) }.is_empty() {
        abort("plus expects no arguments");
    }
    unsafe { peg_unary(Kind::Plus, self_object) }
}

unsafe extern "C" fn pattern_opt(self_object: Oop, pointer: *const Oop, count: usize) -> Oop {
    if !unsafe { args(pointer, count) }.is_empty() {
        abort("opt expects no arguments");
    }
    unsafe { peg_unary(Kind::Opt, self_object) }
}

unsafe extern "C" fn pattern_negate(self_object: Oop, pointer: *const Oop, count: usize) -> Oop {
    if !unsafe { args(pointer, count) }.is_empty() {
        abort("negate expects no arguments");
    }
    unsafe { peg_unary(Kind::Not, self_object) }
}

unsafe extern "C" fn pattern_lookahead(self_object: Oop, pointer: *const Oop, count: usize) -> Oop {
    if !unsafe { args(pointer, count) }.is_empty() {
        abort("lookahead expects no arguments");
    }
    unsafe { peg_unary(Kind::And, self_object) }
}

unsafe extern "C" fn pattern_add(self_object: Oop, pointer: *const Oop, count: usize) -> Oop {
    let values = unsafe { args(pointer, count) };
    if values.len() != 1 {
        abort("add: expects one argument");
    }
    unsafe { state_mut(self_object).children.push(values[0]) };
    self_object
}

unsafe extern "C" fn pattern_from_to(self_object: Oop, pointer: *const Oop, count: usize) -> Oop {
    let values = unsafe { args(pointer, count) };
    if values.len() != 2 {
        abort("from:to: expects two arguments");
    }
    let api = unsafe { api(runtime()) };
    let lo = unsafe { (api.smallIntValue)(values[0]) } as u8;
    let hi = unsafe { (api.smallIntValue)(values[1]) } as u8;
    unsafe { state_mut(self_object).ranges.push(Range { lo, hi }) };
    self_object
}

unsafe fn register_methods(runtime: &Runtime) {
    let api = unsafe { api(runtime) };
    let peg_vt = as_oop(runtime.peg_vt);
    let pattern_vt = as_oop(runtime.pattern_vt);
    let peg_methods: &[(&[u8], birnam_abi::Method)] = &[
        (b"literal:\0", peg_literal),
        (b"any\0", peg_any),
        (b"eof\0", peg_eof),
        (b"charset\0", peg_charset),
        (b"ruleRef:\0", peg_rule_ref),
        (b"seq\0", peg_seq),
        (b"choice\0", peg_choice),
    ];
    for (selector, method) in peg_methods {
        let selector = unsafe { intern(api, selector) };
        unsafe { (api.method)(peg_vt, selector, *method) };
    }
    let pattern_methods: &[(&[u8], birnam_abi::Method)] = &[
        (b"star\0", pattern_star),
        (b"plus\0", pattern_plus),
        (b"opt\0", pattern_opt),
        (b"negate\0", pattern_negate),
        (b"lookahead\0", pattern_lookahead),
        (b"add:\0", pattern_add),
        (b"from:to:\0", pattern_from_to),
    ];
    for (selector, method) in pattern_methods {
        let selector = unsafe { intern(api, selector) };
        unsafe { (api.method)(pattern_vt, selector, *method) };
    }
    unsafe {
        (api.setLayout)(pattern_vt, ptr::null(), 0, birnam_abi::SLOT_RAW);
        (api.setTraceHook)(pattern_vt, trace_pattern);
    }
}

unsafe fn bring_up(api_pointer: *mut IdApi) {
    if api_pointer.is_null() {
        abort("module_init received a null IdApi");
    }
    let api = unsafe { &*api_pointer };
    let runtime = *RUNTIME.get_or_init(|| {
        let reloading = unsafe { (api.isExported)(PEG_NAME.as_ptr().cast()) != 0 };
        let (pattern_vt, peg_vt, peg, grammar_vt) = if reloading {
            (
                unsafe { import(api, PATTERN_VT_NAME) },
                unsafe { import(api, PEG_VT_NAME) },
                unsafe { import(api, PEG_NAME) },
                unsafe { import(api, GRAMMAR_VT_NAME) },
            )
        } else {
            let pattern_vt = unsafe { (api.proto)(api.object_vt) };
            let peg_vt = unsafe { (api.proto)(api.object_vt) };
            let grammar_vt = unsafe { (api.proto)(api.object_vt) };
            let peg = unsafe { (api.alloc)(peg_vt, 0) };
            (pattern_vt, peg_vt, peg, grammar_vt)
        };
        if [pattern_vt, peg_vt, peg, grammar_vt]
            .iter()
            .any(|value| value.is_null())
        {
            abort("PEG exports were incomplete");
        }
        Runtime {
            api: api_pointer as usize,
            pattern_vt: oop_address(pattern_vt),
            peg_vt: oop_address(peg_vt),
            peg: oop_address(peg),
            grammar_vt: oop_address(grammar_vt),
        }
    });
    unsafe { register_methods(&runtime) };
    if unsafe { (api.isExported)(PEG_NAME.as_ptr().cast()) == 0 } {
        unsafe {
            let _ = (api.exportObj)(PATTERN_VT_NAME.as_ptr().cast(), as_oop(runtime.pattern_vt));
            let _ = (api.exportObj)(PEG_VT_NAME.as_ptr().cast(), as_oop(runtime.peg_vt));
            let _ = (api.exportObj)(GRAMMAR_VT_NAME.as_ptr().cast(), as_oop(runtime.grammar_vt));
            let _ = (api.exportObj)(PEG_NAME.as_ptr().cast(), as_oop(runtime.peg));
        }
    }
}

/// Identifies this explicit Rust PEG patterns dylib.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_peg_rust_identity() -> *const c_char {
    RUST_PEG_IDENTITY.as_ptr().cast()
}

/// Initialize the pattern representation against the process-owned IdApi.
///
/// # Safety
///
/// `api` is the live host IdApi table or null for the documented fatal boundary.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api: *mut IdApi, _state: *mut c_void) -> *mut c_void {
    unsafe { bring_up(api) };
    ptr::null_mut()
}

#[cfg(test)]
mod tests {
    use super::{RUST_PEG_IDENTITY, birnam_peg_rust_identity};
    use std::ffi::CStr;

    #[test]
    fn pattern_identity_is_a_terminated_static_string() {
        // SAFETY: the export points to immutable crate storage for the process
        // lifetime.
        let identity = unsafe { CStr::from_ptr(birnam_peg_rust_identity()) };
        assert_eq!(identity.to_bytes_with_nul(), RUST_PEG_IDENTITY);
    }
}
