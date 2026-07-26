//! Explicit Rust Codegen boundary for the first N6.3 runtime slice.
//!
//! This dylib is intentionally exported under `RustCodegen`; it cannot replace
//! the frozen `Codegen` authority until the complete N6.3–N6.6 matrix passes.
//! The current method materializes the Rust PEG CST, lowers it through the pure
//! Codegen core, and returns deterministic generated C without invoking TCC.

use birnam_abi::{IdApi, MarkFn, Method, Oop, borrowed_oop_args};
use birnam_artifact::candidate::{
    CANDIDATE_ABI, CANDIDATE_MAGIC, CANDIDATE_PLATFORM, Manifest as CandidateManifest,
    Operation as CandidateOperation, inspect as inspect_candidate, inspect_manifest,
};
use birnam_artifact::inspect as inspect_artifact;
use birnam_codegen::{CstNode, Emitter, OwnedCst, TranslationUnit, lower_program};
use birnam_image::{Facts, Field, Fields, Heap, Image, RecordKind, SlotKind};
use core::ffi::{c_char, c_long, c_void};
use core::mem::size_of;
use core::ptr;
use std::ffi::{CStr, OsStr, OsString};
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

const IDENTITY: &[u8] = b"rust-codegen-n6.3-emitter\0";
const RUST_CODEGEN_NAME: &[u8] = b"RustCodegen\0";
const RUST_CODEGEN_VT_NAME: &[u8] = b"RustCodegenVt\0";
const LANG_NAME: &[u8] = b"Lang\0";
const TCC_NAME: &[u8] = b"Tcc\0";
const GLOBALS_NAME: &[u8] = b"Globals\0";
const STORE_NAME: &[u8] = b"Store\0";
const VECTOR_NAME: &[u8] = b"Vector\0";
const VECTOR_VT_NAME: &[u8] = b"VectorVt\0";
const PEG_CST_VT_NAME: &[u8] = b"PegCstVt\0";
const ERROR_VT_NAME: &[u8] = b"ErrorVt\0";
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
const COMPILE_CANDIDATE_SELECTOR: &[u8] = b"compileCandidate:named:output:\0";
const EMBEDDED_FILES_SELECTOR: &[u8] = b"embeddedFilesOf:\0";
const MODULE_METADATA_SELECTOR: &[u8] = b"moduleMetadataOf:\0";
const SOURCE_REVISION_SELECTOR: &[u8] = b"sourceRevisionOf:\0";
const RECORD_MODULE_SOURCE_SELECTOR: &[u8] = b"recordModuleSource:named:\0";
const MODULE_SOURCE_FOR_SELECTOR: &[u8] = b"moduleSourceFor:\0";
const SEAL_INITIAL_GENERATION_SELECTOR: &[u8] = b"sealInitialGeneration\0";
const INITIAL_GENERATION_PROJECTION_SELECTOR: &[u8] = b"initialGenerationProjection\0";
const SUCCESSOR_GENERATION_PROJECTION_SELECTOR: &[u8] =
    b"successorGenerationProjection:replacing:with:unit:\0";
