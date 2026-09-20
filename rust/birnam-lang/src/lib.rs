//! Rust language-parser rung.
//!
//! The grammar remains a policy object built through the PEG message API. This
//! crate owns the grammar construction and Lang facade, while the matcher and
//! transient CST remain in the dynamically replaceable PEG rung.

use birnam_abi::{IdApi, MarkFn, Oop, borrowed_oop_args};
use core::ffi::{c_char, c_void};
use core::ptr;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

const RUST_LANG_IDENTITY: &[u8] = b"rust-lang-n5.2-threaded-messages\0";
const PEG_NAME: &[u8] = b"Peg\0";
const LANG_NAME: &[u8] = b"Lang\0";
const LANG_VT_NAME: &[u8] = b"LangVt\0";
const GRAMMAR_NAME: &[u8] = b"BirnamGrammar\0";

#[derive(Clone, Copy)]
struct Runtime {
    api: usize,
    roots: usize,
    exports: usize,
    presence: usize,
    peg: usize,
    lang_vt: usize,
    lang: usize,
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();
static ROOT_REGISTERED: OnceLock<()> = OnceLock::new();
static GRAMMAR: AtomicUsize = AtomicUsize::new(0);

unsafe extern "C" fn mark_runtime_roots(mark: MarkFn) {
    if let Some(runtime) = RUNTIME.get() {
        for value in [runtime.peg, runtime.lang_vt, runtime.lang] {
            unsafe { mark(value as Oop) };
        }
    }
    let grammar = GRAMMAR.load(Ordering::Acquire);
    if grammar != 0 {
        unsafe { mark(grammar as Oop) };
    }
}

fn abort(message: &str) -> ! {
    eprintln!("birnam Rust Lang: {message}");
    std::process::abort();
}

fn runtime() -> &'static Runtime {
    RUNTIME
        .get()
        .unwrap_or_else(|| abort("module_init was not called"))
}

unsafe fn runtime_api(runtime: &Runtime) -> &'static IdApi {
    // SAFETY: module_init records the process-owned table for the process life.
    unsafe { &*(runtime.api as *const IdApi) }
}

fn as_oop(value: usize) -> Oop {
    value as Oop
}

unsafe fn intern(api: &IdApi, name: &[u8]) -> Oop {
    debug_assert_eq!(name.last(), Some(&0));
    // SAFETY: all names are static NUL-terminated selector strings.
    unsafe { (api.intern)(name.as_ptr().cast::<c_char>()) }
}

unsafe fn send(api: &IdApi, receiver: Oop, selector: &[u8], args: &[Oop]) -> Oop {
    let selector = unsafe { intern(api, selector) };
    // SAFETY: the argument slice is live for the synchronous send.
    unsafe { (api.send)(receiver, selector, args.as_ptr(), args.len()) }
}

unsafe fn send0(api: &IdApi, receiver: Oop, selector: &[u8]) -> Oop {
    unsafe { send(api, receiver, selector, &[]) }
}

unsafe fn send1(api: &IdApi, receiver: Oop, selector: &[u8], arg: Oop) -> Oop {
    unsafe { send(api, receiver, selector, &[arg]) }
}

unsafe fn send2(api: &IdApi, receiver: Oop, selector: &[u8], first: Oop, second: Oop) -> Oop {
    unsafe { send(api, receiver, selector, &[first, second]) }
}

unsafe fn literal(api: &IdApi, peg: Oop, bytes: &[u8]) -> Oop {
    let string = unsafe { (birnam_abi::string::make)(bytes.as_ptr(), bytes.len()) };
    unsafe { send1(api, peg, b"literal:\0", string) }
}

unsafe fn rule_ref(api: &IdApi, peg: Oop, name: &[u8]) -> Oop {
    let symbol = unsafe { intern(api, name) };
    unsafe { send1(api, peg, b"ruleRef:\0", symbol) }
}

unsafe fn any(api: &IdApi, peg: Oop) -> Oop {
    unsafe { send0(api, peg, b"any\0") }
}

