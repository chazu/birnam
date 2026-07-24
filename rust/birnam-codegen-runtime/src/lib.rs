//! Explicit Rust Codegen boundary for the first N6.3 runtime slice.
//!
//! This dylib is intentionally exported under `RustCodegen`; it cannot replace
//! the frozen `Codegen` authority until the complete N6.3–N6.6 matrix passes.
//! The current method materializes the Rust PEG CST, lowers it through the pure
//! Codegen core, and returns deterministic generated C without invoking TCC.

use birnam_abi::{IdApi, MarkFn, Method, Oop, borrowed_oop_args};
use birnam_artifact::inspect as inspect_artifact;
use birnam_codegen::{CstNode, Emitter, OwnedCst, TranslationUnit, lower_program};
use core::ffi::{c_char, c_void};
use core::ptr;
use std::ffi::{CStr, OsStr};
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

const IDENTITY: &[u8] = b"rust-codegen-n6.3-emitter\0";
const RUST_CODEGEN_NAME: &[u8] = b"RustCodegen\0";
const RUST_CODEGEN_VT_NAME: &[u8] = b"RustCodegenVt\0";
const LANG_NAME: &[u8] = b"Lang\0";
const TCC_NAME: &[u8] = b"Tcc\0";
const GLOBALS_NAME: &[u8] = b"Globals\0";
const VECTOR_NAME: &[u8] = b"Vector\0";
const VECTOR_VT_NAME: &[u8] = b"VectorVt\0";
const PEG_CST_VT_NAME: &[u8] = b"PegCstVt\0";
const PARSE_SELECTOR: &[u8] = b"parse:\0";
const FURTHEST_FAIL_SELECTOR: &[u8] = b"furthestFail\0";
const RULE_NAME_SELECTOR: &[u8] = b"ruleName\0";
const START_SELECTOR: &[u8] = b"start\0";
const END_SELECTOR: &[u8] = b"end\0";
const CHILD_COUNT_SELECTOR: &[u8] = b"childCount\0";
const CHILD_AT_SELECTOR: &[u8] = b"childAt:\0";
const EMIT_SELECTOR: &[u8] = b"emit:\0";
const EVAL_SELECTOR: &[u8] = b"eval:\0";
const EVAL_PERSISTING_SELECTOR: &[u8] = b"evalPersisting:\0";
const EVAL_FROM_SELECTOR: &[u8] = b"eval:from:\0";
const EVAL_PERSISTING_FROM_SELECTOR: &[u8] = b"evalPersisting:from:\0";
const NEW_SELECTOR: &[u8] = b"new\0";
const COMPILE_SELECTOR: &[u8] = b"compile:\0";
const COMPILE_OUTPUT_SELECTOR: &[u8] = b"compile:output:\0";
const COMPILE_OUTPUT_FROM_SELECTOR: &[u8] = b"compile:output:from:\0";
const COMPILE_OUTPUT_ROOT_SELECTOR: &[u8] = b"compile:output:root:\0";
const EMBEDDED_FILES_SELECTOR: &[u8] = b"embeddedFilesOf:\0";
const MODULE_METADATA_SELECTOR: &[u8] = b"moduleMetadataOf:\0";
const SOURCE_REVISION_SELECTOR: &[u8] = b"sourceRevisionOf:\0";
const RECORD_MODULE_SOURCE_SELECTOR: &[u8] = b"recordModuleSource:named:\0";
const MODULE_SOURCE_FOR_SELECTOR: &[u8] = b"moduleSourceFor:\0";
const SEAL_INITIAL_GENERATION_SELECTOR: &[u8] = b"sealInitialGeneration\0";
const INITIAL_GENERATION_PROJECTION_SELECTOR: &[u8] = b"initialGenerationProjection\0";
const SUCCESSOR_GENERATION_PROJECTION_SELECTOR: &[u8] =
    b"successorGenerationProjection:replacing:with:unit:\0";
const LOAD_SELECTOR: &[u8] = b"load:\0";
const TCC_COMPILE_OUTPUT_SELECTOR: &[u8] = b"compile:output:as:\0";
const BIND_RUNTIME_SELECTOR: &[u8] = b"bindRuntime\0";
const RELOCATE_SELECTOR: &[u8] = b"relocate\0";
const DISPOSABLE_SELECTOR: &[u8] = b"disposable\0";
const SYMBOL_SELECTOR: &[u8] = b"symbol:\0";
const ADD_SYMBOL_SELECTOR: &[u8] = b"addSymbol:value:\0";
const GLOBAL_AT_SELECTOR: &[u8] = b"at:\0";
const GLOBAL_AT_PUT_SELECTOR: &[u8] = b"at:put:\0";
const INSTALL_SELECTOR: &[u8] = b"install:as:on:\0";
const ENTRY_SELECTOR: &[u8] = b"__ev\0";
const MAX_CST_DEPTH: usize = 4096;
const ARTIFACT_ABI: &[u8] = b"birnam-artifact-generation-v1";
const ARTIFACT_TARGET: &[u8] = b"aarch64-macos";
const ARTIFACT_DESCRIPTOR_MAGIC: &[u8] = b"BIRNAM-ARTIFACT-DESCRIPTOR-V1";
const MAX_EMBEDDED_FILE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy)]
struct Runtime {
    api: usize,
    rust_codegen_vt: usize,
    rust_codegen: usize,
    tcc: usize,
    globals: usize,
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();
static LAST_ERROR: Mutex<Option<Vec<u8>>> = Mutex::new(None);
static LIVE_UNITS: Mutex<Vec<usize>> = Mutex::new(Vec::new());
static MODULE_SOURCES: Mutex<Vec<(usize, usize)>> = Mutex::new(Vec::new());
static INITIAL_GENERATION_SEALED: Mutex<bool> = Mutex::new(false);
static LIVE_ARTIFACT_HANDLES: Mutex<Vec<usize>> = Mutex::new(Vec::new());
static ROOT_REGISTERED: OnceLock<()> = OnceLock::new();

fn abort(message: &str) -> ! {
    eprintln!("birnam Rust Codegen: {message}");
    std::process::abort();
}

fn runtime() -> &'static Runtime {
    RUNTIME
        .get()
        .unwrap_or_else(|| abort("module_init was not called"))
}

fn clear_error() {
    if let Ok(mut error) = LAST_ERROR.lock() {
        *error = None;
    }
}

fn note_error(message: &[u8]) {
    if let Ok(mut error) = LAST_ERROR.lock()
        && error.is_none()
    {
        *error = Some(message.to_vec());
    }
}

unsafe fn note_syntax_error(api: &IdApi, lang: Oop) {
    let offset = unsafe { send0(api, lang, FURTHEST_FAIL_SELECTOR) };
    if unsafe { (api.isSmallInt)(offset) } == 0 {
        note_error(b"RustCodegen syntax error");
        return;
    }
    let mut message = b"RustCodegen syntax error at byte ".to_vec();
    message.extend_from_slice(
        unsafe { (api.smallIntValue)(offset) }
            .to_string()
            .as_bytes(),
    );
    note_error(&message);
}

unsafe extern "C" fn mark_live_units(mark: MarkFn) {
    if let Ok(units) = LIVE_UNITS.lock() {
        for &unit in units.iter() {
            // SAFETY: each address was returned by IdApi.alloc and remains rooted
            // by this provider until process teardown.
            unsafe { mark(unit as Oop) };
        }
    }
    let Ok(sources) = MODULE_SOURCES.lock() else {
        return;
    };
    for &(name, source) in sources.iter() {
        // SAFETY: each address was returned by IdApi.alloc and remains rooted
        // by this provider until process teardown.
        unsafe {
            mark(name as Oop);
            mark(source as Oop);
        }
    }
}

#[unsafe(no_mangle)]
/// Resolve a generated global name through the live `Globals` object.
///
/// # Safety
///
/// `name` must be a readable NUL-terminated C string for the duration of the
/// call, as required by the generated-C shim ABI.
pub unsafe extern "C" fn birnam_global(name: *const c_char) -> Oop {
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    let globals = runtime.globals as Oop;
    if globals.is_null() || name.is_null() {
        return api.nilObj;
    }
    let key = unsafe { (api.intern)(name) };
    unsafe { send(api, globals, GLOBAL_AT_SELECTOR, &[key]) }
}

#[unsafe(no_mangle)]
/// Store a generated top-level `let` through the live `Globals` singleton.
///
/// # Safety
///
/// `name` must be a readable NUL-terminated C string for the duration of the
/// call, and `value` must be a live Oop owned by the current generated call.
pub unsafe extern "C" fn birnam_global_set(name: *const c_char, value: Oop) -> Oop {
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    let globals = runtime.globals as Oop;
    if globals.is_null() || name.is_null() {
        return api.nilObj;
    }
    let key = unsafe { (api.intern)(name) };
    unsafe { send(api, globals, GLOBAL_AT_PUT_SELECTOR, &[key, value]) }
}

unsafe fn bind_global_shims(api: &IdApi, unit: Oop) -> bool {
    let name = unsafe { (api.mkString)(b"birnam_global".as_ptr(), 13) };
    let address = unsafe { (api.mkSmallInt)(birnam_global as *const () as usize as i64) };
    if unsafe { send(api, unit, ADD_SYMBOL_SELECTOR, &[name, address]) } == api.nilObj {
        return false;
    }
    let name = unsafe { (api.mkString)(b"birnam_global_set".as_ptr(), 17) };
    let address = unsafe { (api.mkSmallInt)(birnam_global_set as *const () as usize as i64) };
    unsafe { send(api, unit, ADD_SYMBOL_SELECTOR, &[name, address]) != api.nilObj }
}

