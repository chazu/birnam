//! Rust PEG matcher and transient CST.
//!
//! N3.3 ports the ordered matcher, rule memoization, diagnostics, and transient
//! CST over the N3.2 pattern/grammar representation. The explicit dylib remains
//! an oracle until downstream language parity is proven.

use birnam_abi::{IdApi, Oop};
use core::ffi::{c_char, c_void};
use core::mem::size_of;
use core::ptr;
use std::collections::HashMap;
use std::slice;
use std::sync::{Mutex, OnceLock};

const RUST_PEG_IDENTITY: &[u8] = b"rust-peg-n3.3-matcher\0";
const PEG_NAME: &[u8] = b"Peg\0";
const PEG_VT_NAME: &[u8] = b"PegFacadeVt\0";
const PATTERN_VT_NAME: &[u8] = b"PegPatternVt\0";
const GRAMMAR_VT_NAME: &[u8] = b"PegGrammarVt\0";
const CST_VT_NAME: &[u8] = b"PegCstVt\0";

#[repr(u8)]
#[derive(Clone, Copy, PartialEq)]
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
#[derive(Clone)]
struct Range {
    lo: u8,
    hi: u8,
}

#[repr(C)]
#[derive(Clone)]
struct PatternState {
    kind: Kind,
    literal: Vec<u8>,
    children: Vec<Oop>,
    ranges: Vec<Range>,
    rule_name: Oop,
}

struct Rule {
    name: Oop,
    pattern: Oop,
}

struct GrammarState {
    rules: Vec<Rule>,
    furthest_fail: usize,
}

impl GrammarState {
    fn new() -> Self {
        Self {
            rules: Vec::new(),
            furthest_fail: 0,
        }
    }
}

struct TempNode {
    rule: Oop,
    start: usize,
    end: usize,
    children: Vec<usize>,
}

#[derive(Clone)]
struct MatchResult {
    end: usize,
    children: Vec<usize>,
}

#[derive(Clone)]
struct MemoEntry {
    ok: bool,
    end: usize,
    children: Vec<usize>,
    in_progress: bool,
}

struct Matcher {
    source: Vec<u8>,
    rules: HashMap<usize, Oop>,
    patterns: HashMap<usize, std::rc::Rc<PatternState>>,
    nodes: Vec<TempNode>,
    memo: HashMap<(usize, usize), MemoEntry>,
    furthest: usize,
    undefined_rule: Oop,
    left_recursive_rule: Oop,
}

#[repr(C)]
struct CstState {
    rule: Oop,
    start: usize,
    end: usize,
    children: Vec<Oop>,
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
    cst_vt: usize,
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();
static CST_SCRATCH: Mutex<Vec<usize>> = Mutex::new(Vec::new());

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

unsafe fn state_ref(object: Oop) -> &'static PatternState {
    if object.is_null() {
        abort("Pattern object was null");
    }
    // SAFETY: every Rust Pattern object is allocated with this exact payload.
    unsafe { &*(object.cast::<PatternState>()) }
}

unsafe fn grammar_ref(object: Oop) -> &'static GrammarState {
    if object.is_null() {
        abort("Grammar object was null");
    }
    // SAFETY: every Rust Grammar object is allocated with this exact payload.
    unsafe { &*(object.cast::<GrammarState>()) }
}

unsafe fn cst_ref(object: Oop) -> &'static CstState {
    if object.is_null() {
        abort("CST object was null");
    }
    // SAFETY: every Rust CST object is allocated with this exact payload.
    unsafe { &*(object.cast::<CstState>()) }
}

unsafe extern "C" fn trace_pattern(object: Oop, mark: birnam_abi::MarkFn) {
    let (children, rule_name) = {
        let state = unsafe { state_ref(object) };
        (state.children.clone(), state.rule_name)
    };
    for child in children {
        // SAFETY: children are Oop carriers retained by this Pattern.
        unsafe { mark(child) };
    }
    if !rule_name.is_null() {
        // SAFETY: rule_name is an interned Symbol Oop.
        unsafe { mark(rule_name) };
    }
}

unsafe extern "C" fn finalize_pattern(object: Oop) {
    if !object.is_null() {
        // SAFETY: finalization runs once for a live Pattern payload.
        unsafe { ptr::drop_in_place(object.cast::<PatternState>()) };
    }
}