unsafe fn eof(api: &IdApi, peg: Oop) -> Oop {
    unsafe { send0(api, peg, b"eof\0") }
}

unsafe fn star(api: &IdApi, pattern: Oop) -> Oop {
    unsafe { send0(api, pattern, b"star\0") }
}

unsafe fn plus(api: &IdApi, pattern: Oop) -> Oop {
    unsafe { send0(api, pattern, b"plus\0") }
}

unsafe fn optional(api: &IdApi, pattern: Oop) -> Oop {
    unsafe { send0(api, pattern, b"opt\0") }
}

unsafe fn negate(api: &IdApi, pattern: Oop) -> Oop {
    unsafe { send0(api, pattern, b"negate\0") }
}

unsafe fn seq(api: &IdApi, peg: Oop, children: &[Oop]) -> Oop {
    let result = unsafe { send0(api, peg, b"seq\0") };
    for child in children {
        let _ = unsafe { send1(api, result, b"add:\0", *child) };
    }
    result
}

unsafe fn choice(api: &IdApi, peg: Oop, children: &[Oop]) -> Oop {
    let result = unsafe { send0(api, peg, b"choice\0") };
    for child in children {
        let _ = unsafe { send1(api, result, b"add:\0", *child) };
    }
    result
}

unsafe fn charset(api: &IdApi, peg: Oop, ranges: &[(u8, u8)]) -> Oop {
    let result = unsafe { send0(api, peg, b"charset\0") };
    for (lo, hi) in ranges {
        let _ = unsafe {
            send2(
                api,
                result,
                b"from:to:\0",
                (birnam_abi::integer::make)(i64::from(*lo)),
                (birnam_abi::integer::make)(i64::from(*hi)),
            )
        };
    }
    result
}

unsafe fn define_rule(api: &IdApi, grammar: Oop, name: &[u8], pattern: Oop) {
    let symbol = unsafe { intern(api, name) };
    let _ = unsafe { send2(api, grammar, b"rule:is:\0", symbol, pattern) };
}