fn sha256_hex(bytes: &[u8]) -> [u8; 64] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut state = [
        0x6a09e667u32,
        0xbb67ae85,
        0x3c6ef372,
        0xa54ff53a,
        0x510e527f,
        0x9b05688c,
        0x1f83d9ab,
        0x5be0cd19,
    ];
    let block_count = (bytes.len() + 9).div_ceil(64);
    let mut padded = Vec::with_capacity(block_count * 64);
    padded.extend_from_slice(bytes);
    padded.push(0x80);
    padded.resize(block_count * 64 - 8, 0);
    padded.extend_from_slice(&((bytes.len() as u64) * 8).to_be_bytes());
    for chunk in padded.chunks_exact(64) {
        let mut schedule = [0u32; 64];
        for (index, word) in schedule[..16].iter_mut().enumerate() {
            *word = u32::from_be_bytes([
                chunk[index * 4],
                chunk[index * 4 + 1],
                chunk[index * 4 + 2],
                chunk[index * 4 + 3],
            ]);
        }
        for index in 16..64 {
            let s0 = schedule[index - 15].rotate_right(7)
                ^ schedule[index - 15].rotate_right(18)
                ^ (schedule[index - 15] >> 3);
            let s1 = schedule[index - 2].rotate_right(17)
                ^ schedule[index - 2].rotate_right(19)
                ^ (schedule[index - 2] >> 10);
            schedule[index] = schedule[index - 16]
                .wrapping_add(s0)
                .wrapping_add(schedule[index - 7])
                .wrapping_add(s1);
        }
        let mut working = state;
        for index in 0..64 {
            let s1 = working[4].rotate_right(6)
                ^ working[4].rotate_right(11)
                ^ working[4].rotate_right(25);
            let ch = (working[4] & working[5]) ^ ((!working[4]) & working[6]);
            let temp1 = working[7]
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[index])
                .wrapping_add(schedule[index]);
            let s0 = working[0].rotate_right(2)
                ^ working[0].rotate_right(13)
                ^ working[0].rotate_right(22);
            let maj =
                (working[0] & working[1]) ^ (working[0] & working[2]) ^ (working[1] & working[2]);
            let temp2 = s0.wrapping_add(maj);
            working.copy_within(0..7, 1);
            working[0] = temp1.wrapping_add(temp2);
            working[4] = working[4].wrapping_add(temp1);
        }
        for (value, update) in state.iter_mut().zip(working) {
            *value = value.wrapping_add(update);
        }
    }
    let mut output = [0u8; 64];
    for (index, byte) in state.iter().flat_map(|word| word.to_be_bytes()).enumerate() {
        output[index * 2] = b"0123456789abcdef"[(byte >> 4) as usize];
        output[index * 2 + 1] = b"0123456789abcdef"[(byte & 0x0f) as usize];
    }
    output
}

fn descriptor_field(payload: &mut Vec<u8>, value: &[u8]) {
    payload.extend_from_slice(&(value.len() as u32).to_le_bytes());
    payload.extend_from_slice(value);
}

fn descriptor_list(payload: &mut Vec<u8>, values: &[&[u8]]) {
    payload.extend_from_slice(&(values.len() as u32).to_le_bytes());
    for value in values {
        descriptor_field(payload, value);
    }
}

fn descriptor_owned_list(payload: &mut Vec<u8>, values: &[Vec<u8>]) {
    payload.extend_from_slice(&(values.len() as u32).to_le_bytes());
    for value in values {
        descriptor_field(payload, value);
    }
}

fn artifact_identity(path: &[u8]) -> Vec<u8> {
    let mut name = path.rsplit(|byte| *byte == b'/').next().unwrap_or(path);
    if let Some(dot) = name.iter().rposition(|byte| *byte == b'.') {
        name = &name[..dot];
    }
    if let Some(rest) = name.strip_prefix(b"libbirnam_") {
        name = rest;
    } else if let Some(rest) = name.strip_prefix(b"lib") {
        name = rest;
    }
    name.to_vec()
}

fn append_c_string(output: &mut Vec<u8>, bytes: &[u8]) {
    for byte in bytes {
        match byte {
            b'"' => output.extend_from_slice(br#"\""#),
            b'\\' => output.extend_from_slice(br#"\\"#),
            0x20..=0x7e => output.push(*byte),
            other => {
                output.extend_from_slice(&[
                    b'\\',
                    b'0' + ((other >> 6) & 7),
                    b'0' + ((other >> 3) & 7),
                    b'0' + (other & 7),
                ]);
            }
        }
    }
}

fn artifact_source(
    source: &[u8],
    identity: &[u8],
    source_digest: &[u8; 64],
    revision_digest: &[u8; 64],
    requires: &[Vec<u8>],
    native_requires: &[Vec<u8>],
    generated: &[u8],
) -> Vec<u8> {
    let mut payload = Vec::new();
    descriptor_field(&mut payload, b"module");
    descriptor_field(&mut payload, identity);
    descriptor_field(&mut payload, ARTIFACT_ABI);
    descriptor_field(&mut payload, ARTIFACT_TARGET);
    descriptor_field(&mut payload, source_digest);
    descriptor_field(&mut payload, revision_digest);
    descriptor_owned_list(&mut payload, requires);
    descriptor_owned_list(&mut payload, native_requires);
    descriptor_list(&mut payload, &[b"activate", b"source", b"reflection"]);

    let mut descriptor = Vec::with_capacity(ARTIFACT_DESCRIPTOR_MAGIC.len() + 4 + payload.len());
    descriptor.extend_from_slice(ARTIFACT_DESCRIPTOR_MAGIC);
    descriptor.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    descriptor.extend_from_slice(&payload);

    let mut output = Vec::new();
    output.extend_from_slice(b"const char __birnam_artifact_abi[] = \"");
    output.extend_from_slice(ARTIFACT_ABI);
    output.extend_from_slice(b"\";\nconst char __birnam_artifact_identity[] = \"");
    append_c_string(&mut output, identity);
    output.extend_from_slice(b"\";\nconst unsigned char __birnam_artifact_source[] = {");
    for (index, byte) in source.iter().enumerate() {
        if index != 0 {
            output.push(b',');
        }
        output.extend_from_slice(byte.to_string().as_bytes());
    }
    if source.is_empty() {
        output.push(b'0');
    }
    output.extend_from_slice(b"};\nconst unsigned long __birnam_artifact_source_len = ");
    output.extend_from_slice(source.len().to_string().as_bytes());
    output.extend_from_slice(b"UL;\nconst unsigned char __birnam_artifact_descriptor[] = {");
    for (index, byte) in descriptor.iter().enumerate() {
        if index != 0 {
            output.push(b',');
        }
        output.extend_from_slice(byte.to_string().as_bytes());
    }
    output.extend_from_slice(b"};\n");
    output.extend_from_slice(b"const unsigned long __birnam_artifact_descriptor_len = ");
    output.extend_from_slice(descriptor.len().to_string().as_bytes());
    output.extend_from_slice(b"UL;\n");
    output.extend_from_slice(generated);
    output
}

unsafe fn dispose_unit(api: &IdApi, unit: Oop) {
    // SAFETY: callers use this only before installing a method, or after a
    // direct one-shot entry call.  The Rust TCC invariant rejects disposal of
    // installed units, so a failed request cannot release live code.
    let _ = unsafe { send0(api, unit, DISPOSABLE_SELECTOR) };
}

fn retain_unit(unit: Oop) {
    if let Ok(mut units) = LIVE_UNITS.lock() {
        let address = unit as usize;
        if !units.contains(&address) {
            units.push(address);
        }
    }
}

unsafe fn api(runtime: &Runtime) -> &'static IdApi {
    // SAFETY: module_init records the process-owned IdApi for process life.
    unsafe { &*(runtime.api as *const IdApi) }
}

unsafe fn intern(api: &IdApi, name: &[u8]) -> Oop {
    debug_assert_eq!(name.last(), Some(&0));
    // SAFETY: names are static NUL-terminated selector/export bytes.
    unsafe { (api.intern)(name.as_ptr().cast::<c_char>()) }
}

unsafe fn send(api: &IdApi, receiver: Oop, selector: &[u8], args: &[Oop]) -> Oop {
    let selector = unsafe { intern(api, selector) };
    // SAFETY: args remains live across this synchronous dispatch.
    unsafe { (api.send)(receiver, selector, args.as_ptr(), args.len()) }
}

unsafe fn send0(api: &IdApi, receiver: Oop, selector: &[u8]) -> Oop {
    unsafe { send(api, receiver, selector, &[]) }
}

unsafe fn string_bytes(api: &IdApi, object: Oop) -> Option<Vec<u8>> {
    if object.is_null() || unsafe { (api.vtableOf)(object) } != api.string_vt {
        return None;
    }
    let mut length = 0;
    // SAFETY: object is a live String Oop returned by the host.
    let pointer = unsafe { (api.stringBytes)(object, &mut length) };
    if length == 0 {
        return Some(Vec::new());
    }
    if pointer.is_null() {
        return None;
    }
    // SAFETY: stringBytes supplies the object-owned initialized extent.
    Some(unsafe { std::slice::from_raw_parts(pointer, length) }.to_vec())
}

unsafe fn cst_tree(api: &IdApi, node: Oop, depth: usize) -> Option<OwnedCst> {
    if depth >= MAX_CST_DEPTH || node.is_null() {
        return None;
    }
    let rule_object = unsafe { send0(api, node, RULE_NAME_SELECTOR) };
    let rule = unsafe { string_bytes(api, rule_object) }?;
    let start_object = unsafe { send0(api, node, START_SELECTOR) };
    let end_object = unsafe { send0(api, node, END_SELECTOR) };
    let count_object = unsafe { send0(api, node, CHILD_COUNT_SELECTOR) };
    if unsafe { (api.isSmallInt)(start_object) == 0 }
        || unsafe { (api.isSmallInt)(end_object) == 0 }
        || unsafe { (api.isSmallInt)(count_object) == 0 }
    {
        return None;
    }
    let start = unsafe { (api.smallIntValue)(start_object) };
    let end = unsafe { (api.smallIntValue)(end_object) };
    let count = unsafe { (api.smallIntValue)(count_object) };
    if start < 0 || end < start || count < 0 {
        return None;
    }
    let count = usize::try_from(count).ok()?;
    let mut children = Vec::with_capacity(count);
    for index in 0..count {
        let child_index = unsafe { (api.mkSmallInt)(index as i64) };
        let child = unsafe { send(api, node, CHILD_AT_SELECTOR, &[child_index]) };
        children.push(unsafe { cst_tree(api, child, depth + 1) }?);
    }
    Some(OwnedCst::new(
        &rule,
        usize::try_from(start).ok()?,
        usize::try_from(end).ok()?,
        children,
    ))
}

fn skip_cst_wrappers(mut node: &OwnedCst) -> Option<&OwnedCst> {
    for _ in 0..MAX_CST_DEPTH {
        if matches!(node.rule(), b"expr" | b"primary" | b"message") && node.child_count() == 1 {
            node = node.child(0)?;
        } else {
            return Some(node);
        }
    }
    None
}

fn source_span<'a>(node: &OwnedCst, source: &'a [u8]) -> Option<&'a [u8]> {
    source.get(node.start()..node.end())
}