unsafe extern "C" fn trace_grammar(object: Oop, mark: birnam_abi::MarkFn) {
    let rules = {
        let state = unsafe { grammar_ref(object) };
        state
            .rules
            .iter()
            .map(|rule| (rule.name, rule.pattern))
            .collect::<Vec<_>>()
    };
    for (name, pattern) in rules {
        // SAFETY: grammar rules retain both the interned name and Pattern Oop.
        unsafe {
            mark(name);
            mark(pattern);
        }
    }
}

unsafe extern "C" fn finalize_grammar(object: Oop) {
    if !object.is_null() {
        // SAFETY: finalization runs once for a live Grammar payload.
        unsafe { ptr::drop_in_place(object.cast::<GrammarState>()) };
    }
}

unsafe extern "C" fn trace_cst(object: Oop, mark: birnam_abi::MarkFn) {
    let children = unsafe { cst_ref(object).children.clone() };
    for child in children {
        // SAFETY: CST children are retained Oop values.
        unsafe { mark(child) };
    }
}

unsafe extern "C" fn finalize_cst(object: Oop) {
    if !object.is_null() {
        // SAFETY: finalization runs once for a live CST payload.
        unsafe { ptr::drop_in_place(object.cast::<CstState>()) };
    }
}

unsafe extern "C" fn cst_scratch_roots(mark: birnam_abi::MarkFn) {
    if let Some(runtime) = RUNTIME.get() {
        for value in [
            runtime.pattern_vt,
            runtime.peg_vt,
            runtime.peg,
            runtime.grammar_vt,
            runtime.cst_vt,
        ] {
            // SAFETY: these are the PEG module's process-owned cached Oops.
            unsafe { mark(value as Oop) };
        }
    }
    let roots = CST_SCRATCH
        .lock()
        .unwrap_or_else(|_| abort("CST scratch root lock poisoned"))
        .clone();
    for root in roots.iter().copied() {
        // SAFETY: scratch entries are live CST Oops retained until conversion ends.
        unsafe { mark(root as Oop) };
    }
}

fn scratch_push(object: Oop) {
    CST_SCRATCH
        .lock()
        .unwrap_or_else(|_| abort("CST scratch root lock poisoned"))
        .push(object as usize);
}