unsafe fn build_grammar(api: &IdApi, peg: Oop) -> Oop {
    let grammar = unsafe { send0(api, peg, b"grammar\0") };

    let newline = unsafe { charset(api, peg, &[(b'\n', b'\n')]) };
    let comment = unsafe {
        seq(
            api,
            peg,
            &[
                literal(api, peg, b"//"),
                star(api, seq(api, peg, &[negate(api, newline), any(api, peg)])),
            ],
        )
    };
    let space_char = unsafe {
        charset(
            api,
            peg,
            &[(b' ', b' '), (b'\t', b'\t'), (b'\n', b'\n'), (b'\r', b'\r')],
        )
    };
    let whitespace = unsafe { star(api, choice(api, peg, &[space_char, comment])) };
    let spacing1 = unsafe { plus(api, choice(api, peg, &[space_char, comment])) };
    let alpha = unsafe { charset(api, peg, &[(b'a', b'z'), (b'A', b'Z'), (b'_', b'_')]) };
    let alnum = unsafe {
        charset(
            api,
            peg,
            &[(b'a', b'z'), (b'A', b'Z'), (b'0', b'9'), (b'_', b'_')],
        )
    };
    let digit = unsafe { charset(api, peg, &[(b'0', b'9')]) };

    unsafe {
        define_rule(
            api,
            grammar,
            b"ident\0",
            seq(api, peg, &[alpha, star(api, alnum)]),
        );
        define_rule(
            api,
            grammar,
            b"number\0",
            seq(
                api,
                peg,
                &[optional(api, literal(api, peg, b"-")), plus(api, digit)],
            ),
        );
        define_rule(
            api,
            grammar,
            b"float\0",
            seq(
                api,
                peg,
                &[
                    optional(api, literal(api, peg, b"-")),
                    plus(api, digit),
                    literal(api, peg, b"."),
                    plus(api, digit),
                ],
            ),
        );
        define_rule(
            api,
            grammar,
            b"string\0",
            seq(
                api,
                peg,
                &[
                    literal(api, peg, b"\""),
                    star(
                        api,
                        choice(
                            api,
                            peg,
                            &[
                                seq(api, peg, &[literal(api, peg, b"\\"), any(api, peg)]),
                                seq(
                                    api,
                                    peg,
                                    &[negate(api, literal(api, peg, b"\"")), any(api, peg)],
                                ),
                            ],
                        ),
                    ),
                    literal(api, peg, b"\""),
                ],
            ),
        );
        define_rule(
            api,
            grammar,
            b"binop\0",
            charset(
                api,
                peg,
                &[
                    (b'=', b'='),
                    (b'<', b'<'),
                    (b'>', b'>'),
                    (b'&', b'&'),
                    (b'|', b'|'),
                    (b',', b','),
                    (b'+', b'+'),
                    (b'-', b'-'),
                    (b'*', b'*'),
                    (b'/', b'/'),
                    (b'~', b'~'),
                    (b'%', b'%'),
                    (b'@', b'@'),
                    (b'!', b'!'),
                    (b'^', b'^'),
                ],
            ),
        );
        define_rule(
            api,
            grammar,
            b"symbol\0",
            seq(
                api,
                peg,
                &[
                    literal(api, peg, b"#"),
                    choice(
                        api,
                        peg,
                        &[
                            plus(
                                api,
                                seq(
                                    api,
                                    peg,
                                    &[rule_ref(api, peg, b"ident\0"), literal(api, peg, b":")],
                                ),
                            ),
                            rule_ref(api, peg, b"ident\0"),
                            plus(api, rule_ref(api, peg, b"binop\0")),
                        ],
                    ),
                ],
            ),
        );
        define_rule(
            api,
            grammar,
            b"primary\0",
            choice(
                api,
                peg,
                &[
                    rule_ref(api, peg, b"float\0"),
                    rule_ref(api, peg, b"number\0"),
                    rule_ref(api, peg, b"string\0"),
                    rule_ref(api, peg, b"symbol\0"),
                    rule_ref(api, peg, b"ident\0"),
                ],
            ),
        );
        define_rule(
            api,
            grammar,
            b"keyword\0",
            seq(
                api,
                peg,
                &[rule_ref(api, peg, b"ident\0"), literal(api, peg, b":")],
            ),
        );
        define_rule(
            api,
            grammar,
            b"keywordMsg\0",
            plus(
                api,
                seq(
                    api,
                    peg,
                    &[
                        rule_ref(api, peg, b"keyword\0"),
                        whitespace,
                        rule_ref(api, peg, b"expr\0"),
                        whitespace,
                    ],
                ),
            ),
        );
        define_rule(
            api,
            grammar,
            b"binops\0",
            plus(api, rule_ref(api, peg, b"binop\0")),
        );
        define_rule(
            api,
            grammar,
            b"binaryMsg\0",
            seq(
                api,
                peg,
                &[
                    negate(
                        api,
                        choice(
                            api,
                            peg,
                            &[
                                seq(api, peg, &[literal(api, peg, b"=>"), spacing1]),
                                seq(
                                    api,
                                    peg,
                                    &[
                                        literal(api, peg, b"=>"),
                                        negate(api, rule_ref(api, peg, b"binop\0")),
                                    ],
                                ),
                            ],
                        ),
                    ),
                    rule_ref(api, peg, b"binops\0"),
                    whitespace,
                    rule_ref(api, peg, b"expr\0"),
                ],
            ),
        );
        define_rule(api, grammar, b"unaryMsg\0", rule_ref(api, peg, b"ident\0"));
        define_rule(
            api,
            grammar,
            b"threadDispatch\0",
            choice(
                api,
                peg,
                &[
                    rule_ref(api, peg, b"keywordMsg\0"),
                    rule_ref(api, peg, b"binaryMsg\0"),
                    rule_ref(api, peg, b"unaryMsg\0"),
                ],
            ),
        );
        let thread_leg = seq(
            api,
            peg,
            &[
                literal(api, peg, b"=>"),
                spacing1,
                rule_ref(api, peg, b"threadDispatch\0"),
            ],
        );
        define_rule(
            api,
            grammar,
            b"threadedMsg\0",
            seq(
                api,
                peg,
                &[
                    thread_leg,
                    star(api, seq(api, peg, &[whitespace, thread_leg])),
                ],
            ),
        );
        define_rule(
            api,
            grammar,
            b"message\0",
            choice(
                api,
                peg,
                &[
                    rule_ref(api, peg, b"threadedMsg\0"),
                    rule_ref(api, peg, b"keywordMsg\0"),
                    plus(
                        api,
                        seq(api, peg, &[rule_ref(api, peg, b"binaryMsg\0"), whitespace]),
                    ),
                    plus(
                        api,
                        seq(api, peg, &[rule_ref(api, peg, b"unaryMsg\0"), whitespace]),
                    ),
                ],
            ),
        );
        define_rule(
            api,
            grammar,
            b"send\0",
            seq(
                api,
                peg,
                &[
                    literal(api, peg, b"["),
                    whitespace,
                    rule_ref(api, peg, b"expr\0"),
                    whitespace,
                    rule_ref(api, peg, b"message\0"),
                    whitespace,
                    literal(api, peg, b"]"),
                ],
            ),
        );
        let block_header = seq(
            api,
            peg,
            &[
                star(
                    api,
                    seq(
                        api,
                        peg,
                        &[
                            literal(api, peg, b":"),
                            rule_ref(api, peg, b"ident\0"),
                            whitespace,
                        ],
                    ),
                ),
                literal(api, peg, b"|"),
            ],
        );
        define_rule(
            api,
            grammar,
            b"block\0",
            seq(
                api,
                peg,
                &[
                    literal(api, peg, b"["),
                    whitespace,
                    block_header,
                    whitespace,
                    optional(api, rule_ref(api, peg, b"statements\0")),
                    whitespace,
                    literal(api, peg, b"]"),
                ],
            ),
        );
        define_rule(
            api,
            grammar,
            b"vectorLit\0",
            seq(
                api,
                peg,
                &[
                    literal(api, peg, b"#["),
                    whitespace,
                    star(
                        api,
                        seq(api, peg, &[rule_ref(api, peg, b"expr\0"), whitespace]),
                    ),
                    literal(api, peg, b"]"),
                ],
            ),
        );
        define_rule(
            api,
            grammar,
            b"mapEntry\0",
            seq(
                api,
                peg,
                &[
                    rule_ref(api, peg, b"expr\0"),
                    whitespace,
                    literal(api, peg, b"->"),
                    whitespace,
                    rule_ref(api, peg, b"expr\0"),
                ],
            ),
        );
        define_rule(
            api,
            grammar,
            b"mapLit\0",
            seq(
                api,
                peg,
                &[
                    literal(api, peg, b"{"),
                    whitespace,
                    star(
                        api,
                        seq(
                            api,
                            peg,
                            &[
                                rule_ref(api, peg, b"mapEntry\0"),
                                whitespace,
                                optional(api, literal(api, peg, b",")),
                                whitespace,
                            ],
                        ),
                    ),
                    literal(api, peg, b"}"),
                ],
            ),
        );
        define_rule(
            api,
            grammar,
            b"logicVar\0",
            seq(
                api,
                peg,
                &[literal(api, peg, b"?"), rule_ref(api, peg, b"ident\0")],
            ),
        );
        define_rule(
            api,
            grammar,
            b"expr\0",
            choice(
                api,
                peg,
                &[
                    rule_ref(api, peg, b"block\0"),
                    rule_ref(api, peg, b"send\0"),
                    rule_ref(api, peg, b"vectorLit\0"),
                    rule_ref(api, peg, b"mapLit\0"),
                    rule_ref(api, peg, b"logicVar\0"),
                    rule_ref(api, peg, b"primary\0"),
                ],
            ),
        );
        define_rule(
            api,
            grammar,
            b"letBinding\0",
            seq(
                api,
                peg,
                &[
                    literal(api, peg, b"let"),
                    negate(api, alnum),
                    whitespace,
                    rule_ref(api, peg, b"ident\0"),
                    whitespace,
                    literal(api, peg, b":="),
                    whitespace,
                    rule_ref(api, peg, b"expr\0"),
                ],
            ),
        );
        define_rule(
            api,
            grammar,
            b"statement\0",
            choice(
                api,
                peg,
                &[
                    rule_ref(api, peg, b"letBinding\0"),
                    rule_ref(api, peg, b"expr\0"),
                ],
            ),
        );
        define_rule(
            api,
            grammar,
            b"statements\0",
            seq(
                api,
                peg,
                &[
                    rule_ref(api, peg, b"statement\0"),
                    star(
                        api,
                        seq(
                            api,
                            peg,
                            &[
                                whitespace,
                                // The separator is optional: every expr form is
                                // self-delimiting (bracketed group or single
                                // token) and nothing juxtaposes at statement
                                // level, so whitespace alone already separates.
                                optional(api, literal(api, peg, b";")),
                                whitespace,
                                rule_ref(api, peg, b"statement\0"),
                            ],
                        ),
                    ),
                    optional(api, seq(api, peg, &[whitespace, literal(api, peg, b";")])),
                ],
            ),
        );
        define_rule(
            api,
            grammar,
            b"program\0",
            seq(
                api,
                peg,
                &[
                    whitespace,
                    optional(api, rule_ref(api, peg, b"statements\0")),
                    whitespace,
                    eof(api, peg),
                ],
            ),
        );
    }
    grammar
}