fn decode_string_literal(raw: &[u8]) -> Option<Vec<u8>> {
    if raw.len() < 2 || raw.first() != Some(&b'"') || raw.last() != Some(&b'"') {
        return None;
    }
    let inner = &raw[1..raw.len() - 1];
    let mut decoded = Vec::with_capacity(inner.len());
    let mut index = 0;
    while index < inner.len() {
        if inner[index] == b'\\'
            && index + 1 < inner.len()
            && let Some(byte) = match inner[index + 1] {
                b'"' => Some(b'"'),
                b'\\' => Some(b'\\'),
                b'n' => Some(b'\n'),
                b't' => Some(b'\t'),
                b'r' => Some(b'\r'),
                b'0' => Some(0),
                _ => None,
            }
        {
            decoded.push(byte);
            index += 2;
            continue;
        }
        decoded.push(inner[index]);
        index += 1;
    }
    Some(decoded)
}

fn embedded_literal_path(node: &OwnedCst, source: &[u8]) -> Option<Vec<u8>> {
    if node.rule() != b"send" {
        return None;
    }
    let node = skip_cst_wrappers(node)?;
    if node.rule() != b"send" || node.child_count() != 2 {
        return None;
    }
    let receiver = skip_cst_wrappers(node.child(0)?)?;
    if receiver.rule() != b"ident" || source_span(receiver, source)? != b"System" {
        return None;
    }
    let message = skip_cst_wrappers(node.child(1)?)?;
    if message.rule() != b"keywordMsg" || message.child_count() != 2 {
        return None;
    }
    let selector = source_span(message.child(0)?, source)?;
    if selector != b"embedFile:" {
        return None;
    }
    let argument = skip_cst_wrappers(message.child(1)?)?;
    if argument.rule() != b"string" {
        return None;
    }
    decode_string_literal(source_span(argument, source)?)
}

fn valid_relative_path(path: &[u8]) -> bool {
    !path.is_empty()
        && !path.starts_with(b"/")
        && !path
            .split(|byte| *byte == b'/')
            .any(|part| part == b"." || part == b"..")
}

fn read_embedded_file(root: &[u8], relative: &[u8]) -> Option<Vec<u8>> {
    if !valid_relative_path(relative) {
        note_error(
            b"RustCodegen embedFile: path must be a non-empty relative path without '.' or '..'",
        );
        return None;
    }
    let root = std::str::from_utf8(root).ok()?;
    let relative = std::str::from_utf8(relative).ok()?;
    let path = Path::new(root).join(relative);
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => {
            note_error(b"RustCodegen embedFile: could not read the file");
            return None;
        }
    };
    if bytes.len() > MAX_EMBEDDED_FILE_BYTES {
        note_error(b"RustCodegen embedFile: file exceeds the 16 MiB limit");
        return None;
    }
    Some(bytes)
}

fn rewrite_embedded(node: OwnedCst, source: &[u8], root: &[u8]) -> Option<OwnedCst> {
    if let Some(path) = embedded_literal_path(&node, source) {
        let bytes = read_embedded_file(root, &path)?;
        return Some(OwnedCst::embedded_string(node.start(), node.end(), bytes));
    }
    let rule = node.rule().to_vec();
    let start = node.start();
    let end = node.end();
    if let Some(bytes) = node.embedded_bytes() {
        return Some(OwnedCst::embedded_string(start, end, bytes.to_vec()));
    }
    let mut children = Vec::with_capacity(node.child_count());
    for index in 0..node.child_count() {
        let child = node.child(index)?.to_owned();
        children.push(rewrite_embedded(child, source, root)?);
    }
    Some(OwnedCst::new(&rule, start, end, children))
}

fn collect_embedded_paths(node: &OwnedCst, source: &[u8], depth: usize, paths: &mut Vec<Vec<u8>>) {
    if depth >= MAX_CST_DEPTH {
        return;
    }
    if let Some(path) = embedded_literal_path(node, source)
        && !paths.iter().any(|existing| existing == &path)
    {
        paths.push(path);
    }
    for index in 0..node.child_count() {
        if let Some(child) = node.child(index) {
            collect_embedded_paths(child, source, depth + 1, paths);
        }
    }
}

fn source_directory(path: &[u8]) -> Vec<u8> {
    match path.iter().rposition(|byte| *byte == b'/') {
        Some(0) => b"/".to_vec(),
        Some(index) => path[..index].to_vec(),
        None => b".".to_vec(),
    }
}

fn symbol_literal(node: &OwnedCst, source: &[u8]) -> Option<Vec<u8>> {
    let node = skip_cst_wrappers(node)?;
    source_span(node, source)?
        .strip_prefix(b"#")
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
}

fn vector_symbols(node: &OwnedCst, source: &[u8]) -> Option<Vec<Vec<u8>>> {
    let node = skip_cst_wrappers(node)?;
    if node.rule() != b"vectorLit" {
        return None;
    }
    let mut values = Vec::with_capacity(node.child_count());
    for index in 0..node.child_count() {
        values.push(symbol_literal(node.child(index)?, source)?);
    }
    Some(values)
}

fn keyword_selector_args<'a>(
    node: &'a OwnedCst,
    source: &[u8],
) -> Option<(Vec<u8>, Vec<&'a OwnedCst>)> {
    let node = skip_cst_wrappers(node)?;
    if node.rule() != b"keywordMsg" || node.child_count() == 0 || node.child_count() % 2 != 0 {
        return None;
    }
    let mut selector = Vec::new();
    let mut args = Vec::with_capacity(node.child_count() / 2);
    let mut index = 0;
    while index < node.child_count() {
        selector.extend_from_slice(source_span(node.child(index)?, source)?);
        args.push(node.child(index + 1)?);
        index += 2;
    }
    Some((selector, args))
}

fn collect_artifact_metadata(
    node: &OwnedCst,
    source: &[u8],
    depth: usize,
    module: &mut Option<(Vec<u8>, Vec<Vec<u8>>)>,
    native_requires: &mut Vec<Vec<u8>>,
) {
    if depth >= MAX_CST_DEPTH {
        return;
    }
    if node.rule() == b"send" && node.child_count() == 2 {
        let receiver = skip_cst_wrappers(node.child(0).unwrap_or(node));
        let message = skip_cst_wrappers(node.child(1).unwrap_or(node));
        if let (Some(receiver), Some(message)) = (receiver, message)
            && let Some((selector, args)) = keyword_selector_args(message, source)
        {
            if receiver.rule() == b"ident"
                && source_span(receiver, source) == Some(b"Module")
                && selector == b"named:requires:"
                && args.len() == 2
                && let (Some(name), Some(requires)) = (
                    symbol_literal(args[0], source),
                    vector_symbols(args[1], source),
                )
            {
                *module = Some((name, requires));
            }
            if receiver.rule() == b"ident"
                && source_span(receiver, source) == Some(b"Alien")
                && selector == b"dlopen:"
                && args.len() == 1
                && let Some(argument) = skip_cst_wrappers(args[0])
                    .and_then(|argument| source_span(argument, source))
                    .and_then(decode_string_literal)
                && !native_requires.iter().any(|value| value == &argument)
            {
                native_requires.push(argument);
            }
        }
    }
    for index in 0..node.child_count() {
        if let Some(child) = node.child(index) {
            collect_artifact_metadata(child, source, depth + 1, module, native_requires);
        }
    }
}

type ModuleMetadata = (Vec<u8>, Vec<Vec<u8>>);
type ArtifactMetadata = (Option<ModuleMetadata>, Vec<Vec<u8>>);