const GENERATION_SOURCES_SELECTOR: &[u8] = b"generationSources\0";
const GENERATION_METHOD_SOURCES_SELECTOR: &[u8] = b"generationMethodSources\0";
const CURRENT_GENERATION_DIGEST_SELECTOR: &[u8] = b"currentGenerationDigest\0";
const COMPILE_IMAGE_CODE_SELECTOR: &[u8] = b"compileImageCode:\0";
const PREFLIGHT_IMAGE_SELECTOR: &[u8] = b"preflightImage:\0";
const MAP_IMAGE_CODE_SELECTOR: &[u8] = b"mapImageCode:\0";
const REPLAY_IMAGE_FACTS_SELECTOR: &[u8] = b"replayImageFacts:\0";
const REPLAY_METHODS_SELECTOR: &[u8] = b"replayMethods\0";
const REPLAY_ALIENS_SELECTOR: &[u8] = b"replayAliens\0";
const DERIVE_IMAGE_GLOBALS_SELECTOR: &[u8] = b"deriveImageGlobals\0";
const DERIVE_IMAGE_METHOD_TARGETS_SELECTOR: &[u8] = b"deriveImageMethodTargets\0";
const INSPECT_CANDIDATE_SELECTOR: &[u8] = b"inspectCandidate:into:\0";
const MAP_CANDIDATE_SELECTOR: &[u8] = b"mapCandidate:\0";
const CANDIDATE_MAPPED_SELECTOR: &[u8] = b"candidateMapped:\0";
const CANDIDATE_CONTENT_PATH_SELECTOR: &[u8] = b"candidateContentPath:\0";
const PREPARE_ACTIVATION_SELECTOR: &[u8] = b"prepareActivation:\0";
const PREPARE_ACTIVATION_PRIOR_SELECTOR: &[u8] = b"prepareActivation:prior:\0";
const PREPARE_ACTIVATION_PLAN_SELECTOR: &[u8] = b"prepareActivation:plan:\0";
const PREPARE_ROLLBACK_SELECTOR: &[u8] = b"prepareRollback:\0";
const PREPARE_ROLLBACK_PLAN_SELECTOR: &[u8] = b"prepareRollback:plan:\0";
const COMMIT_PREPARED_SELECTOR: &[u8] = b"commitPrepared:\0";
const COMMIT_PREPARED_ROLLBACK_SELECTOR: &[u8] = b"commitPreparedRollback:\0";
const DISCARD_PREPARED_SELECTOR: &[u8] = b"discardPrepared:\0";
const RECONCILE_CANDIDATE_SELECTOR: &[u8] = b"reconcileCandidate:\0";
const RECONCILE_ALL_CANDIDATES_SELECTOR: &[u8] = b"reconcileAllCandidates\0";
const BEGIN_IMAGE_RECOVERY_SELECTOR: &[u8] = b"beginImageDispatchRecovery\0";
const END_IMAGE_RECOVERY_SELECTOR: &[u8] = b"endImageDispatchRecovery\0";
const MANAGED_METHOD_CHANGED_SELECTOR: &[u8] = b"managedMethodChanged:on:\0";
const AUTHORIZE_METHOD_MUTATION_SELECTOR: &[u8] = b"authorizeManagedMethodMutation\0";
const ENTER_RUNNING_SELECTOR: &[u8] = b"enterRunning\0";
const BEGIN_QUIESCENCE_SELECTOR: &[u8] = b"beginQuiescence\0";
const END_QUIESCENCE_SELECTOR: &[u8] = b"endQuiescence\0";
const LOAD_SELECTOR: &[u8] = b"load:\0";
const LOAD_HOST_CAPABILITY_SELECTOR: &[u8] = b"loadHostCapability:\0";
const TCC_COMPILE_OUTPUT_SELECTOR: &[u8] = b"compile:output:as:\0";
const BIND_RUNTIME_SELECTOR: &[u8] = b"bindRuntime\0";
const RELOCATE_SELECTOR: &[u8] = b"relocate\0";
const DISPOSABLE_SELECTOR: &[u8] = b"disposable\0";
const SYMBOL_SELECTOR: &[u8] = b"symbol:\0";
const ADD_SYMBOL_SELECTOR: &[u8] = b"addSymbol:value:\0";
const GLOBAL_AT_SELECTOR: &[u8] = b"at:\0";
const GLOBAL_AT_PUT_SELECTOR: &[u8] = b"at:put:\0";
const CURRENT_VALUE_SELECTOR: &[u8] = b"currentValue:attr:\0";
const HAS_VALUE_SELECTOR: &[u8] = b"hasValue:attr:\0";
const CELL_FOR_SELECTOR: &[u8] = b"cellFor:attr:\0";
const SET_SELECTOR: &[u8] = b"set:\0";
const INSTALL_SELECTOR: &[u8] = b"install:as:on:\0";
const ENTRY_SELECTOR: &[u8] = b"__ev\0";
const STORE_ASSERT_SELECTOR: &[u8] = b"assert:attr:value:\0";
const STORE_RAW_CURRENT_SELECTOR: &[u8] = b"rawCurrentValue:attr:\0";
const SET_METHOD_OBSERVER_SELECTOR: &[u8] = b"setMethodMutationObserver:\0";
const SET_METHOD_AUTHORIZER_SELECTOR: &[u8] = b"setMethodMutationAuthorizer:\0";
const CODEGEN_NAME: &[u8] = b"Codegen\0";
const MODULE_NAME: &[u8] = b"Module\0";
const MODULE_IS_LOADED_SELECTOR: &[u8] = b"isLoaded:\0";
const DISPATCH_NAME: &[u8] = b"dispatch\0";
const ACTIVE_GENERATION_NAME: &[u8] = b"activeGeneration\0";
const GLOBALS_ENTITY_NAME: &[u8] = b"__globals__\0";
const GLOBAL_SOURCES_ENTITY_NAME: &[u8] = b"__globalsrc__\0";
const MAX_CST_DEPTH: usize = 4096;
const ARTIFACT_ABI: &[u8] = b"birnam-artifact-generation-v1";
const ARTIFACT_TARGET: &[u8] = b"aarch64-macos";
const ARTIFACT_DESCRIPTOR_MAGIC: &[u8] = b"BIRNAM-ARTIFACT-DESCRIPTOR-V1";
const MAX_EMBEDDED_FILE_BYTES: usize = 16 * 1024 * 1024;
const MAX_IMAGE_PREFLIGHT_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Clone, Copy)]
struct Runtime {
    api: usize,
    rust_codegen_vt: usize,
    rust_codegen: usize,
    tcc: usize,
    globals: usize,
    store: usize,
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();
static LAST_ERROR: Mutex<Option<Vec<u8>>> = Mutex::new(None);
static LIVE_UNITS: Mutex<Vec<usize>> = Mutex::new(Vec::new());
static MODULE_SOURCES: Mutex<Vec<(usize, usize)>> = Mutex::new(Vec::new());
static INITIAL_GENERATION_SEALED: Mutex<bool> = Mutex::new(false);
static LIVE_ARTIFACT_HANDLES: Mutex<Vec<usize>> = Mutex::new(Vec::new());
static CANDIDATES: Mutex<Vec<CandidateState>> = Mutex::new(Vec::new());
static PREPARED: Mutex<Vec<PreparedState>> = Mutex::new(Vec::new());
static CANDIDATE_ROOTS: Mutex<Vec<CandidateRoot>> = Mutex::new(Vec::new());
static ROOT_REGISTERED: OnceLock<()> = OnceLock::new();
static STORE_CALLBACKS_REGISTERED: AtomicBool = AtomicBool::new(false);
static MOP_MUTATION_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
static CANDIDATE_PREPARING: AtomicBool = AtomicBool::new(false);
static HOST_CAPABILITY_LOADING: AtomicBool = AtomicBool::new(false);
static IMAGE_DISPATCH_RECOVERY: AtomicBool = AtomicBool::new(false);
static IMAGE_CODE_LOAD_SEQUENCE: AtomicUsize = AtomicUsize::new(0);
static RUNTIME_PHASE: AtomicU8 = AtomicU8::new(0);
static IMAGE_PREVIOUS_PHASE: AtomicU8 = AtomicU8::new(0);
static ACTIVE_GENERATION: AtomicUsize = AtomicUsize::new(0);

const PHASE_BOOTSTRAPPING: u8 = 0;
const PHASE_RUNNING: u8 = 1;
const PHASE_QUIESCENT: u8 = 2;
const PHASE_IMAGE_RECOVERY: u8 = 3;

unsafe extern "C" {
    fn dlclose(handle: *mut c_void) -> i32;
}

#[allow(dead_code)]
struct CandidateState {
    unit: usize,
    name: usize,
    source: usize,
    author_path: Vec<u8>,
    checksum: [u8; 64],
    source_digest: [u8; 64],
    non_method_digest: [u8; 64],
    method_digest: [u8; 64],
    revision_digest: [u8; 64],
    manifest: Vec<u8>,
    requires: Vec<Vec<u8>>,
    operations: Vec<CandidateOperationState>,
    handle: usize,
    validate: usize,
    prepare: usize,
    activate: usize,
    rollback: usize,
    prepared: usize,
    active: bool,
    predecessor: usize,
}

struct PreparedState {
    token: usize,
    unit: usize,
    rollback: bool,
    prior: usize,
    plan: PublicationPlanState,
}

#[derive(Clone, Copy)]
struct CandidateRoot {
    salt: u64,
    index: isize,
    value: usize,
}

#[derive(Clone, Copy, Default)]
struct PublicationPlanState {
    unit: usize,
    name: usize,
    prior: usize,
    rollback: bool,
    event: usize,
    basis: usize,
    expected_generation: usize,
    successor_generation: usize,
    enforce_lifecycle: bool,
}

#[derive(Clone)]
struct CandidateOperationState {
    target_name: Vec<u8>,
    selector: Vec<u8>,
    install: bool,
    target: usize,
    vtable: usize,
    prior_method: usize,
    prior_present: bool,
    post_method: usize,
    post_present: bool,
}

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
    if let Ok(candidates) = CANDIDATES.lock() {
        for candidate in candidates.iter() {
            // SAFETY: candidate units and metadata are retained by this
            // process-local registry until explicit lifecycle cleanup exists.
            unsafe {
                mark(candidate.unit as Oop);
                mark(candidate.name as Oop);
                mark(candidate.source as Oop);
                if candidate.predecessor != 0 {
                    mark(candidate.predecessor as Oop);
                }
                for operation in &candidate.operations {
                    mark(operation.target as Oop);
                    mark(operation.vtable as Oop);
                }
            }
        }
    }
    if let Ok(prepared) = PREPARED.lock() {
        for state in prepared.iter() {
            // SAFETY: prepared tokens are Object instances retained by this
            // process-local lifecycle registry until consumed or discarded.
            unsafe {
                mark(state.token as Oop);
                if state.plan.event != 0 {
                    mark(state.plan.event as Oop);
                }
                if state.plan.basis != 0 {
                    mark(state.plan.basis as Oop);
                }
                if state.plan.expected_generation != 0 {
                    mark(state.plan.expected_generation as Oop);
                }
                if state.plan.successor_generation != 0 {
                    mark(state.plan.successor_generation as Oop);
                }
            }
        }
    }
    if let Ok(roots) = CANDIDATE_ROOTS.lock() {
        for root in roots.iter() {
            // SAFETY: candidate root slots are generated Oops retained until
            // process teardown; the provider keeps block captures alive across
            // candidate activation and collection.
            unsafe { mark(root.value as Oop) };
        }
    }
    let generation = ACTIVE_GENERATION.load(Ordering::Acquire);
    if generation != 0 {
        // SAFETY: the generation is a projection object retained for the
        // process lifetime once an explicit publication commits.
        unsafe { mark(generation as Oop) };
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
    let value = unsafe { send(api, globals, GLOBAL_AT_SELECTOR, &[key]) };
    if value != api.nilObj && !value.is_null() {
        return value;
    }
    if unsafe { (api.isExported)(name.cast()) } != 0 {
        unsafe { (api.import)(name.cast()) }
    } else {
        api.nilObj
    }
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

#[unsafe(export_name = "birnam_rust_codegen_global")]
/// Rust-owned generated-global lookup used by candidate artifacts.
///
/// # Safety
/// `name` must point to a readable NUL-terminated string for the synchronous call.
pub unsafe extern "C" fn birnam_rust_codegen_global(name: *const c_char) -> Oop {
    // SAFETY: this is the Rust-owned spelling of the same generated-global
    // contract; the candidate emitter supplies a live NUL-terminated name.
    unsafe { birnam_global(name) }
}

#[unsafe(export_name = "birnam_rust_codegen_global_b")]
/// Rust-owned baked global lookup used by candidate artifacts.
///
/// # Safety
/// `symbol` must be a live Symbol Oop owned by the current IdApi floor.
pub unsafe extern "C" fn birnam_rust_codegen_global_b(symbol: Oop) -> Oop {
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    if symbol.is_null() {
        return api.nilObj;
    }
    let name = unsafe { (api.symbolName)(symbol) };
    // SAFETY: symbolName returns the interned NUL-terminated storage for a
    // live Symbol, which remains valid for the synchronous send.
    unsafe { birnam_rust_codegen_global(name.cast()) }
}

#[unsafe(export_name = "birnam_rust_codegen_global_set")]
/// Rust-owned generated-global setter used by candidate artifacts.
///
/// # Safety
/// `name` must point to a readable NUL-terminated string and `value` must be a live Oop.
pub unsafe extern "C" fn birnam_rust_codegen_global_set(name: *const c_char, value: Oop) -> Oop {
    // SAFETY: forwarded generated-global ABI contract.
    unsafe { birnam_global_set(name, value) }
}

#[unsafe(export_name = "birnam_rust_codegen_record_global_src")]
/// Record a generated top-level binding source in the Rust-owned Globals mirror.
///
/// # Safety
/// `name` and `source` must reference readable buffers of the declared form.
pub unsafe extern "C" fn birnam_rust_codegen_record_global_src(
    name: *const c_char,
    source: *const u8,
    length: c_long,
) {
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    if name.is_null() || source.is_null() || length < 0 {
        return;
    }
    let source = unsafe { (api.mkString)(source, length as usize) };
    let entity = unsafe { intern(api, GLOBAL_SOURCES_ENTITY_NAME) };
    let key = unsafe { (api.intern)(name) };
    let _ = unsafe {
        send(
            api,
            runtime.store as Oop,
            STORE_ASSERT_SELECTOR,
            &[entity, key, source],
        )
    };
}

#[unsafe(export_name = "birnam_rust_codegen_record_module_source")]
/// Retain a generated module source pair in the Rust Codegen provider registry.
///
/// # Safety
/// `name` and `source` must reference readable buffers of the declared form.
pub unsafe extern "C" fn birnam_rust_codegen_record_module_source(
    name: *const c_char,
    source: *const u8,
    length: c_long,
) {
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    if name.is_null() || source.is_null() || length < 0 {
        return;
    }
    let source = unsafe { (api.mkString)(source, length as usize) };
    let key = unsafe { (api.intern)(name) };
    if let Ok(mut modules) = MODULE_SOURCES.lock() {
        modules.push((key as usize, source as usize));
    }
}

#[unsafe(export_name = "birnam_rust_codegen_is_error")]
/// Return one when `value` is an Error object in the current floor.
///
/// # Safety
/// `value` must be a pointer-sized ABI carrier supplied by the active IdApi.
pub unsafe extern "C" fn birnam_rust_codegen_is_error(value: Oop) -> c_long {
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    if value.is_null() || value == api.nilObj {
        return 0;
    }
    let error_vt = unsafe { (api.import)(ERROR_VT_NAME.as_ptr().cast()) };
    if error_vt.is_null() || error_vt == api.nilObj {
        return 0;
    }
    (unsafe { (api.vtableOf)(value) } == error_vt) as c_long
}

#[unsafe(export_name = "birnam_rust_codegen_install_method")]
/// Install one generated method through the selected Store MOP.
///
/// # Safety
/// All Oop carriers and the method address must remain valid for this synchronous call.
pub unsafe extern "C" fn birnam_rust_codegen_install_method(
    vtable: Oop,
    selector: Oop,
    method: *mut c_void,
    _source: *const u8,
    _length: c_long,
) -> Oop {
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    if CANDIDATE_PREPARING.load(Ordering::Acquire) {
        return runtime.store as Oop;
    }
    // SAFETY: generated C supplies a Method address in the ABI's pointer
    // carrier position; Store consumes the same raw carrier representation.
    let method: Oop = method;
    unsafe {
        send(
            api,
            runtime.store as Oop,
            b"addMethod:on:with:\0",
            &[selector, vtable, method],
        )
    }
}

#[unsafe(export_name = "birnam_rust_codegen_remove_method")]
/// Remove one generated direct method through the selected Store MOP.
///
/// # Safety
/// `target` and `selector` must be live Oop carriers from the active IdApi.
pub unsafe extern "C" fn birnam_rust_codegen_remove_method(
    target: Oop,
    selector: Oop,
    _source: *const u8,
    _length: c_long,
) -> Oop {
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    let vtable = unsafe { (api.vtableOf)(target) };
    if vtable.is_null() || unsafe { (api.vtableOwner)(vtable) } != target {
        return api.nilObj;
    }
    unsafe {
        send(
            api,
            runtime.store as Oop,
            b"removeMethod:on:\0",
            &[selector, vtable],
        )
    }
}

#[unsafe(export_name = "birnam_rust_codegen_candidate_root_set")]
/// Store one generated candidate capture in the Rust root provider.
///
/// # Safety
/// `value` must be a live Oop or a null ABI carrier.
pub unsafe extern "C" fn birnam_rust_codegen_candidate_root_set(
    salt: u64,
    index: c_long,
    value: Oop,
) -> Oop {
    let value = value as usize;
    let index = index as isize;
    if let Ok(mut roots) = CANDIDATE_ROOTS.lock() {
        if let Some(root) = roots
            .iter_mut()
            .find(|root| root.salt == salt && root.index == index)
        {
            root.value = value;
        } else {
            roots.push(CandidateRoot { salt, index, value });
        }
    }
    value as Oop
}

#[unsafe(export_name = "birnam_rust_codegen_candidate_root_get")]
/// Load one generated candidate capture from the Rust root provider.
///
/// # Safety
/// The call must occur while the Rust Codegen module remains initialized.
pub unsafe extern "C" fn birnam_rust_codegen_candidate_root_get(salt: u64, index: c_long) -> Oop {
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    let index = index as isize;
    CANDIDATE_ROOTS
        .lock()
        .ok()
        .and_then(|roots| {
            roots
                .iter()
                .find(|root| root.salt == salt && root.index == index)
                .copied()
        })
        .map(|root| root.value as Oop)
        .unwrap_or(api.nilObj)
}

#[unsafe(export_name = "birnam_rust_codegen_candidate_prepare_mode")]
/// Toggle candidate preparation mode for generated method-install shims.
///
/// # Safety
/// The caller must use the generated candidate lifecycle protocol.
pub unsafe extern "C" fn birnam_rust_codegen_candidate_prepare_mode(on: c_long) {
    CANDIDATE_PREPARING.store(on != 0, Ordering::Release);
}

#[unsafe(export_name = "birnam_rust_codegen_candidate_validate_binding")]
/// Validate one candidate target and selector binding against the live floor.
///
/// # Safety
/// `target_name` and `selector_name` must be readable NUL-terminated strings.
pub unsafe extern "C" fn birnam_rust_codegen_candidate_validate_binding(
    target_name: *const c_char,
    selector_name: *const c_char,
    arity: c_long,
    _removal: c_long,
) -> Oop {
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    if target_name.is_null() || selector_name.is_null() || arity < 0 {
        return api.falseObj;
    }
    let target_name = unsafe { CStr::from_ptr(target_name.cast()) }.to_bytes();
    let selector_name = unsafe { CStr::from_ptr(selector_name.cast()) }.to_bytes();
    let target = unsafe { global_by_name(api, runtime, target_name) };
    if target.is_null()
        || target == api.nilObj
        || unsafe { (api.vtableOf)(target) }.is_null()
        || unsafe { (api.vtableOwner)((api.vtableOf)(target)) } != target
        || selector_arity(selector_name) != arity as usize
    {
        return api.falseObj;
    }
    api.trueObj
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

fn candidate_manifest(metadata: &RevisionMetadata, source_digest: &[u8; 64]) -> Vec<u8> {
    let mut manifest = Vec::new();
    manifest.extend_from_slice(b"v2|");
    manifest.extend_from_slice(ARTIFACT_ABI);
    manifest.push(b'|');
    manifest.extend_from_slice(&metadata.name);
    manifest.push(b'|');
    manifest.extend_from_slice(source_digest);
    manifest.push(b'|');
    manifest.extend_from_slice(&metadata.non_method_digest);
    manifest.push(b'|');
    manifest.extend_from_slice(&metadata.method_digest);
    manifest.push(b'|');
    manifest.extend_from_slice(&metadata.revision_digest);
    manifest.push(b'|');
    for (index, requirement) in metadata.requires.iter().enumerate() {
        if index != 0 {
            manifest.push(b',');
        }
        manifest.extend_from_slice(requirement);
    }
    manifest.push(b'|');
    for (index, operation) in metadata.operations.iter().enumerate() {
        if index != 0 {
            manifest.push(b';');
        }
        manifest.extend_from_slice(&operation.target);
        manifest.push(b',');
        manifest.extend_from_slice(&operation.selector);
        manifest.push(b',');
        manifest.push(if operation.install { b'I' } else { b'R' });
        manifest.push(b',');
        manifest.extend_from_slice(selector_arity(&operation.selector).to_string().as_bytes());
    }
    manifest
}

fn candidate_generated(
    generated: &[u8],
    manifest: &[u8],
    operations: &[RevisionOperation],
    rust_owned_shims: bool,
) -> Vec<u8> {
    // The mixed stack still owns the candidate callback ABI in Zig Codegen.
    // Keep these declarations local to the candidate instead of exporting a
    // second Rust owner for the same dynamic symbols before the final cutover.
    let generated = if rust_owned_shims {
        rewrite_codegen_shims(generated)
    } else {
        generated.to_vec()
    };
    let prepare_mode = codegen_shim_name(b"birnam_candidate_prepare_mode", rust_owned_shims);
    let validate_binding =
        codegen_shim_name(b"birnam_candidate_validate_binding", rust_owned_shims);
    let mut output = Vec::with_capacity(generated.len() + manifest.len() + 512);
    output.extend_from_slice(b"extern void ");
    output.extend_from_slice(prepare_mode);
    output.extend_from_slice(b"(long on);\n");
    output.extend_from_slice(b"extern void* ");
    output.extend_from_slice(validate_binding);
    output.extend_from_slice(b"(const char*, const char*, long, long);\n");
    output.extend_from_slice(b"const char __birnam_candidate_manifest[] = \"");
    append_c_string(&mut output, manifest);
    output.extend_from_slice(b"\";\n");
    output.extend_from_slice(&generated);
    output.extend_from_slice(
        b"void* __birnam_candidate_validate(void* s, void** a, long n){(void)s;(void)a;(void)n;\n"
            .as_slice(),
    );
    for operation in operations {
        output.extend_from_slice(b"if(");
        output.extend_from_slice(validate_binding);
        output.extend_from_slice(b"(\"");
        append_c_string(&mut output, &operation.target);
        output.extend_from_slice(b"\",\"");
        append_c_string(&mut output, &operation.selector);
        output.extend_from_slice(b"\",");
        output.extend_from_slice(selector_arity(&operation.selector).to_string().as_bytes());
        output.extend_from_slice(b"L,");
        output.extend_from_slice(if operation.install { b"0L" } else { b"1L" });
        output.extend_from_slice(b")!=birnam_true())return birnam_false();\n");
    }
    output.extend_from_slice(b"return birnam_true();}\n");
    output.extend_from_slice(
        format!(
            "void* __birnam_candidate_prepare(void* s, void** a, long n){{{}(1L);(void)__ev(s,a,n);{}(0L);return birnam_true();}}\n",
            String::from_utf8_lossy(prepare_mode),
            String::from_utf8_lossy(prepare_mode),
        )
        .as_bytes(),
    );
    output.extend_from_slice(
        b"void* __birnam_candidate_activate(void* s, void** a, long n){return __ev(s,a,n);}\n"
            .as_slice(),
    );
    output.extend_from_slice(
        b"void* __birnam_candidate_rollback(void* s, void** a, long n){(void)s;(void)a;(void)n;return birnam_nil();}\n"
            .as_slice(),
    );
    output
}

fn codegen_shim_name(name: &[u8], rust_owned: bool) -> &'static [u8] {
    if !rust_owned {
        return match name {
            b"birnam_candidate_prepare_mode" => b"birnam_candidate_prepare_mode",
            b"birnam_candidate_validate_binding" => b"birnam_candidate_validate_binding",
            _ => unreachable!("unknown Codegen shim"),
        };
    }
    match name {
        b"birnam_global" => b"birnam_rust_codegen_global",
        b"birnam_global_b" => b"birnam_rust_codegen_global_b",
        b"birnam_global_set" => b"birnam_rust_codegen_global_set",
        b"birnam_record_global_src" => b"birnam_rust_codegen_record_global_src",
        b"birnam_record_module_source" => b"birnam_rust_codegen_record_module_source",
        b"birnam_is_error" => b"birnam_rust_codegen_is_error",
        b"birnam_install_method" => b"birnam_rust_codegen_install_method",
        b"birnam_remove_method" => b"birnam_rust_codegen_remove_method",
        b"birnam_candidate_root_set" => b"birnam_rust_codegen_candidate_root_set",
        b"birnam_candidate_root_get" => b"birnam_rust_codegen_candidate_root_get",
        b"birnam_candidate_prepare_mode" => b"birnam_rust_codegen_candidate_prepare_mode",
        b"birnam_candidate_validate_binding" => b"birnam_rust_codegen_candidate_validate_binding",
        _ => unreachable!("unknown Codegen shim"),
    }
}

fn rewrite_codegen_shims(generated: &[u8]) -> Vec<u8> {
    let mut rewritten = generated.to_vec();
    const NAMES: [&[u8]; 12] = [
        b"birnam_candidate_validate_binding",
        b"birnam_candidate_prepare_mode",
        b"birnam_candidate_root_set",
        b"birnam_candidate_root_get",
        b"birnam_record_module_source",
        b"birnam_record_global_src",
        b"birnam_install_method",
        b"birnam_remove_method",
        b"birnam_global_set",
        b"birnam_global_b",
        b"birnam_global",
        b"birnam_is_error",
    ];
    for name in NAMES {
        rewritten = replace_bytes(&rewritten, name, codegen_shim_name(name, true));
    }
    rewritten
}

fn replace_bytes(input: &[u8], old: &[u8], new: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len());
    let mut cursor = 0;
    while let Some(relative) = input[cursor..]
        .windows(old.len())
        .position(|window| window == old)
    {
        let start = cursor + relative;
        output.extend_from_slice(&input[cursor..start]);
        output.extend_from_slice(new);
        cursor = start + old.len();
    }
    output.extend_from_slice(&input[cursor..]);
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

unsafe fn intern_bytes(api: &IdApi, name: &[u8]) -> Oop {
    let mut terminated = Vec::with_capacity(name.len() + 1);
    terminated.extend_from_slice(name);
    terminated.push(0);
    unsafe { (api.intern)(terminated.as_ptr().cast::<c_char>()) }
}

unsafe fn global_by_name(api: &IdApi, runtime: &Runtime, name: &[u8]) -> Oop {
    let symbol = unsafe { intern_bytes(api, name) };
    let globals = runtime.globals as Oop;
    if !globals.is_null() {
        let value = unsafe { send(api, globals, GLOBAL_AT_SELECTOR, &[symbol]) };
        if value != api.nilObj && !value.is_null() {
            return value;
        }
    }
    let mut terminated = Vec::with_capacity(name.len() + 1);
    terminated.extend_from_slice(name);
    terminated.push(0);
    if unsafe { (api.isExported)(terminated.as_ptr().cast()) } != 0 {
        unsafe { (api.import)(terminated.as_ptr().cast()) }
    } else {
        api.nilObj
    }
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
    #[allow(dead_code)]
    operations: Vec<RevisionOperation>,
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
    for operation in &operations {
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
        operations,
        non_method_digest,
        method_digest,
        revision_digest,
    })
}

#[allow(dead_code)]
fn candidate_manifest_matches(
    manifest: CandidateManifest<'_>,
    source: &[u8],
    metadata: &RevisionMetadata,
) -> bool {
    if manifest.name != metadata.name
        || manifest.source_digest != sha256_hex(source)
        || manifest.non_method_digest != metadata.non_method_digest
        || manifest.method_digest != metadata.method_digest
        || manifest.revision_digest != metadata.revision_digest
    {
        return false;
    }
    let mut dependencies = Vec::new();
    for (index, dependency) in metadata.requires.iter().enumerate() {
        if index != 0 {
            dependencies.push(b',');
        }
        dependencies.extend_from_slice(dependency);
    }
    if manifest.dependencies != dependencies {
        return false;
    }
    if manifest.operation_count() != metadata.operations.len() {
        return false;
    }
    for (index, operation) in metadata.operations.iter().enumerate() {
        let Some(candidate) = manifest.operation(index) else {
            return false;
        };
        if !candidate_operation_matches(candidate, operation) {
            return false;
        }
    }
    true
}

#[allow(dead_code)]
fn candidate_operation_matches(
    candidate: CandidateOperation<'_>,
    operation: &RevisionOperation,
) -> bool {
    candidate.target == operation.target
        && candidate.selector == operation.selector
        && candidate.install == operation.install
        && candidate.arity == selector_arity(&operation.selector) as u32
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
    let Some((source, metadata)) = (unsafe { parse_revision_metadata(api, values[0]) }) else {
        note_error(b"RustCodegen module source is not indexable");
        return api.nilObj;
    };
    if metadata.name != name {
        note_error(b"RustCodegen module source identity does not match its name");
        return api.nilObj;
    }
    if let Ok(sealed) = INITIAL_GENERATION_SEALED.lock()
        && *sealed
    {
        note_error(b"RustCodegen module introduction is not available");
        return api.nilObj;
    }
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

unsafe fn durable_current_generation(api: &IdApi) -> Oop {
    let runtime = runtime();
    let store = runtime.store as Oop;
    if store.is_null() || store == api.nilObj {
        return api.nilObj;
    }
    let dispatch = unsafe { intern(api, DISPATCH_NAME) };
    let attribute = unsafe { intern(api, ACTIVE_GENERATION_NAME) };
    unsafe { send(api, store, CURRENT_VALUE_SELECTOR, &[dispatch, attribute]) }
}

unsafe fn durable_generation_is_canonical(api: &IdApi, generation: Oop) -> bool {
    if generation.is_null() || generation == api.nilObj {
        return false;
    }
    let runtime = runtime();
    let store = runtime.store as Oop;
    if store.is_null() || store == api.nilObj {
        return false;
    }
    let source_map = unsafe {
        send(
            api,
            store,
            CURRENT_VALUE_SELECTOR,
            &[generation, intern(api, b"sourceMap\0")],
        )
    };
    let recorded = unsafe {
        send(
            api,
            store,
            CURRENT_VALUE_SELECTOR,
            &[generation, intern(api, b"baseDigest\0")],
        )
    };
    let Some(actual) = (unsafe { generation_digest(api, source_map) }) else {
        return false;
    };
    unsafe { string_bytes(api, recorded) }
        .is_some_and(|digest| digest.as_slice() == actual.as_slice())
}

unsafe fn image_generation_matches_durable(api: &IdApi) -> bool {
    let durable = unsafe { durable_current_generation(api) };
    if durable == api.nilObj {
        return true;
    }
    let runtime = runtime();
    let projection = unsafe {
        send(
            api,
            runtime.rust_codegen as Oop,
            INITIAL_GENERATION_PROJECTION_SELECTOR,
            &[],
        )
    };
    if projection.is_null() || projection == api.nilObj {
        return false;
    }
    let projection_digest = unsafe { vector_item(api, projection, 2) };
    let store = runtime.store as Oop;
    let durable_digest = unsafe {
        send(
            api,
            store,
            CURRENT_VALUE_SELECTOR,
            &[durable, intern(api, b"baseDigest\0")],
        )
    };
    let projection_bytes = unsafe { string_bytes(api, projection_digest) };
    let durable_bytes = unsafe { string_bytes(api, durable_digest) };
    projection_bytes == durable_bytes
}

/// A portable image is authorized by the generation digest embedded in its
/// authority record. Once Store has a durable active generation, stale images
/// must fail before code mapping or fact mutation; data-only runtimes without
/// that authority retain the lower-level format behavior.
unsafe fn image_authority_matches_durable(api: &IdApi, authority: &[u8; 64]) -> bool {
    let durable = unsafe { durable_current_generation(api) };
    if durable.is_null() || durable == api.nilObj {
        return true;
    }
    if !unsafe { durable_generation_is_canonical(api, durable) } {
        return false;
    }
    let store = runtime().store as Oop;
    let recorded = unsafe {
        send(
            api,
            store,
            CURRENT_VALUE_SELECTOR,
            &[durable, intern(api, b"baseDigest\0")],
        )
    };
    unsafe { string_bytes(api, recorded) }.is_some_and(|digest| digest == authority)
}

unsafe fn durable_generation_entry(api: &IdApi, generation: Oop, name: Oop) -> Option<Oop> {
    let store = runtime().store as Oop;
    let source_map = unsafe {
        send(
            api,
            store,
            CURRENT_VALUE_SELECTOR,
            &[generation, intern(api, b"sourceMap\0")],
        )
    };
    let vector_vt = unsafe { (api.import)(VECTOR_VT_NAME.as_ptr().cast()) };
    let count = unsafe { vector_length(api, source_map, vector_vt) }?;
    for index in 0..count {
        let entry = unsafe { vector_item(api, source_map, index) };
        if unsafe { vector_length(api, entry, vector_vt) } != Some(3) {
            return None;
        }
        if unsafe { vector_item(api, entry, 0) } == name {
            return Some(entry);
        }
    }
    None
}

unsafe fn durable_generation_plan_is_valid(
    api: &IdApi,
    unit: Oop,
    name: Oop,
    prior: Oop,
    expected: Oop,
    successor: Oop,
    action: &[u8],
) -> bool {
    if !unsafe { durable_generation_is_canonical(api, expected) }
        || !unsafe { durable_generation_is_canonical(api, successor) }
    {
        return false;
    }
    let Some(expected_entry) = (unsafe { durable_generation_entry(api, expected, name) }) else {
        return false;
    };
    let Some(successor_entry) = (unsafe { durable_generation_entry(api, successor, name) }) else {
        return false;
    };
    let store = runtime().store as Oop;
    let expected_predecessor = unsafe {
        send(
            api,
            store,
            CURRENT_VALUE_SELECTOR,
            &[expected, intern(api, b"predecessor\0")],
        )
    };
    let expected_delta = unsafe {
        send(
            api,
            store,
            CURRENT_VALUE_SELECTOR,
            &[expected, intern(api, b"deltaUnit\0")],
        )
    };
    let successor_predecessor = unsafe {
        send(
            api,
            store,
            CURRENT_VALUE_SELECTOR,
            &[successor, intern(api, b"predecessor\0")],
        )
    };
    let successor_delta = unsafe {
        send(
            api,
            store,
            CURRENT_VALUE_SELECTOR,
            &[successor, intern(api, b"deltaUnit\0")],
        )
    };
    if action == b"activate\0" {
        (unsafe { vector_item(api, expected_entry, 2) }) == prior
            && (unsafe { vector_item(api, successor_entry, 2) }) == unit
            && successor_predecessor == expected
            && successor_delta == unit
    } else {
        (unsafe { vector_item(api, expected_entry, 2) }) == unit
            && expected_predecessor == successor
            && expected_delta == unit
            && (unsafe { vector_item(api, successor_entry, 2) }) == prior
    }
}

unsafe fn set_durable_current_generation(api: &IdApi, generation: Oop) -> bool {
    let runtime = runtime();
    let store = runtime.store as Oop;
    if store.is_null() || store == api.nilObj {
        return false;
    }
    let cell = unsafe {
        send(
            api,
            store,
            CELL_FOR_SELECTOR,
            &[
                intern(api, DISPATCH_NAME),
                intern(api, ACTIVE_GENERATION_NAME),
            ],
        )
    };
    if cell.is_null() || cell == api.nilObj {
        return false;
    }
    unsafe { send(api, cell, SET_SELECTOR, &[generation]) != api.nilObj }
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

unsafe fn visit_generation_source(
    api: &IdApi,
    modules: &[(usize, usize)],
    names: &[Vec<u8>],
    index: usize,
    state: &mut [u8],
    ordered: &mut Vec<Oop>,
) -> bool {
    if state[index] == 2 {
        return true;
    }
    if state[index] == 1 {
        return false;
    }
    state[index] = 1;
    let source = modules[index].1 as Oop;
    let Some((_source_bytes, metadata)) = (unsafe { parse_revision_metadata(api, source) }) else {
        return false;
    };
    for requirement in metadata.requires {
        if let Some(dependency) = names.iter().position(|name| *name == requirement)
            && !unsafe { visit_generation_source(api, modules, names, dependency, state, ordered) }
        {
            return false;
        }
    }
    state[index] = 2;
    ordered.push(source);
    true
}

unsafe fn ordered_generation_sources(api: &IdApi) -> Option<Vec<Oop>> {
    let Ok(module_guard) = MODULE_SOURCES.lock() else {
        return None;
    };
    let mut modules = module_guard.clone();
    drop(module_guard);
    modules.sort_by(|left, right| {
        let left_name = unsafe { symbol_bytes(api, left.0 as Oop) }.unwrap_or_default();
        let right_name = unsafe { symbol_bytes(api, right.0 as Oop) }.unwrap_or_default();
        left_name.cmp(&right_name)
    });
    let names = modules
        .iter()
        .map(|(name, _)| unsafe { symbol_bytes(api, *name as Oop) })
        .collect::<Option<Vec<_>>>()?;
    let mut state = vec![0; modules.len()];
    let mut ordered = Vec::with_capacity(modules.len());
    for index in 0..modules.len() {
        if !unsafe {
            visit_generation_source(api, &modules, &names, index, &mut state, &mut ordered)
        } {
            return None;
        }
    }
    Some(ordered)
}

unsafe extern "C" fn rust_codegen_generation_sources(
    _self_object: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    if count != 0 {
        abort("RustCodegen generationSources: expects no arguments");
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
    let vector = unsafe { (api.import)(VECTOR_NAME.as_ptr().cast()) };
    let Some(sources) = (unsafe { ordered_generation_sources(api) }) else {
        note_error(b"RustCodegen generation source registry is malformed");
        return api.nilObj;
    };
    if vector.is_null() || vector == api.nilObj {
        note_error(b"RustCodegen Vector is unavailable");
        return api.nilObj;
    }
    unsafe { make_vector(api, vector, &sources) }
}

unsafe extern "C" fn rust_codegen_generation_method_sources(
    _self_object: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    if count != 0 {
        abort("RustCodegen generationMethodSources: expects no arguments");
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
    let vector = unsafe { (api.import)(VECTOR_NAME.as_ptr().cast()) };
    let Some(sources) = (unsafe { ordered_generation_sources(api) }) else {
        note_error(b"RustCodegen generation source registry is malformed");
        return api.nilObj;
    };
    if vector.is_null() || vector == api.nilObj {
        note_error(b"RustCodegen Vector is unavailable");
        return api.nilObj;
    }
    let mut methods = Vec::new();
    for source in sources {
        let Some((_source_bytes, metadata)) = (unsafe { parse_revision_metadata(api, source) })
        else {
            note_error(b"RustCodegen generation source is not indexable");
            return api.nilObj;
        };
        methods.extend(
            metadata
                .operations
                .into_iter()
                .filter(|operation| operation.install)
                .map(|operation| operation.source),
        );
    }
    let method_objects = methods
        .iter()
        .map(|source| unsafe { (api.mkString)(source.as_ptr(), source.len()) })
        .collect::<Vec<_>>();
    unsafe { make_vector(api, vector, &method_objects) }
}

unsafe extern "C" fn rust_codegen_current_generation_digest(
    _self_object: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    if count != 0 {
        abort("RustCodegen currentGenerationDigest: expects no arguments");
    }
    let api = unsafe { api(runtime()) };
    let durable = unsafe { durable_current_generation(api) };
    if durable != api.nilObj {
        if !unsafe { durable_generation_is_canonical(api, durable) } {
            note_error(b"RustCodegen durable generation is not canonical");
            return api.nilObj;
        }
        return unsafe {
            send(
                api,
                runtime().store as Oop,
                CURRENT_VALUE_SELECTOR,
                &[durable, intern(api, b"baseDigest\0")],
            )
        };
    }
    let generation = ACTIVE_GENERATION.load(Ordering::Acquire);
    if generation == 0 {
        return api.nilObj;
    }
    unsafe { vector_item(api, generation as Oop, 2) }
}

unsafe extern "C" fn rust_codegen_compile_image_code(
    self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustCodegen compileImageCode: received invalid arguments"));
    if values.len() != 1 {
        abort("RustCodegen compileImageCode: expects one output path");
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
    let Some(sources) = (unsafe { ordered_generation_sources(api) }) else {
        note_error(b"RustCodegen image source ordering failed");
        return api.nilObj;
    };
    if sources.is_empty() {
        note_error(b"RustCodegen image has no generation sources");
        return api.nilObj;
    }
    if !unsafe { image_generation_matches_durable(api) } {
        note_error(b"RustCodegen image generation is stale");
        return api.nilObj;
    }
    let mut program = Vec::new();
    for source in sources {
        let Some(bytes) = (unsafe { string_bytes(api, source) }) else {
            note_error(b"RustCodegen image source is not a String");
            return api.nilObj;
        };
        program.extend_from_slice(&bytes);
        program.extend_from_slice(b";\n");
    }
    let source_object = unsafe { (api.mkString)(program.as_ptr(), program.len()) };
    let arguments = [source_object, values[0]];
    unsafe { rust_codegen_compile_output(self_object, arguments.as_ptr(), arguments.len()) }
}

/// Validate a portable v4 image without changing the live runtime.
///
/// This is deliberately a framing boundary: the pure image crate checks the
/// header, record envelopes, single authority, and authority/code binding, but
/// this method does not map code, materialize heap objects, swizzle references,
/// or import Store facts. Those operations remain owned by the image loader
/// until their N6.6 authority and recovery gates are complete.
unsafe extern "C" fn rust_codegen_preflight_image(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustCodegen preflightImage: received invalid arguments"));
    if values.len() != 1 {
        abort("RustCodegen preflightImage: expects one image path");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    clear_error();
    let Some(path_bytes) = (unsafe { string_bytes(api, values[0]) }) else {
        note_error(b"RustCodegen image path was not a String");
        return api.nilObj;
    };
    let path = Path::new(OsStr::from_bytes(&path_bytes));
    let Ok(metadata) = fs::metadata(path) else {
        note_error(b"RustCodegen image preflight could not stat the image");
        return api.nilObj;
    };
    if metadata.len() > MAX_IMAGE_PREFLIGHT_BYTES {
        note_error(b"RustCodegen image preflight image is too large");
        return api.nilObj;
    }
    let Ok(bytes) = fs::read(path) else {
        note_error(b"RustCodegen image preflight could not read the image");
        return api.nilObj;
    };
    let Ok(image) = Image::parse(&bytes) else {
        note_error(b"RustCodegen image preflight framing is invalid");
        return api.nilObj;
    };
    if image.validate_facts().is_err() {
        note_error(b"RustCodegen image preflight facts are invalid");
        return api.nilObj;
    }
    if !unsafe { image_authority_matches_durable(api, image.authority.digest) } {
        note_error(b"RustCodegen image preflight authority is stale");
        return api.nilObj;
    }
    api.trueObj
}

/// Stage the checksum-verified v4 code unit through the existing Rust artifact
/// loader. This is deliberately an image-recovery-only handoff: it maps the
/// captured dylib and runs its entry point, but does not materialize heap
/// records or apply facts. Those operations remain owned by later recovery
/// slices.
unsafe extern "C" fn rust_codegen_map_image_code(
    self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustCodegen mapImageCode: received invalid arguments"));
    if values.len() != 1 {
        abort("RustCodegen mapImageCode: expects one image path");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    clear_error();
    if !IMAGE_DISPATCH_RECOVERY.load(Ordering::Acquire) {
        note_error(b"RustCodegen image code mapping requires image recovery");
        return api.nilObj;
    }
    let Some(path_bytes) = (unsafe { string_bytes(api, values[0]) }) else {
        note_error(b"RustCodegen image path was not a String");
        return api.nilObj;
    };
    let path = Path::new(OsStr::from_bytes(&path_bytes));
    let Ok(bytes) = fs::read(path) else {
        note_error(b"RustCodegen image code mapping could not read the image");
        return api.nilObj;
    };
    let Ok(image) = Image::parse(&bytes) else {
        note_error(b"RustCodegen image code mapping framing is invalid");
        return api.nilObj;
    };
    if image.validate_facts().is_err() {
        note_error(b"RustCodegen image code mapping facts are invalid");
        return api.nilObj;
    }
    if !unsafe { image_authority_matches_durable(api, image.authority.digest) } {
        note_error(b"RustCodegen image code mapping authority is stale");
        return api.nilObj;
    }
    let Some(code) = image.code() else {
        note_error(b"RustCodegen image has no code unit");
        return api.nilObj;
    };

    let sequence = IMAGE_CODE_LOAD_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let mut staged_path = std::env::temp_dir().into_os_string().into_vec();
    staged_path.extend_from_slice(b"birnam-image-code-");
    staged_path.extend_from_slice(std::process::id().to_string().as_bytes());
    staged_path.push(b'-');
    staged_path.extend_from_slice(sequence.to_string().as_bytes());
    staged_path.extend_from_slice(b".dylib");
    let staged_path_ref = Path::new(OsStr::from_bytes(&staged_path));
    let Ok(mut staged) = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(staged_path_ref)
    else {
        note_error(b"RustCodegen image code mapping could not stage the unit");
        return api.nilObj;
    };
    if staged.write_all(code.unit).is_err() {
        let _ = fs::remove_file(staged_path_ref);
        note_error(b"RustCodegen image code mapping could not stage the unit");
        return api.nilObj;
    }
    drop(staged);
    let staged_object = unsafe { (api.mkString)(staged_path.as_ptr(), staged_path.len()) };
    let result = unsafe { rust_codegen_load(self_object, &staged_object, 1) };
    let _ = fs::remove_file(staged_path_ref);
    result
}

unsafe fn materialize_image_field_with_relocations(
    api: &IdApi,
    field: Field<'_>,
    relocations: Option<&[Oop]>,
) -> Result<Oop, ()> {
    match field {
        Field::Nil => Ok(api.nilObj),
        Field::True => Ok(api.trueObj),
        Field::False => Ok(api.falseObj),
        Field::Integer(value) => Ok(unsafe { (api.mkSmallInt)(value) }),
        Field::FloatBits(bits) => Ok(unsafe { (api.mkFloat)(f64::from_bits(bits)) }),
        Field::Symbol(bytes) => Ok(unsafe { intern_bytes(api, bytes) }),
        Field::String(bytes) => Ok(unsafe { (api.mkString)(bytes.as_ptr(), bytes.len()) }),
        Field::NullOop => Ok(ptr::null_mut()),
        Field::ExportRef(bytes) => {
            let mut terminated = Vec::with_capacity(bytes.len() + 1);
            terminated.extend_from_slice(bytes);
            terminated.push(0);
            let value = unsafe { (api.import)(terminated.as_ptr().cast()) };
            if value.is_null() { Err(()) } else { Ok(value) }
        }
        Field::HeapRef(index) => relocations
            .and_then(|entries| entries.get(index as usize).copied())
            .filter(|value| !value.is_null())
            .ok_or(()),
    }
}

struct PendingHeapObject<'a> {
    object: Oop,
    layout: Vec<SlotKind>,
    encoded_slots: &'a [u8],
}

unsafe fn materialize_image_heaps(api: &IdApi, image: Image<'_>) -> Result<Vec<Oop>, ()> {
    let mut relocations = Vec::new();
    let mut pending = Vec::new();
    let mut records = image.records();
    loop {
        let record = records.next_record().map_err(|_| ())?;
        let Some(record) = record else { break };
        if record.kind != RecordKind::Heap {
            continue;
        }
        let mut heap = Heap::parse(record.payload).map_err(|_| ())?;
        while let Some(header) = heap.peek_header().map_err(|_| ())? {
            let vtable_name = header.vtable_name;
            let mut terminated = Vec::with_capacity(vtable_name.len() + 1);
            terminated.extend_from_slice(vtable_name);
            terminated.push(0);
            let vtable = unsafe { (api.import)(terminated.as_ptr().cast()) };
            if vtable.is_null() || vtable == api.nilObj {
                return Err(());
            }
            let word_count = usize::try_from(header.word_count).map_err(|_| ())?;
            let payload_size = word_count.checked_mul(size_of::<usize>()).ok_or(())?;
            let mut layout = Vec::with_capacity(word_count);
            for word in 0..word_count {
                let kind = match unsafe { (api.slotKind)(vtable, word) } {
                    birnam_abi::SLOT_OOP => SlotKind::Oop,
                    birnam_abi::SLOT_CODE => SlotKind::Code,
                    _ => SlotKind::RawWord,
                };
                layout.push(kind);
            }
            let object = unsafe { (api.alloc)(vtable, payload_size) };
            if object.is_null() {
                return Err(());
            }
            let encoded_slots = heap
                .next_object(&layout)
                .map_err(|_| ())?
                .ok_or(())?
                .encoded_slots;
            relocations.push(object);
            pending.push(PendingHeapObject {
                object,
                layout,
                encoded_slots,
            });
        }
    }

    for pending in pending {
        let mut cursor = 0usize;
        for (word, kind) in pending.layout.iter().copied().enumerate() {
            match kind {
                SlotKind::Oop => {
                    let suffix = pending.encoded_slots.get(cursor..).ok_or(())?;
                    let mut fields = Fields::new(suffix);
                    let field = fields.next_field().map_err(|_| ())?.ok_or(())?;
                    cursor = cursor.checked_add(fields.consumed()).ok_or(())?;
                    let value = unsafe {
                        materialize_image_field_with_relocations(api, field, Some(&relocations))?
                    };
                    unsafe {
                        (pending.object as *mut usize)
                            .add(word)
                            .write(value as usize);
                    }
                }
                SlotKind::RawWord => {
                    let end = cursor.checked_add(size_of::<usize>()).ok_or(())?;
                    let bytes = pending.encoded_slots.get(cursor..end).ok_or(())?;
                    let raw = usize::from_le_bytes(bytes.try_into().map_err(|_| ())?);
                    unsafe {
                        (pending.object as *mut usize).add(word).write(raw);
                    }
                    cursor = end;
                }
                SlotKind::Code => unsafe {
                    // Code slots are intentionally elided from v4 heap
                    // payloads; mapped-unit recovery fills them later. Keep
                    // the freshly allocated object deterministic and match
                    // the Zig loader's zeroed code-slot contract.
                    (pending.object as *mut usize).add(word).write(0);
                },
            }
        }
        if cursor != pending.encoded_slots.len() {
            return Err(());
        }
    }
    Ok(relocations)
}

/// Replay v4 facts through the Rust Store image ABI after all heap objects and
/// field values have been decoded successfully. Heap allocation and swizzling
/// happen before Store mutation so malformed relocation data cannot masquerade
/// as successful recovery or leave a partial fact load behind.
unsafe extern "C" fn rust_codegen_replay_image_facts(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustCodegen replayImageFacts: received invalid arguments"));
    if values.len() != 1 {
        abort("RustCodegen replayImageFacts: expects one image path");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    clear_error();
    if !IMAGE_DISPATCH_RECOVERY.load(Ordering::Acquire)
        || RUNTIME_PHASE.load(Ordering::Acquire) != PHASE_IMAGE_RECOVERY
    {
        note_error(b"RustCodegen image facts replay requires image recovery");
        return api.nilObj;
    }
    let Some(path_bytes) = (unsafe { string_bytes(api, values[0]) }) else {
        note_error(b"RustCodegen image path was not a String");
        return api.nilObj;
    };
    let path = Path::new(OsStr::from_bytes(&path_bytes));
    let Ok(bytes) = fs::read(path) else {
        note_error(b"RustCodegen image facts could not read the image");
        return api.nilObj;
    };
    let Ok(image) = Image::parse(&bytes) else {
        note_error(b"RustCodegen image facts framing is invalid");
        return api.nilObj;
    };
    if image.validate_facts().is_err() {
        note_error(b"RustCodegen image facts framing is invalid");
        return api.nilObj;
    }
    if !unsafe { image_authority_matches_durable(api, image.authority.digest) } {
        note_error(b"RustCodegen image facts authority is stale");
        return api.nilObj;
    }

    unsafe { (api.gcSetAuto)(0) };
    struct AutoGc<'a>(&'a IdApi);
    impl Drop for AutoGc<'_> {
        fn drop(&mut self) {
            unsafe { (self.0.gcSetAuto)(1) };
        }
    }
    let _auto_gc = AutoGc(api);
    let relocations = match unsafe { materialize_image_heaps(api, image) } {
        Ok(relocations) => relocations,
        Err(_) => {
            note_error(b"RustCodegen image heap materialization failed");
            return api.nilObj;
        }
    };

    let mut pending = Vec::new();
    let mut has_facts = false;
    let mut max_basis = 0i64;
    let mut records = image.records();
    loop {
        let record = match records.next_record() {
            Ok(Some(record)) => record,
            Ok(None) => break,
            Err(_) => {
                note_error(b"RustCodegen image facts framing is invalid");
                return api.nilObj;
            }
        };
        if record.kind != RecordKind::Facts {
            continue;
        }
        has_facts = true;
        let Ok(mut facts) = Facts::parse(record.payload) else {
            note_error(b"RustCodegen image facts framing is invalid");
            return api.nilObj;
        };
        // v4 serializes all fixed-width integers little-endian, independently
        // of the host that is replaying the image.
        max_basis = max_basis.max(i64::from_le_bytes(facts.basis.to_le_bytes()));
        loop {
            let Ok(next) = facts.next_fact() else {
                note_error(b"RustCodegen image facts framing is invalid");
                return api.nilObj;
            };
            let Some(fact) = next else { break };
            if fact.op > 1 {
                note_error(b"RustCodegen image facts operation is invalid");
                return api.nilObj;
            }
            let Ok(entity) = (unsafe {
                materialize_image_field_with_relocations(api, fact.entity, Some(&relocations))
            }) else {
                note_error(b"RustCodegen image facts require materialized heap references");
                return api.nilObj;
            };
            let Ok(attribute) = (unsafe {
                materialize_image_field_with_relocations(api, fact.attribute, Some(&relocations))
            }) else {
                note_error(b"RustCodegen image facts require materialized heap references");
                return api.nilObj;
            };
            let Ok(value) = (unsafe {
                materialize_image_field_with_relocations(api, fact.value, Some(&relocations))
            }) else {
                note_error(b"RustCodegen image facts require materialized heap references");
                return api.nilObj;
            };
            pending.push((entity, attribute, value, fact.tx, fact.op));
        }
    }
    // Heap-only images are valid framing inputs, but they have no Store
    // timeline to replace. Match the Zig loader's `facts_began` guard and
    // leave the existing Store untouched rather than clearing it through an
    // empty image-load transaction.
    if !has_facts {
        return unsafe { (api.mkSmallInt)(0) };
    }
    let store = runtime.store as Oop;
    if store.is_null() || store == api.nilObj {
        note_error(b"RustCodegen image facts Store is unavailable");
        return api.nilObj;
    }
    if unsafe { send0(api, store, b"imageLoadBegin\0") } == api.nilObj {
        note_error(b"RustCodegen image facts Store load could not begin");
        return api.nilObj;
    }
    for (entity, attribute, value, tx, op) in pending.iter().copied() {
        let arguments = [
            entity,
            attribute,
            value,
            unsafe { (api.mkSmallInt)(i64::from_le_bytes(tx.to_le_bytes())) },
            unsafe { (api.mkSmallInt)(i64::from(op)) },
        ];
        if unsafe { send(api, store, b"imageAddFact:attr:value:tx:op:\0", &arguments) }
            == api.nilObj
        {
            note_error(b"RustCodegen image facts Store append failed");
            return api.nilObj;
        }
    }
    let end_arguments = [unsafe { (api.mkSmallInt)(max_basis) }, api.falseObj];
    if unsafe { send(api, store, b"imageLoadEnd:raw:\0", &end_arguments) } == api.nilObj {
        note_error(b"RustCodegen image facts Store load could not finish");
        return api.nilObj;
    }
    let _ = unsafe { send0(api, store, b"backfillMirror\0") };
    // Relational rules are canonical Store facts, but the query engine keeps a
    // derived in-memory cache. Rebuild it after image facts replace the store,
    // matching the authoritative Zig image loader's recovery boundary.
    if unsafe { (api.isExported)(c"db".as_ptr().cast()) } != 0 {
        let _ = unsafe { send0(api, store, b"rebuildRules\0") };
    }
    unsafe { (api.mkSmallInt)(pending.len() as i64) }
}

unsafe extern "C" fn rust_codegen_replay_methods(
    self_object: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    if count != 0 {
        abort("RustCodegen replayMethods: expects no arguments");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    clear_error();
    if !IMAGE_DISPATCH_RECOVERY.load(Ordering::Acquire)
        || RUNTIME_PHASE.load(Ordering::Acquire) != PHASE_IMAGE_RECOVERY
    {
        note_error(b"RustCodegen method replay requires image recovery");
        return api.nilObj;
    }
    let Some(sources) = (unsafe { ordered_generation_sources(api) }) else {
        note_error(b"RustCodegen image method source ordering failed");
        return api.nilObj;
    };
    // Preflight every indexed operation before executing the first one. This
    // prevents a malformed later source from leaving an earlier method replay
    // applied to the live dispatch dictionary.
    let mut replay_sources = Vec::new();
    for source in sources {
        let Some((_source_bytes, metadata)) = (unsafe { parse_revision_metadata(api, source) })
        else {
            note_error(b"RustCodegen image method source is not indexable");
            return api.nilObj;
        };
        replay_sources.extend(
            metadata
                .operations
                .into_iter()
                .map(|operation| operation.source),
        );
    }
    // Compile every operation through the pure emitter before evaluating the
    // first one. This closes the gap between metadata parsing and execution:
    // a later generated-C failure cannot leave an earlier method mutation
    // applied to the live dispatch dictionary.
    for source in &replay_sources {
        clear_error();
        let operation_source = unsafe { (api.mkString)(source.as_ptr(), source.len()) };
        let emitted = unsafe { send(api, self_object, EMIT_SELECTOR, &[operation_source]) };
        if emitted == api.nilObj || LAST_ERROR.lock().ok().is_some_and(|error| error.is_some()) {
            note_error(b"RustCodegen image method replay preflight failed");
            return api.nilObj;
        }
    }
    let error_vt = unsafe { (api.import)(ERROR_VT_NAME.as_ptr().cast()) };
    let mut replayed = 0usize;
    for source in replay_sources {
        clear_error();
        let operation_source = unsafe { (api.mkString)(source.as_ptr(), source.len()) };
        let result = unsafe { send(api, self_object, EVAL_SELECTOR, &[operation_source]) };
        if result == api.nilObj
            || (!error_vt.is_null() && unsafe { (api.vtableOf)(result) } == error_vt)
        {
            note_error(b"RustCodegen image method replay failed");
            return api.nilObj;
        }
        replayed += 1;
    }
    unsafe { (api.mkSmallInt)(replayed as i64) }
}

unsafe extern "C" fn rust_codegen_replay_aliens(
    self_object: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    if count != 0 {
        abort("RustCodegen replayAliens: expects no arguments");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    clear_error();
    if !IMAGE_DISPATCH_RECOVERY.load(Ordering::Acquire)
        || RUNTIME_PHASE.load(Ordering::Acquire) != PHASE_IMAGE_RECOVERY
    {
        note_error(b"RustCodegen alien replay requires image recovery");
        return api.nilObj;
    }
    let store = runtime.store as Oop;
    if store.is_null() || store == api.nilObj {
        return unsafe { (api.mkSmallInt)(0) };
    }
    let entity = unsafe { intern(api, b"__aliens__\0") };
    let attr_count = unsafe { send(api, store, b"attrCount:\0", &[entity]) };
    if unsafe { (api.isSmallInt)(attr_count) } == 0 {
        note_error(b"RustCodegen alien replay journal is malformed");
        return api.nilObj;
    }
    let count = unsafe { (api.smallIntValue)(attr_count) };
    let mut records = Vec::new();
    for index in 0..count {
        let attr = unsafe {
            send(
                api,
                store,
                b"attrAt:index:\0",
                &[entity, (api.mkSmallInt)(index)],
            )
        };
        let Some(name) = (unsafe { symbol_bytes(api, attr) }) else {
            continue;
        };
        let Some(index) = name
            .strip_prefix(b"__alien_")
            .and_then(|suffix| core::str::from_utf8(suffix).ok())
            .and_then(|suffix| suffix.parse::<i64>().ok())
        else {
            continue;
        };
        let source = unsafe { send(api, store, CURRENT_VALUE_SELECTOR, &[entity, attr]) };
        if source.is_null()
            || source == api.nilObj
            || unsafe { string_bytes(api, source) }.is_none()
        {
            continue;
        }
        records.push((index, source));
    }
    records.sort_by_key(|(index, _)| *index);
    let error_vt = unsafe { (api.import)(ERROR_VT_NAME.as_ptr().cast()) };
    let mut replayed = 0usize;
    for (_, source) in records {
        clear_error();
        let result = unsafe { send(api, self_object, EVAL_SELECTOR, &[source]) };
        if result == api.nilObj
            && (error_vt.is_null() || unsafe { (api.vtableOf)(result) } != error_vt)
            && LAST_ERROR.lock().ok().is_some_and(|error| error.is_some())
        {
            continue;
        }
        if result != api.nilObj
            && (error_vt.is_null() || unsafe { (api.vtableOf)(result) } != error_vt)
        {
            replayed += 1;
        }
    }
    // Individual alien failures are best-effort during image recovery. Do not
    // leak the last failed wrapper's diagnostic after returning a usable count.
    clear_error();
    unsafe { (api.mkSmallInt)(replayed as i64) }
}

unsafe fn derive_named_global(api: &IdApi, self_object: Oop, name: Oop) -> bool {
    let runtime = runtime();
    let store = runtime.store as Oop;
    let globals = unsafe { intern(api, GLOBALS_ENTITY_NAME) };
    let sources = unsafe { intern(api, GLOBAL_SOURCES_ENTITY_NAME) };
    if unsafe { send(api, store, HAS_VALUE_SELECTOR, &[globals, name]) } == api.trueObj {
        return false;
    }
    let source = unsafe { send(api, store, CURRENT_VALUE_SELECTOR, &[sources, name]) };
    let Some(source_bytes) = (unsafe { string_bytes(api, source) }) else {
        return false;
    };
    let Some(name_bytes) = (unsafe { symbol_bytes(api, name) }) else {
        return false;
    };
    let mut program = b"let ".to_vec();
    program.extend_from_slice(&name_bytes);
    program.extend_from_slice(b" := ");
    program.extend_from_slice(&source_bytes);
    let program = unsafe { (api.mkString)(program.as_ptr(), program.len()) };
    clear_error();
    let result = unsafe { send(api, self_object, EVAL_PERSISTING_SELECTOR, &[program]) };
    result != api.nilObj && LAST_ERROR.lock().ok().is_none_or(|error| error.is_none())
}

unsafe extern "C" fn rust_codegen_derive_image_method_targets(
    self_object: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    if count != 0 {
        abort("RustCodegen deriveImageMethodTargets: expects no arguments");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    clear_error();
    if !IMAGE_DISPATCH_RECOVERY.load(Ordering::Acquire)
        || RUNTIME_PHASE.load(Ordering::Acquire) != PHASE_IMAGE_RECOVERY
    {
        note_error(b"RustCodegen image method-target derivation requires image recovery");
        return api.nilObj;
    }
    let Some(sources) = (unsafe { ordered_generation_sources(api) }) else {
        note_error(b"RustCodegen image method-target source ordering failed");
        return api.nilObj;
    };
    let mut names = Vec::<Vec<u8>>::new();
    let mut derived = 0usize;
    for source in sources {
        let Some((_source_bytes, metadata)) = (unsafe { parse_revision_metadata(api, source) })
        else {
            note_error(b"RustCodegen image method-target source is not indexable");
            return api.nilObj;
        };
        for operation in metadata.operations {
            if names.contains(&operation.target) {
                continue;
            }
            names.push(operation.target.clone());
            let name = unsafe { intern_bytes(api, &operation.target) };
            if unsafe { derive_named_global(api, self_object, name) } {
                derived += 1;
            }
        }
    }
    unsafe { (api.mkSmallInt)(derived as i64) }
}

unsafe extern "C" fn rust_codegen_derive_image_globals(
    self_object: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    if count != 0 {
        abort("RustCodegen deriveImageGlobals: expects no arguments");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    clear_error();
    if !IMAGE_DISPATCH_RECOVERY.load(Ordering::Acquire)
        || RUNTIME_PHASE.load(Ordering::Acquire) != PHASE_IMAGE_RECOVERY
    {
        note_error(b"RustCodegen image global derivation requires image recovery");
        return api.nilObj;
    }
    let store = runtime.store as Oop;
    let sources = unsafe { intern(api, GLOBAL_SOURCES_ENTITY_NAME) };
    let attr_count = unsafe { send(api, store, b"attrCount:\0", &[sources]) };
    if unsafe { (api.isSmallInt)(attr_count) } == 0 {
        note_error(b"RustCodegen image global source journal is malformed");
        return api.nilObj;
    }
    let count = unsafe { (api.smallIntValue)(attr_count) };
    let mut names = Vec::new();
    for index in 0..count {
        let name = unsafe {
            send(
                api,
                store,
                b"attrAt:index:\0",
                &[sources, (api.mkSmallInt)(index)],
            )
        };
        if unsafe { symbol_bytes(api, name) }.is_some() {
            names.push(name);
        }
    }
    let mut derived = 0usize;
    for name in names {
        if unsafe { derive_named_global(api, self_object, name) } {
            derived += 1;
        }
    }
    unsafe { (api.mkSmallInt)(derived as i64) }
}

unsafe fn emit_source(
    api: &IdApi,
    source_object: Oop,
    artifact: bool,
    persist_top_level: bool,
    source_path: Option<&[u8]>,
    source_path_is_root: bool,
    candidate_salt: Option<u64>,
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
    let mut emitter = candidate_salt
        .map(Emitter::new_candidate_artifact)
        .unwrap_or_default();
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
    let Some(output) = (unsafe { emit_source(api, values[0], false, false, None, false, None) })
    else {
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
    let Some(source) = (unsafe {
        emit_source(
            api,
            values[0],
            false,
            false,
            source_path.as_deref(),
            false,
            None,
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
    let Some(source) = (unsafe {
        emit_source(
            api,
            values[0],
            false,
            true,
            source_path.as_deref(),
            false,
            None,
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
            None,
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

unsafe extern "C" fn rust_codegen_compile_candidate(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }.unwrap_or_else(|| {
        abort("RustCodegen compileCandidate:named:output: received invalid arguments")
    });
    if values.len() != 3 {
        abort("RustCodegen compileCandidate:named:output: expects source, name, and output");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    clear_error();
    unsafe { (api.gcSetAuto)(0) };
    struct AutoGc<'a>(&'a IdApi);
    impl Drop for AutoGc<'_> {
        fn drop(&mut self) {
            unsafe { (self.0.gcSetAuto)(1) };
        }
    }
    let _auto_gc = AutoGc(api);

    let Some(source_bytes) = (unsafe { string_bytes(api, values[0]) }) else {
        note_error(b"RustCodegen candidate source was not a String");
        return api.nilObj;
    };
    let Some(name) = (unsafe { symbol_bytes(api, values[1]) }) else {
        note_error(b"RustCodegen candidate name was not a Symbol");
        return api.nilObj;
    };
    let Some(path_bytes) = (unsafe { string_bytes(api, values[2]) }) else {
        note_error(b"RustCodegen candidate output path was not a String");
        return api.nilObj;
    };
    if path_bytes.is_empty() || path_bytes.contains(&0) {
        note_error(b"RustCodegen candidate output path is invalid");
        return api.nilObj;
    }
    let Some((_, metadata)) = (unsafe { parse_revision_metadata(api, values[0]) }) else {
        note_error(b"RustCodegen candidate source is not indexable");
        return api.nilObj;
    };
    if metadata.name != name {
        note_error(b"RustCodegen candidate metadata mismatches name");
        return api.nilObj;
    }
    if metadata.operations.is_empty() {
        note_error(b"RustCodegen candidate has no method delta");
        return api.nilObj;
    }

    let mut program = Vec::new();
    for operation in &metadata.operations {
        program.extend_from_slice(&operation.source);
        program.extend_from_slice(b";\n");
    }
    let program_object = unsafe { (api.mkString)(program.as_ptr(), program.len()) };
    let source_digest = sha256_hex(&source_bytes);
    let artifact_salt = u64::from_be_bytes(source_digest[..8].try_into().unwrap());
    let Some(generated) = (unsafe {
        emit_source(
            api,
            program_object,
            true,
            false,
            None,
            false,
            Some(artifact_salt),
        )
    }) else {
        note_error(b"RustCodegen candidate source could not be emitted");
        return api.nilObj;
    };
    let native_requires = unsafe { artifact_metadata(api, values[0]) }
        .map(|(_, native_requires)| native_requires)
        .unwrap_or_default();
    let manifest = candidate_manifest(&metadata, &source_digest);
    let rust_owned_shims = unsafe { (api.isExported)(CODEGEN_NAME.as_ptr().cast()) } == 0;
    let candidate_generated = candidate_generated(
        &generated,
        &manifest,
        &metadata.operations,
        rust_owned_shims,
    );
    let artifact = artifact_source(
        &source_bytes,
        &metadata.name,
        &source_digest,
        &metadata.revision_digest,
        &metadata.requires,
        &native_requires,
        &candidate_generated,
    );
    let tcc = runtime.tcc as Oop;
    if tcc.is_null() || tcc == api.nilObj {
        note_error(b"RustCodegen Tcc is unavailable");
        return api.nilObj;
    }
    let c_source = unsafe { (api.mkString)(artifact.as_ptr(), artifact.len()) };
    let kind = unsafe { intern(api, b"dll\0") };
    let result = unsafe {
        send(
            api,
            tcc,
            TCC_COMPILE_OUTPUT_SELECTOR,
            &[c_source, values[2], kind],
        )
    };
    if result == api.nilObj {
        note_error(b"RustCodegen candidate could not write the compiled dylib");
        return api.nilObj;
    }
    let output_path = Path::new(OsStr::from_bytes(&path_bytes));
    let Ok(artifact_bytes) = fs::read(output_path) else {
        note_error(b"RustCodegen candidate artifact is unreadable");
        return api.nilObj;
    };
    let checksum = sha256_hex(&artifact_bytes);
    let mut sidecar = Vec::new();
    sidecar.extend_from_slice(CANDIDATE_MAGIC);
    sidecar.push(b'\n');
    sidecar.extend_from_slice(b"artifact=");
    sidecar.extend_from_slice(&checksum);
    sidecar.extend_from_slice(b"\nruntime=");
    sidecar.extend_from_slice(ARTIFACT_ABI);
    sidecar.extend_from_slice(b"\ncandidate=");
    sidecar.extend_from_slice(CANDIDATE_ABI.to_string().as_bytes());
    sidecar.extend_from_slice(b"\nplatform=");
    sidecar.extend_from_slice(CANDIDATE_PLATFORM);
    sidecar.extend_from_slice(b"\nmanifest=");
    sidecar.extend_from_slice(&manifest);
    sidecar.extend_from_slice(b"\nsource=");
    sidecar.extend_from_slice(source_bytes.len().to_string().as_bytes());
    sidecar.push(b'\n');
    sidecar.extend_from_slice(&source_bytes);
    let mut sidecar_path = path_bytes;
    sidecar_path.extend_from_slice(b".candidate");
    if write_atomic(Path::new(OsStr::from_bytes(&sidecar_path)), &sidecar).is_none() {
        note_error(b"RustCodegen candidate manifest could not be written");
        return api.nilObj;
    }
    values[2]
}

fn fixed_macho_name(bytes: &[u8]) -> Option<&[u8]> {
    let end = bytes.iter().position(|byte| *byte == 0)?;
    Some(&bytes[..end])
}

fn publish_exclusive(path: &Path, bytes: &[u8]) -> Option<()> {
    match fs::read(path) {
        Ok(existing) => return (existing == bytes).then_some(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return None,
    }
    let mut file = match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return fs::read(path)
                .ok()
                .filter(|existing| existing == bytes)
                .map(|_| ());
        }
        Err(_) => return None,
    };
    if file.write_all(bytes).is_err() || file.sync_all().is_err() {
        drop(file);
        let _ = fs::remove_file(path);
        return None;
    }
    Some(())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Option<()> {
    let sequence = IMAGE_CODE_LOAD_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let mut temporary = path.as_os_str().as_bytes().to_vec();
    temporary.extend_from_slice(b".tmp.");
    temporary.extend_from_slice(std::process::id().to_string().as_bytes());
    temporary.push(b'.');
    temporary.extend_from_slice(sequence.to_string().as_bytes());
    let temporary = Path::new(OsStr::from_bytes(&temporary));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temporary)
        .ok()?;
    if file.write_all(bytes).is_err() || file.sync_all().is_err() {
        drop(file);
        let _ = fs::remove_file(temporary);
        return None;
    }
    drop(file);
    if fs::rename(temporary, path).is_err() {
        let _ = fs::remove_file(temporary);
        return None;
    }
    Some(())
}

fn ensure_candidate_content_path(author_path: &[u8], checksum: &[u8; 64]) -> Option<Vec<u8>> {
    let source_path = Path::new(OsStr::from_bytes(author_path));
    let parent = source_path
        .parent()
        .unwrap_or_else(|| Path::new(OsStr::from_bytes(b".")));
    let object_dir = parent.join(".birnam-candidates");
    fs::create_dir_all(&object_dir).ok()?;

    let source = fs::read(source_path).ok()?;
    if sha256_hex(&source) != *checksum {
        return None;
    }
    let mut file_name = OsString::from_vec(checksum.to_vec());
    file_name.push(".dylib");
    let destination = object_dir.join(file_name);
    publish_exclusive(&destination, &source)?;
    let published = fs::read(&destination).ok()?;
    if sha256_hex(&published) != *checksum {
        return None;
    }

    let mut author_sidecar = author_path.to_vec();
    author_sidecar.extend_from_slice(b".candidate");
    let sidecar = fs::read(Path::new(OsStr::from_bytes(&author_sidecar))).ok()?;
    let mut destination_sidecar = destination.as_os_str().as_bytes().to_vec();
    destination_sidecar.extend_from_slice(b".candidate");
    publish_exclusive(Path::new(OsStr::from_bytes(&destination_sidecar)), &sidecar)?;
    Some(destination.as_os_str().as_bytes().to_vec())
}

/// Apply the frozen candidate Mach-O policy before any `dlopen` or initializer
/// can run. This mirrors the Zig Codegen check byte-for-byte at the contract
/// level: thin arm64 dylib, no init sections, and only libSystem as a load
/// dependency.
fn candidate_macho_safe(bytes: &[u8]) -> bool {
    if bytes.len() < 32
        || u32::from_le_bytes(bytes[0..4].try_into().unwrap()) != 0xfeedfacf
        || u32::from_le_bytes(bytes[4..8].try_into().unwrap()) != 0x0100000c
        || u32::from_le_bytes(bytes[12..16].try_into().unwrap()) != 6
    {
        return false;
    }
    let ncmds = u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize;
    let sizeofcmds = u32::from_le_bytes(bytes[20..24].try_into().unwrap()) as usize;
    let Some(commands_end) = 32usize.checked_add(sizeofcmds) else {
        return false;
    };
    if commands_end > bytes.len() {
        return false;
    }
    let mut offset = 32usize;
    for _ in 0..ncmds {
        let command_end = offset.checked_add(8).and_then(|end| {
            let command_size = u32::from_le_bytes(bytes[offset + 4..end].try_into().ok()?) as usize;
            (command_size >= 8).then_some((command_size, offset.checked_add(command_size)?))
        });
        let Some((command_size, next)) = command_end else {
            return false;
        };
        if next > commands_end {
            return false;
        }
        let command = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        if command == 0x19 {
            if command_size < 72 {
                return false;
            }
            let nsects =
                u32::from_le_bytes(bytes[offset + 64..offset + 68].try_into().unwrap()) as usize;
            let Some(sections_end) = nsects
                .checked_mul(80)
                .and_then(|size| 72usize.checked_add(size))
            else {
                return false;
            };
            if sections_end > command_size {
                return false;
            }
            let mut section = offset + 72;
            for _ in 0..nsects {
                let name = fixed_macho_name(&bytes[section..section + 16]);
                if matches!(name, Some(b"__mod_init_func" | b"__init_offsets")) {
                    return false;
                }
                section += 80;
            }
        } else if matches!(
            command,
            0xc | 0x18 | 0x8000_0018 | 0x1f | 0x8000_001f | 0x20 | 0x23 | 0x8000_0023
        ) {
            if command_size < 24 {
                return false;
            }
            let name_offset =
                u32::from_le_bytes(bytes[offset + 8..offset + 12].try_into().unwrap()) as usize;
            if name_offset >= command_size {
                return false;
            }
            let name = fixed_macho_name(&bytes[offset + name_offset..next]);
            if command != 0xc || name != Some(b"/usr/lib/libSystem.B.dylib") {
                return false;
            }
        }
        offset = next;
    }
    offset == commands_end
}

unsafe fn assert_candidate_fact(
    api: &IdApi,
    runtime: &Runtime,
    unit: Oop,
    attribute: &[u8],
    value: Oop,
) -> bool {
    let store = runtime.store as Oop;
    if store.is_null() {
        return false;
    }
    let attribute = unsafe { intern(api, attribute) };
    unsafe { send(api, store, STORE_ASSERT_SELECTOR, &[unit, attribute, value]) != api.nilObj }
}

unsafe fn candidate_manifest_vector(api: &IdApi, metadata: &RevisionMetadata) -> Oop {
    let vector = unsafe { (api.import)(VECTOR_NAME.as_ptr().cast()) };
    if vector.is_null() {
        return api.nilObj;
    }
    let mut values = unsafe { send0(api, vector, NEW_SELECTOR) };
    for requirement in &metadata.requires {
        let requirement = unsafe { intern_bytes(api, requirement) };
        values = unsafe { send(api, values, b"add:\0", &[requirement]) };
    }
    values
}

unsafe extern "C" fn rust_codegen_inspect_candidate(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustCodegen inspectCandidate:into: received invalid arguments"));
    if values.len() != 2 {
        abort("RustCodegen inspectCandidate:into: expects path and unit");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    clear_error();
    let Some(path) = (unsafe { string_bytes(api, values[0]) }) else {
        note_error(b"RustCodegen candidate path was not a String");
        return api.nilObj;
    };
    if path.is_empty() || path.contains(&0) {
        note_error(b"RustCodegen candidate path is invalid");
        return api.nilObj;
    }
    if values[1].is_null() || values[1] == api.nilObj {
        note_error(b"RustCodegen candidate unit is invalid");
        return api.nilObj;
    }
    let mut sidecar_path = path.clone();
    sidecar_path.extend_from_slice(b".candidate");
    let sidecar = match fs::read(Path::new(OsStr::from_bytes(&sidecar_path))) {
        Ok(bytes) => bytes,
        Err(_) => {
            note_error(b"RustCodegen candidate manifest is unavailable");
            return api.nilObj;
        }
    };
    let envelope = match inspect_candidate(&sidecar) {
        Ok(envelope) => envelope,
        Err(_) => {
            note_error(b"RustCodegen candidate manifest is invalid");
            return api.nilObj;
        }
    };
    let artifact = match fs::read(Path::new(OsStr::from_bytes(&path))) {
        Ok(bytes) => bytes,
        Err(_) => {
            note_error(b"RustCodegen candidate artifact is unavailable");
            return api.nilObj;
        }
    };
    if !candidate_macho_safe(&artifact) {
        note_error(b"RustCodegen candidate artifact is unsafe");
        return api.nilObj;
    }
    let checksum = sha256_hex(&artifact);
    if envelope.artifact_digest != checksum {
        note_error(b"RustCodegen candidate artifact checksum mismatches");
        return api.nilObj;
    }
    unsafe { (api.gcSetAuto)(0) };
    struct AutoGc<'a>(&'a IdApi);
    impl Drop for AutoGc<'_> {
        fn drop(&mut self) {
            unsafe { (self.0.gcSetAuto)(1) };
        }
    }
    let _auto_gc = AutoGc(api);
    let source_object = unsafe { (api.mkString)(envelope.source.as_ptr(), envelope.source.len()) };
    let Some((source, metadata)) = (unsafe { parse_revision_metadata(api, source_object) }) else {
        note_error(b"RustCodegen candidate source is not indexable");
        return api.nilObj;
    };
    let Ok(manifest) = inspect_manifest(envelope.manifest) else {
        note_error(b"RustCodegen candidate manifest is invalid");
        return api.nilObj;
    };
    if !candidate_manifest_matches(manifest, &source, &metadata) {
        note_error(b"RustCodegen candidate metadata mismatches source");
        return api.nilObj;
    }
    let operations = (0..manifest.operation_count())
        .filter_map(|index| manifest.operation(index))
        .map(|operation| CandidateOperationState {
            target_name: operation.target.to_vec(),
            selector: operation.selector.to_vec(),
            install: operation.install,
            target: 0,
            vtable: 0,
            prior_method: 0,
            prior_present: false,
            post_method: 0,
            post_present: false,
        })
        .collect();
    let name = unsafe { intern_bytes(api, &metadata.name) };
    if let Ok(candidates) = CANDIDATES.lock()
        && let Some(existing) = candidates
            .iter()
            .find(|candidate| candidate.unit == values[1] as usize)
    {
        if existing.checksum == checksum {
            return values[1];
        }
        note_error(b"RustCodegen candidate unit is already staged");
        return api.nilObj;
    }
    let digest = sha256_hex(&source);
    let non_method = unsafe {
        (api.mkString)(
            metadata.non_method_digest.as_ptr(),
            metadata.non_method_digest.len(),
        )
    };
    let method = unsafe {
        (api.mkString)(
            metadata.method_digest.as_ptr(),
            metadata.method_digest.len(),
        )
    };
    let revision = unsafe {
        (api.mkString)(
            metadata.revision_digest.as_ptr(),
            metadata.revision_digest.len(),
        )
    };
    let source_hash = unsafe { (api.mkString)(digest.as_ptr(), digest.len()) };
    let checksum_object = unsafe { (api.mkString)(checksum.as_ptr(), checksum.len()) };
    let requires = unsafe { candidate_manifest_vector(api, &metadata) };
    let facts = [
        (b"unitName\0".as_slice(), name),
        (b"kind\0".as_slice(), unsafe {
            intern(api, b"candidateArtifact\0")
        }),
        (b"sourceHash\0".as_slice(), source_hash),
        (b"authoredSource\0".as_slice(), source_object),
        (b"nonMethodDigest\0".as_slice(), non_method),
        (b"methodDigest\0".as_slice(), method),
        (b"revisionDigest\0".as_slice(), revision),
        (b"artifactChecksum\0".as_slice(), checksum_object),
        (b"platform\0".as_slice(), unsafe {
            intern(api, b"aarch64-macos\0")
        }),
        (b"runtimeAbi\0".as_slice(), unsafe {
            intern_bytes(api, ARTIFACT_ABI)
        }),
        (b"candidateAbi\0".as_slice(), unsafe { (api.mkSmallInt)(3) }),
        (b"requires\0".as_slice(), requires),
        (b"mapped\0".as_slice(), api.falseObj),
    ];
    for (attribute, value) in facts {
        if !unsafe { assert_candidate_fact(api, runtime, values[1], attribute, value) } {
            note_error(b"RustCodegen candidate Store is unavailable");
            return api.nilObj;
        }
    }
    if let Ok(mut candidates) = CANDIDATES.lock() {
        candidates.push(CandidateState {
            unit: values[1] as usize,
            name: name as usize,
            source: source_object as usize,
            author_path: path,
            checksum,
            source_digest: digest,
            non_method_digest: metadata.non_method_digest,
            method_digest: metadata.method_digest,
            revision_digest: metadata.revision_digest,
            manifest: envelope.manifest.to_vec(),
            requires: metadata.requires.clone(),
            operations,
            handle: 0,
            validate: 0,
            prepare: 0,
            activate: 0,
            rollback: 0,
            prepared: 0,
            active: false,
            predecessor: 0,
        });
        return values[1];
    }
    note_error(b"RustCodegen candidate registry is unavailable");
    api.nilObj
}

unsafe fn map_candidate_failure(api: &IdApi, handle: *mut c_void, message: &[u8]) -> Oop {
    if !handle.is_null() {
        // SAFETY: the handle came from this map attempt and has not been
        // retained in the live candidate registry.
        unsafe { dlclose(handle) };
    }
    note_error(message);
    api.nilObj
}

unsafe fn direct_method_present(api: &IdApi, target: Oop, selector: Oop) -> bool {
    let vtable = unsafe { (api.vtableOf)(target) };
    if vtable.is_null() {
        return false;
    }
    let mut index = 0;
    loop {
        let entry = unsafe { (api.vtableSelectorAt)(vtable, index) };
        if entry.is_null() {
            return false;
        }
        if entry == selector {
            return true;
        }
        index = index.saturating_add(1);
    }
}

unsafe extern "C" fn rust_codegen_authorize_method_mutation() -> Oop {
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    if MOP_MUTATION_IN_PROGRESS.load(Ordering::Acquire)
        || HOST_CAPABILITY_LOADING.load(Ordering::Acquire)
        || matches!(
            RUNTIME_PHASE.load(Ordering::Acquire),
            PHASE_BOOTSTRAPPING | PHASE_QUIESCENT | PHASE_IMAGE_RECOVERY
        )
    {
        return api.trueObj;
    }
    note_error(b"RustCodegen dispatch mutation requires quiescence");
    api.nilObj
}

unsafe extern "C" fn rust_codegen_authorize_method_mutation_method(
    _self_object: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    if count != 0 {
        abort("RustCodegen authorizeManagedMethodMutation: expects no arguments");
    }
    unsafe { rust_codegen_authorize_method_mutation() }
}

unsafe extern "C" fn rust_codegen_enter_running(
    _self_object: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    if count != 0 {
        abort("RustCodegen enterRunning: expects no arguments");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    clear_error();
    // A host calls enterRunning only after selecting the complete stack. In a
    // Rust-only stack this is the first safe point to claim Store callbacks;
    // when Zig Codegen is present, ensure_store_callbacks leaves its owner
    // untouched under the single-registration law.
    let legacy_codegen_present = unsafe { (api.isExported)(CODEGEN_NAME.as_ptr().cast()) != 0 };
    unsafe { ensure_store_callbacks(api, runtime) };
    if !legacy_codegen_present && !STORE_CALLBACKS_REGISTERED.load(Ordering::Acquire) {
        note_error(b"RustCodegen cannot enter running without Store callbacks");
        return api.nilObj;
    }
    if RUNTIME_PHASE
        .compare_exchange(
            PHASE_BOOTSTRAPPING,
            PHASE_RUNNING,
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .is_ok()
        || RUNTIME_PHASE
            .compare_exchange(
                PHASE_QUIESCENT,
                PHASE_RUNNING,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    {
        return api.trueObj;
    }
    note_error(b"RustCodegen cannot enter running from the current phase");
    api.nilObj
}

unsafe extern "C" fn rust_codegen_begin_quiescence(
    _self_object: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    if count != 0 {
        abort("RustCodegen beginQuiescence: expects no arguments");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    clear_error();
    if RUNTIME_PHASE
        .compare_exchange(
            PHASE_RUNNING,
            PHASE_QUIESCENT,
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .is_ok()
    {
        return api.trueObj;
    }
    note_error(b"RustCodegen cannot begin quiescence from the current phase");
    api.nilObj
}

unsafe extern "C" fn rust_codegen_end_quiescence(
    _self_object: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    if count != 0 {
        abort("RustCodegen endQuiescence: expects no arguments");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    clear_error();
    if RUNTIME_PHASE
        .compare_exchange(
            PHASE_QUIESCENT,
            PHASE_RUNNING,
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .is_ok()
    {
        return api.trueObj;
    }
    note_error(b"RustCodegen cannot end quiescence from the current phase");
    api.nilObj
}

unsafe extern "C" fn rust_codegen_managed_method_changed(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }.unwrap_or_else(|| {
        abort("RustCodegen managedMethodChanged:on: received invalid arguments")
    });
    if values.len() != 2 {
        abort("RustCodegen managedMethodChanged:on: expects selector and vtable");
    }
    if MOP_MUTATION_IN_PROGRESS.load(Ordering::Acquire)
        || IMAGE_DISPATCH_RECOVERY.load(Ordering::Acquire)
    {
        return runtime().rust_codegen as Oop;
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    let selector_name = unsafe { CStr::from_ptr((api.symbolName)(values[0])) }
        .to_bytes()
        .to_vec();
    let vtable = values[1] as usize;
    let mut watches = Vec::new();
    if let Ok(candidates) = CANDIDATES.lock() {
        for candidate in candidates.iter().filter(|candidate| candidate.active) {
            for operation in &candidate.operations {
                if operation.vtable == vtable && operation.selector == selector_name {
                    watches.push((
                        candidate.unit,
                        operation.post_method,
                        operation.post_present,
                        operation.vtable,
                        operation.selector.clone(),
                    ));
                }
            }
        }
    }
    for (unit, post_method, post_present, operation_vtable, operation_selector) in watches {
        let selector = unsafe { intern_bytes(api, &operation_selector) };
        let current = unsafe {
            send(
                api,
                runtime.store as Oop,
                STORE_RAW_CURRENT_SELECTOR,
                &[operation_vtable as Oop, selector],
            )
        };
        if !post_present || current as usize != post_method {
            let drifted = unsafe { intern(api, b"drifted\0") };
            let _ =
                unsafe { assert_candidate_fact(api, runtime, unit as Oop, b"state\0", drifted) };
            let _ = unsafe {
                assert_candidate_fact(
                    api,
                    runtime,
                    unit as Oop,
                    b"rollbackAvailable\0",
                    api.falseObj,
                )
            };
        }
    }
    _self_object
}

unsafe fn ensure_store_callbacks(api: &IdApi, runtime: &Runtime) {
    if STORE_CALLBACKS_REGISTERED.load(Ordering::Acquire)
        || runtime.store as Oop == api.nilObj
        || (runtime.store as Oop).is_null()
    {
        return;
    }
    // A mixed stack keeps Zig Codegen's callback owner. Rust becomes the owner
    // only when no legacy Codegen export is present, avoiding registration-law
    // conflicts while the migration is opt-in.
    if unsafe { (api.isExported)(CODEGEN_NAME.as_ptr().cast()) } != 0 {
        return;
    }
    let observer = unsafe {
        send(
            api,
            runtime.store as Oop,
            SET_METHOD_OBSERVER_SELECTOR,
            &[runtime.rust_codegen as Oop],
        )
    };
    if observer == api.nilObj {
        return;
    }
    let callback = unsafe {
        (api.mkSmallInt)(rust_codegen_authorize_method_mutation as *const () as usize as i64)
    };
    let authorizer = unsafe {
        send(
            api,
            runtime.store as Oop,
            SET_METHOD_AUTHORIZER_SELECTOR,
            &[callback],
        )
    };
    if authorizer != api.nilObj {
        STORE_CALLBACKS_REGISTERED.store(true, Ordering::Release);
    }
}

unsafe extern "C" fn rust_codegen_map_candidate(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustCodegen mapCandidate: received invalid arguments"));
    if values.len() != 1 {
        abort("RustCodegen mapCandidate: expects one unit");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    clear_error();
    unsafe { ensure_store_callbacks(api, runtime) };

    let (author_path, checksum, manifest, requires, existing_handle) = {
        let Ok(candidates) = CANDIDATES.lock() else {
            note_error(b"RustCodegen candidate registry is unavailable");
            return api.nilObj;
        };
        let Some(candidate) = candidates
            .iter()
            .find(|candidate| candidate.unit == values[0] as usize)
        else {
            note_error(b"RustCodegen candidate unit is unknown");
            return api.nilObj;
        };
        (
            candidate.author_path.clone(),
            candidate.checksum,
            candidate.manifest.clone(),
            candidate.requires.clone(),
            candidate.handle,
        )
    };
    if existing_handle != 0 {
        let _ = unsafe { assert_candidate_fact(api, runtime, values[0], b"mapped\0", api.trueObj) };
        return values[0];
    }

    if !requires.is_empty() {
        let module = unsafe { (api.import)(MODULE_NAME.as_ptr().cast()) };
        if module.is_null() || module == api.nilObj {
            note_error(b"RustCodegen candidate dependencies are missing");
            return api.nilObj;
        }
        for requirement in &requires {
            let requirement = unsafe { intern_bytes(api, requirement) };
            if unsafe { send(api, module, MODULE_IS_LOADED_SELECTOR, &[requirement]) }
                != api.trueObj
            {
                note_error(b"RustCodegen candidate dependencies are missing");
                return api.nilObj;
            }
        }
    }

    let Some(content_path) = ensure_candidate_content_path(&author_path, &checksum) else {
        note_error(b"RustCodegen candidate publication failed");
        return api.nilObj;
    };
    let mut content_path_z = content_path;
    content_path_z.push(0);
    // RTLD_LAZY | RTLD_GLOBAL: candidates resolve the frozen birnam_* ABI from
    // the already-loaded native rung set, matching the Zig mapper.
    let handle = unsafe { (api.dlopen)(content_path_z.as_ptr().cast(), 9) };
    if handle.is_null() {
        note_error(b"RustCodegen candidate mapping failed");
        return api.nilObj;
    }
    let abi_symbol = unsafe { (api.dlsym)(handle, c"__birnam_artifact_abi".as_ptr()) };
    if abi_symbol.is_null()
        || unsafe { CStr::from_ptr(abi_symbol.cast::<c_char>()) }.to_bytes() != ARTIFACT_ABI
    {
        return unsafe {
            map_candidate_failure(api, handle, b"RustCodegen candidate ABI mismatches")
        };
    }
    let manifest_symbol = unsafe { (api.dlsym)(handle, c"__birnam_candidate_manifest".as_ptr()) };
    if manifest_symbol.is_null()
        || unsafe { CStr::from_ptr(manifest_symbol.cast::<c_char>()) }.to_bytes() != manifest
    {
        return unsafe {
            map_candidate_failure(api, handle, b"RustCodegen candidate manifest mismatches")
        };
    }
    let validate_symbol = unsafe { (api.dlsym)(handle, c"__birnam_candidate_validate".as_ptr()) };
    let prepare_symbol = unsafe { (api.dlsym)(handle, c"__birnam_candidate_prepare".as_ptr()) };
    let activate_symbol = unsafe { (api.dlsym)(handle, c"__birnam_candidate_activate".as_ptr()) };
    let rollback_symbol = unsafe { (api.dlsym)(handle, c"__birnam_candidate_rollback".as_ptr()) };
    let init_symbol = unsafe { (api.dlsym)(handle, c"__birnam_unit_init".as_ptr()) };
    if validate_symbol.is_null()
        || prepare_symbol.is_null()
        || activate_symbol.is_null()
        || rollback_symbol.is_null()
        || init_symbol.is_null()
    {
        return unsafe {
            map_candidate_failure(api, handle, b"RustCodegen candidate hooks are unavailable")
        };
    }

    // SAFETY: the generated candidate ABI exports these exact function
    // signatures, and all calls occur while the dlopen handle is retained.
    let init: unsafe extern "C" fn() = unsafe { core::mem::transmute(init_symbol) };
    let validate: Method = unsafe { core::mem::transmute(validate_symbol) };
    unsafe { init() };
    if unsafe { validate(api.nilObj, ptr::null(), 0) } != api.trueObj {
        return unsafe {
            map_candidate_failure(api, handle, b"RustCodegen candidate validation failed")
        };
    }

    let Ok(parsed_manifest) = inspect_manifest(&manifest) else {
        return unsafe {
            map_candidate_failure(api, handle, b"RustCodegen candidate manifest is invalid")
        };
    };
    for index in 0..parsed_manifest.operation_count() {
        let Some(operation) = parsed_manifest.operation(index) else {
            return unsafe {
                map_candidate_failure(api, handle, b"RustCodegen candidate manifest is invalid")
            };
        };
        if operation.install {
            continue;
        }
        let target = unsafe { global_by_name(api, runtime, operation.target) };
        let selector = unsafe { intern_bytes(api, operation.selector) };
        if target.is_null()
            || target == api.nilObj
            || !unsafe { direct_method_present(api, target, selector) }
        {
            return unsafe {
                map_candidate_failure(api, handle, b"RustCodegen candidate validation failed")
            };
        }
    }

    let mapped =
        unsafe { assert_candidate_fact(api, runtime, values[0], b"mapped\0", api.trueObj) };
    if !mapped {
        return unsafe {
            map_candidate_failure(api, handle, b"RustCodegen candidate Store is unavailable")
        };
    }
    let Ok(mut candidates) = CANDIDATES.lock() else {
        return unsafe {
            map_candidate_failure(
                api,
                handle,
                b"RustCodegen candidate registry is unavailable",
            )
        };
    };
    let Some(candidate) = candidates
        .iter_mut()
        .find(|candidate| candidate.unit == values[0] as usize)
    else {
        drop(candidates);
        return unsafe {
            map_candidate_failure(api, handle, b"RustCodegen candidate unit is unknown")
        };
    };
    candidate.handle = handle as usize;
    candidate.validate = validate_symbol as usize;
    candidate.prepare = prepare_symbol as usize;
    candidate.activate = activate_symbol as usize;
    candidate.rollback = rollback_symbol as usize;
    values[0]
}

unsafe extern "C" fn rust_codegen_candidate_mapped(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustCodegen candidateMapped: received invalid arguments"));
    if values.len() != 1 {
        abort("RustCodegen candidateMapped: expects one unit");
    }
    let api = unsafe { api(runtime()) };
    if let Ok(candidates) = CANDIDATES.lock()
        && let Some(candidate) = candidates
            .iter()
            .find(|candidate| candidate.unit == values[0] as usize)
    {
        return if candidate.handle != 0 {
            api.trueObj
        } else {
            api.falseObj
        };
    }
    api.falseObj
}

unsafe extern "C" fn rust_codegen_candidate_content_path(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustCodegen candidateContentPath: received invalid arguments"));
    if values.len() != 1 {
        abort("RustCodegen candidateContentPath: expects one unit");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    clear_error();
    let (author_path, checksum) = {
        let Ok(candidates) = CANDIDATES.lock() else {
            note_error(b"RustCodegen candidate registry is unavailable");
            return api.nilObj;
        };
        let Some(candidate) = candidates
            .iter()
            .find(|candidate| candidate.unit == values[0] as usize)
        else {
            note_error(b"RustCodegen candidate unit is unknown");
            return api.nilObj;
        };
        (candidate.author_path.clone(), candidate.checksum)
    };
    let Some(path) = ensure_candidate_content_path(&author_path, &checksum) else {
        note_error(b"RustCodegen candidate publication failed");
        return api.nilObj;
    };
    unsafe { (api.mkString)(path.as_ptr(), path.len()) }
}

unsafe fn parse_publication_plan(
    api: &IdApi,
    unit: Oop,
    name: Oop,
    plan: Oop,
    action: &[u8],
) -> Option<(usize, PublicationPlanState)> {
    let vector_vt = unsafe { (api.import)(VECTOR_VT_NAME.as_ptr().cast()) };
    if vector_vt.is_null() || unsafe { (api.vtableOf)(plan) } != vector_vt {
        note_error(b"RustCodegen publication plan is not a Vector");
        return None;
    }
    if unsafe { vector_length(api, plan, vector_vt) } != Some(8) {
        note_error(b"RustCodegen publication plan has the wrong shape");
        return None;
    }
    let action_object = unsafe { intern(api, action) };
    if unsafe { vector_item(api, plan, 0) } != action_object
        || unsafe { vector_item(api, plan, 1) } != unit
        || unsafe { vector_item(api, plan, 2) } != name
    {
        note_error(b"RustCodegen publication plan identity mismatches");
        return None;
    }
    let prior = unsafe { vector_item(api, plan, 3) };
    let event = unsafe { vector_item(api, plan, 4) };
    let basis = unsafe { vector_item(api, plan, 5) };
    let expected_generation = unsafe { vector_item(api, plan, 6) };
    let successor_generation = unsafe { vector_item(api, plan, 7) };
    if event.is_null()
        || event == api.nilObj
        || basis.is_null()
        || basis == api.nilObj
        || expected_generation.is_null()
        || expected_generation == api.nilObj
        || successor_generation.is_null()
        || successor_generation == api.nilObj
    {
        note_error(b"RustCodegen publication plan is incomplete");
        return None;
    }
    if unsafe { durable_current_generation(api) } != api.nilObj
        && !unsafe {
            durable_generation_plan_is_valid(
                api,
                unit,
                name,
                prior,
                expected_generation,
                successor_generation,
                action,
            )
        }
    {
        note_error(b"RustCodegen publication plan does not match durable generation");
        return None;
    }
    Some((
        if prior == api.nilObj {
            0
        } else {
            prior as usize
        },
        PublicationPlanState {
            unit: unit as usize,
            name: name as usize,
            prior: if prior == api.nilObj {
                0
            } else {
                prior as usize
            },
            rollback: action == b"rollback\0",
            event: event as usize,
            basis: basis as usize,
            expected_generation: expected_generation as usize,
            successor_generation: successor_generation as usize,
            enforce_lifecycle: true,
        },
    ))
}

unsafe fn publication_plan_is_current(api: &IdApi, plan: PublicationPlanState) -> bool {
    let durable = unsafe { durable_current_generation(api) };
    if durable != api.nilObj {
        let prior = if plan.prior == 0 {
            api.nilObj
        } else {
            plan.prior as Oop
        };
        let action = if plan.rollback {
            b"rollback\0".as_slice()
        } else {
            b"activate\0".as_slice()
        };
        return durable == plan.expected_generation as Oop
            && unsafe { durable_generation_is_canonical(api, plan.expected_generation as Oop) }
            && unsafe { durable_generation_is_canonical(api, plan.successor_generation as Oop) }
            && unsafe {
                durable_generation_plan_is_valid(
                    api,
                    plan.unit as Oop,
                    plan.name as Oop,
                    prior,
                    plan.expected_generation as Oop,
                    plan.successor_generation as Oop,
                    action,
                )
            };
    }
    let current = ACTIVE_GENERATION.load(Ordering::Acquire);
    current == 0 || current == plan.expected_generation
}

unsafe extern "C" fn rust_codegen_prepare_activation(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustCodegen prepareActivation: received invalid arguments"));
    if values.len() != 1 && values.len() != 2 {
        abort("RustCodegen prepareActivation: expects unit or unit and prior");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    clear_error();
    let (name, validate, prepare, prepared, active, mut operations) = {
        let Ok(candidates) = CANDIDATES.lock() else {
            note_error(b"RustCodegen candidate registry is unavailable");
            return api.nilObj;
        };
        let Some(candidate) = candidates
            .iter()
            .find(|candidate| candidate.unit == values[0] as usize)
        else {
            note_error(b"RustCodegen candidate unit is unknown");
            return api.nilObj;
        };
        (
            candidate.name as Oop,
            candidate.validate,
            candidate.prepare,
            candidate.prepared,
            candidate.active,
            candidate.operations.clone(),
        )
    };
    let (prior, plan) = if let Some(raw_plan) = values.get(1).copied()
        && unsafe { (api.vtableOf)(raw_plan) }
            == unsafe { (api.import)(VECTOR_VT_NAME.as_ptr().cast()) }
    {
        let Some((prior, plan)) =
            (unsafe { parse_publication_plan(api, values[0], name, raw_plan, b"activate\0") })
        else {
            return api.nilObj;
        };
        (prior, plan)
    } else {
        (
            values
                .get(1)
                .copied()
                .filter(|value| *value != api.nilObj)
                .map(|value| value as usize)
                .unwrap_or(0),
            PublicationPlanState::default(),
        )
    };
    if plan.enforce_lifecycle && !unsafe { publication_plan_is_current(api, plan) } {
        note_error(b"RustCodegen publication plan is stale");
        return api.nilObj;
    }
    if validate == 0 || prepare == 0 {
        note_error(b"RustCodegen candidate is not mapped");
        return api.nilObj;
    }
    if prepared != 0 {
        note_error(b"RustCodegen candidate is already prepared");
        return api.nilObj;
    }
    if active {
        note_error(b"RustCodegen candidate is already active");
        return api.nilObj;
    }
    if prior == values[0] as usize {
        note_error(b"RustCodegen candidate prior is itself");
        return api.nilObj;
    }
    if prior != 0 {
        let prior_active = CANDIDATES
            .lock()
            .ok()
            .and_then(|candidates| {
                candidates
                    .iter()
                    .find(|candidate| candidate.unit == prior)
                    .map(|candidate| candidate.active)
            })
            .unwrap_or(false);
        if !prior_active {
            note_error(b"RustCodegen candidate prior is not active");
            return api.nilObj;
        }
    }
    // SAFETY: hook addresses were checked and retained by the mapping boundary.
    let validate: Method = unsafe { core::mem::transmute(validate) };
    let prepare: Method = unsafe { core::mem::transmute(prepare) };
    if unsafe { validate(api.nilObj, ptr::null(), 0) } != api.trueObj {
        note_error(b"RustCodegen candidate validation failed");
        return api.nilObj;
    }
    for operation in &mut operations {
        let target = unsafe { global_by_name(api, runtime, &operation.target_name) };
        if target.is_null() || target == api.nilObj {
            note_error(b"RustCodegen candidate target is missing");
            return api.nilObj;
        }
        let vtable = unsafe { (api.vtableOf)(target) };
        if vtable.is_null() || unsafe { (api.vtableOwner)(vtable) } != target {
            note_error(b"RustCodegen candidate target is unstable");
            return api.nilObj;
        }
        let selector = unsafe { intern_bytes(api, &operation.selector) };
        let prior = unsafe {
            send(
                api,
                runtime.store as Oop,
                STORE_RAW_CURRENT_SELECTOR,
                &[vtable, selector],
            )
        };
        let prior_present = !prior.is_null();
        if !operation.install && !prior_present {
            note_error(b"RustCodegen candidate direct method is missing");
            return api.nilObj;
        }
        operation.target = target as usize;
        operation.vtable = vtable as usize;
        operation.prior_method = prior as usize;
        operation.prior_present = prior_present;
    }
    if unsafe { prepare(api.nilObj, ptr::null(), 0) } == api.nilObj {
        note_error(b"RustCodegen candidate preparation failed");
        return api.nilObj;
    }
    let token = {
        let vector = unsafe { (api.import)(VECTOR_NAME.as_ptr().cast()) };
        if vector.is_null() || vector == api.nilObj {
            api.nilObj
        } else {
            unsafe { send0(api, vector, NEW_SELECTOR) }
        }
    };
    if token.is_null() || token == api.nilObj {
        note_error(b"RustCodegen prepared token allocation failed");
        return api.nilObj;
    }
    let Ok(mut prepared_states) = PREPARED.lock() else {
        note_error(b"RustCodegen prepared registry is unavailable");
        return api.nilObj;
    };
    if let Ok(mut candidates) = CANDIDATES.lock()
        && let Some(candidate) = candidates
            .iter_mut()
            .find(|candidate| candidate.unit == values[0] as usize)
    {
        candidate.prepared = token as usize;
        candidate.operations = operations;
        prepared_states.push(PreparedState {
            token: token as usize,
            unit: values[0] as usize,
            rollback: false,
            prior,
            plan,
        });
        return token;
    }
    note_error(b"RustCodegen candidate registry is unavailable");
    api.nilObj
}

unsafe extern "C" fn rust_codegen_commit_prepared(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustCodegen commitPrepared: received invalid arguments"));
    if values.len() != 1 {
        abort("RustCodegen commitPrepared: expects one token");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    clear_error();
    let token = values[0] as usize;
    let (unit, prior, plan) = {
        let Ok(prepared) = PREPARED.lock() else {
            note_error(b"RustCodegen prepared registry is unavailable");
            return api.nilObj;
        };
        let Some(state) = prepared.iter().find(|state| state.token == token) else {
            note_error(b"RustCodegen prepared token is invalid");
            return api.nilObj;
        };
        (state.unit, state.prior, state.plan)
    };
    if plan.enforce_lifecycle
        && !matches!(
            RUNTIME_PHASE.load(Ordering::Acquire),
            PHASE_BOOTSTRAPPING | PHASE_QUIESCENT
        )
    {
        if let Ok(mut candidates) = CANDIDATES.lock()
            && let Some(candidate) = candidates
                .iter_mut()
                .find(|candidate| candidate.unit == unit)
        {
            candidate.prepared = 0;
        }
        if let Ok(mut prepared) = PREPARED.lock() {
            prepared.retain(|state| state.token != token);
        }
        note_error(b"RustCodegen publication commit requires quiescence");
        return api.nilObj;
    }
    if plan.enforce_lifecycle && !unsafe { publication_plan_is_current(api, plan) } {
        if let Ok(mut candidates) = CANDIDATES.lock()
            && let Some(candidate) = candidates
                .iter_mut()
                .find(|candidate| candidate.unit == unit)
        {
            candidate.prepared = 0;
        }
        if let Ok(mut prepared) = PREPARED.lock() {
            prepared.retain(|state| state.token != token);
        }
        note_error(b"RustCodegen publication commit is stale");
        return api.nilObj;
    }
    let activate = {
        let Ok(candidates) = CANDIDATES.lock() else {
            note_error(b"RustCodegen candidate registry is unavailable");
            return api.nilObj;
        };
        let Some(candidate) = candidates.iter().find(|candidate| candidate.unit == unit) else {
            note_error(b"RustCodegen candidate unit is unknown");
            return api.nilObj;
        };
        if candidate.prepared != token || candidate.active || candidate.activate == 0 {
            note_error(b"RustCodegen prepared token is invalid");
            return api.nilObj;
        }
        candidate.activate
    };
    // SAFETY: the hook is retained by the mapped candidate handle.
    let activate: Method = unsafe { core::mem::transmute(activate) };
    MOP_MUTATION_IN_PROGRESS.store(true, Ordering::Release);
    let result = unsafe { activate(api.nilObj, ptr::null(), 0) };
    MOP_MUTATION_IN_PROGRESS.store(false, Ordering::Release);
    if result == api.nilObj {
        note_error(b"RustCodegen candidate activation failed");
        return api.nilObj;
    }
    let mut operations = {
        let Ok(candidates) = CANDIDATES.lock() else {
            note_error(b"RustCodegen candidate registry is unavailable");
            return api.nilObj;
        };
        let Some(candidate) = candidates.iter().find(|candidate| candidate.unit == unit) else {
            note_error(b"RustCodegen candidate unit is unknown");
            return api.nilObj;
        };
        candidate.operations.clone()
    };
    for operation in &mut operations {
        let selector = unsafe { intern_bytes(api, &operation.selector) };
        let vtable = operation.vtable as Oop;
        let current = unsafe {
            send(
                api,
                runtime.store as Oop,
                STORE_RAW_CURRENT_SELECTOR,
                &[vtable, selector],
            )
        };
        operation.post_method = current as usize;
        operation.post_present = !current.is_null();
    }
    let active = unsafe { intern(api, b"active\0") };
    if prior != 0 {
        let prior_state = unsafe { intern(api, b"superseded\0") };
        if !(unsafe {
            assert_candidate_fact(api, runtime, unit as Oop, b"supersedes\0", prior as Oop)
        }) || !(unsafe {
            assert_candidate_fact(api, runtime, prior as Oop, b"state\0", prior_state)
        }) {
            note_error(b"RustCodegen candidate Store is unavailable");
            return api.nilObj;
        }
    }
    if !(unsafe { assert_candidate_fact(api, runtime, unit as Oop, b"state\0", active) }) {
        note_error(b"RustCodegen candidate Store is unavailable");
        return api.nilObj;
    }
    let _ = unsafe {
        assert_candidate_fact(
            api,
            runtime,
            unit as Oop,
            b"rollbackAvailable\0",
            api.trueObj,
        )
    };
    if plan.enforce_lifecycle {
        let action = unsafe { intern(api, b"activate\0") };
        if !(unsafe {
            assert_candidate_fact(api, runtime, plan.event as Oop, b"unit\0", unit as Oop)
        }) || !(unsafe {
            assert_candidate_fact(api, runtime, plan.event as Oop, b"action\0", action)
        }) || !(unsafe {
            assert_candidate_fact(
                api,
                runtime,
                plan.event as Oop,
                b"basis\0",
                plan.basis as Oop,
            )
        }) || (prior != 0
            && !(unsafe {
                assert_candidate_fact(
                    api,
                    runtime,
                    plan.event as Oop,
                    b"priorUnit\0",
                    prior as Oop,
                )
            }))
            || !(unsafe {
                assert_candidate_fact(
                    api,
                    runtime,
                    unit as Oop,
                    b"activationEvent\0",
                    plan.event as Oop,
                )
            })
        {
            note_error(b"RustCodegen publication event Store is unavailable");
            return api.nilObj;
        }
        if unsafe { durable_current_generation(api) } != api.nilObj
            && !unsafe { set_durable_current_generation(api, plan.successor_generation as Oop) }
        {
            note_error(b"RustCodegen durable generation update failed");
            return api.nilObj;
        }
        ACTIVE_GENERATION.store(plan.successor_generation, Ordering::Release);
    }
    if let Ok(mut candidates) = CANDIDATES.lock() {
        let index = candidates
            .iter()
            .position(|candidate| candidate.unit == unit);
        if let Some(index) = index {
            candidates[index].active = true;
            candidates[index].prepared = 0;
            candidates[index].operations = operations;
            candidates[index].predecessor = prior;
            if prior != 0
                && let Some(previous) = candidates
                    .iter_mut()
                    .find(|previous| previous.unit == prior)
            {
                previous.active = false;
            }
        }
    }
    if let Ok(mut prepared) = PREPARED.lock() {
        prepared.retain(|state| state.token != token);
    }
    unit as Oop
}

unsafe fn candidate_operations_match_post(
    api: &IdApi,
    runtime: &Runtime,
    operations: &[CandidateOperationState],
) -> bool {
    for operation in operations {
        let selector = unsafe { intern_bytes(api, &operation.selector) };
        let current = unsafe {
            send(
                api,
                runtime.store as Oop,
                STORE_RAW_CURRENT_SELECTOR,
                &[operation.vtable as Oop, selector],
            )
        };
        let current_present = !current.is_null();
        if current_present != operation.post_present
            || (current_present && current as usize != operation.post_method)
        {
            return false;
        }
    }
    true
}

unsafe extern "C" fn rust_codegen_reconcile_candidate(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustCodegen reconcileCandidate: received invalid arguments"));
    if values.len() != 1 {
        abort("RustCodegen reconcileCandidate: expects one unit");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    clear_error();
    let (active, operations) = {
        let Ok(candidates) = CANDIDATES.lock() else {
            note_error(b"RustCodegen candidate registry is unavailable");
            return api.nilObj;
        };
        let Some(candidate) = candidates
            .iter()
            .find(|candidate| candidate.unit == values[0] as usize)
        else {
            note_error(b"RustCodegen candidate unit is unknown");
            return api.nilObj;
        };
        (candidate.active, candidate.operations.clone())
    };
    if !active {
        return values[0];
    }
    if unsafe { candidate_operations_match_post(api, runtime, &operations) } {
        return values[0];
    }
    let drifted = unsafe { intern(api, b"drifted\0") };
    let _ = unsafe { assert_candidate_fact(api, runtime, values[0], b"state\0", drifted) };
    let _ = unsafe {
        assert_candidate_fact(
            api,
            runtime,
            values[0],
            b"rollbackAvailable\0",
            api.falseObj,
        )
    };
    note_error(b"RustCodegen candidate dispatch drifted");
    api.nilObj
}

unsafe extern "C" fn rust_codegen_reconcile_all_candidates(
    _self_object: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    if count != 0 {
        abort("RustCodegen reconcileAllCandidates: expects no arguments");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    clear_error();
    let units = if let Ok(candidates) = CANDIDATES.lock() {
        candidates
            .iter()
            .filter(|candidate| candidate.active)
            .map(|candidate| candidate.unit as Oop)
            .collect::<Vec<_>>()
    } else {
        note_error(b"RustCodegen candidate registry is unavailable");
        return api.nilObj;
    };
    for unit in units {
        let result =
            unsafe { rust_codegen_reconcile_candidate(runtime.rust_codegen as Oop, &unit, 1) };
        if result == api.nilObj {
            return api.nilObj;
        }
    }
    api.trueObj
}

unsafe extern "C" fn rust_codegen_begin_image_dispatch_recovery(
    _self_object: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    if count != 0 {
        abort("RustCodegen beginImageDispatchRecovery: expects no arguments");
    }
    let previous = RUNTIME_PHASE.swap(PHASE_IMAGE_RECOVERY, Ordering::AcqRel);
    IMAGE_PREVIOUS_PHASE.store(previous, Ordering::Release);
    IMAGE_DISPATCH_RECOVERY.store(true, Ordering::Release);
    if let Ok(mut candidates) = CANDIDATES.lock() {
        for candidate in candidates.iter_mut() {
            candidate.active = false;
        }
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    api.trueObj
}

unsafe extern "C" fn rust_codegen_end_image_dispatch_recovery(
    _self_object: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    if count != 0 {
        abort("RustCodegen endImageDispatchRecovery: expects no arguments");
    }
    IMAGE_DISPATCH_RECOVERY.store(false, Ordering::Release);
    let previous = IMAGE_PREVIOUS_PHASE.load(Ordering::Acquire);
    let _ = RUNTIME_PHASE.compare_exchange(
        PHASE_IMAGE_RECOVERY,
        previous,
        Ordering::AcqRel,
        Ordering::Acquire,
    );
    unsafe { api(runtime()) }.trueObj
}

unsafe extern "C" fn rust_codegen_prepare_rollback(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustCodegen prepareRollback: received invalid arguments"));
    if values.len() != 1 && values.len() != 2 {
        abort("RustCodegen prepareRollback: expects unit or unit and plan");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    clear_error();
    let (name, prepared, active, predecessor, operations) = {
        let Ok(candidates) = CANDIDATES.lock() else {
            note_error(b"RustCodegen candidate registry is unavailable");
            return api.nilObj;
        };
        let Some(candidate) = candidates
            .iter()
            .find(|candidate| candidate.unit == values[0] as usize)
        else {
            note_error(b"RustCodegen candidate unit is unknown");
            return api.nilObj;
        };
        (
            candidate.name as Oop,
            candidate.prepared,
            candidate.active,
            candidate.predecessor,
            candidate.operations.clone(),
        )
    };
    let (prior, plan) = if let Some(raw_plan) = values.get(1).copied() {
        let Some((prior, plan)) =
            (unsafe { parse_publication_plan(api, values[0], name, raw_plan, b"rollback\0") })
        else {
            return api.nilObj;
        };
        if prior != predecessor {
            note_error(b"RustCodegen rollback publication prior mismatches predecessor");
            return api.nilObj;
        }
        (prior, plan)
    } else {
        (predecessor, PublicationPlanState::default())
    };
    if plan.enforce_lifecycle && !unsafe { publication_plan_is_current(api, plan) } {
        note_error(b"RustCodegen rollback publication plan is stale");
        return api.nilObj;
    }
    if !active {
        note_error(b"RustCodegen candidate is not active");
        return api.nilObj;
    }
    if prepared != 0 {
        note_error(b"RustCodegen candidate is already prepared");
        return api.nilObj;
    }
    for operation in &operations {
        let selector = unsafe { intern_bytes(api, &operation.selector) };
        let current = unsafe {
            send(
                api,
                runtime.store as Oop,
                STORE_RAW_CURRENT_SELECTOR,
                &[operation.vtable as Oop, selector],
            )
        };
        let current_present = !current.is_null();
        if current_present != operation.post_present
            || (current_present && current as usize != operation.post_method)
        {
            let drifted = unsafe { intern(api, b"drifted\0") };
            let _ = unsafe { assert_candidate_fact(api, runtime, values[0], b"state\0", drifted) };
            let _ = unsafe {
                assert_candidate_fact(
                    api,
                    runtime,
                    values[0],
                    b"rollbackAvailable\0",
                    api.falseObj,
                )
            };
            note_error(b"RustCodegen candidate dispatch drifted");
            return api.nilObj;
        }
    }
    let token = {
        let vector = unsafe { (api.import)(VECTOR_NAME.as_ptr().cast()) };
        if vector.is_null() || vector == api.nilObj {
            api.nilObj
        } else {
            unsafe { send0(api, vector, NEW_SELECTOR) }
        }
    };
    if token.is_null() || token == api.nilObj {
        note_error(b"RustCodegen prepared token allocation failed");
        return api.nilObj;
    }
    let Ok(mut prepared_states) = PREPARED.lock() else {
        note_error(b"RustCodegen prepared registry is unavailable");
        return api.nilObj;
    };
    if let Ok(mut candidates) = CANDIDATES.lock()
        && let Some(candidate) = candidates
            .iter_mut()
            .find(|candidate| candidate.unit == values[0] as usize)
    {
        candidate.prepared = token as usize;
        prepared_states.push(PreparedState {
            token: token as usize,
            unit: values[0] as usize,
            rollback: true,
            prior,
            plan,
        });
        return token;
    }
    note_error(b"RustCodegen candidate registry is unavailable");
    api.nilObj
}

unsafe extern "C" fn rust_codegen_commit_prepared_rollback(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustCodegen commitPreparedRollback: received invalid arguments"));
    if values.len() != 1 {
        abort("RustCodegen commitPreparedRollback: expects one token");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    clear_error();
    let token = values[0] as usize;
    let (unit, rollback, prior, plan) = {
        let Ok(prepared) = PREPARED.lock() else {
            note_error(b"RustCodegen prepared registry is unavailable");
            return api.nilObj;
        };
        let Some(state) = prepared.iter().find(|state| state.token == token) else {
            note_error(b"RustCodegen prepared token is invalid");
            return api.nilObj;
        };
        (state.unit, state.rollback, state.prior, state.plan)
    };
    if !rollback {
        note_error(b"RustCodegen prepared token is invalid");
        return api.nilObj;
    }
    if plan.enforce_lifecycle
        && !matches!(
            RUNTIME_PHASE.load(Ordering::Acquire),
            PHASE_BOOTSTRAPPING | PHASE_QUIESCENT
        )
    {
        if let Ok(mut candidates) = CANDIDATES.lock()
            && let Some(candidate) = candidates
                .iter_mut()
                .find(|candidate| candidate.unit == unit)
        {
            candidate.prepared = 0;
        }
        if let Ok(mut prepared) = PREPARED.lock() {
            prepared.retain(|state| state.token != token);
        }
        note_error(b"RustCodegen publication rollback requires quiescence");
        return api.nilObj;
    }
    if plan.enforce_lifecycle && !unsafe { publication_plan_is_current(api, plan) } {
        if let Ok(mut candidates) = CANDIDATES.lock()
            && let Some(candidate) = candidates
                .iter_mut()
                .find(|candidate| candidate.unit == unit)
        {
            candidate.prepared = 0;
        }
        if let Ok(mut prepared) = PREPARED.lock() {
            prepared.retain(|state| state.token != token);
        }
        note_error(b"RustCodegen publication rollback is stale");
        return api.nilObj;
    }
    let operations = {
        let Ok(candidates) = CANDIDATES.lock() else {
            note_error(b"RustCodegen candidate registry is unavailable");
            return api.nilObj;
        };
        let Some(candidate) = candidates.iter().find(|candidate| candidate.unit == unit) else {
            note_error(b"RustCodegen candidate unit is unknown");
            return api.nilObj;
        };
        if candidate.prepared != token || !candidate.active {
            note_error(b"RustCodegen prepared token is invalid");
            return api.nilObj;
        }
        candidate.operations.clone()
    };
    MOP_MUTATION_IN_PROGRESS.store(true, Ordering::Release);
    for operation in operations.iter().rev() {
        let selector = unsafe { intern_bytes(api, &operation.selector) };
        let vtable = operation.vtable as Oop;
        let result = if operation.prior_present {
            let method = operation.prior_method as Oop;
            unsafe {
                send(
                    api,
                    runtime.store as Oop,
                    b"addMethod:on:with:\0",
                    &[selector, vtable, method],
                )
            }
        } else {
            unsafe {
                send(
                    api,
                    runtime.store as Oop,
                    b"removeMethod:on:\0",
                    &[selector, vtable],
                )
            }
        };
        if result == api.nilObj {
            MOP_MUTATION_IN_PROGRESS.store(false, Ordering::Release);
            note_error(b"RustCodegen candidate rollback failed");
            return api.nilObj;
        }
    }
    MOP_MUTATION_IN_PROGRESS.store(false, Ordering::Release);
    let rolled_back = unsafe { intern(api, b"rolledBack\0") };
    if prior != 0 {
        let active = unsafe { intern(api, b"active\0") };
        if !(unsafe { assert_candidate_fact(api, runtime, prior as Oop, b"state\0", active) }) {
            note_error(b"RustCodegen candidate Store is unavailable");
            return api.nilObj;
        }
    }
    if !(unsafe { assert_candidate_fact(api, runtime, unit as Oop, b"state\0", rolled_back) }) {
        note_error(b"RustCodegen candidate Store is unavailable");
        return api.nilObj;
    }
    let _ = unsafe {
        assert_candidate_fact(
            api,
            runtime,
            unit as Oop,
            b"rollbackAvailable\0",
            api.falseObj,
        )
    };
    if plan.enforce_lifecycle {
        let action = unsafe { intern(api, b"rollback\0") };
        if !(unsafe {
            assert_candidate_fact(api, runtime, plan.event as Oop, b"unit\0", unit as Oop)
        }) || !(unsafe {
            assert_candidate_fact(api, runtime, plan.event as Oop, b"action\0", action)
        }) || !(unsafe {
            assert_candidate_fact(
                api,
                runtime,
                plan.event as Oop,
                b"basis\0",
                plan.basis as Oop,
            )
        }) || (prior != 0
            && !(unsafe {
                assert_candidate_fact(
                    api,
                    runtime,
                    plan.event as Oop,
                    b"priorUnit\0",
                    prior as Oop,
                )
            }))
            || !(unsafe {
                assert_candidate_fact(
                    api,
                    runtime,
                    unit as Oop,
                    b"activationEvent\0",
                    plan.event as Oop,
                )
            })
        {
            note_error(b"RustCodegen publication event Store is unavailable");
            return api.nilObj;
        }
        if unsafe { durable_current_generation(api) } != api.nilObj
            && !unsafe { set_durable_current_generation(api, plan.successor_generation as Oop) }
        {
            note_error(b"RustCodegen durable generation update failed");
            return api.nilObj;
        }
        ACTIVE_GENERATION.store(plan.successor_generation, Ordering::Release);
    }
    if let Ok(mut candidates) = CANDIDATES.lock() {
        let index = candidates
            .iter()
            .position(|candidate| candidate.unit == unit);
        if let Some(index) = index {
            candidates[index].active = false;
            candidates[index].prepared = 0;
            if prior != 0
                && let Some(previous) = candidates
                    .iter_mut()
                    .find(|candidate| candidate.unit == prior)
            {
                previous.active = true;
            }
        }
    }
    if let Ok(mut prepared) = PREPARED.lock() {
        prepared.retain(|state| state.token != token);
    }
    unit as Oop
}

unsafe extern "C" fn rust_codegen_discard_prepared(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustCodegen discardPrepared: received invalid arguments"));
    if values.len() != 1 {
        abort("RustCodegen discardPrepared: expects one token");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    clear_error();
    let token = values[0] as usize;
    let unit = {
        let Ok(prepared) = PREPARED.lock() else {
            note_error(b"RustCodegen prepared registry is unavailable");
            return api.nilObj;
        };
        let Some(state) = prepared.iter().find(|state| state.token == token) else {
            note_error(b"RustCodegen prepared token is invalid");
            return api.nilObj;
        };
        state.unit
    };
    if let Ok(mut candidates) = CANDIDATES.lock()
        && let Some(candidate) = candidates
            .iter_mut()
            .find(|candidate| candidate.unit == unit)
    {
        if candidate.prepared != token {
            note_error(b"RustCodegen prepared token is invalid");
            return api.nilObj;
        }
        candidate.prepared = 0;
    }
    if let Ok(mut prepared) = PREPARED.lock() {
        prepared.retain(|state| state.token != token);
        return api.trueObj;
    }
    note_error(b"RustCodegen prepared registry is unavailable");
    api.nilObj
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
    if INITIAL_GENERATION_SEALED
        .lock()
        .ok()
        .is_some_and(|sealed| *sealed)
        && !HOST_CAPABILITY_LOADING.load(Ordering::Acquire)
        && !IMAGE_DISPATCH_RECOVERY.load(Ordering::Acquire)
    {
        note_error(b"RustCodegen module introduction is not available");
        return api.nilObj;
    }
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
        return unsafe {
            map_candidate_failure(api, handle, b"RustCodegen artifact ABI marker is missing")
        };
    }
    // SAFETY: the artifact contract requires a NUL-terminated immutable ABI
    // marker exported alongside the descriptor.
    let abi = unsafe { CStr::from_ptr(abi_symbol.cast::<c_char>()) };
    if abi.to_bytes() != ARTIFACT_ABI {
        return unsafe {
            map_candidate_failure(
                api,
                handle,
                b"RustCodegen artifact ABI marker is incompatible",
            )
        };
    }
    let descriptor_symbol =
        unsafe { (api.dlsym)(handle, c"__birnam_artifact_descriptor".as_ptr()) };
    let descriptor_len_symbol =
        unsafe { (api.dlsym)(handle, c"__birnam_artifact_descriptor_len".as_ptr()) };
    if descriptor_symbol.is_null() {
        return unsafe {
            map_candidate_failure(api, handle, b"RustCodegen artifact descriptor is missing")
        };
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
            return unsafe {
                map_candidate_failure(api, handle, b"RustCodegen artifact descriptor is missing")
            };
        }
        let payload_len = u32::from_le_bytes(
            header[ARTIFACT_DESCRIPTOR_MAGIC.len()..header_len]
                .try_into()
                .unwrap(),
        ) as usize;
        let Some(length) = header_len.checked_add(payload_len) else {
            return unsafe {
                map_candidate_failure(
                    api,
                    handle,
                    b"RustCodegen artifact descriptor is incompatible",
                )
            };
        };
        length
    };
    let descriptor =
        unsafe { core::slice::from_raw_parts(descriptor_symbol.cast::<u8>(), descriptor_len) };
    let Ok(parsed_descriptor) = inspect_artifact(descriptor) else {
        return unsafe {
            map_candidate_failure(
                api,
                handle,
                b"RustCodegen artifact descriptor is incompatible",
            )
        };
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
        return unsafe {
            map_candidate_failure(
                api,
                handle,
                b"RustCodegen artifact native requirement is unavailable",
            )
        };
    }
    if parsed_descriptor.requires.count() != 0 {
        let module = unsafe { (api.import)(MODULE_NAME.as_ptr().cast()) };
        if module.is_null() || module == api.nilObj {
            return unsafe {
                map_candidate_failure(
                    api,
                    handle,
                    b"RustCodegen artifact module dependency surface is unavailable",
                )
            };
        }
        for requirement in parsed_descriptor.requires.iter() {
            let requirement = unsafe { intern_bytes(api, requirement) };
            if unsafe { send(api, module, MODULE_IS_LOADED_SELECTOR, &[requirement]) }
                != api.trueObj
            {
                return unsafe {
                    map_candidate_failure(
                        api,
                        handle,
                        b"RustCodegen artifact module requirement is unavailable",
                    )
                };
            }
        }
    }
    let init_symbol = unsafe { (api.dlsym)(handle, c"__birnam_unit_init".as_ptr()) };
    let entry_symbol = unsafe { (api.dlsym)(handle, c"__ev".as_ptr()) };
    if init_symbol.is_null() || entry_symbol.is_null() {
        return unsafe {
            map_candidate_failure(
                api,
                handle,
                b"RustCodegen artifact entry points are incomplete",
            )
        };
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

unsafe extern "C" fn rust_codegen_load_host_capability(
    self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    struct HostCapabilityGuard;
    impl Drop for HostCapabilityGuard {
        fn drop(&mut self) {
            HOST_CAPABILITY_LOADING.store(false, Ordering::Release);
        }
    }
    HOST_CAPABILITY_LOADING.store(true, Ordering::Release);
    let _guard = HostCapabilityGuard;
    unsafe { rust_codegen_load(self_object, pointer, count) }
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
        let store = unsafe { (host_api.import)(STORE_NAME.as_ptr().cast()) };
        Runtime {
            api: api_pointer as usize,
            rust_codegen_vt: rust_codegen_vt as usize,
            rust_codegen: rust_codegen as usize,
            tcc: tcc as usize,
            globals: globals as usize,
            store: store as usize,
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
            intern(api, GENERATION_SOURCES_SELECTOR),
            rust_codegen_generation_sources,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, GENERATION_METHOD_SOURCES_SELECTOR),
            rust_codegen_generation_method_sources,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, CURRENT_GENERATION_DIGEST_SELECTOR),
            rust_codegen_current_generation_digest,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, COMPILE_IMAGE_CODE_SELECTOR),
            rust_codegen_compile_image_code,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, PREFLIGHT_IMAGE_SELECTOR),
            rust_codegen_preflight_image,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, MAP_IMAGE_CODE_SELECTOR),
            rust_codegen_map_image_code,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, REPLAY_IMAGE_FACTS_SELECTOR),
            rust_codegen_replay_image_facts,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, REPLAY_METHODS_SELECTOR),
            rust_codegen_replay_methods,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, REPLAY_ALIENS_SELECTOR),
            rust_codegen_replay_aliens,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, DERIVE_IMAGE_GLOBALS_SELECTOR),
            rust_codegen_derive_image_globals,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, DERIVE_IMAGE_METHOD_TARGETS_SELECTOR),
            rust_codegen_derive_image_method_targets,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, INSPECT_CANDIDATE_SELECTOR),
            rust_codegen_inspect_candidate,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, MAP_CANDIDATE_SELECTOR),
            rust_codegen_map_candidate,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, CANDIDATE_MAPPED_SELECTOR),
            rust_codegen_candidate_mapped,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, CANDIDATE_CONTENT_PATH_SELECTOR),
            rust_codegen_candidate_content_path,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, PREPARE_ACTIVATION_SELECTOR),
            rust_codegen_prepare_activation,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, PREPARE_ACTIVATION_PRIOR_SELECTOR),
            rust_codegen_prepare_activation,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, PREPARE_ACTIVATION_PLAN_SELECTOR),
            rust_codegen_prepare_activation,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, PREPARE_ROLLBACK_SELECTOR),
            rust_codegen_prepare_rollback,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, PREPARE_ROLLBACK_PLAN_SELECTOR),
            rust_codegen_prepare_rollback,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, COMMIT_PREPARED_SELECTOR),
            rust_codegen_commit_prepared,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, COMMIT_PREPARED_ROLLBACK_SELECTOR),
            rust_codegen_commit_prepared_rollback,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, DISCARD_PREPARED_SELECTOR),
            rust_codegen_discard_prepared,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, RECONCILE_CANDIDATE_SELECTOR),
            rust_codegen_reconcile_candidate,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, RECONCILE_ALL_CANDIDATES_SELECTOR),
            rust_codegen_reconcile_all_candidates,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, BEGIN_IMAGE_RECOVERY_SELECTOR),
            rust_codegen_begin_image_dispatch_recovery,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, END_IMAGE_RECOVERY_SELECTOR),
            rust_codegen_end_image_dispatch_recovery,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, MANAGED_METHOD_CHANGED_SELECTOR),
            rust_codegen_managed_method_changed,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, AUTHORIZE_METHOD_MUTATION_SELECTOR),
            rust_codegen_authorize_method_mutation_method,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, ENTER_RUNNING_SELECTOR),
            rust_codegen_enter_running,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, BEGIN_QUIESCENCE_SELECTOR),
            rust_codegen_begin_quiescence,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, END_QUIESCENCE_SELECTOR),
            rust_codegen_end_quiescence,
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
            intern(api, COMPILE_CANDIDATE_SELECTOR),
            rust_codegen_compile_candidate,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, LOAD_SELECTOR),
            rust_codegen_load,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, LOAD_HOST_CAPABILITY_SELECTOR),
            rust_codegen_load_host_capability,
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
    use super::{
        IDENTITY, RevisionMetadata, RevisionOperation, artifact_source,
        birnam_codegen_rust_identity, candidate_manifest_matches, sha256_hex,
    };
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

    #[test]
    fn candidate_manifest_matches_static_revision_operations() {
        let source = b"candidate source";
        let metadata = RevisionMetadata {
            name: b"candidate".to_vec(),
            requires: vec![b"depA".to_vec(), b"depB".to_vec()],
            operations: vec![RevisionOperation {
                target: b"Target".to_vec(),
                selector: b"answer:".to_vec(),
                install: true,
                source: b"operation".to_vec(),
            }],
            non_method_digest: *b"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            method_digest: *b"abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789",
            revision_digest: *b"fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210",
        };
        let mut manifest = b"v2|birnam-artifact-generation-v1|candidate|".to_vec();
        manifest.extend_from_slice(&sha256_hex(source));
        manifest.push(b'|');
        manifest.extend_from_slice(&metadata.non_method_digest);
        manifest.push(b'|');
        manifest.extend_from_slice(&metadata.method_digest);
        manifest.push(b'|');
        manifest.extend_from_slice(&metadata.revision_digest);
        manifest.extend_from_slice(b"|depA,depB|Target,answer:,I,1");
        let parsed = birnam_artifact::candidate::inspect_manifest(&manifest).unwrap();
        assert!(candidate_manifest_matches(parsed, source, &metadata));

        let mut malformed = manifest;
        let arity = malformed
            .windows(b"Target,answer:,I,1".len())
            .position(|window| window == b"Target,answer:,I,1")
            .unwrap();
        malformed[arity + b"Target,answer:,I,".len()] = b'2';
        let parsed = birnam_artifact::candidate::inspect_manifest(&malformed).unwrap();
        assert!(!candidate_manifest_matches(parsed, source, &metadata));
    }
}