unsafe extern "C" fn lang_parse(_self_object: Oop, pointer: *const Oop, count: usize) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("Lang parse: received invalid arguments"));
    if values.len() != 1 {
        abort("Lang parse: expects one argument");
    }
    let runtime = runtime();
    let api = unsafe { runtime_api(runtime) };
    let grammar = as_oop(GRAMMAR.load(Ordering::Acquire));
    if grammar.is_null() {
        abort("Lang grammar was not initialized");
    }
    let root = unsafe { intern(api, b"program\0") };
    unsafe { send2(api, grammar, b"parse:from:\0", values[0], root) }
}

unsafe extern "C" fn lang_grammar(_self_object: Oop, pointer: *const Oop, count: usize) -> Oop {
    if !unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("Lang grammar received invalid arguments"))
        .is_empty()
    {
        abort("Lang grammar expects no arguments");
    }
    as_oop(GRAMMAR.load(Ordering::Acquire))
}

unsafe extern "C" fn lang_furthest_fail(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    if !unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("Lang furthestFail received invalid arguments"))
        .is_empty()
    {
        abort("Lang furthestFail expects no arguments");
    }
    let runtime = runtime();
    let api = unsafe { runtime_api(runtime) };
    let grammar = as_oop(GRAMMAR.load(Ordering::Acquire));
    unsafe { send0(api, grammar, b"furthestFail\0") }
}