unsafe fn artifact_metadata(api: &IdApi, source_object: Oop) -> Option<ArtifactMetadata> {
    let source = unsafe { string_bytes(api, source_object) }?;
    let lang = unsafe { (api.import)(LANG_NAME.as_ptr().cast()) };
    let cst = unsafe { send(api, lang, PARSE_SELECTOR, &[source_object]) };
    let cst_vt = unsafe { (api.import)(PEG_CST_VT_NAME.as_ptr().cast()) };
    if cst.is_null() || cst == api.nilObj || unsafe { (api.vtableOf)(cst) != cst_vt } {
        return None;
    }
    let tree = unsafe { cst_tree(api, cst, 0) }?;
    let mut module = None;
    let mut native_requires = Vec::new();
    collect_artifact_metadata(&tree, &source, 0, &mut module, &mut native_requires);
    Some((module, native_requires))
}

struct RevisionOperation {
    target: Vec<u8>,
    selector: Vec<u8>,
    install: bool,
    source: Vec<u8>,
}

struct RevisionMetadata {
    name: Vec<u8>,
    requires: Vec<Vec<u8>>,
    non_method_digest: [u8; 64],
    method_digest: [u8; 64],
    revision_digest: [u8; 64],
}

fn append_digest_field(input: &mut Vec<u8>, bytes: &[u8]) {
    input.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
    input.extend_from_slice(bytes);
}

fn selector_arity(selector: &[u8]) -> usize {
    let colons = selector.iter().filter(|byte| **byte == b':').count();
    if colons != 0 {
        return colons;
    }
    match selector.first() {
        None => 0,
        Some(byte) if byte.is_ascii_alphabetic() || *byte == b'_' => 0,
        Some(_) => 1,
    }
}

fn block_parameter_count(node: &OwnedCst) -> Option<usize> {
    let node = skip_cst_wrappers(node)?;
    if node.rule() != b"block" {
        return None;
    }
    Some(
        (0..node.child_count())
            .filter_map(|index| node.child(index))
            .filter(|child| child.rule() == b"ident")
            .count(),
    )
}

fn revision_metadata(tree: &OwnedCst, source: &[u8]) -> Option<RevisionMetadata> {
    let program = skip_cst_wrappers(tree)?;
    if program.rule() != b"program" || program.child_count() != 1 {
        return None;
    }
    let statements = program.child(0)?;
    if statements.rule() != b"statements" {
        return None;
    }
    let mut name = None;
    let mut requires = Vec::new();
    let mut operations = Vec::new();
    let mut non_method_spans = Vec::new();
    for index in 0..statements.child_count() {
        let statement = statements.child(index)?;
        if statement.child_count() == 0 {
            continue;
        }
        let whole = skip_cst_wrappers(statement.child(0)?)?;
        let whole_source = source_span(whole, source)?;
        if whole.rule() != b"send" || whole.child_count() != 2 {
            non_method_spans.push(whole_source.to_vec());
            continue;
        }
        let receiver = skip_cst_wrappers(whole.child(0)?)?;
        let message = skip_cst_wrappers(whole.child(1)?)?;
        let Some((selector, args)) = keyword_selector_args(message, source) else {
            non_method_spans.push(whole_source.to_vec());
            continue;
        };
        if receiver.rule() == b"ident"
            && source_span(receiver, source) == Some(b"Module".as_slice())
            && selector == b"named:requires:"
            && args.len() == 2
        {
            if name.is_some() {
                return None;
            }
            name = Some(symbol_literal(args[0], source)?);
            requires = vector_symbols(args[1], source)?;
            non_method_spans.push(whole_source.to_vec());
            continue;
        }
        if receiver.rule() != b"ident"
            || source_span(receiver, source) != Some(b"Reflect".as_slice())
        {
            non_method_spans.push(whole_source.to_vec());
            continue;
        }
        let is_define = (selector == b"defineMethod:on:as:" && args.len() == 3)
            || (selector == b"defineMethod:on:as:doc:" && args.len() == 4);
        let is_remove = selector == b"removeMethod:on:" && args.len() == 2;
        if !is_define && !is_remove {
            non_method_spans.push(whole_source.to_vec());
            continue;
        }
        let operation_selector = symbol_literal(args[0], source)?;
        let target = skip_cst_wrappers(args[1])?;
        if target.rule() != b"ident" {
            return None;
        }
        let target = source_span(target, source)?.to_vec();
        if is_define {
            let parameters = block_parameter_count(args[2])?;
            if parameters != selector_arity(&operation_selector) + 1 {
                return None;
            }
        }
        if operations.iter().any(|operation: &RevisionOperation| {
            operation.target == target && operation.selector == operation_selector
        }) {
            return None;
        }
        operations.push(RevisionOperation {
            target,
            selector: operation_selector,
            install: is_define,
            source: whole_source.to_vec(),
        });
    }
    let name = name?;
    let mut non_method_input = b"birnam.non-method.v1\0".to_vec();
    for span in non_method_spans {
        append_digest_field(&mut non_method_input, &span);
    }
    let non_method_digest = sha256_hex(&non_method_input);

    let mut method_input = b"birnam.methods.v1\0".to_vec();
    for operation in operations {
        method_input.push(if operation.install { b'I' } else { b'R' });
        append_digest_field(&mut method_input, &operation.target);
        append_digest_field(&mut method_input, &operation.selector);
        append_digest_field(&mut method_input, &operation.source);
    }
    let method_digest = sha256_hex(&method_input);

    let mut revision_input = b"birnam.source-revision.v1\0".to_vec();
    append_digest_field(&mut revision_input, &name);
    revision_input.extend_from_slice(&(requires.len() as u64).to_be_bytes());
    for requirement in &requires {
        append_digest_field(&mut revision_input, requirement);
    }
    append_digest_field(&mut revision_input, source);
    let revision_digest = sha256_hex(&revision_input);

    Some(RevisionMetadata {
        name,
        requires,
        non_method_digest,
        method_digest,
        revision_digest,
    })
}

unsafe fn parse_revision_metadata(
    api: &IdApi,
    source_object: Oop,
) -> Option<(Vec<u8>, RevisionMetadata)> {
    let source = unsafe { string_bytes(api, source_object) }?;
    let lang = unsafe { (api.import)(LANG_NAME.as_ptr().cast()) };
    let cst = unsafe { send(api, lang, PARSE_SELECTOR, &[source_object]) };
    let cst_vt = unsafe { (api.import)(PEG_CST_VT_NAME.as_ptr().cast()) };
    if cst.is_null() || cst == api.nilObj || unsafe { (api.vtableOf)(cst) != cst_vt } {
        return None;
    }
    let tree = unsafe { cst_tree(api, cst, 0) }?;
    let metadata = revision_metadata(&tree, &source)?;
    Some((source, metadata))
}

unsafe fn symbol_bytes(api: &IdApi, symbol: Oop) -> Option<Vec<u8>> {
    if symbol.is_null() || unsafe { (api.vtableOf)(symbol) != api.symbol_vt } {
        return None;
    }
    let pointer = unsafe { (api.symbolName)(symbol) };
    if pointer.is_null() {
        return None;
    }
    Some(unsafe { CStr::from_ptr(pointer) }.to_bytes().to_vec())
}

unsafe extern "C" fn rust_codegen_record_module_source(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }.unwrap_or_else(|| {
        abort("RustCodegen recordModuleSource:named: received invalid arguments")
    });
    if values.len() != 2 {
        abort("RustCodegen recordModuleSource:named: expects source and name");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    unsafe { (api.gcSetAuto)(0) };
    struct AutoGc<'a>(&'a IdApi);
    impl Drop for AutoGc<'_> {
        fn drop(&mut self) {
            unsafe { (self.0.gcSetAuto)(1) };
        }
    }
    let _auto_gc = AutoGc(api);
    let Some(name) = (unsafe { symbol_bytes(api, values[1]) }) else {
        note_error(b"RustCodegen module name was not a Symbol");
        return api.nilObj;
    };
    let Some((Some((declared_name, _requires)), _native_requires)) =
        (unsafe { artifact_metadata(api, values[0]) })
    else {
        note_error(b"RustCodegen module source is not indexable");
        return api.nilObj;
    };
    if declared_name != name {
        note_error(b"RustCodegen module source identity does not match its name");
        return api.nilObj;
    }
    if let Ok(sealed) = INITIAL_GENERATION_SEALED.lock()
        && *sealed
    {
        note_error(b"RustCodegen module introduction is not available");
        return api.nilObj;
    }
    let source = unsafe { string_bytes(api, values[0]) };
    let Some(source) = source else {
        note_error(b"RustCodegen module source was not a String");
        return api.nilObj;
    };
    if let Ok(mut modules) = MODULE_SOURCES.lock() {
        if let Some(&(_, existing_source)) = modules.iter().find(|&&(existing_name, _)| {
            unsafe { symbol_bytes(api, existing_name as Oop) }
                .as_deref()
                .is_some_and(|existing_name| existing_name == name.as_slice())
        }) {
            let existing = existing_source as Oop;
            if unsafe { string_bytes(api, existing) }.as_deref() == Some(source.as_slice()) {
                return values[0];
            }
            note_error(b"RustCodegen duplicate module source");
            return api.nilObj;
        }
        modules.push((values[1] as usize, values[0] as usize));
        return values[0];
    }
    note_error(b"RustCodegen module source registry is unavailable");
    api.nilObj
}