fn scratch_clear() {
    CST_SCRATCH
        .lock()
        .unwrap_or_else(|_| abort("CST scratch root lock poisoned"))
        .clear();
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

unsafe fn new_grammar() -> Oop {
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    let object = unsafe { (api.alloc)(as_oop(runtime.grammar_vt), size_of::<GrammarState>()) };
    if object.is_null() {
        abort("could not allocate Grammar object");
    }
    // SAFETY: the IdApi allocation has exactly the requested payload size.
    unsafe { ptr::write(object.cast::<GrammarState>(), GrammarState::new()) };
    unsafe { (api.setFinalizer)(object, finalize_grammar) };
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

unsafe fn rule_symbol(api: &IdApi, object: Oop) -> Oop {
    if unsafe { (api.vtableOf)(object) } != api.string_vt {
        return object;
    }
    let bytes = unsafe { string_bytes(api, object) };
    let mut name = bytes;
    name.push(0);
    // SAFETY: `name` is NUL-terminated for the duration of intern.
    unsafe { (api.intern)(name.as_ptr().cast::<c_char>()) }
}

fn note(matcher: &mut Matcher, position: usize) {
    matcher.furthest = matcher.furthest.max(position);
}

fn index_rules(rules: impl IntoIterator<Item = (Oop, Oop)>) -> HashMap<usize, Oop> {
    let mut index = HashMap::new();
    for (name, pattern) in rules {
        index.entry(name as usize).or_insert(pattern);
    }
    index
}

fn named_rule(rules: &HashMap<usize, Oop>, name: Oop) -> Option<Oop> {
    rules.get(&(name as usize)).copied()
}

fn match_pattern(matcher: &mut Matcher, pattern: Oop, position: usize) -> Option<MatchResult> {
    // Matching never calls the floor or user code. Keep each visited pattern's
    // immutable data once per parse, while allowing recursive matching to mutate
    // memo/CST scratch. Rc clones allocate nothing and hold no native-code roots.
    // The Grammar owns the original graph throughout this synchronous match.
    let state = std::rc::Rc::clone(
        matcher
            .patterns
            .entry(pattern as usize)
            .or_insert_with(|| std::rc::Rc::new(unsafe { state_ref(pattern) }.clone())),
    );
    let kind = state.kind;
    let literal = &state.literal;
    let children = &state.children;
    let ranges = &state.ranges;
    let rule_name = state.rule_name;
    match kind {
        Kind::Literal => {
            if position + literal.len() > matcher.source.len()
                || matcher.source[position..position + literal.len()] != *literal
            {
                note(matcher, position);
                return None;
            }
            Some(MatchResult {
                end: position + literal.len(),
                children: Vec::new(),
            })
        }
        Kind::Any => {
            if position >= matcher.source.len() {
                note(matcher, position);
                return None;
            }
            Some(MatchResult {
                end: position + 1,
                children: Vec::new(),
            })
        }
        Kind::Eof => {
            if position != matcher.source.len() {
                note(matcher, position);
                return None;
            }
            Some(MatchResult {
                end: position,
                children: Vec::new(),
            })
        }
        Kind::Charset => {
            let byte = matcher.source.get(position).copied();
            let Some(byte) = byte else {
                note(matcher, position);
                return None;
            };
            if ranges
                .iter()
                .any(|range| byte >= range.lo && byte <= range.hi)
            {
                Some(MatchResult {
                    end: position + 1,
                    children: Vec::new(),
                })
            } else {
                note(matcher, position);
                None
            }
        }
        Kind::Seq => {
            let mut at = position;
            let mut matched = Vec::new();
            for child in children.iter().copied() {
                let result = match_pattern(matcher, child, at)?;
                matched.extend(result.children);
                at = result.end;
            }
            Some(MatchResult {
                end: at,
                children: matched,
            })
        }
        Kind::Choice => {
            for child in children.iter().copied() {
                if let Some(result) = match_pattern(matcher, child, position) {
                    return Some(result);
                }
            }
            None
        }
        Kind::Star | Kind::Plus => {
            let child = children
                .first()
                .copied()
                .unwrap_or_else(|| abort("repetition has no child"));
            let mut at = position;
            let mut count = 0;
            let mut matched = Vec::new();
            while let Some(result) = match_pattern(matcher, child, at) {
                matched.extend(result.children);
                if result.end == at {
                    break;
                }
                at = result.end;
                count += 1;
            }
            if kind == Kind::Plus && count == 0 {
                return None;
            }
            Some(MatchResult {
                end: at,
                children: matched,
            })
        }
        Kind::Opt => {
            let child = children
                .first()
                .copied()
                .unwrap_or_else(|| abort("optional has no child"));
            match match_pattern(matcher, child, position) {
                Some(result) => Some(result),
                None => Some(MatchResult {
                    end: position,
                    children: Vec::new(),
                }),
            }
        }
        Kind::And => {
            let child = children
                .first()
                .copied()
                .unwrap_or_else(|| abort("lookahead has no child"));
            match_pattern(matcher, child, position).map(|_| MatchResult {
                end: position,
                children: Vec::new(),
            })
        }
        Kind::Not => {
            let child = children
                .first()
                .copied()
                .unwrap_or_else(|| abort("negation has no child"));
            match match_pattern(matcher, child, position) {
                Some(_) => None,
                None => Some(MatchResult {
                    end: position,
                    children: Vec::new(),
                }),
            }
        }
        Kind::RuleRef => {
            let rule_pattern = named_rule(&matcher.rules, rule_name).unwrap_or_else(|| {
                matcher.undefined_rule = rule_name;
                ptr::null_mut()
            });
            if rule_pattern.is_null() {
                return None;
            }
            let key = (rule_pattern as usize, position);
            if let Some(entry) = matcher.memo.get(&key) {
                if entry.in_progress {
                    matcher.left_recursive_rule = rule_name;
                    return None;
                }
                return if entry.ok {
                    Some(MatchResult {
                        end: entry.end,
                        children: entry.children.clone(),
                    })
                } else {
                    None
                };
            }
            matcher.memo.insert(
                key,
                MemoEntry {
                    ok: false,
                    end: position,
                    children: Vec::new(),
                    in_progress: true,
                },
            );
            if let Some(inner) = match_pattern(matcher, rule_pattern, position) {
                let node = matcher.nodes.len();
                matcher.nodes.push(TempNode {
                    rule: rule_name,
                    start: position,
                    end: inner.end,
                    children: inner.children,
                });
                let wrapped = vec![node];
                matcher.memo.insert(
                    key,
                    MemoEntry {
                        ok: true,
                        end: inner.end,
                        children: wrapped.clone(),
                        in_progress: false,
                    },
                );
                Some(MatchResult {
                    end: inner.end,
                    children: wrapped,
                })
            } else {
                matcher.memo.insert(
                    key,
                    MemoEntry {
                        ok: false,
                        end: position,
                        children: Vec::new(),
                        in_progress: false,
                    },
                );
                None
            }
        }
    }
}

unsafe fn peg_error(kind: &[u8], message: &[u8]) -> Oop {
    let api = unsafe { api(runtime()) };
    let error_facade = unsafe { import(api, b"Error\0") };
    if error_facade.is_null() {
        return api.nilObj;
    }
    let kind_symbol = unsafe { intern(api, kind) };
    let message_string = unsafe { (api.mkString)(message.as_ptr(), message.len()) };
    let selector = unsafe { intern(api, b"kind:message:\0") };
    let values = [kind_symbol, message_string];
    // SAFETY: values is a live, contiguous argument buffer for this send.
    unsafe { (api.send)(error_facade, selector, values.as_ptr(), values.len()) }
}

fn syntax_position(source: &[u8], offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut column = 1;
    for byte in source.iter().take(offset.min(source.len())) {
        if *byte == b'\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}

unsafe fn temp_to_cst(matcher: &Matcher, node_index: usize) -> Oop {
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    let node = matcher
        .nodes
        .get(node_index)
        .unwrap_or_else(|| abort("CST node index was invalid"));
    let rule = node.rule;
    let start = node.start;
    let end = node.end;
    let child_indices = node.children.clone();
    let object = unsafe { (api.alloc)(as_oop(runtime.cst_vt), size_of::<CstState>()) };
    if object.is_null() {
        abort("could not allocate CST object");
    }
    // Initialize before publishing the object to the scratch root provider; a
    // child allocation may trigger GC immediately after this point.
    unsafe {
        ptr::write(
            object.cast::<CstState>(),
            CstState {
                rule,
                start,
                end,
                children: Vec::new(),
            },
        );
    }
    scratch_push(object);
    unsafe { (api.setFinalizer)(object, finalize_cst) };
    let mut children = Vec::with_capacity(child_indices.len());
    for child in child_indices {
        children.push(unsafe { temp_to_cst(matcher, child) });
    }
    // SAFETY: the allocation has exactly the requested CST payload size.
    unsafe {
        (*object.cast::<CstState>()).children = children;
    }
    object
}

unsafe fn parse_grammar(grammar: Oop, source_object: Oop, root_object: Oop) -> Oop {
    let api = unsafe { api(runtime()) };
    let source = unsafe { string_bytes(api, source_object) };
    let root_name = unsafe { rule_symbol(api, root_object) };
    let rules = {
        let state = unsafe { grammar_ref(grammar) };
        index_rules(state.rules.iter().map(|rule| (rule.name, rule.pattern)))
    };
    let mut matcher = Matcher {
        source,
        rules,
        patterns: HashMap::new(),
        nodes: Vec::new(),
        memo: HashMap::new(),
        furthest: 0,
        undefined_rule: ptr::null_mut(),
        left_recursive_rule: ptr::null_mut(),
    };
    let Some(root_pattern) = named_rule(&matcher.rules, root_name) else {
        matcher.undefined_rule = root_name;
        unsafe { (*grammar.cast::<GrammarState>()).furthest_fail = 0 };
        return unsafe { peg_error(b"undefinedRule\0", b"undefined rule") };
    };
    let result = match_pattern(&mut matcher, root_pattern, 0);
    if let Some(result) = result {
        unsafe { (*grammar.cast::<GrammarState>()).furthest_fail = matcher.furthest };
        let root_index = matcher.nodes.len();
        matcher.nodes.push(TempNode {
            rule: root_name,
            start: 0,
            end: result.end,
            children: result.children,
        });
        let cst = unsafe { temp_to_cst(&matcher, root_index) };
        scratch_clear();
        return cst;
    }
    unsafe { (*grammar.cast::<GrammarState>()).furthest_fail = matcher.furthest };
    if !matcher.undefined_rule.is_null() {
        let name = unsafe { (api.symbolName)(matcher.undefined_rule) };
        let mut message = b"undefined rule: ".to_vec();
        message.extend_from_slice(unsafe { std::ffi::CStr::from_ptr(name.cast()) }.to_bytes());
        return unsafe { peg_error(b"undefinedRule\0", &message) };
    }
    if !matcher.left_recursive_rule.is_null() {
        let name = unsafe { (api.symbolName)(matcher.left_recursive_rule) };
        let mut message = b"left-recursive rule: ".to_vec();
        message.extend_from_slice(unsafe { std::ffi::CStr::from_ptr(name.cast()) }.to_bytes());
        return unsafe { peg_error(b"leftRecursion\0", &message) };
    }
    let (line, column) = syntax_position(&matcher.source, matcher.furthest);
    let message = format!("syntax error at line {line}, col {column}");
    unsafe { peg_error(b"syntaxError\0", message.as_bytes()) }
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

unsafe extern "C" fn peg_grammar(_self_object: Oop, pointer: *const Oop, count: usize) -> Oop {
    if !unsafe { args(pointer, count) }.is_empty() {
        abort("grammar expects no arguments");
    }
    unsafe { new_grammar() }
}

unsafe fn vector_children(vector: Oop) -> Vec<Oop> {
    let api = unsafe { api(runtime()) };
    let size_selector = unsafe { intern(api, b"size\0") };
    let at_selector = unsafe { intern(api, b"at:\0") };
    let empty: [Oop; 0] = [];
    let size = unsafe { (api.send)(vector, size_selector, empty.as_ptr(), 0) };
    let count = unsafe { (api.smallIntValue)(size) };
    if count < 0 {
        abort("Vector size was negative");
    }
    let mut children = Vec::with_capacity(count as usize);
    for index in 0..count as usize {
        let argument = [unsafe { (api.mkSmallInt)(index as i64) }];
        children.push(unsafe { (api.send)(vector, at_selector, argument.as_ptr(), 1) });
    }
    children
}

unsafe extern "C" fn peg_seq_of(_self_object: Oop, pointer: *const Oop, count: usize) -> Oop {
    let values = unsafe { args(pointer, count) };
    if values.len() != 1 {
        abort("seqOf: expects one argument");
    }
    let object = unsafe { new_pattern(Kind::Seq) };
    unsafe { state_mut(object).children = vector_children(values[0]) };
    object
}

unsafe extern "C" fn peg_choice_of(_self_object: Oop, pointer: *const Oop, count: usize) -> Oop {
    let values = unsafe { args(pointer, count) };
    if values.len() != 1 {
        abort("choiceOf: expects one argument");
    }
    let object = unsafe { new_pattern(Kind::Choice) };
    unsafe { state_mut(object).children = vector_children(values[0]) };
    object
}

unsafe extern "C" fn peg_star_of(_self_object: Oop, pointer: *const Oop, count: usize) -> Oop {
    let values = unsafe { args(pointer, count) };
    if values.len() != 1 {
        abort("star: expects one argument");
    }
    unsafe { peg_unary(Kind::Star, values[0]) }
}

unsafe extern "C" fn peg_plus_of(_self_object: Oop, pointer: *const Oop, count: usize) -> Oop {
    let values = unsafe { args(pointer, count) };
    if values.len() != 1 {
        abort("plus: expects one argument");
    }
    unsafe { peg_unary(Kind::Plus, values[0]) }
}

unsafe extern "C" fn peg_opt_of(_self_object: Oop, pointer: *const Oop, count: usize) -> Oop {
    let values = unsafe { args(pointer, count) };
    if values.len() != 1 {
        abort("opt: expects one argument");
    }
    unsafe { peg_unary(Kind::Opt, values[0]) }
}

unsafe extern "C" fn peg_not_of(_self_object: Oop, pointer: *const Oop, count: usize) -> Oop {
    let values = unsafe { args(pointer, count) };
    if values.len() != 1 {
        abort("not: expects one argument");
    }
    unsafe { peg_unary(Kind::Not, values[0]) }
}

unsafe extern "C" fn peg_and_of(_self_object: Oop, pointer: *const Oop, count: usize) -> Oop {
    let values = unsafe { args(pointer, count) };
    if values.len() != 1 {
        abort("and: expects one argument");
    }
    unsafe { peg_unary(Kind::And, values[0]) }
}

unsafe extern "C" fn peg_any_of(_self_object: Oop, pointer: *const Oop, count: usize) -> Oop {
    let values = unsafe { args(pointer, count) };
    if values.len() != 1 {
        abort("anyOf: expects one argument");
    }
    let api = unsafe { api(runtime()) };
    let bytes = unsafe { string_bytes(api, values[0]) };
    let object = unsafe { new_pattern(Kind::Charset) };
    let state = unsafe { state_mut(object) };
    state
        .ranges
        .extend(bytes.into_iter().map(|byte| Range { lo: byte, hi: byte }));
    object
}

unsafe fn byte_argument(api: &IdApi, object: Oop) -> u8 {
    if unsafe { (api.vtableOf)(object) } == api.string_vt {
        return unsafe { string_bytes(api, object) }
            .first()
            .copied()
            .unwrap_or(0);
    }
    unsafe { (api.smallIntValue)(object) as u8 }
}

unsafe extern "C" fn peg_range_to(_self_object: Oop, pointer: *const Oop, count: usize) -> Oop {
    let values = unsafe { args(pointer, count) };
    if values.len() != 2 {
        abort("range:to: expects two arguments");
    }
    let api = unsafe { api(runtime()) };
    let lo = unsafe { byte_argument(api, values[0]) };
    let hi = unsafe { byte_argument(api, values[1]) };
    let object = unsafe { new_pattern(Kind::Charset) };
    unsafe { state_mut(object).ranges.push(Range { lo, hi }) };
    object
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

unsafe extern "C" fn grammar_rule_is(self_object: Oop, pointer: *const Oop, count: usize) -> Oop {
    let values = unsafe { args(pointer, count) };
    if values.len() != 2 {
        abort("rule:is: expects two arguments");
    }
    let api = unsafe { api(runtime()) };
    let name = unsafe { rule_symbol(api, values[0]) };
    let state = unsafe { &mut *(self_object.cast::<GrammarState>()) };
    if let Some(rule) = state.rules.iter_mut().find(|rule| rule.name == name) {
        rule.pattern = values[1];
    } else {
        state.rules.push(Rule {
            name,
            pattern: values[1],
        });
    }
    self_object
}

unsafe extern "C" fn grammar_furthest_fail(
    self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    if !unsafe { args(pointer, count) }.is_empty() {
        abort("furthestFail expects no arguments");
    }
    let api = unsafe { api(runtime()) };
    let state = unsafe { &*(self_object.cast::<GrammarState>()) };
    unsafe { (api.mkSmallInt)(state.furthest_fail as i64) }
}

unsafe extern "C" fn grammar_parse_from(
    self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { args(pointer, count) };
    if values.len() != 2 {
        abort("parse:from: expects two arguments");
    }
    unsafe { parse_grammar(self_object, values[0], values[1]) }
}

unsafe extern "C" fn cst_rule(self_object: Oop, pointer: *const Oop, count: usize) -> Oop {
    if !unsafe { args(pointer, count) }.is_empty() {
        abort("rule expects no arguments");
    }
    unsafe { cst_ref(self_object).rule }
}

unsafe extern "C" fn cst_rule_name(self_object: Oop, pointer: *const Oop, count: usize) -> Oop {
    if !unsafe { args(pointer, count) }.is_empty() {
        abort("ruleName expects no arguments");
    }
    let api = unsafe { api(runtime()) };
    let rule = unsafe { cst_ref(self_object).rule };
    let name = unsafe { (api.symbolName)(rule) };
    let bytes = unsafe { std::ffi::CStr::from_ptr(name.cast()) }
        .to_bytes()
        .to_vec();
    unsafe { (api.mkString)(bytes.as_ptr(), bytes.len()) }
}

unsafe extern "C" fn cst_start(self_object: Oop, pointer: *const Oop, count: usize) -> Oop {
    if !unsafe { args(pointer, count) }.is_empty() {
        abort("start expects no arguments");
    }
    let api = unsafe { api(runtime()) };
    let start = unsafe { cst_ref(self_object).start };
    unsafe { (api.mkSmallInt)(start as i64) }
}

unsafe extern "C" fn cst_end(self_object: Oop, pointer: *const Oop, count: usize) -> Oop {
    if !unsafe { args(pointer, count) }.is_empty() {
        abort("end expects no arguments");
    }
    let api = unsafe { api(runtime()) };
    let end = unsafe { cst_ref(self_object).end };
    unsafe { (api.mkSmallInt)(end as i64) }
}

unsafe extern "C" fn cst_child_count(self_object: Oop, pointer: *const Oop, count: usize) -> Oop {
    if !unsafe { args(pointer, count) }.is_empty() {
        abort("childCount expects no arguments");
    }
    let api = unsafe { api(runtime()) };
    let count = unsafe { cst_ref(self_object).children.len() };
    unsafe { (api.mkSmallInt)(count as i64) }
}

unsafe extern "C" fn cst_child_at(self_object: Oop, pointer: *const Oop, count: usize) -> Oop {
    let values = unsafe { args(pointer, count) };
    if values.len() != 1 {
        abort("childAt: expects one argument");
    }
    let api = unsafe { api(runtime()) };
    let index = unsafe { (api.smallIntValue)(values[0]) };
    if index < 0 {
        return api.nilObj;
    }
    unsafe {
        cst_ref(self_object)
            .children
            .get(index as usize)
            .copied()
            .unwrap_or(api.nilObj)
    }
}

unsafe extern "C" fn cst_children(self_object: Oop, pointer: *const Oop, count: usize) -> Oop {
    if !unsafe { args(pointer, count) }.is_empty() {
        abort("children expects no arguments");
    }
    let api = unsafe { api(runtime()) };
    let vector = unsafe { import(api, b"Vector\0") };
    if vector.is_null() {
        return api.nilObj;
    }
    let new_selector = unsafe { intern(api, b"new\0") };
    let add_selector = unsafe { intern(api, b"add:\0") };
    let empty: [Oop; 0] = [];
    let mut result = unsafe { (api.send)(vector, new_selector, empty.as_ptr(), 0) };
    let children = unsafe { cst_ref(self_object).children.clone() };
    for child in children {
        let values = [child];
        result = unsafe { (api.send)(result, add_selector, values.as_ptr(), values.len()) };
    }
    result
}

unsafe fn register_methods(runtime: &Runtime) {
    let api = unsafe { api(runtime) };
    let peg_vt = as_oop(runtime.peg_vt);
    let pattern_vt = as_oop(runtime.pattern_vt);
    let grammar_vt = as_oop(runtime.grammar_vt);
    let cst_vt = as_oop(runtime.cst_vt);
    let peg_methods: &[(&[u8], birnam_abi::Method)] = &[
        (b"literal:\0", peg_literal),
        (b"any\0", peg_any),
        (b"eof\0", peg_eof),
        (b"charset\0", peg_charset),
        (b"ruleRef:\0", peg_rule_ref),
        (b"seq\0", peg_seq),
        (b"choice\0", peg_choice),
        (b"grammar\0", peg_grammar),
        (b"newGrammar\0", peg_grammar),
        (b"anyByte\0", peg_any),
        (b"seqOf:\0", peg_seq_of),
        (b"choiceOf:\0", peg_choice_of),
        (b"star:\0", peg_star_of),
        (b"plus:\0", peg_plus_of),
        (b"opt:\0", peg_opt_of),
        (b"not:\0", peg_not_of),
        (b"and:\0", peg_and_of),
        (b"anyOf:\0", peg_any_of),
        (b"range:to:\0", peg_range_to),
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
    let grammar_methods: &[(&[u8], birnam_abi::Method)] = &[
        (b"rule:is:\0", grammar_rule_is),
        (b"furthestFail\0", grammar_furthest_fail),
        (b"parse:from:\0", grammar_parse_from),
    ];
    for (selector, method) in grammar_methods {
        let selector = unsafe { intern(api, selector) };
        unsafe { (api.method)(grammar_vt, selector, *method) };
    }
    let cst_methods: &[(&[u8], birnam_abi::Method)] = &[
        (b"rule\0", cst_rule),
        (b"ruleName\0", cst_rule_name),
        (b"start\0", cst_start),
        (b"end\0", cst_end),
        (b"childCount\0", cst_child_count),
        (b"childAt:\0", cst_child_at),
        (b"children\0", cst_children),
    ];
    for (selector, method) in cst_methods {
        let selector = unsafe { intern(api, selector) };
        unsafe { (api.method)(cst_vt, selector, *method) };
    }
    unsafe {
        (api.setLayout)(pattern_vt, ptr::null(), 0, birnam_abi::SLOT_RAW);
        (api.setTraceHook)(pattern_vt, trace_pattern);
        (api.setLayout)(grammar_vt, ptr::null(), 0, birnam_abi::SLOT_RAW);
        (api.setTraceHook)(grammar_vt, trace_grammar);
        (api.setLayout)(cst_vt, ptr::null(), 0, birnam_abi::SLOT_RAW);
        (api.setTraceHook)(cst_vt, trace_cst);
    }
}

unsafe fn bring_up(api_pointer: *mut IdApi) {
    if api_pointer.is_null() {
        abort("module_init received a null IdApi");
    }
    let api = unsafe { &*api_pointer };
    let runtime = *RUNTIME.get_or_init(|| {
        let reloading = unsafe { (api.isExported)(PEG_NAME.as_ptr().cast()) != 0 };
        let (pattern_vt, peg_vt, peg, grammar_vt, cst_vt) = if reloading {
            (
                unsafe { import(api, PATTERN_VT_NAME) },
                unsafe { import(api, PEG_VT_NAME) },
                unsafe { import(api, PEG_NAME) },
                unsafe { import(api, GRAMMAR_VT_NAME) },
                unsafe { import(api, CST_VT_NAME) },
            )
        } else {
            let pattern_vt = unsafe { (api.proto)(api.object_vt) };
            let peg_vt = unsafe { (api.proto)(api.object_vt) };
            let grammar_vt = unsafe { (api.proto)(api.object_vt) };
            let cst_vt = unsafe { (api.proto)(api.object_vt) };
            let peg = unsafe { (api.alloc)(peg_vt, 0) };
            (pattern_vt, peg_vt, peg, grammar_vt, cst_vt)
        };
        if [pattern_vt, peg_vt, peg, grammar_vt, cst_vt]
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
            cst_vt: oop_address(cst_vt),
        }
    });
    unsafe { register_methods(&runtime) };
    if unsafe { (api.isExported)(PEG_NAME.as_ptr().cast()) == 0 } {
        unsafe { (api.gcRegisterRootProvider)(cst_scratch_roots) };
    }
    if unsafe { (api.isExported)(PEG_NAME.as_ptr().cast()) == 0 } {
        unsafe {
            let _ = (api.exportObj)(PATTERN_VT_NAME.as_ptr().cast(), as_oop(runtime.pattern_vt));
            let _ = (api.exportObj)(PEG_VT_NAME.as_ptr().cast(), as_oop(runtime.peg_vt));
            let _ = (api.exportObj)(GRAMMAR_VT_NAME.as_ptr().cast(), as_oop(runtime.grammar_vt));
            let _ = (api.exportObj)(CST_VT_NAME.as_ptr().cast(), as_oop(runtime.cst_vt));
            let _ = (api.exportObj)(PEG_NAME.as_ptr().cast(), as_oop(runtime.peg));
        }
    }
}

/// Identifies this explicit Rust PEG matcher dylib.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_peg_rust_identity() -> *const c_char {
    RUST_PEG_IDENTITY.as_ptr().cast()
}

/// Declare that this module treats IdApi Symbol-name pointers as borrowed.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> core::ffi::c_int {
    1
}

/// Initialize the matcher and transient CST against the process-owned IdApi.
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
mod matching_tests;

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