unsafe fn bring_up(api_pointer: *mut IdApi, presence: *const birnam_abi::namespace_presence::ApiV1, imports: *const birnam_abi::namespace_import::ApiV1, exports: *const birnam_abi::namespace_export::ApiV1, roots: *const birnam_abi::root_registry::ApiV1) {
    if api_pointer.is_null() {
        abort("module_init received a null IdApi");
    }
    let api = unsafe { &*api_pointer };
    let runtime = *RUNTIME.get_or_init(|| {
        let peg = unsafe { birnam_abi::namespace_import::import(imports, PEG_NAME.as_ptr().cast::<c_char>()) };
        if peg.is_null() {
            abort("Lang requires the Peg facade");
        }
        let reloading = unsafe { birnam_abi::namespace_presence::contains(presence, LANG_NAME.as_ptr().cast::<c_char>()) != 0 };
        let (lang_vt, lang) = if reloading {
            let lang_vt = unsafe { birnam_abi::namespace_import::import(imports, LANG_VT_NAME.as_ptr().cast::<c_char>()) };
            let lang = unsafe { birnam_abi::namespace_import::import(imports, LANG_NAME.as_ptr().cast::<c_char>()) };
            (lang_vt, lang)
        } else {
            let lang_vt = unsafe { (api.proto)(api.object_vt) };
            let lang = unsafe { (api.alloc)(lang_vt, 0) };
            (lang_vt, lang)
        };
        if lang_vt.is_null() || lang.is_null() {
            abort("Lang exports were incomplete");
        }
        Runtime {
            api: api_pointer as usize,
            roots: roots as usize,
            exports: exports as usize,
            presence: presence as usize,
            peg: peg as usize,
            lang_vt: lang_vt as usize,
            lang: lang as usize,
        }
    });
    let api = unsafe { runtime_api(&runtime) };
    let exports = runtime.exports as *const birnam_abi::namespace_export::ApiV1;
    let presence = runtime.presence as *const birnam_abi::namespace_presence::ApiV1;
    ROOT_REGISTERED.get_or_init(|| unsafe {
        birnam_abi::root_registry::register(runtime.roots as *const birnam_abi::root_registry::ApiV1, mark_runtime_roots);
    });
    let grammar = unsafe { build_grammar(api, as_oop(runtime.peg)) };
    GRAMMAR.store(grammar as usize, Ordering::Release);
    let lang_vt = as_oop(runtime.lang_vt);
    unsafe {
        (api.method)(lang_vt, intern(api, b"parse:\0"), lang_parse);
        (api.method)(lang_vt, intern(api, b"grammar\0"), lang_grammar);
        (api.method)(lang_vt, intern(api, b"furthestFail\0"), lang_furthest_fail);
        let _ = birnam_abi::namespace_export::export(exports, GRAMMAR_NAME.as_ptr().cast(), grammar);
        if birnam_abi::namespace_presence::contains(presence, LANG_NAME.as_ptr().cast()) == 0 {
            let _ = birnam_abi::namespace_export::export(exports, LANG_VT_NAME.as_ptr().cast(), lang_vt);
            let _ = birnam_abi::namespace_export::export(exports, LANG_NAME.as_ptr().cast(), as_oop(runtime.lang));
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn birnam_lang_rust_identity() -> *const c_char {
    RUST_LANG_IDENTITY.as_ptr().cast()
}

/// Declare that this module treats IdApi Symbol-name pointers as borrowed.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> core::ffi::c_int {
    1
}

/// Initialize the Rust grammar facade over the process-owned IdApi and PEG.
///
/// # Safety
///
/// `api` is the live host IdApi table or null for the documented fatal boundary.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api: *mut IdApi, _state: *mut c_void, presence: *const birnam_abi::namespace_presence::ApiV1, imports: *const birnam_abi::namespace_import::ApiV1, exports: *const birnam_abi::namespace_export::ApiV1, _vtables: *const birnam_abi::vtable_enumeration::ApiV2, _observation: *const birnam_abi::method_observation::ApiV1, roots: *const birnam_abi::root_registry::ApiV1, _settings: *const birnam_abi::gc_settings::ApiV1, _layouts: *const birnam_abi::payload_layout::ApiV1, _hooks: *const birnam_abi::trace_hooks::ApiV1) -> *mut c_void {
    unsafe { bring_up(api, presence, imports, exports, roots) };
    ptr::null_mut()
}

#[cfg(test)]
mod tests {
    use super::{RUST_LANG_IDENTITY, birnam_lang_rust_identity};
    use std::ffi::CStr;

    #[test]
    fn identity_is_a_terminated_static_string() {
        // SAFETY: the export points to immutable crate storage for process life.
        let identity = unsafe { CStr::from_ptr(birnam_lang_rust_identity()) };
        assert_eq!(identity.to_bytes_with_nul(), RUST_LANG_IDENTITY);
    }
}

birnam_abi::integer_dependency!();

birnam_abi::string_dependency!();