unsafe extern "C" fn rust_codegen_module_source_for(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustCodegen moduleSourceFor: received invalid arguments"));
    if values.len() != 1 {
        abort("RustCodegen moduleSourceFor: expects one name");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    let Some(name) = (unsafe { symbol_bytes(api, values[0]) }) else {
        return api.nilObj;
    };
    let Ok(modules) = MODULE_SOURCES.lock() else {
        return api.nilObj;
    };
    for &(existing_name, source) in modules.iter() {
        if unsafe { symbol_bytes(api, existing_name as Oop) }
            .as_deref()
            .is_some_and(|existing| existing == name.as_slice())
        {
            return source as Oop;
        }
    }
    api.nilObj
}

unsafe extern "C" fn rust_codegen_seal_initial_generation(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustCodegen sealInitialGeneration received invalid arguments"));
    if !values.is_empty() {
        abort("RustCodegen sealInitialGeneration expects no arguments");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    if let Ok(mut sealed) = INITIAL_GENERATION_SEALED.lock() {
        *sealed = true;
        return api.trueObj;
    }
    api.nilObj
}

unsafe fn make_vector(api: &IdApi, vector_proto: Oop, items: &[Oop]) -> Oop {
    let mut result = unsafe { send0(api, vector_proto, NEW_SELECTOR) };
    for &item in items {
        result = unsafe { send(api, result, b"add:\0", &[item]) };
    }
    result
}

unsafe fn vector_item(api: &IdApi, vector: Oop, index: usize) -> Oop {
    let index = unsafe { (api.mkSmallInt)(index as i64) };
    unsafe { send(api, vector, GLOBAL_AT_SELECTOR, &[index]) }
}

unsafe fn vector_length(api: &IdApi, vector: Oop, vector_vt: Oop) -> Option<usize> {
    if vector.is_null() || unsafe { (api.vtableOf)(vector) != vector_vt } {
        return None;
    }
    let size = unsafe { send0(api, vector, b"size\0") };
    if unsafe { (api.isSmallInt)(size) } == 0 {
        return None;
    }
    usize::try_from(unsafe { (api.smallIntValue)(size) }).ok()
}

fn is_digest(bytes: &[u8]) -> bool {
    bytes.len() == 64
        && bytes.iter().all(|byte| {
            byte.is_ascii_digit() || (b'a'..=b'f').contains(byte) || (b'A'..=b'F').contains(byte)
        })
}

unsafe fn valid_revision(api: &IdApi, revision: Oop) -> bool {
    let vector_vt = unsafe { (api.import)(VECTOR_VT_NAME.as_ptr().cast()) };
    if unsafe { vector_length(api, revision, vector_vt) } != Some(7) {
        return false;
    }
    if unsafe { vector_item(api, revision, 0) } != unsafe { intern(api, b"sourceRevision\0") }
        || unsafe { symbol_bytes(api, vector_item(api, revision, 1)) }.is_none()
    {
        return false;
    }
    let requirements = unsafe { vector_item(api, revision, 2) };
    let Some(requirement_count) = (unsafe { vector_length(api, requirements, vector_vt) }) else {
        return false;
    };
    for index in 0..requirement_count {
        if unsafe { symbol_bytes(api, vector_item(api, requirements, index)) }.is_none() {
            return false;
        }
    }
    if unsafe { string_bytes(api, vector_item(api, revision, 3)) }.is_none() {
        return false;
    }
    for index in 4..7 {
        let Some(digest) = (unsafe { string_bytes(api, vector_item(api, revision, index)) }) else {
            return false;
        };
        if !is_digest(&digest) {
            return false;
        }
    }
    true
}

unsafe fn revision_transition_allowed(api: &IdApi, current: Oop, successor: Oop) -> bool {
    let vector_vt = unsafe { (api.import)(VECTOR_VT_NAME.as_ptr().cast()) };
    if vector_vt.is_null()
        || !unsafe { valid_revision(api, current) }
        || !unsafe { valid_revision(api, successor) }
    {
        return false;
    }
    let tag = unsafe { intern(api, b"sourceRevision\0") };
    if unsafe { vector_item(api, current, 0) } != tag
        || unsafe { vector_item(api, successor, 0) } != tag
    {
        return false;
    }
    if unsafe { symbol_bytes(api, vector_item(api, current, 1)) }
        != unsafe { symbol_bytes(api, vector_item(api, successor, 1)) }
    {
        return false;
    }
    let current_requires = unsafe { vector_item(api, current, 2) };
    let successor_requires = unsafe { vector_item(api, successor, 2) };
    let Some(current_count) = (unsafe { vector_length(api, current_requires, vector_vt) }) else {
        return false;
    };
    let Some(successor_count) = (unsafe { vector_length(api, successor_requires, vector_vt) })
    else {
        return false;
    };
    if current_count != successor_count {
        return false;
    }
    for index in 0..current_count {
        if unsafe { symbol_bytes(api, vector_item(api, current_requires, index)) }
            != unsafe { symbol_bytes(api, vector_item(api, successor_requires, index)) }
        {
            return false;
        }
    }
    (unsafe { string_bytes(api, vector_item(api, current, 4)) })
        == (unsafe { string_bytes(api, vector_item(api, successor, 4)) })
}

unsafe fn generation_digest(api: &IdApi, source_map: Oop) -> Option<[u8; 64]> {
    let vector_vt = unsafe { (api.import)(VECTOR_VT_NAME.as_ptr().cast()) };
    let count = unsafe { vector_length(api, source_map, vector_vt) }?;
    let mut digest_input = b"birnam.dispatch-generation.v1\0".to_vec();
    digest_input.extend_from_slice(&(count as u64).to_be_bytes());
    let mut names = Vec::with_capacity(count);
    for index in 0..count {
        let entry = unsafe { vector_item(api, source_map, index) };
        if unsafe { vector_length(api, entry, vector_vt) } != Some(3) {
            return None;
        }
        let name = unsafe { symbol_bytes(api, vector_item(api, entry, 0)) }?;
        let revision = unsafe { vector_item(api, entry, 1) };
        if !unsafe { valid_revision(api, revision) } {
            return None;
        }
        if unsafe { symbol_bytes(api, vector_item(api, revision, 1)) }? != name {
            return None;
        }
        if names.iter().any(|existing| existing == &name) {
            return None;
        }
        names.push(name.clone());
        let digest = unsafe { string_bytes(api, vector_item(api, revision, 6)) }?;
        digest_input.extend_from_slice(&(name.len() as u64).to_be_bytes());
        digest_input.extend_from_slice(&name);
        digest_input.extend_from_slice(&(digest.len() as u64).to_be_bytes());
        digest_input.extend_from_slice(&digest);
    }
    Some(sha256_hex(&digest_input))
}

unsafe extern "C" fn rust_codegen_initial_generation_projection(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }.unwrap_or_else(|| {
        abort("RustCodegen initialGenerationProjection received invalid arguments")
    });
    if !values.is_empty() {
        abort("RustCodegen initialGenerationProjection expects no arguments");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    unsafe { (api.gcSetAuto)(0) };
    struct AutoGc<'a>(&'a IdApi);
    impl Drop for AutoGc<'_> {
        fn drop(&mut self) {
            unsafe { (self.0.gcSetAuto)(1) };
        }
    }
    let _auto_gc = AutoGc(api);
    let vector_proto = unsafe { (api.import)(VECTOR_NAME.as_ptr().cast()) };
    if vector_proto.is_null() {
        note_error(b"RustCodegen Vector is unavailable");
        return api.nilObj;
    }
    let Ok(module_guard) = MODULE_SOURCES.lock() else {
        note_error(b"RustCodegen module source registry is unavailable");
        return api.nilObj;
    };
    let mut modules = module_guard.clone();
    drop(module_guard);
    modules.sort_by(|left, right| {
        let left_name = unsafe { symbol_bytes(api, left.0 as Oop) }.unwrap_or_default();
        let right_name = unsafe { symbol_bytes(api, right.0 as Oop) }.unwrap_or_default();
        left_name.cmp(&right_name)
    });

    let mut entries = Vec::with_capacity(modules.len());
    for (name, source) in modules {
        let name = name as Oop;
        let source = source as Oop;
        let revision = unsafe {
            send(
                api,
                runtime.rust_codegen as Oop,
                SOURCE_REVISION_SELECTOR,
                &[source],
            )
        };
        if revision.is_null() || revision == api.nilObj {
            note_error(b"RustCodegen source revision is unavailable");
            return api.nilObj;
        }
        entries.push(unsafe { make_vector(api, vector_proto, &[name, revision, api.nilObj]) });
    }
    let source_map = unsafe { make_vector(api, vector_proto, &entries) };
    let Some(digest) = (unsafe { generation_digest(api, source_map) }) else {
        note_error(b"RustCodegen generation projection is malformed");
        return api.nilObj;
    };
    let digest_object = unsafe { (api.mkString)(digest.as_ptr(), digest.len()) };
    let tag = unsafe { intern(api, b"generationProjection\0") };
    unsafe { make_vector(api, vector_proto, &[tag, source_map, digest_object]) }
}

unsafe extern "C" fn rust_codegen_successor_generation_projection(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }.unwrap_or_else(|| {
        abort("RustCodegen successorGenerationProjection received invalid arguments")
    });
    if values.len() != 4 {
        abort(
            "RustCodegen successorGenerationProjection expects source map, name, revision, and unit",
        );
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    unsafe { (api.gcSetAuto)(0) };
    struct AutoGc<'a>(&'a IdApi);
    impl Drop for AutoGc<'_> {
        fn drop(&mut self) {
            unsafe { (self.0.gcSetAuto)(1) };
        }
    }
    let _auto_gc = AutoGc(api);
    let vector_proto = unsafe { (api.import)(VECTOR_NAME.as_ptr().cast()) };
    let vector_vt = unsafe { (api.import)(VECTOR_VT_NAME.as_ptr().cast()) };
    let Some(count) = (unsafe { vector_length(api, values[0], vector_vt) }) else {
        note_error(b"RustCodegen source map is malformed");
        return api.nilObj;
    };
    if vector_proto.is_null() {
        note_error(b"RustCodegen Vector is unavailable");
        return api.nilObj;
    }
    let Some(name) = (unsafe { symbol_bytes(api, values[1]) }) else {
        note_error(b"RustCodegen module name was not a Symbol");
        return api.nilObj;
    };
    let mut entries = Vec::with_capacity(count);
    let mut found = false;
    for index in 0..count {
        let entry = unsafe { vector_item(api, values[0], index) };
        if unsafe { vector_length(api, entry, vector_vt) } != Some(3) {
            note_error(b"RustCodegen source map is malformed");
            return api.nilObj;
        }
        let entry_name = unsafe { vector_item(api, entry, 0) };
        let Some(entry_name_bytes) = (unsafe { symbol_bytes(api, entry_name) }) else {
            note_error(b"RustCodegen source map has a non-symbol module name");
            return api.nilObj;
        };
        if entry_name_bytes == name {
            if found {
                note_error(b"RustCodegen source map has duplicate module names");
                return api.nilObj;
            }
            found = true;
            if !(unsafe { revision_transition_allowed(api, vector_item(api, entry, 1), values[2]) })
            {
                note_error(b"RustCodegen top-level evolution is not available");
                return api.nilObj;
            }
            entries.push(unsafe {
                make_vector(api, vector_proto, &[values[1], values[2], values[3]])
            });
        } else {
            entries.push(entry);
        }
    }
    if !found {
        note_error(b"RustCodegen module introduction is not available");
        return api.nilObj;
    }
    let successor = unsafe { make_vector(api, vector_proto, &entries) };
    let Some(digest) = (unsafe { generation_digest(api, successor) }) else {
        note_error(b"RustCodegen successor projection is malformed");
        return api.nilObj;
    };
    let digest_object = unsafe { (api.mkString)(digest.as_ptr(), digest.len()) };
    let tag = unsafe { intern(api, b"generationProjection\0") };
    unsafe { make_vector(api, vector_proto, &[tag, successor, digest_object]) }
}

unsafe fn emit_source(
    api: &IdApi,
    source_object: Oop,
    artifact: bool,
    persist_top_level: bool,
    source_path: Option<&[u8]>,
    source_path_is_root: bool,
) -> Option<Vec<u8>> {
    let source = unsafe { string_bytes(api, source_object) }?;
    let lang = unsafe { (api.import)(LANG_NAME.as_ptr().cast()) };
    let cst = unsafe { send(api, lang, PARSE_SELECTOR, &[source_object]) };
    let cst_vt = unsafe { (api.import)(PEG_CST_VT_NAME.as_ptr().cast()) };
    if cst.is_null() || cst == api.nilObj || unsafe { (api.vtableOf)(cst) != cst_vt } {
        unsafe { note_syntax_error(api, lang) };
        return None;
    }
    let tree = unsafe { cst_tree(api, cst, 0) }?;
    let root = if source_path_is_root {
        source_path.map(ToOwned::to_owned)
    } else {
        source_path.map(source_directory)
    };
    let tree = rewrite_embedded(tree, &source, root.as_deref().unwrap_or(b"."))?;
    let program = match lower_program(&tree, &source) {
        Ok(program) => program,
        Err(_) => {
            note_error(b"RustCodegen could not lower the source");
            return None;
        }
    };
    let mut emitter = Emitter::new();
    if persist_top_level {
        if emitter.emit_program_persisting(&program).is_err() {
            note_error(b"RustCodegen could not emit the source");
            return None;
        }
    } else {
        if emitter.emit_program(&program).is_err() {
            note_error(b"RustCodegen could not emit the source");
            return None;
        }
    }
    let (function_defs, body) = emitter.finish_parts();
    let mut unit = TranslationUnit::new(artifact);
    unit.push_function(&function_defs);
    unit.push_body(&body);
    Some(unit.finish())
}

unsafe extern "C" fn rust_codegen_emit(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustCodegen emit: received invalid arguments"));
    if values.len() != 1 {
        abort("RustCodegen emit: expects one source argument");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    clear_error();
    let Some(output) = (unsafe { emit_source(api, values[0], false, false, None, false) }) else {
        note_error(b"RustCodegen could not lower the source");
        return api.nilObj;
    };
    // SAFETY: mkString copies the generated bytes before this call returns.
    unsafe { (api.mkString)(output.as_ptr(), output.len()) }
}

unsafe extern "C" fn rust_codegen_embedded_files(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustCodegen embeddedFilesOf: received invalid arguments"));
    if values.len() != 1 {
        abort("RustCodegen embeddedFilesOf: expects one source argument");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    let vector_proto = unsafe { (api.import)(VECTOR_NAME.as_ptr().cast()) };
    if vector_proto.is_null() {
        note_error(b"RustCodegen Vector is unavailable");
        return api.nilObj;
    }
    let result = unsafe { send0(api, vector_proto, NEW_SELECTOR) };
    let Some(source) = (unsafe { string_bytes(api, values[0]) }) else {
        note_error(b"RustCodegen source was not a String");
        return result;
    };
    let lang = unsafe { (api.import)(LANG_NAME.as_ptr().cast()) };
    let cst = unsafe { send(api, lang, PARSE_SELECTOR, &[values[0]]) };
    let cst_vt = unsafe { (api.import)(PEG_CST_VT_NAME.as_ptr().cast()) };
    if cst.is_null() || cst == api.nilObj || unsafe { (api.vtableOf)(cst) != cst_vt } {
        return result;
    }
    let Some(tree) = (unsafe { cst_tree(api, cst, 0) }) else {
        return result;
    };
    let mut paths = Vec::new();
    collect_embedded_paths(&tree, &source, 0, &mut paths);
    let mut result = result;
    for path in paths {
        let value = unsafe { (api.mkString)(path.as_ptr(), path.len()) };
        result = unsafe { send(api, result, b"add:\0", &[value]) };
    }
    result
}

unsafe extern "C" fn rust_codegen_module_metadata(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustCodegen moduleMetadataOf: received invalid arguments"));
    if values.len() != 1 {
        abort("RustCodegen moduleMetadataOf: expects one source argument");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    let Some((source, metadata)) = (unsafe { parse_revision_metadata(api, values[0]) }) else {
        return api.nilObj;
    };
    let vector_proto = unsafe { (api.import)(VECTOR_NAME.as_ptr().cast()) };
    if vector_proto.is_null() {
        note_error(b"RustCodegen Vector is unavailable");
        return api.nilObj;
    }
    let make_vector = |api: &IdApi, items: &[Oop]| -> Oop {
        let mut result = unsafe { send0(api, vector_proto, NEW_SELECTOR) };
        for &item in items {
            result = unsafe { send(api, result, b"add:\0", &[item]) };
        }
        result
    };
    let name_symbol = unsafe { (api.intern)(metadata.name.as_ptr().cast()) };
    let require_symbols = metadata
        .requires
        .iter()
        .map(|requirement| unsafe { (api.intern)(requirement.as_ptr().cast()) })
        .collect::<Vec<_>>();
    let digest = sha256_hex(&source);
    let digest_object = unsafe { (api.mkString)(digest.as_ptr(), digest.len()) };
    let revision_object = unsafe {
        (api.mkString)(
            metadata.revision_digest.as_ptr(),
            metadata.revision_digest.len(),
        )
    };
    let requires_vector = make_vector(api, &require_symbols);
    make_vector(
        api,
        &[
            unsafe { intern(api, b"moduleMetadata\0") },
            name_symbol,
            requires_vector,
            digest_object,
            revision_object,
        ],
    )
}

unsafe extern "C" fn rust_codegen_source_revision(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustCodegen sourceRevisionOf: received invalid arguments"));
    if values.len() != 1 {
        abort("RustCodegen sourceRevisionOf: expects one source argument");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    let Some((_source, metadata)) = (unsafe { parse_revision_metadata(api, values[0]) }) else {
        note_error(b"RustCodegen source was not a String");
        return api.nilObj;
    };
    let vector_proto = unsafe { (api.import)(VECTOR_NAME.as_ptr().cast()) };
    if vector_proto.is_null() {
        note_error(b"RustCodegen Vector is unavailable");
        return api.nilObj;
    }
    let make_vector = |items: &[Oop]| -> Oop {
        let mut result = unsafe { send0(api, vector_proto, NEW_SELECTOR) };
        for &item in items {
            result = unsafe { send(api, result, b"add:\0", &[item]) };
        }
        result
    };
    let name_symbol = unsafe { (api.intern)(metadata.name.as_ptr().cast()) };
    let require_symbols = metadata
        .requires
        .iter()
        .map(|requirement| unsafe { (api.intern)(requirement.as_ptr().cast()) })
        .collect::<Vec<_>>();
    let requires_vector = make_vector(&require_symbols);
    let non_method_digest = unsafe {
        (api.mkString)(
            metadata.non_method_digest.as_ptr(),
            metadata.non_method_digest.len(),
        )
    };
    let method_digest = unsafe {
        (api.mkString)(
            metadata.method_digest.as_ptr(),
            metadata.method_digest.len(),
        )
    };
    let revision_digest = unsafe {
        (api.mkString)(
            metadata.revision_digest.as_ptr(),
            metadata.revision_digest.len(),
        )
    };
    make_vector(&[
        unsafe { intern(api, b"sourceRevision\0") },
        name_symbol,
        requires_vector,
        values[0],
        non_method_digest,
        method_digest,
        revision_digest,
    ])
}

unsafe extern "C" fn rust_codegen_eval(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustCodegen eval: received invalid arguments"));
    if values.len() != 1 && values.len() != 2 {
        abort("RustCodegen eval: expects source or source and source path");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    clear_error();
    let source_path = if values.len() == 2 {
        unsafe { string_bytes(api, values[1]) }
    } else {
        None
    };
    if values.len() == 2 && source_path.is_none() {
        note_error(b"RustCodegen source path was not a String");
        return api.nilObj;
    }
    let Some(source) =
        (unsafe { emit_source(api, values[0], false, false, source_path.as_deref(), false) })
    else {
        note_error(b"RustCodegen could not lower the source");
        return api.nilObj;
    };
    let tcc = runtime.tcc as Oop;
    if tcc.is_null() {
        note_error(b"RustCodegen Tcc is unavailable");
        return api.nilObj;
    }
    let unit = unsafe { send0(api, tcc, NEW_SELECTOR) };
    if unit.is_null() || unit == api.nilObj {
        note_error(b"RustCodegen could not create a Tcc unit");
        return api.nilObj;
    }
    let c_source = unsafe { (api.mkString)(source.as_ptr(), source.len()) };
    if unsafe { send(api, unit, COMPILE_SELECTOR, &[c_source]) } == api.nilObj
        || !unsafe { bind_global_shims(api, unit) }
        || unsafe { send0(api, unit, BIND_RUNTIME_SELECTOR) } == api.nilObj
        || unsafe { send0(api, unit, RELOCATE_SELECTOR) } == api.nilObj
    {
        unsafe { dispose_unit(api, unit) };
        note_error(b"RustCodegen generated C failed to compile");
        return api.nilObj;
    }
    let entry_name = unsafe { (api.mkString)(b"__ev".as_ptr(), 4) };
    let entry_selector = unsafe { intern(api, ENTRY_SELECTOR) };
    if !source.windows(5).any(|window| window == b"__blk") {
        let symbol = unsafe { send(api, unit, SYMBOL_SELECTOR, &[entry_name]) };
        if unsafe { (api.isSmallInt)(symbol) } == 0 {
            unsafe { dispose_unit(api, unit) };
            note_error(b"RustCodegen could not resolve generated entry point");
            return api.nilObj;
        }
        let address = unsafe { (api.smallIntValue)(symbol) } as usize;
        // SAFETY: TCC's `symbol:` returns the generated `__ev` address with the
        // frozen Method ABI; the unit is still relocated and not installed.
        let entry: Method = unsafe { core::mem::transmute(address) };
        let result = unsafe { entry(api.nilObj, ptr::null(), 0) };
        unsafe { dispose_unit(api, unit) };
        return result;
    }

    let harness_vt = unsafe { (api.proto)(api.object_vt) };
    if harness_vt.is_null() {
        unsafe { dispose_unit(api, unit) };
        return api.nilObj;
    }
    let instance = unsafe { (api.alloc)(harness_vt, 0) };
    if instance.is_null() {
        unsafe { dispose_unit(api, unit) };
        return api.nilObj;
    }
    if unsafe {
        send(
            api,
            unit,
            INSTALL_SELECTOR,
            &[entry_name, entry_selector, harness_vt],
        )
    } == api.nilObj
    {
        unsafe { dispose_unit(api, unit) };
        return api.nilObj;
    }
    let result = unsafe { send0(api, instance, ENTRY_SELECTOR) };
    retain_unit(unit);
    result
}

unsafe extern "C" fn rust_codegen_eval_persisting(
    self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustCodegen evalPersisting: received invalid arguments"));
    if values.len() != 1 && values.len() != 2 {
        abort("RustCodegen evalPersisting: expects source or source and source path");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    clear_error();
    let source_path = if values.len() == 2 {
        unsafe { string_bytes(api, values[1]) }
    } else {
        None
    };
    if values.len() == 2 && source_path.is_none() {
        note_error(b"RustCodegen source path was not a String");
        return api.nilObj;
    }
    let Some(source) =
        (unsafe { emit_source(api, values[0], false, true, source_path.as_deref(), false) })
    else {
        note_error(b"RustCodegen could not lower the source");
        return api.nilObj;
    };
    let tcc = runtime.tcc as Oop;
    if tcc.is_null() {
        note_error(b"RustCodegen Tcc is unavailable");
        return api.nilObj;
    }
    let unit = unsafe { send0(api, tcc, NEW_SELECTOR) };
    if unit.is_null() || unit == api.nilObj {
        note_error(b"RustCodegen could not create a Tcc unit");
        return api.nilObj;
    }
    let c_source = unsafe { (api.mkString)(source.as_ptr(), source.len()) };
    if unsafe { send(api, unit, COMPILE_SELECTOR, &[c_source]) } == api.nilObj
        || !unsafe { bind_global_shims(api, unit) }
        || unsafe { send0(api, unit, BIND_RUNTIME_SELECTOR) } == api.nilObj
        || unsafe { send0(api, unit, RELOCATE_SELECTOR) } == api.nilObj
    {
        unsafe { dispose_unit(api, unit) };
        note_error(b"RustCodegen generated C failed to compile");
        return api.nilObj;
    }
    let entry_name = unsafe { (api.mkString)(b"__ev".as_ptr(), 4) };
    let symbol = unsafe { send(api, unit, SYMBOL_SELECTOR, &[entry_name]) };
    if unsafe { (api.isSmallInt)(symbol) } == 0 {
        unsafe { dispose_unit(api, unit) };
        note_error(b"RustCodegen could not resolve generated entry point");
        return api.nilObj;
    }
    let address = unsafe { (api.smallIntValue)(symbol) } as usize;
    // SAFETY: `symbol:` returns the generated Method-ABI entry point and the
    // unit remains relocated but uninstalled for this direct invocation.
    let entry: Method = unsafe { core::mem::transmute(address) };
    let result = unsafe { entry(api.nilObj, ptr::null(), 0) };
    unsafe { dispose_unit(api, unit) };
    let _ = self_object;
    result
}

unsafe extern "C" fn rust_codegen_compile_output(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    unsafe { rust_codegen_compile_output_impl(pointer, count, false) }
}

unsafe extern "C" fn rust_codegen_compile_output_root(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    unsafe { rust_codegen_compile_output_impl(pointer, count, true) }
}

unsafe fn rust_codegen_compile_output_impl(
    pointer: *const Oop,
    count: usize,
    source_path_is_root: bool,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustCodegen compile:output: received invalid arguments"));
    if values.len() != 2 && values.len() != 3 {
        abort("RustCodegen compile:output: expects source/output or source/output/source path");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    clear_error();
    let source_path = if values.len() == 3 {
        unsafe { string_bytes(api, values[2]) }
    } else {
        None
    };
    if values.len() == 3 && source_path.is_none() {
        note_error(b"RustCodegen source path was not a String");
        return api.nilObj;
    }
    let Some(source) = (unsafe {
        emit_source(
            api,
            values[0],
            true,
            false,
            source_path.as_deref(),
            source_path_is_root,
        )
    }) else {
        note_error(b"RustCodegen could not lower the source");
        return api.nilObj;
    };
    let tcc = runtime.tcc as Oop;
    if tcc.is_null() {
        note_error(b"RustCodegen Tcc is unavailable");
        return api.nilObj;
    }
    let Some(source_bytes) = (unsafe { string_bytes(api, values[0]) }) else {
        note_error(b"RustCodegen source was not a String");
        return api.nilObj;
    };
    let Some(path_bytes) = (unsafe { string_bytes(api, values[1]) }) else {
        note_error(b"RustCodegen output path was not a String");
        return api.nilObj;
    };
    let (identity, requires, native_requires) = match unsafe { artifact_metadata(api, values[0]) } {
        Some((Some((identity, requires)), native_requires)) => {
            (identity, requires, native_requires)
        }
        Some((None, native_requires)) => {
            (artifact_identity(&path_bytes), Vec::new(), native_requires)
        }
        None => (artifact_identity(&path_bytes), Vec::new(), Vec::new()),
    };
    let source_digest = sha256_hex(&source_bytes);
    let revision_digest = unsafe { parse_revision_metadata(api, values[0]) }
        .map(|(_, metadata)| metadata.revision_digest)
        .unwrap_or(source_digest);
    let artifact = artifact_source(
        &source_bytes,
        &identity,
        &source_digest,
        &revision_digest,
        &requires,
        &native_requires,
        &source,
    );
    let c_source = unsafe { (api.mkString)(artifact.as_ptr(), artifact.len()) };
    let kind = unsafe { intern(api, b"dll\0") };
    let result = unsafe {
        send(
            api,
            tcc,
            TCC_COMPILE_OUTPUT_SELECTOR,
            &[c_source, values[1], kind],
        )
    };
    if result == api.nilObj {
        note_error(b"RustCodegen could not write the compiled dylib");
        api.nilObj
    } else {
        values[1]
    }
}

unsafe extern "C" fn rust_codegen_load(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustCodegen load: received invalid arguments"));
    if values.len() != 1 {
        abort("RustCodegen load: expects one path argument");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    clear_error();
    let Some(mut path) = (unsafe { string_bytes(api, values[0]) }) else {
        note_error(b"RustCodegen load path was not a String");
        return api.nilObj;
    };
    path.push(0);
    // RTLD_LAZY | RTLD_GLOBAL: generated artifacts intentionally resolve their
    // frozen birnam_* runtime ABI from the loaded native rung set.
    let handle = unsafe { (api.dlopen)(path.as_ptr().cast(), 9) };
    if handle.is_null() {
        note_error(b"RustCodegen could not open the compiled dylib");
        return api.nilObj;
    }
    let abi_symbol = unsafe { (api.dlsym)(handle, c"__birnam_artifact_abi".as_ptr()) };
    if abi_symbol.is_null() {
        note_error(b"RustCodegen artifact ABI marker is missing");
        return api.nilObj;
    }
    // SAFETY: the artifact contract requires a NUL-terminated immutable ABI
    // marker exported alongside the descriptor.
    let abi = unsafe { CStr::from_ptr(abi_symbol.cast::<c_char>()) };
    if abi.to_bytes() != ARTIFACT_ABI {
        note_error(b"RustCodegen artifact ABI marker is incompatible");
        return api.nilObj;
    }
    let descriptor_symbol =
        unsafe { (api.dlsym)(handle, c"__birnam_artifact_descriptor".as_ptr()) };
    let descriptor_len_symbol =
        unsafe { (api.dlsym)(handle, c"__birnam_artifact_descriptor_len".as_ptr()) };
    if descriptor_symbol.is_null() {
        note_error(b"RustCodegen artifact descriptor is missing");
        return api.nilObj;
    }
    // SAFETY: the artifact output owns an immutable byte array and matching
    // process-width length symbol for the lifetime of the retained handle. Old
    // Zig artifacts only export the descriptor array, so derive its framed
    // length from the frozen magic/payload header when the optional symbol is
    // absent.
    let descriptor_len = if !descriptor_len_symbol.is_null() {
        unsafe { *(descriptor_len_symbol.cast::<usize>()) }
    } else {
        let header_len = ARTIFACT_DESCRIPTOR_MAGIC.len() + 4;
        let header =
            unsafe { core::slice::from_raw_parts(descriptor_symbol.cast::<u8>(), header_len) };
        if &header[..ARTIFACT_DESCRIPTOR_MAGIC.len()] != ARTIFACT_DESCRIPTOR_MAGIC {
            note_error(b"RustCodegen artifact descriptor is missing");
            return api.nilObj;
        }
        let payload_len = u32::from_le_bytes(
            header[ARTIFACT_DESCRIPTOR_MAGIC.len()..header_len]
                .try_into()
                .unwrap(),
        ) as usize;
        let Some(length) = header_len.checked_add(payload_len) else {
            note_error(b"RustCodegen artifact descriptor is incompatible");
            return api.nilObj;
        };
        length
    };
    let descriptor =
        unsafe { core::slice::from_raw_parts(descriptor_symbol.cast::<u8>(), descriptor_len) };
    let Ok(parsed_descriptor) = inspect_artifact(descriptor) else {
        note_error(b"RustCodegen artifact descriptor is incompatible");
        return api.nilObj;
    };
    let artifact_path = &path[..path.len() - 1];
    let artifact_path = Path::new(OsStr::from_bytes(artifact_path));
    let directory = artifact_path.parent().unwrap_or_else(|| Path::new("."));
    if !parsed_descriptor.native_requires.iter().all(|requirement| {
        !requirement.is_empty()
            && !requirement.contains(&0)
            && !requirement.contains(&b'/')
            && fs::metadata(directory.join(OsStr::from_bytes(requirement))).is_ok()
    }) {
        note_error(b"RustCodegen artifact native requirement is unavailable");
        return api.nilObj;
    }
    let init_symbol = unsafe { (api.dlsym)(handle, c"__birnam_unit_init".as_ptr()) };
    let entry_symbol = unsafe { (api.dlsym)(handle, c"__ev".as_ptr()) };
    if init_symbol.is_null() || entry_symbol.is_null() {
        note_error(b"RustCodegen artifact entry points are incomplete");
        return api.nilObj;
    }
    // SAFETY: validated artifact exports use the frozen no-argument initializer
    // and Method ABI entry point.
    let init: unsafe extern "C" fn() = unsafe { core::mem::transmute(init_symbol) };
    let entry: Method = unsafe { core::mem::transmute(entry_symbol) };
    unsafe { init() };
    if let Ok(mut handles) = LIVE_ARTIFACT_HANDLES.lock() {
        handles.push(handle as usize);
    }
    unsafe { entry(api.nilObj, ptr::null(), 0) }
}

unsafe extern "C" fn rust_codegen_last_error(
    _self_object: Oop,
    _pointer: *const Oop,
    _count: usize,
) -> Oop {
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    let Ok(error) = LAST_ERROR.lock() else {
        return api.nilObj;
    };
    let Some(message) = error.as_deref() else {
        return api.nilObj;
    };
    unsafe { (api.mkString)(message.as_ptr(), message.len()) }
}

unsafe fn bring_up(api_pointer: *mut IdApi) {
    if api_pointer.is_null() {
        abort("module_init received a null IdApi");
    }
    let host_api = unsafe { &*api_pointer };
    let runtime = *RUNTIME.get_or_init(|| {
        let reloading = unsafe { (host_api.isExported)(RUST_CODEGEN_NAME.as_ptr().cast()) != 0 };
        let (rust_codegen_vt, rust_codegen) = if reloading {
            (
                unsafe { (host_api.import)(RUST_CODEGEN_VT_NAME.as_ptr().cast()) },
                unsafe { (host_api.import)(RUST_CODEGEN_NAME.as_ptr().cast()) },
            )
        } else {
            let vt = unsafe { (host_api.proto)(host_api.object_vt) };
            let object = unsafe { (host_api.alloc)(vt, 0) };
            (vt, object)
        };
        if rust_codegen_vt.is_null() || rust_codegen.is_null() {
            abort("RustCodegen exports were incomplete");
        }
        let tcc = unsafe { (host_api.import)(TCC_NAME.as_ptr().cast()) };
        if tcc.is_null() {
            abort("RustCodegen requires Tcc to be loaded");
        }
        let globals = unsafe { (host_api.import)(GLOBALS_NAME.as_ptr().cast()) };
        Runtime {
            api: api_pointer as usize,
            rust_codegen_vt: rust_codegen_vt as usize,
            rust_codegen: rust_codegen as usize,
            tcc: tcc as usize,
            globals: globals as usize,
        }
    });
    let api = unsafe { api(&runtime) };
    ROOT_REGISTERED.get_or_init(|| unsafe {
        (api.gcRegisterRootProvider)(mark_live_units);
    });
    unsafe {
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, EMIT_SELECTOR),
            rust_codegen_emit,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, EMBEDDED_FILES_SELECTOR),
            rust_codegen_embedded_files,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, MODULE_METADATA_SELECTOR),
            rust_codegen_module_metadata,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, SOURCE_REVISION_SELECTOR),
            rust_codegen_source_revision,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, RECORD_MODULE_SOURCE_SELECTOR),
            rust_codegen_record_module_source,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, MODULE_SOURCE_FOR_SELECTOR),
            rust_codegen_module_source_for,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, SEAL_INITIAL_GENERATION_SELECTOR),
            rust_codegen_seal_initial_generation,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, INITIAL_GENERATION_PROJECTION_SELECTOR),
            rust_codegen_initial_generation_projection,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, SUCCESSOR_GENERATION_PROJECTION_SELECTOR),
            rust_codegen_successor_generation_projection,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, EVAL_SELECTOR),
            rust_codegen_eval,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, EVAL_FROM_SELECTOR),
            rust_codegen_eval,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, EVAL_PERSISTING_SELECTOR),
            rust_codegen_eval_persisting,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, EVAL_PERSISTING_FROM_SELECTOR),
            rust_codegen_eval_persisting,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, COMPILE_OUTPUT_SELECTOR),
            rust_codegen_compile_output,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, COMPILE_OUTPUT_FROM_SELECTOR),
            rust_codegen_compile_output,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, COMPILE_OUTPUT_ROOT_SELECTOR),
            rust_codegen_compile_output_root,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, LOAD_SELECTOR),
            rust_codegen_load,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, b"lastError\0"),
            rust_codegen_last_error,
        );
        if (api.isExported)(RUST_CODEGEN_NAME.as_ptr().cast()) == 0 {
            let _ = (api.exportObj)(
                RUST_CODEGEN_VT_NAME.as_ptr().cast(),
                runtime.rust_codegen_vt as Oop,
            );
            let _ = (api.exportObj)(
                RUST_CODEGEN_NAME.as_ptr().cast(),
                runtime.rust_codegen as Oop,
            );
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn birnam_codegen_rust_identity() -> *const c_char {
    IDENTITY.as_ptr().cast()
}

#[unsafe(no_mangle)]
///
/// # Safety
///
/// `api` must point to the live process-owned IdApi table for the duration of
/// the module lifetime, as required by the module ABI.
pub unsafe extern "C" fn module_init(api: *mut IdApi, _state: *mut c_void) -> *mut c_void {
    unsafe { bring_up(api) };
    ptr::null_mut()
}

#[cfg(test)]
mod tests {
    use super::{IDENTITY, artifact_source, birnam_codegen_rust_identity, sha256_hex};
    use std::ffi::CStr;

    #[test]
    fn identity_is_a_terminated_static_string() {
        // SAFETY: the export points to immutable crate storage for process life.
        let identity = unsafe { CStr::from_ptr(birnam_codegen_rust_identity()) };
        assert_eq!(identity.to_bytes_with_nul(), IDENTITY);
    }

    #[test]
    fn artifact_digest_and_descriptor_are_deterministic() {
        assert_eq!(
            sha256_hex(b""),
            *b"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"7"),
            *b"7902699be42c8a8e46fbbb4501726517e86b22c56a189f7625a6da49081b2451"
        );
        let output = artifact_source(
            b"7",
            b"rust-codegen",
            &sha256_hex(b"7"),
            &sha256_hex(b"7"),
            &[],
            &[],
            b"void* __ev(void* self, void** args, long n){}",
        );
        assert!(
            output
                .windows(b"const unsigned char __birnam_artifact_descriptor[] = {".len())
                .any(|window| window == b"const unsigned char __birnam_artifact_descriptor[] = {")
        );
    }
}
