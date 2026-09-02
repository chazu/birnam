//! Canonical Rust Codegen runtime.
//!
//! This dylib publishes the `Codegen` and `Reflect` globals used by the host.
//! It materializes the Rust PEG CST, lowers it through the pure Codegen core,
//! and returns deterministic generated C without invoking TCC.

use birnam_abi::{
    IdApi, MarkFn, Method, Oop, PORTABLE_CODE_API_V1_VERSION, PORTABLE_CODE_API_V2_VERSION,
    PORTABLE_PROGRAM_VIEW_API_V1_VERSION, PortableCodeApiV1, PortableCodeApiV2,
    PortableProgramViewApiV1, SLOT_OOP, SLOT_RAW, borrowed_oop_args,
    foreign_address::{self, KIND_BORROWED},
};
use birnam_artifact::{
    code_binding_inventory, materialization_selection, portable_inventory,
    portable_resumable_manifest,
};
use birnam_artifact_codec::candidate::{
    CANDIDATE_ABI, CANDIDATE_MAGIC, CANDIDATE_PLATFORM, Manifest as CandidateManifest,
    ManifestVersion as CandidateManifestVersion, Operation as CandidateOperation,
    inspect as inspect_candidate, inspect_manifest,
};
use birnam_artifact_codec::inspect as inspect_artifact;
use birnam_artifact_codec::resumable_manifest::{self, ManifestEntrySpec};
use birnam_artifact_codec::stackless_selection::{
    Disposition as SelectionDisposition, ImplementationKind as SelectionImplementationKind,
    ImplementationSpec, MAX_CANONICAL_VALUE_BYTES, MAX_IMPLEMENTATION_ROWS, MAX_PATH_DEPTH,
    RootKind, SelectionRequest, SelectionSpec, compose_generation_selection,
    inspect_artifact_selection, inspect_generation_selection, materialize_artifact_selection,
    replace_generation_selection,
};
use birnam_codegen::artifact_stackless_selection::{
    Disposition as CodegenDisposition, ImplementationKind as CodegenImplementationKind,
    RootKind as CodegenRootKind, SelectionSpec as CodegenSelectionSpec,
};
use birnam_codegen::portable_c::compile_synchronous_block_v1;
use birnam_codegen::resumable::{
    ImplementationKey, InventoryImplementationKind, InventoryRootKind, implementation_inventory,
    resolve_block_implementation,
};
use birnam_codegen::resumable_block::{
    compile_block_with_literals_v1, compile_portable_materialization_block_v1,
};
use birnam_codegen::resumable_c::{
    BlockLiteralDisposition, BlockLiteralSpec, PortableBlockLiteralSpec,
};
use birnam_codegen::resumable_descriptor;
use birnam_codegen::resumable_named::{
    compile_named_adapter_v1, compile_portable_materialization_named_v1,
};
use birnam_codegen::resumable_registration_c;
use birnam_codegen::resumable_resolver_c;
use birnam_codegen::{
    BlockResumableEmission, CstNode, Emitter, LowerError, NamedResumableEmission, OwnedCst,
    Program, TranslationUnit, lower_program,
};
use core::ffi::{c_char, c_int, c_long, c_void};
use core::mem::size_of;
use core::ptr;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{CStr, CString, OsStr, OsString};
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

const IDENTITY: &[u8] = b"rust-codegen-n6.3-emitter\0";
const RUST_CODEGEN_NAME: &[u8] = b"RustCodegen\0";
const RUST_CODEGEN_VT_NAME: &[u8] = b"RustCodegenVt\0";
// C1.3 constructs the frozen family under explicit adapter names.  C1.6 is
// the only phase allowed to publish these objects as Codegen/Reflect; keeping
// the intermediate names visible makes the adapter inspectable without
// weakening C1.1's expected-rejection gate.
const ADAPTER_CODEGEN_NAME: &[u8] = b"RustCodegenAdapter\0";
const ADAPTER_CODEGEN_VT_NAME: &[u8] = b"RustCodegenAdapterVt\0";
const ADAPTER_REFLECT_NAME: &[u8] = b"RustReflectAdapter\0";
const ADAPTER_REFLECT_VT_NAME: &[u8] = b"RustReflectAdapterVt\0";
const ADAPTER_COLLECTOR_VT_NAME: &[u8] = b"RustReflectFactCollectorVt\0";
const CANONICAL_ENABLE_ENV: &str = "BIRNAM_RUST_CODEGEN_CANONICAL";
const CANONICAL_CODEGEN_VT_NAME: &[u8] = b"CodegenVt\0";
const CANONICAL_CODEGEN_NAME: &[u8] = b"Codegen\0";
const CANONICAL_REFLECT_VT_NAME: &[u8] = b"ReflectVt\0";
const CANONICAL_REFLECT_NAME: &[u8] = b"Reflect\0";
const CANONICAL_COLLECTOR_VT_NAME: &[u8] = b"ReflectFactCollectorVt\0";
const DISCARD_RECEIPT_NAME: &[u8] = b"DiscardReceipt\0";
const DISCARD_RECEIPT_VT_NAME: &[u8] = b"DiscardReceiptVt\0";
const PORTABLE_MAPPED_UNIT_VT_NAME: &[u8] = b"_BirnamPortableMappedUnitVt\0";
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
const COMPILE_OUTPUT_SELECTION_SELECTOR: &[u8] = b"compile:output:selection:\0";
const COMPILE_INITIAL_MODULE_SELECTION_SELECTOR: &[u8] =
    b"compileInitialModule:output:selection:\0";
const COMPILE_CANDIDATE_SELECTION_SELECTOR: &[u8] = b"compileCandidate:named:output:selection:\0";
const EMBEDDED_FILES_SELECTOR: &[u8] = b"embeddedFilesOf:\0";
const MODULE_METADATA_SELECTOR: &[u8] = b"moduleMetadataOf:\0";
const SOURCE_REVISION_SELECTOR: &[u8] = b"sourceRevisionOf:\0";
const RECORD_MODULE_SOURCE_SELECTOR: &[u8] = b"recordModuleSource:named:\0";
const MODULE_SOURCE_FOR_SELECTOR: &[u8] = b"moduleSourceFor:\0";
const SEAL_INITIAL_GENERATION_SELECTOR: &[u8] = b"sealInitialGeneration\0";
const INITIAL_GENERATION_PROJECTION_SELECTOR: &[u8] = b"initialGenerationProjection\0";
const INITIALIZE_CURRENT_GENERATION_SELECTOR: &[u8] = b"initializeCurrentGeneration\0";
const SUCCESSOR_GENERATION_PROJECTION_SELECTOR: &[u8] =
    b"successorGenerationProjection:replacing:with:unit:\0";
const GENERATION_SOURCES_SELECTOR: &[u8] = b"generationSources\0";
const GENERATION_METHOD_SOURCES_SELECTOR: &[u8] = b"generationMethodSources\0";
const CURRENT_GENERATION_DIGEST_SELECTOR: &[u8] = b"currentGenerationDigest\0";
const COMPILE_IMAGE_CODE_SELECTOR: &[u8] = b"compileImageCode:\0";
const COMPILE_IMAGE_CODE_MANIFEST_SELECTOR: &[u8] = b"compileImageCode:manifest:\0";
const REBUILD_IMAGE_CODE_SELECTOR: &[u8] = b"rebuildImageCode\0";
const REPLAY_ALIENS_SELECTOR: &[u8] = b"replayAliens\0";
const DERIVE_IMAGE_GLOBALS_SELECTOR: &[u8] = b"deriveImageGlobals\0";
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
const ADOPT_COMPOSED_GENERATION_SELECTOR: &[u8] = b"adoptComposedGeneration:\0";
const BEGIN_IMAGE_RECOVERY_SELECTOR: &[u8] = b"beginImageDispatchRecovery\0";
const END_IMAGE_RECOVERY_SELECTOR: &[u8] = b"endImageDispatchRecovery\0";
const MANAGED_METHOD_CHANGED_SELECTOR: &[u8] = b"managedMethodChanged:on:\0";
const AUTHORIZE_METHOD_MUTATION_SELECTOR: &[u8] = b"authorizeManagedMethodMutation\0";
const ENTER_RUNNING_SELECTOR: &[u8] = b"enterRunning\0";
const BEGIN_QUIESCENCE_SELECTOR: &[u8] = b"beginQuiescence\0";
const END_QUIESCENCE_SELECTOR: &[u8] = b"endQuiescence\0";
const RETAINED_GENERATION_COUNT_SELECTOR: &[u8] = b"retainedGenerationCount\0";
const RETAINED_GENERATION_BYTES_SELECTOR: &[u8] = b"retainedGenerationBytes\0";
const LIVE_VTABLE_COUNT_SELECTOR: &[u8] = b"liveVtableCount\0";
const LIVE_SYMBOL_COUNT_SELECTOR: &[u8] = b"liveSymbolCount\0";
const LIVE_SYMBOL_NAME_BYTES_SELECTOR: &[u8] = b"liveSymbolNameBytes\0";
const LIVE_HEAP_BYTES_SELECTOR: &[u8] = b"liveHeapBytes\0";
const MATERIALIZE_SYNCHRONOUS_SELECTOR: &[u8] = b"materializeSynchronous:captures:\0";
const MATERIALIZE_RESUMABLE_SELECTOR: &[u8] = b"materializeResumable:captures:\0";
const MATERIALIZATION_COUNT_SELECTOR: &[u8] = b"materializationCount\0";
const EVICT_MATERIALIZATIONS_SELECTOR: &[u8] = b"evictMaterializations\0";
const CURRENT_CODE_BINDING_COUNT_SELECTOR: &[u8] = b"currentCodeBindingCount\0";
const RETIRING_CODE_BINDING_COUNT_SELECTOR: &[u8] = b"retiringCodeBindingCount\0";
const CODE_BINDING_GENERATED_BYTES_SELECTOR: &[u8] = b"codeBindingGeneratedBytes\0";
const REAP_CODE_BINDINGS_SELECTOR: &[u8] = b"reapCodeBindings\0";
const PORTABLE_IMAGE_BINDINGS_SELECTOR: &[u8] = b"portableImageBindings\0";
const COMPILE_PORTABLE_IMAGE_CODE_SELECTOR: &[u8] =
    b"compilePortableImageCode:bindings:selection:\0";
const BEGIN_PORTABLE_IMAGE_REBUILD_SELECTOR: &[u8] =
    b"beginPortableImageRebuild:bindings:selection:\0";
const BEGIN_PORTABLE_IMAGE_MAPPED_SELECTOR: &[u8] =
    b"beginPortableImageMapped:programs:bindings:selection:\0";
const PORTABLE_IMAGE_BLOCK_VTABLE_SELECTOR: &[u8] = b"portableImageBlockVtable:\0";
const PREPARE_PORTABLE_IMAGE_BINDING_SELECTOR: &[u8] =
    b"preparePortableImageBinding:implementation:\0";
const FINISH_PORTABLE_IMAGE_UNIT_SELECTOR: &[u8] = b"finishPortableImageUnit\0";
const CANCEL_PORTABLE_IMAGE_UNIT_SELECTOR: &[u8] = b"cancelPortableImageUnit\0";
const INSTALL_PORTABLE_IMAGE_BINDING_SELECTOR: &[u8] =
    b"installPortableImageBinding:on:as:source:\0";
const LOAD_SELECTOR: &[u8] = b"load:\0";
const LOAD_INITIAL_MODULE_SELECTOR: &[u8] = b"loadInitialModule:\0";
const LOAD_HOST_CAPABILITY_SELECTOR: &[u8] = b"loadHostCapability:\0";
const TCC_COMPILE_OUTPUT_SELECTOR: &[u8] = b"compile:output:as:\0";
const BIND_RUNTIME_SELECTOR: &[u8] = b"bindRuntime\0";
const RELOCATE_SELECTOR: &[u8] = b"relocate\0";
const DISPOSABLE_SELECTOR: &[u8] = b"disposable\0";
const OWN_CODE_GENERATION_SELECTOR: &[u8] = b"ownCodeGeneration:\0";
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
const STORE_RETRACT_SELECTOR: &[u8] = b"retract:attr:value:\0";
const STORE_RAW_CURRENT_SELECTOR: &[u8] = b"rawCurrentValue:attr:\0";
const SET_METHOD_OBSERVER_SELECTOR: &[u8] = b"setMethodMutationObserver:\0";
const SET_METHOD_AUTHORIZER_SELECTOR: &[u8] = b"setMethodMutationAuthorizer:\0";
const CODEGEN_NAME: &[u8] = b"Codegen\0";
const MODULE_IS_LOADED_SELECTOR: &[u8] = b"isLoaded:\0";
const DISPATCH_NAME: &[u8] = b"dispatch\0";
const ACTIVE_GENERATION_NAME: &[u8] = b"activeGeneration\0";
const DISPATCH_GENERATION_NAME: &[u8] = b"DispatchGeneration\0";
const FROM_PROJECTION_SELECTOR: &[u8] = b"fromProjection:predecessor:delta:\0";
const GLOBALS_ENTITY_NAME: &[u8] = b"__globals__\0";
const GLOBAL_SOURCES_ENTITY_NAME: &[u8] = b"__globalsrc__\0";
const METHOD_OPS_ENTITY_NAME: &[u8] = b"__methodops__\0";
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
    adapter_codegen_vt: usize,
    adapter_codegen: usize,
    adapter_reflect_vt: usize,
    adapter_reflect: usize,
    adapter_collector_vt: usize,
    discard_receipt_vt: usize,
    discard_receipt: usize,
    portable_mapped_unit_vt: usize,
    tcc: usize,
    globals: usize,
    store: usize,
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();
static LAST_ERROR: Mutex<Option<Vec<u8>>> = Mutex::new(None);
static LIVE_UNITS: Mutex<Vec<usize>> = Mutex::new(Vec::new());
#[derive(Clone)]
struct ModuleSourceEntry {
    name: usize,
    source: usize,
    artifact_selection: Option<Vec<u8>>,
}

static MODULE_SOURCES: Mutex<Vec<ModuleSourceEntry>> = Mutex::new(Vec::new());
static INITIAL_GENERATION_SEALED: Mutex<bool> = Mutex::new(false);
static LIVE_ARTIFACT_HANDLES: Mutex<Vec<usize>> = Mutex::new(Vec::new());
static CANDIDATES: Mutex<Vec<CandidateState>> = Mutex::new(Vec::new());
static PREPARED: Mutex<Vec<PreparedState>> = Mutex::new(Vec::new());
static CANDIDATE_ROOTS: Mutex<Vec<CandidateRoot>> = Mutex::new(Vec::new());
static VTABLE_CACHE_ROOTS: Mutex<Vec<VtableCacheRoot>> = Mutex::new(Vec::new());
static MATERIALIZATIONS: Mutex<Vec<MaterializationEntry>> = Mutex::new(Vec::new());
static CODE_BINDINGS: Mutex<Vec<CodeBindingEntry>> = Mutex::new(Vec::new());
static PORTABLE_IMAGE_STAGE: Mutex<Option<PortableImageStage>> = Mutex::new(None);
static INITIALIZER_TRANSACTION: Mutex<Option<InitializerTransaction>> = Mutex::new(None);
static RETAINED_GENERATIONS: Mutex<Vec<RetainedGeneration>> = Mutex::new(Vec::new());
static MAPPED_IMAGE_RESUMABLES: Mutex<Option<MappedImageResumables>> = Mutex::new(None);
static ROOT_REGISTERED: OnceLock<()> = OnceLock::new();
static STORE_CALLBACKS_REGISTERED: AtomicBool = AtomicBool::new(false);
static MOP_MUTATION_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
static CANDIDATE_PREPARING: AtomicBool = AtomicBool::new(false);
static HOST_CAPABILITY_LOADING: AtomicBool = AtomicBool::new(false);
static IMAGE_DISPATCH_RECOVERY: AtomicBool = AtomicBool::new(false);
static IMAGE_CODE_LOAD_SEQUENCE: AtomicUsize = AtomicUsize::new(0);
static NEXT_CODE_GENERATION_LEASE: AtomicU64 = AtomicU64::new(1);
static NEXT_MATERIALIZATION_NAMESPACE: AtomicU64 = AtomicU64::new(1);
static NEXT_MATERIALIZATION_AGE: AtomicU64 = AtomicU64::new(1);
static NEXT_PREPARED_TOKEN: AtomicU64 = AtomicU64::new(1);
static RUNTIME_PHASE: AtomicU8 = AtomicU8::new(0);
static IMAGE_PREVIOUS_PHASE: AtomicU8 = AtomicU8::new(0);
static ACTIVE_GENERATION: AtomicUsize = AtomicUsize::new(0);
static METHOD_OP_PRESENT: AtomicBool = AtomicBool::new(false);

const MAX_MATERIALIZATION_CACHE_ENTRIES: usize = 64;
const MAX_MATERIALIZATION_CACHE_PROGRAM_BYTES: usize = 64 << 20;
const MAX_MATERIALIZATION_C_SOURCE_BYTES: usize = 128 << 20;
const MAX_CURRENT_CODE_BINDINGS: usize = 262_144;

struct MaterializationEntry {
    disposition: MaterializationDisposition,
    digest: [u8; 32],
    program: Vec<u8>,
    vtable: usize,
    unit: usize,
    generated_bytes: usize,
    age: u64,
    ready: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MaterializationDisposition {
    Synchronous,
    Resumable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CodeBindingLifecycle {
    Staging,
    Current,
    Retiring,
}

/// Process-local carrier for the current semantic method association and its
/// physical NamedAdapter. Durable image-v8 meaning excludes `unit`,
/// `generation`, `method`, and `lifecycle`; those fields only keep native code
/// attributed until resolve-and-pin quiescence permits retirement.
struct CodeBindingEntry {
    target: usize,
    vtable: usize,
    selector: usize,
    implementation: usize,
    provenance: usize,
    unit: usize,
    generation: u64,
    method: usize,
    authority: [u8; 32],
    generated_bytes: usize,
    lifecycle: CodeBindingLifecycle,
}

fn canonical_exports_requested() -> bool {
    // The canonical dynamic name is the production Rust Codegen path after
    // C1.6. Fixture tests can still exercise the private Rust surface by
    // explicitly setting this switch to a false value.
    std::env::var_os(CANONICAL_ENABLE_ENV).is_none_or(|value| {
        let value = value.as_encoded_bytes();
        value != b"0" && value != b"false" && value != b"no"
    })
}

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
    artifact_selection: Vec<u8>,
    resumable_manifest: Vec<u8>,
    expected_predecessor_digest: [u8; 64],
    projected_authority: Option<[u8; 32]>,
    projected_selection: Vec<u8>,
    requires: Vec<Vec<u8>>,
    operations: Vec<CandidateOperationState>,
    initializer_writes: Vec<CandidateGlobalMutation>,
    handle: usize,
    validate: usize,
    prepare: usize,
    activate: usize,
    rollback: usize,
    prepared: usize,
    active: bool,
    installed_method_ever: bool,
    all_installed_methods_resumable: bool,
    predecessor: usize,
    rollback_leases: Vec<StacklessGenerationLeaseV1>,
}

struct RetainedGeneration {
    unit: usize,
    handle: usize,
    mapped_bytes: usize,
    runtime_retirement_eligible: bool,
    runtime_retired: bool,
    rollback_leases: Vec<StacklessGenerationLeaseV1>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
struct StacklessGenerationLeaseV1 {
    token_index: u64,
    token_generation: u64,
}

#[derive(Clone)]
struct MappedImageResumables;

#[repr(C)]
struct PortableMappedUnitPayload {
    handle: usize,
    code_generation_identity: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PortableImageCarrierKind {
    Rebuilt,
    Mapped,
}

struct PortableImageStage {
    carrier_kind: PortableImageCarrierKind,
    owner: usize,
    handle: usize,
    generation: u64,
    authority: [u8; 32],
    bindings: Vec<u8>,
    binding_count: u32,
    program_count: u32,
    generated_bytes: usize,
    finish: usize,
    cancel: usize,
    end_context: usize,
    prepare_binding: usize,
    binding_entry_address: usize,
    committed: bool,
    prepared_bindings: u32,
    installed_bindings: u32,
}

#[repr(C)]
struct DiscardReceiptPayload {
    generation: Oop,
    logical: Oop,
    physical: Oop,
    reason: Oop,
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
    generation: u64,
    index: isize,
    value: usize,
}

#[derive(Clone, Copy)]
struct VtableCacheRoot {
    generation: u64,
    index: u64,
    value: usize,
}

#[derive(Clone, Copy, Default)]
struct PublicationPlanState {
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

#[derive(Clone)]
struct CandidateGlobalMutation {
    owner: usize,
    name: Vec<u8>,
    key: usize,
    prior: usize,
    post: usize,
}

struct InitializerTransaction {
    unit: usize,
    writes: Vec<CandidateGlobalMutation>,
}

#[derive(Clone, Copy)]
#[repr(C)]
struct StoreCompensationApiV1 {
    abi_version: u32,
    byte_size: usize,
    reserve: unsafe extern "C" fn(usize, usize) -> c_int,
    consume: unsafe extern "C" fn(usize, usize) -> c_int,
    release: unsafe extern "C" fn(usize) -> usize,
    reserved: unsafe extern "C" fn(usize) -> usize,
}

static STORE_COMPENSATION_API: OnceLock<StoreCompensationApiV1> = OnceLock::new();

#[unsafe(no_mangle)]
/// Configure the private Store compensation seam selected by the host.
///
/// This versioned table is intentionally independent of frozen `IdApi` and is
/// never projected into source or image state.
pub unsafe extern "C" fn birnam_codegen_store_compensation_configure_v1(
    table: *const c_void,
) -> c_int {
    let Some(table) = (unsafe { table.cast::<StoreCompensationApiV1>().as_ref() }) else {
        return 0;
    };
    if table.abi_version != 1 || table.byte_size != size_of::<StoreCompensationApiV1>() {
        return 0;
    }
    match STORE_COMPENSATION_API.set(*table) {
        Ok(()) => 1,
        Err(_) if STORE_COMPENSATION_API.get().is_some() => 1,
        Err(_) => 0,
    }
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

unsafe fn error_value(api: &IdApi, kind: &[u8]) -> Oop {
    let factory = unsafe { (api.import)(c"Error".as_ptr()) };
    if factory.is_null() || factory == api.nilObj {
        return api.nilObj;
    }
    let kind = unsafe { intern_bytes(api, kind) };
    unsafe { send(api, factory, b"kind:\0", &[kind]) }
}

unsafe fn note_syntax_error(api: &IdApi, lang: Oop, source_object: Oop) {
    let offset = unsafe { send0(api, lang, FURTHEST_FAIL_SELECTOR) };
    if unsafe { (api.isSmallInt)(offset) } == 0 {
        note_error(b"RustCodegen syntax error");
        return;
    }
    let source = unsafe { string_bytes(api, source_object) }.unwrap_or_default();
    let raw_offset = unsafe { (api.smallIntValue)(offset) };
    let clamped = raw_offset.clamp(0, source.len() as i64) as usize;
    let line_start = source[..clamped]
        .iter()
        .rposition(|&byte| byte == b'\n')
        .map_or(0, |index| index + 1);
    let line = source[..clamped]
        .iter()
        .filter(|&&byte| byte == b'\n')
        .count()
        + 1;
    let column = clamped - line_start + 1;
    let line_end = source[clamped..]
        .iter()
        .position(|&byte| byte == b'\n')
        .map_or(source.len(), |index| clamped + index);
    let mut message = format!("{line}:{column}: syntax error\n").into_bytes();
    message.extend_from_slice(&source[line_start..line_end]);
    message.push(b'\n');
    for &byte in &source[line_start..clamped] {
        message.push(if byte == b'\t' { b'\t' } else { b' ' });
    }
    message.push(b'^');
    note_error(&message);
}

unsafe extern "C" fn mark_live_units(mark: MarkFn) {
    if let Some(runtime) = RUNTIME.get() {
        for value in [
            runtime.rust_codegen_vt,
            runtime.rust_codegen,
            runtime.adapter_codegen_vt,
            runtime.adapter_codegen,
            runtime.adapter_reflect_vt,
            runtime.adapter_reflect,
            runtime.adapter_collector_vt,
            runtime.discard_receipt_vt,
            runtime.discard_receipt,
            runtime.portable_mapped_unit_vt,
            runtime.tcc,
            runtime.globals,
            runtime.store,
        ] {
            // SAFETY: Runtime fields are process-owned Oops cached by this
            // module and remain semantically live while the module is loaded.
            unsafe { mark(value as Oop) };
        }
    }
    if let Ok(units) = LIVE_UNITS.lock() {
        for &unit in units.iter() {
            // SAFETY: each address was returned by IdApi.alloc and remains rooted
            // by this provider until process teardown.
            unsafe { mark(unit as Oop) };
        }
    }
    if let Ok(materializations) = MATERIALIZATIONS.lock() {
        for entry in materializations.iter() {
            // SAFETY: bounded ready entries form an explicit cache lease;
            // staging entries protect their unit and vtable across
            // allocation-capable setup. After eviction, live Block vtables
            // retain the same unit through their two-edge private carrier.
            unsafe {
                if entry.vtable != 0 {
                    mark(entry.vtable as Oop);
                }
                if entry.unit != 0 {
                    mark(entry.unit as Oop);
                }
            }
        }
    }
    if let Ok(bindings) = CODE_BINDINGS.lock() {
        for binding in bindings.iter() {
            // Current semantic bindings retain target, selector,
            // implementation, provenance, and native owner. Retiring rows
            // retain the same graph only until Runtime generation quiescence;
            // staging rows make publication allocation-safe.
            for value in [
                binding.target,
                binding.vtable,
                binding.selector,
                binding.implementation,
                binding.provenance,
                binding.unit,
            ] {
                if value != 0 {
                    unsafe { mark(value as Oop) };
                }
            }
        }
    }
    if let Ok(stage) = PORTABLE_IMAGE_STAGE.lock()
        && let Some(stage) = stage.as_ref()
        && stage.owner != 0
    {
        // SAFETY: an active recovery stage owns this Oop until commit or
        // compensation transfers/releases it.
        unsafe { mark(stage.owner as Oop) };
    }
    let Ok(sources) = MODULE_SOURCES.lock() else {
        return;
    };
    for entry in sources.iter() {
        // SAFETY: each address was returned by IdApi.alloc and remains rooted
        // by this provider until process teardown.
        unsafe {
            mark(entry.name as Oop);
            mark(entry.source as Oop);
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
                for write in &candidate.initializer_writes {
                    mark(write.key as Oop);
                    if write.prior != 0 {
                        mark(write.prior as Oop);
                    }
                    if write.post != 0 {
                        mark(write.post as Oop);
                    }
                }
            }
        }
    }
    if let Ok(transaction) = INITIALIZER_TRANSACTION.lock()
        && let Some(transaction) = transaction.as_ref()
    {
        for write in &transaction.writes {
            unsafe {
                mark(write.key as Oop);
                if write.prior != 0 {
                    mark(write.prior as Oop);
                }
                if write.post != 0 {
                    mark(write.post as Oop);
                }
            }
        }
    }
    if let Ok(retained) = RETAINED_GENERATIONS.lock() {
        for generation in retained.iter() {
            // SAFETY: retained native mappings keep their process-local
            // generation identity rooted for telemetry and discard receipts.
            unsafe { mark(generation.unit as Oop) };
        }
    }
    if let Ok(prepared) = PREPARED.lock() {
        for state in prepared.iter() {
            // SAFETY: prepared token IDs are native integers; only the plan
            // Oops need tracing until the token is consumed or discarded.
            unsafe {
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
            // SAFETY: candidate root slots are generated Oops retained while
            // their mapped generation is admitted. Logical discard withdraws
            // the whole generation before collection and physical retirement.
            unsafe { mark(root.value as Oop) };
        }
    }
    if let Ok(roots) = VTABLE_CACHE_ROOTS.lock() {
        for root in roots.iter() {
            // SAFETY: generated code publishes only vtable Oops. The cache is
            // process-owned rather than mapping-owned, so its provider remains
            // callable while candidate mappings quiesce or unload.
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

fn begin_initializer_transaction(unit: usize) -> bool {
    let Ok(mut transaction) = INITIALIZER_TRANSACTION.lock() else {
        return false;
    };
    if transaction.is_some() {
        return false;
    }
    *transaction = Some(InitializerTransaction {
        unit,
        writes: Vec::new(),
    });
    true
}

fn take_initializer_transaction(unit: usize) -> Option<Vec<CandidateGlobalMutation>> {
    let mut transaction = INITIALIZER_TRANSACTION.lock().ok()?;
    let active = transaction.take()?;
    (active.unit == unit).then_some(active.writes)
}

fn reserve_initializer_compensation(owner: usize, facts: usize) -> bool {
    STORE_COMPENSATION_API
        .get()
        .is_some_and(|table| unsafe { (table.reserve)(owner, facts) == 1 })
}

fn consume_initializer_compensation(owner: usize, facts: usize) -> bool {
    STORE_COMPENSATION_API
        .get()
        .is_some_and(|table| unsafe { (table.consume)(owner, facts) == 1 })
}

fn release_initializer_compensation(owner: usize) -> usize {
    STORE_COMPENSATION_API
        .get()
        .map_or(0, |table| unsafe { (table.release)(owner) })
}

fn release_initializer_compensations(writes: &[CandidateGlobalMutation]) {
    let mut owners = BTreeSet::new();
    for write in writes {
        if owners.insert(write.owner) {
            let _ = release_initializer_compensation(write.owner);
        }
    }
}

unsafe fn current_global_raw(api: &IdApi, key: Oop) -> Oop {
    unsafe {
        send(
            api,
            runtime().store as Oop,
            STORE_RAW_CURRENT_SELECTOR,
            &[intern(api, GLOBALS_ENTITY_NAME), key],
        )
    }
}

unsafe fn restore_initializer_writes(api: &IdApi, writes: &[CandidateGlobalMutation]) -> bool {
    let store = runtime().store as Oop;
    let entity = unsafe { intern(api, GLOBALS_ENTITY_NAME) };
    for write in writes.iter().rev() {
        let key = write.key as Oop;
        let current = unsafe { current_global_raw(api, key) };
        if current as usize != write.post {
            return false;
        }
        if !current.is_null() {
            if !consume_initializer_compensation(write.owner, 1)
                || unsafe { send(api, store, STORE_RETRACT_SELECTOR, &[entity, key, current]) }
                    != store
            {
                return false;
            }
        }
        if write.prior != 0 {
            if !consume_initializer_compensation(write.owner, 1)
                || unsafe {
                    send(
                        api,
                        store,
                        STORE_ASSERT_SELECTOR,
                        &[entity, key, write.prior as Oop],
                    )
                } != store
            {
                return false;
            }
        }
    }
    release_initializer_compensations(writes);
    true
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
    let transaction_write = INITIALIZER_TRANSACTION.lock().ok().and_then(|transaction| {
        transaction.as_ref().map(|transaction| {
            let is_new = !transaction
                .writes
                .iter()
                .any(|write| write.key == key as usize);
            (transaction.unit, is_new)
        })
    });
    let pending_write = transaction_write.and_then(|(owner, is_new)| {
        is_new.then(|| CandidateGlobalMutation {
            owner,
            name: unsafe { CStr::from_ptr(name) }.to_bytes().to_vec(),
            key: key as usize,
            prior: unsafe { current_global_raw(api, key) } as usize,
            post: value as usize,
        })
    });
    if let Some(pending_write) = &pending_write
        && !reserve_initializer_compensation(pending_write.owner, 2)
    {
        return unsafe { error_value(api, b"historyResourceLimit") };
    }
    let result = unsafe { send(api, globals, GLOBAL_AT_PUT_SELECTOR, &[key, value]) };
    if result != value {
        if let Some(pending_write) = &pending_write {
            if !consume_initializer_compensation(pending_write.owner, 2) {
                abort("candidate initializer compensation reservation was corrupted");
            }
        }
        return result;
    }
    if let Some(pending_write) = pending_write {
        let Ok(mut transaction) = INITIALIZER_TRANSACTION.lock() else {
            return api.nilObj;
        };
        let Some(transaction) = transaction.as_mut() else {
            return api.nilObj;
        };
        if let Some(write) = transaction
            .writes
            .iter_mut()
            .find(|write| write.key == key as usize)
        {
            write.post = value as usize;
        } else {
            transaction.writes.push(pending_write);
        }
    }
    result
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
    let Some(name) = (unsafe { reflect_symbol_name(api, symbol) }) else {
        return api.nilObj;
    };
    let name =
        CString::new(name).unwrap_or_else(|_| abort("Symbol name unexpectedly contained NUL"));
    // SAFETY: the owned CString remains live for the complete lookup.
    unsafe { birnam_rust_codegen_global(name.as_ptr()) }
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

/// Frozen generated-artifact spelling for the Rust-owned baked-global lookup.
///
/// The Rust Codegen dylib keeps the implementation-specific exports above so
/// mixed-floor fixtures can identify their owner, while production artifacts
/// continue to link against the shared `birnam_*` ABI.
#[unsafe(export_name = "birnam_global_b")]
pub unsafe extern "C" fn birnam_global_b(symbol: Oop) -> Oop {
    unsafe { birnam_rust_codegen_global_b(symbol) }
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

#[unsafe(export_name = "birnam_record_global_src")]
pub unsafe extern "C" fn birnam_record_global_src(
    name: *const c_char,
    source: *const u8,
    length: c_long,
) {
    unsafe { birnam_rust_codegen_record_global_src(name, source, length) }
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
        modules.push(ModuleSourceEntry {
            name: key as usize,
            source: source as usize,
            artifact_selection: None,
        });
    }
}

#[unsafe(export_name = "birnam_record_module_source")]
pub unsafe extern "C" fn birnam_record_module_source(
    name: *const c_char,
    source: *const u8,
    length: c_long,
) {
    unsafe { birnam_rust_codegen_record_module_source(name, source, length) }
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

#[unsafe(export_name = "birnam_is_error")]
pub unsafe extern "C" fn birnam_is_error(value: Oop) -> c_long {
    unsafe { birnam_rust_codegen_is_error(value) }
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

#[unsafe(export_name = "birnam_install_method")]
pub unsafe extern "C" fn birnam_install_method(
    vtable: Oop,
    selector: Oop,
    method: *mut c_void,
    source: *const u8,
    length: c_long,
) -> Oop {
    unsafe { birnam_rust_codegen_install_method(vtable, selector, method, source, length) }
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
    if CANDIDATE_PREPARING.load(Ordering::Acquire) {
        return runtime.store as Oop;
    }
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

#[unsafe(export_name = "birnam_remove_method")]
pub unsafe extern "C" fn birnam_remove_method(
    target: Oop,
    selector: Oop,
    source: *const u8,
    length: c_long,
) -> Oop {
    unsafe { birnam_rust_codegen_remove_method(target, selector, source, length) }
}

#[unsafe(export_name = "birnam_rust_codegen_candidate_root_set")]
/// Store one generated candidate capture in the Rust root provider.
///
/// # Safety
/// `value` must be a live Oop or a null ABI carrier.
pub unsafe extern "C" fn birnam_rust_codegen_candidate_root_set(
    generation: u64,
    index: c_long,
    value: Oop,
) -> Oop {
    let value = value as usize;
    let index = index as isize;
    if let Ok(mut roots) = CANDIDATE_ROOTS.lock() {
        if let Some(root) = roots
            .iter_mut()
            .find(|root| root.generation == generation && root.index == index)
        {
            root.value = value;
        } else {
            roots.push(CandidateRoot {
                generation,
                index,
                value,
            });
        }
    }
    value as Oop
}

#[unsafe(export_name = "birnam_candidate_root_set")]
pub unsafe extern "C" fn birnam_candidate_root_set(
    generation: u64,
    index: c_long,
    value: Oop,
) -> Oop {
    unsafe { birnam_rust_codegen_candidate_root_set(generation, index, value) }
}

#[unsafe(export_name = "birnam_rust_codegen_candidate_root_get")]
/// Load one generated candidate capture from the Rust root provider.
///
/// # Safety
/// The call must occur while the Rust Codegen module remains initialized.
pub unsafe extern "C" fn birnam_rust_codegen_candidate_root_get(
    generation: u64,
    index: c_long,
) -> Oop {
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    let index = index as isize;
    CANDIDATE_ROOTS
        .lock()
        .ok()
        .and_then(|roots| {
            roots
                .iter()
                .find(|root| root.generation == generation && root.index == index)
                .copied()
        })
        .map(|root| root.value as Oop)
        .unwrap_or(api.nilObj)
}

#[unsafe(export_name = "birnam_candidate_root_get")]
pub unsafe extern "C" fn birnam_candidate_root_get(generation: u64, index: c_long) -> Oop {
    unsafe { birnam_rust_codegen_candidate_root_get(generation, index) }
}

#[unsafe(export_name = "birnam_rust_codegen_vtable_cache_get")]
pub unsafe extern "C" fn birnam_rust_codegen_vtable_cache_get(generation: u64, index: u64) -> Oop {
    if generation == 0 {
        return core::ptr::null_mut();
    }
    VTABLE_CACHE_ROOTS
        .lock()
        .ok()
        .and_then(|roots| {
            roots
                .iter()
                .find(|root| root.generation == generation && root.index == index)
                .map(|root| root.value as Oop)
        })
        .unwrap_or(core::ptr::null_mut())
}

#[unsafe(export_name = "birnam_vtable_cache_get")]
pub unsafe extern "C" fn birnam_vtable_cache_get(generation: u64, index: u64) -> Oop {
    unsafe { birnam_rust_codegen_vtable_cache_get(generation, index) }
}

#[unsafe(export_name = "birnam_rust_codegen_vtable_cache_set")]
pub unsafe extern "C" fn birnam_rust_codegen_vtable_cache_set(
    generation: u64,
    index: u64,
    value: Oop,
) -> Oop {
    if generation == 0 || value.is_null() {
        return core::ptr::null_mut();
    }
    let value = value as usize;
    if let Ok(mut roots) = VTABLE_CACHE_ROOTS.lock() {
        if let Some(root) = roots
            .iter_mut()
            .find(|root| root.generation == generation && root.index == index)
        {
            root.value = value;
        } else {
            roots.push(VtableCacheRoot {
                generation,
                index,
                value,
            });
        }
    }
    value as Oop
}

#[unsafe(export_name = "birnam_vtable_cache_set")]
pub unsafe extern "C" fn birnam_vtable_cache_set(generation: u64, index: u64, value: Oop) -> Oop {
    unsafe { birnam_rust_codegen_vtable_cache_set(generation, index, value) }
}

unsafe fn portable_code_api(api: &IdApi) -> Option<&'static PortableCodeApiV1> {
    let object = unsafe { (api.import)(c"_BirnamPortableCodeApiV1".as_ptr()) };
    let vtable = unsafe { (api.import)(c"_BirnamPortableCodeApiV1Vt".as_ptr()) };
    if object.is_null()
        || object == api.nilObj
        || vtable.is_null()
        || vtable == api.nilObj
        || unsafe { (api.vtableOf)(object) } != vtable
        || unsafe { (api.objectPayloadSize)(object) } != size_of::<PortableCodeApiV1>()
    {
        return None;
    }
    let capability = unsafe { &*object.cast::<PortableCodeApiV1>() };
    if capability.abi_version != PORTABLE_CODE_API_V1_VERSION
        || capability.struct_size != size_of::<PortableCodeApiV1>() as u64
    {
        return None;
    }
    Some(capability)
}

unsafe fn portable_code_api_v2(api: &IdApi) -> Option<&'static PortableCodeApiV2> {
    let object = unsafe { (api.import)(c"_BirnamPortableCodeApiV2".as_ptr()) };
    let vtable = unsafe { (api.import)(c"_BirnamPortableCodeApiV2Vt".as_ptr()) };
    if object.is_null()
        || object == api.nilObj
        || vtable.is_null()
        || vtable == api.nilObj
        || unsafe { (api.vtableOf)(object) } != vtable
        || unsafe { (api.objectPayloadSize)(object) } != size_of::<PortableCodeApiV2>()
    {
        return None;
    }
    let capability = unsafe { &*object.cast::<PortableCodeApiV2>() };
    if capability.abi_version != PORTABLE_CODE_API_V2_VERSION
        || capability.struct_size != size_of::<PortableCodeApiV2>() as u64
    {
        return None;
    }
    Some(capability)
}

unsafe fn portable_program_view_api(api: &IdApi) -> Option<&'static PortableProgramViewApiV1> {
    let object = unsafe { (api.import)(c"_BirnamPortableProgramViewApiV1".as_ptr()) };
    let vtable = unsafe { (api.import)(c"_BirnamPortableProgramViewApiV1Vt".as_ptr()) };
    if object.is_null()
        || object == api.nilObj
        || vtable.is_null()
        || vtable == api.nilObj
        || unsafe { (api.vtableOf)(object) } != vtable
        || unsafe { (api.objectPayloadSize)(object) } != size_of::<PortableProgramViewApiV1>()
    {
        return None;
    }
    let capability = unsafe { &*object.cast::<PortableProgramViewApiV1>() };
    if capability.abi_version != PORTABLE_PROGRAM_VIEW_API_V1_VERSION
        || capability.struct_size != size_of::<PortableProgramViewApiV1>() as u64
    {
        return None;
    }
    Some(capability)
}

unsafe fn copy_code_object_program(api: &IdApi, code_object: Oop) -> Option<Vec<u8>> {
    let capability = unsafe { portable_program_view_api(api) }?;
    let length = unsafe { (capability.copy_program)(code_object, ptr::null_mut(), 0) };
    if length == 0 || length > 64 << 20 {
        return None;
    }
    let mut bytes = vec![0; length];
    if unsafe { (capability.copy_program)(code_object, bytes.as_mut_ptr(), bytes.len()) } != length
        || birnam_codegen::portable::program_signature(&bytes).is_err()
    {
        return None;
    }
    Some(bytes)
}

#[unsafe(export_name = "birnam_rust_codegen_portable_code_admit_v1")]
pub unsafe extern "C" fn birnam_rust_codegen_portable_code_admit_v1(
    bytes: *const u8,
    length: usize,
) -> Oop {
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    let Some(capability) = (unsafe { portable_code_api(api) }) else {
        return api.nilObj;
    };
    unsafe { (capability.admit_program)(bytes, length) }
}

#[unsafe(export_name = "birnam_portable_code_admit_v1")]
pub unsafe extern "C" fn birnam_portable_code_admit_v1(bytes: *const u8, length: usize) -> Oop {
    unsafe { birnam_rust_codegen_portable_code_admit_v1(bytes, length) }
}

#[unsafe(export_name = "birnam_rust_codegen_portable_block_new_v1")]
pub unsafe extern "C" fn birnam_rust_codegen_portable_block_new_v1(
    block_vtable: Oop,
    code_object: Oop,
    captures: *const Oop,
    capture_count: usize,
) -> Oop {
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    let Some(capability) = (unsafe { portable_code_api(api) }) else {
        return api.nilObj;
    };
    unsafe { (capability.new_block)(block_vtable, code_object, captures, capture_count) }
}

#[unsafe(export_name = "birnam_portable_block_new_v1")]
pub unsafe extern "C" fn birnam_portable_block_new_v1(
    block_vtable: Oop,
    code_object: Oop,
    captures: *const Oop,
    capture_count: usize,
) -> Oop {
    unsafe {
        birnam_rust_codegen_portable_block_new_v1(
            block_vtable,
            code_object,
            captures,
            capture_count,
        )
    }
}

#[unsafe(export_name = "birnam_rust_codegen_portable_materialized_block_new_v2")]
pub unsafe extern "C" fn birnam_rust_codegen_portable_materialized_block_new_v2(
    block_vtable: Oop,
    code_object: Oop,
    captures: *const Oop,
    capture_count: usize,
    materialization_owner: Oop,
) -> Oop {
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    let Some(capability) = (unsafe { portable_code_api_v2(api) }) else {
        return api.nilObj;
    };
    unsafe {
        (capability.new_materialized_block)(
            block_vtable,
            code_object,
            captures,
            capture_count,
            materialization_owner,
        )
    }
}

#[unsafe(export_name = "birnam_portable_materialized_block_new_v2")]
pub unsafe extern "C" fn birnam_portable_materialized_block_new_v2(
    block_vtable: Oop,
    code_object: Oop,
    captures: *const Oop,
    capture_count: usize,
    materialization_owner: Oop,
) -> Oop {
    unsafe {
        birnam_rust_codegen_portable_materialized_block_new_v2(
            block_vtable,
            code_object,
            captures,
            capture_count,
            materialization_owner,
        )
    }
}

#[unsafe(export_name = "birnam_rust_codegen_candidate_prepare_mode")]
/// Toggle candidate preparation mode for generated method-install shims.
///
/// # Safety
/// The caller must use the generated candidate lifecycle protocol.
pub unsafe extern "C" fn birnam_rust_codegen_candidate_prepare_mode(on: c_long) {
    CANDIDATE_PREPARING.store(on != 0, Ordering::Release);
}

#[unsafe(export_name = "birnam_candidate_prepare_mode")]
pub unsafe extern "C" fn birnam_candidate_prepare_mode(on: c_long) {
    unsafe { birnam_rust_codegen_candidate_prepare_mode(on) }
}

#[unsafe(export_name = "birnam_rust_codegen_candidate_preparing")]
pub extern "C" fn birnam_rust_codegen_candidate_preparing() -> c_long {
    CANDIDATE_PREPARING.load(Ordering::Acquire) as c_long
}

#[unsafe(export_name = "birnam_candidate_preparing")]
pub extern "C" fn birnam_candidate_preparing() -> c_long {
    birnam_rust_codegen_candidate_preparing()
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

#[unsafe(export_name = "birnam_candidate_validate_binding")]
pub unsafe extern "C" fn birnam_candidate_validate_binding(
    target_name: *const c_char,
    selector_name: *const c_char,
    arity: c_long,
    removal: c_long,
) -> Oop {
    unsafe {
        birnam_rust_codegen_candidate_validate_binding(target_name, selector_name, arity, removal)
    }
}

unsafe fn bind_global_shims(api: &IdApi, unit: Oop) -> bool {
    let name = unsafe { (api.mkString)(b"birnam_global".as_ptr(), 13) };
    let Some(address) = (unsafe { borrowed_address(api, birnam_global as *const () as usize) })
    else {
        return false;
    };
    if unsafe { send(api, unit, ADD_SYMBOL_SELECTOR, &[name, address]) } == api.nilObj {
        return false;
    }
    let name = unsafe { (api.mkString)(b"birnam_global_b".as_ptr(), 15) };
    let Some(address) =
        (unsafe { borrowed_address(api, birnam_rust_codegen_global_b as *const () as usize) })
    else {
        return false;
    };
    if unsafe { send(api, unit, ADD_SYMBOL_SELECTOR, &[name, address]) } == api.nilObj {
        return false;
    }
    let name = unsafe { (api.mkString)(b"birnam_global_set".as_ptr(), 17) };
    let Some(address) = (unsafe { borrowed_address(api, birnam_global_set as *const () as usize) })
    else {
        return false;
    };
    if unsafe { send(api, unit, ADD_SYMBOL_SELECTOR, &[name, address]) } == api.nilObj {
        return false;
    }
    let name = unsafe { (api.mkString)(b"birnam_vtable_cache_get".as_ptr(), 23) };
    let Some(address) =
        (unsafe { borrowed_address(api, birnam_vtable_cache_get as *const () as usize) })
    else {
        return false;
    };
    if unsafe { send(api, unit, ADD_SYMBOL_SELECTOR, &[name, address]) } == api.nilObj {
        return false;
    }
    let name = unsafe { (api.mkString)(b"birnam_vtable_cache_set".as_ptr(), 23) };
    let Some(address) =
        (unsafe { borrowed_address(api, birnam_vtable_cache_set as *const () as usize) })
    else {
        return false;
    };
    if unsafe { send(api, unit, ADD_SYMBOL_SELECTOR, &[name, address]) } == api.nilObj {
        return false;
    }
    let name = unsafe { (api.mkString)(b"birnam_portable_code_admit_v1".as_ptr(), 29) };
    let Some(address) =
        (unsafe { borrowed_address(api, birnam_portable_code_admit_v1 as *const () as usize) })
    else {
        return false;
    };
    if unsafe { send(api, unit, ADD_SYMBOL_SELECTOR, &[name, address]) } == api.nilObj {
        return false;
    }
    let name = unsafe { (api.mkString)(b"birnam_portable_block_new_v1".as_ptr(), 28) };
    let Some(address) =
        (unsafe { borrowed_address(api, birnam_portable_block_new_v1 as *const () as usize) })
    else {
        return false;
    };
    if unsafe { send(api, unit, ADD_SYMBOL_SELECTOR, &[name, address]) } == api.nilObj {
        return false;
    }
    let name = unsafe {
        (api.mkString)(
            b"birnam_portable_materialized_block_new_v2".as_ptr(),
            b"birnam_portable_materialized_block_new_v2".len(),
        )
    };
    let Some(address) = (unsafe {
        borrowed_address(
            api,
            birnam_portable_materialized_block_new_v2 as *const () as usize,
        )
    }) else {
        return false;
    };
    if unsafe { send(api, unit, ADD_SYMBOL_SELECTOR, &[name, address]) } == api.nilObj {
        return false;
    }
    let Some(registration) = (unsafe { floor_symbol(api, b"birnam_register_resumable_v1\0") })
    else {
        return false;
    };
    let name = unsafe { (api.mkString)(b"birnam_register_resumable_v1".as_ptr(), 28) };
    let Some(address) = (unsafe { borrowed_address(api, registration) }) else {
        return false;
    };
    if unsafe { send(api, unit, ADD_SYMBOL_SELECTOR, &[name, address]) } == api.nilObj {
        return false;
    }
    let Some(registration) = (unsafe { floor_symbol(api, b"birnam_register_resumable_v2\0") })
    else {
        return false;
    };
    let name = unsafe { (api.mkString)(b"birnam_register_resumable_v2".as_ptr(), 28) };
    let Some(address) = (unsafe { borrowed_address(api, registration) }) else {
        return false;
    };
    unsafe { send(api, unit, ADD_SYMBOL_SELECTOR, &[name, address]) != api.nilObj }
}

unsafe fn borrowed_address(api: &IdApi, address: usize) -> Option<Oop> {
    unsafe { foreign_address::allocate(api, address, KIND_BORROWED) }
}

unsafe fn foreign_address_value(api: &IdApi, object: Oop) -> Option<usize> {
    unsafe { foreign_address::value(api, object) }.map(|(address, _kind)| address)
}

unsafe fn bind_jit_code_generation_identity(api: &IdApi, unit: Oop) -> bool {
    let name = unsafe {
        (api.mkString)(
            b"__birnam_code_generation_identity".as_ptr(),
            b"__birnam_code_generation_identity".len(),
        )
    };
    let symbol = unsafe { send(api, unit, SYMBOL_SELECTOR, &[name]) };
    let Some(address) = (unsafe { foreign_address_value(api, symbol) }) else {
        return false;
    };
    if address == 0 {
        return false;
    }
    // SAFETY: the generated translation unit exports one writable u64 carrier
    // and TCC keeps it mapped while the Unit object owns the generation.
    unsafe { ptr::write(address as *mut u64, unit as usize as u64) };
    true
}

unsafe fn bind_mapped_code_generation_identity(
    api: &IdApi,
    handle: *mut c_void,
    generation: u64,
) -> bool {
    if generation == 0 {
        return false;
    }
    let symbol = unsafe { (api.dlsym)(handle, c"__birnam_code_generation_identity".as_ptr()) };
    if symbol.is_null() {
        return false;
    }
    // SAFETY: every current generated artifact exports this writable u64
    // carrier, and the handle remains mapped through this write.
    unsafe { ptr::write(symbol.cast::<u64>(), generation) };
    true
}

#[repr(C)]
struct DlInfo {
    filename: *const c_char,
    base: *mut c_void,
    symbol_name: *const c_char,
    symbol_address: *mut c_void,
}

#[cfg_attr(target_os = "linux", link(name = "dl"))]
unsafe extern "C" {
    fn dladdr(address: *const c_void, info: *mut DlInfo) -> c_int;
}

unsafe fn floor_symbol(api: &IdApi, name: &[u8]) -> Option<usize> {
    if name.last() != Some(&0) {
        return None;
    }
    static FLOOR_HANDLE: OnceLock<usize> = OnceLock::new();
    let handle = *FLOOR_HANDLE.get_or_init(|| {
        let mut info = DlInfo {
            filename: ptr::null(),
            base: ptr::null_mut(),
            symbol_name: ptr::null(),
            symbol_address: ptr::null_mut(),
        };
        // The host deliberately loads the selected floor RTLD_LOCAL. Discover
        // that exact image from one frozen IdApi function pointer, then acquire
        // a process-lifetime handle to the same image for private symbol lookup.
        if unsafe { dladdr(api.dlsym as *const () as *const c_void, &mut info) } == 0
            || info.filename.is_null()
        {
            return 0;
        }
        #[cfg(target_os = "macos")]
        const RTLD_NOW_LOCAL: c_int = 2 | 4;
        #[cfg(not(target_os = "macos"))]
        const RTLD_NOW_LOCAL: c_int = 2;
        unsafe { (api.dlopen)(info.filename, RTLD_NOW_LOCAL) as usize }
    });
    if handle == 0 {
        return None;
    }
    let address = unsafe { (api.dlsym)(handle as *mut c_void, name.as_ptr().cast()) };
    (!address.is_null()).then_some(address as usize)
}

unsafe fn bind_baked_global_shims(api: &IdApi, unit: Oop, baked_globals: &[Vec<u8>]) -> bool {
    for (index, name_bytes) in baked_globals.iter().enumerate() {
        let symbol = unsafe { intern_bytes(api, name_bytes) };
        let mut name = format!("__g{index}").into_bytes();
        let name_object = unsafe { (api.mkString)(name.as_mut_ptr(), name.len()) };
        let Some(address) = (unsafe { borrowed_address(api, symbol as usize) }) else {
            return false;
        };
        if unsafe { send(api, unit, ADD_SYMBOL_SELECTOR, &[name_object, address]) } == api.nilObj {
            return false;
        }
    }
    true
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
    include_source: bool,
    artifact_selection: Option<&[u8]>,
    resumable_manifest: Option<&[u8]>,
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
    if include_source {
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
        output.extend_from_slice(b"UL;\n");
    } else {
        output.extend_from_slice(b"\";\n");
    }
    output.extend_from_slice(b"const unsigned char __birnam_artifact_descriptor[] = {");
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
    if let Some(selection) = artifact_selection {
        output.extend_from_slice(b"const unsigned char __birnam_artifact_selection_v1[] = {");
        for (index, byte) in selection.iter().enumerate() {
            if index != 0 {
                output.push(b',');
            }
            output.extend_from_slice(byte.to_string().as_bytes());
        }
        output.extend_from_slice(
            b"};\nconst unsigned long long __birnam_artifact_selection_v1_size = ",
        );
        output.extend_from_slice(selection.len().to_string().as_bytes());
        output.extend_from_slice(b"ULL;\n");
    }
    if let Some(manifest) = resumable_manifest {
        output.extend_from_slice(b"const unsigned char __birnam_resumable_manifest_v1[] = {");
        for (index, byte) in manifest.iter().enumerate() {
            if index != 0 {
                output.push(b',');
            }
            output.extend_from_slice(byte.to_string().as_bytes());
        }
        output.extend_from_slice(
            b"};\nconst unsigned long long __birnam_resumable_manifest_v1_size = ",
        );
        output.extend_from_slice(manifest.len().to_string().as_bytes());
        output.extend_from_slice(b"ULL;\n");
    }
    output.extend_from_slice(generated);
    output
}

fn candidate_manifest(
    metadata: &RevisionMetadata,
    source_digest: &[u8; 64],
    selection_digest: &[u8; 64],
) -> Vec<u8> {
    let mut manifest = Vec::new();
    manifest.extend_from_slice(b"v4|");
    manifest.extend_from_slice(ARTIFACT_ABI);
    manifest.push(b'|');
    manifest.extend_from_slice(metadata.name.len().to_string().as_bytes());
    manifest.push(b':');
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
    manifest.extend_from_slice(selection_digest);
    manifest.push(b'|');
    manifest.extend_from_slice(b"1:");
    manifest.extend_from_slice(&metadata.revision_digest);
    manifest.push(b'|');
    manifest.extend_from_slice(metadata.requires.len().to_string().as_bytes());
    manifest.push(b':');
    for requirement in &metadata.requires {
        manifest.extend_from_slice(requirement.len().to_string().as_bytes());
        manifest.push(b':');
        manifest.extend_from_slice(requirement);
    }
    manifest.push(b'|');
    manifest.extend_from_slice(metadata.operations.len().to_string().as_bytes());
    manifest.push(b':');
    for operation in &metadata.operations {
        manifest.extend_from_slice(operation.target.len().to_string().as_bytes());
        manifest.push(b':');
        manifest.extend_from_slice(&operation.target);
        manifest.extend_from_slice(operation.selector.len().to_string().as_bytes());
        manifest.push(b':');
        manifest.extend_from_slice(&operation.selector);
        manifest.push(if operation.install { b'I' } else { b'R' });
        manifest.extend_from_slice(selector_arity(&operation.selector).to_string().as_bytes());
        manifest.push(b':');
    }
    manifest
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct OwnedSelectionRequest {
    source_revision: [u8; 32],
    root_kind: RootKind,
    root_index: u32,
    path: Vec<u32>,
    implementation_kind: SelectionImplementationKind,
    disposition: SelectionDisposition,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct OwnedSelectionRequestKey {
    source_revision: [u8; 32],
    root_kind: RootKind,
    root_index: u32,
    path: Vec<u32>,
    implementation_kind: SelectionImplementationKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ParsedSelectionRequests {
    Exact(Vec<OwnedSelectionRequest>),
    WholeUnitResumable,
}

impl OwnedSelectionRequest {
    fn key(&self) -> OwnedSelectionRequestKey {
        OwnedSelectionRequestKey {
            source_revision: self.source_revision,
            root_kind: self.root_kind,
            root_index: self.root_index,
            path: self.path.clone(),
            implementation_kind: self.implementation_kind,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SelectionRequestError {
    Vector,
    Limit,
    Row,
    Tag,
    Digest,
    RootKind,
    Index,
    Path,
    ImplementationKind,
    Disposition,
    Shape,
    Duplicate,
}

fn smallint_u32(api: &IdApi, value: Oop) -> Option<u32> {
    if unsafe { (api.isSmallInt)(value) } == 0 {
        return None;
    }
    u32::try_from(unsafe { (api.smallIntValue)(value) }).ok()
}

unsafe fn parse_selection_requests(
    api: &IdApi,
    request: Oop,
) -> Result<ParsedSelectionRequests, SelectionRequestError> {
    if unsafe { symbol_bytes(api, request) }.as_deref() == Some(b"resumableRequired") {
        return Ok(ParsedSelectionRequests::WholeUnitResumable);
    }
    let vector_vt = unsafe { (api.import)(VECTOR_VT_NAME.as_ptr().cast()) };
    if vector_vt.is_null() || vector_vt == api.nilObj {
        return Err(SelectionRequestError::Vector);
    }
    let count =
        unsafe { vector_length(api, request, vector_vt) }.ok_or(SelectionRequestError::Vector)?;
    if count > MAX_IMPLEMENTATION_ROWS {
        return Err(SelectionRequestError::Limit);
    }
    let mut requests = Vec::with_capacity(count);
    let mut keys = BTreeSet::new();
    for index in 0..count {
        let row = unsafe { vector_item(api, request, index) };
        if unsafe { vector_length(api, row, vector_vt) } != Some(7) {
            return Err(SelectionRequestError::Row);
        }
        let tag = unsafe { symbol_bytes(api, vector_item(api, row, 0)) }
            .ok_or(SelectionRequestError::Tag)?;
        if tag != b"implementationDisposition" {
            return Err(SelectionRequestError::Tag);
        }
        let digest_spelling = unsafe { string_bytes(api, vector_item(api, row, 1)) }
            .ok_or(SelectionRequestError::Digest)?;
        let digest_spelling: [u8; 64] = digest_spelling
            .try_into()
            .map_err(|_| SelectionRequestError::Digest)?;
        let source_revision =
            decode_hex_digest(&digest_spelling).ok_or(SelectionRequestError::Digest)?;
        let root_kind = match unsafe { symbol_bytes(api, vector_item(api, row, 2)) }.as_deref() {
            Some(b"nonMethodProgram") => RootKind::NonMethodProgram,
            Some(b"methodOperation") => RootKind::MethodOperation,
            _ => return Err(SelectionRequestError::RootKind),
        };
        let root_index = smallint_u32(api, unsafe { vector_item(api, row, 3) })
            .ok_or(SelectionRequestError::Index)?;
        let path_object = unsafe { vector_item(api, row, 4) };
        let path_count = unsafe { vector_length(api, path_object, vector_vt) }
            .ok_or(SelectionRequestError::Path)?;
        if path_count > MAX_PATH_DEPTH {
            return Err(SelectionRequestError::Limit);
        }
        let mut path = Vec::with_capacity(path_count);
        for path_index in 0..path_count {
            path.push(
                smallint_u32(api, unsafe { vector_item(api, path_object, path_index) })
                    .ok_or(SelectionRequestError::Path)?,
            );
        }
        let implementation_kind =
            match unsafe { symbol_bytes(api, vector_item(api, row, 5)) }.as_deref() {
                Some(b"namedAdapter") => SelectionImplementationKind::NamedAdapter,
                Some(b"blockBody") => SelectionImplementationKind::BlockBody,
                _ => return Err(SelectionRequestError::ImplementationKind),
            };
        let disposition = match unsafe { symbol_bytes(api, vector_item(api, row, 6)) }.as_deref() {
            Some(b"synchronous") => SelectionDisposition::Synchronous,
            Some(b"resumableRequired") => SelectionDisposition::ResumableRequired,
            _ => return Err(SelectionRequestError::Disposition),
        };
        let valid_shape = match implementation_kind {
            SelectionImplementationKind::NamedAdapter => {
                root_kind == RootKind::MethodOperation && path.is_empty()
            }
            SelectionImplementationKind::BlockBody => !path.is_empty(),
        } && (root_kind != RootKind::NonMethodProgram || root_index == 0);
        if !valid_shape {
            return Err(SelectionRequestError::Shape);
        }
        let key = OwnedSelectionRequestKey {
            source_revision,
            root_kind,
            root_index,
            path: path.clone(),
            implementation_kind,
        };
        if !keys.insert(key) {
            return Err(SelectionRequestError::Duplicate);
        }
        requests.push(OwnedSelectionRequest {
            source_revision,
            root_kind,
            root_index,
            path,
            implementation_kind,
            disposition,
        });
    }
    Ok(ParsedSelectionRequests::Exact(requests))
}

fn expand_selection_requests(
    metadata: &RevisionMetadata,
    requests: ParsedSelectionRequests,
) -> Option<Vec<OwnedSelectionRequest>> {
    match requests {
        ParsedSelectionRequests::Exact(requests) => Some(requests),
        ParsedSelectionRequests::WholeUnitResumable => {
            let source_revision = decode_hex_digest(&metadata.revision_digest)?;
            metadata
                .inventory
                .iter()
                .map(|implementation| {
                    Some(OwnedSelectionRequest {
                        source_revision,
                        root_kind: match implementation.root_kind {
                            InventoryRootKind::NonMethodProgram => RootKind::NonMethodProgram,
                            InventoryRootKind::MethodOperation => RootKind::MethodOperation,
                        },
                        root_index: implementation.root_index,
                        path: implementation.path.clone(),
                        implementation_kind: match implementation.implementation_kind {
                            InventoryImplementationKind::NamedAdapter => {
                                SelectionImplementationKind::NamedAdapter
                            }
                            InventoryImplementationKind::BlockBody => {
                                SelectionImplementationKind::BlockBody
                            }
                        },
                        disposition: SelectionDisposition::ResumableRequired,
                    })
                })
                .collect()
        }
    }
}

fn artifact_selection(metadata: &RevisionMetadata) -> Option<Vec<u8>> {
    artifact_selection_with_requests(metadata, &[])
}

fn artifact_selection_with_requests(
    metadata: &RevisionMetadata,
    requests: &[OwnedSelectionRequest],
) -> Option<Vec<u8>> {
    let revision = decode_hex_digest(&metadata.revision_digest)?;
    let inventory: Vec<_> = metadata
        .inventory
        .iter()
        .map(|implementation| ImplementationSpec {
            source_revision: revision,
            root_kind: match implementation.root_kind {
                InventoryRootKind::NonMethodProgram => RootKind::NonMethodProgram,
                InventoryRootKind::MethodOperation => RootKind::MethodOperation,
            },
            root_index: implementation.root_index,
            path: implementation.path.as_slice(),
            implementation_kind: match implementation.implementation_kind {
                InventoryImplementationKind::NamedAdapter => {
                    SelectionImplementationKind::NamedAdapter
                }
                InventoryImplementationKind::BlockBody => SelectionImplementationKind::BlockBody,
            },
        })
        .collect();
    let requests: Vec<_> = requests
        .iter()
        .map(|request| SelectionRequest {
            implementation: ImplementationSpec {
                source_revision: request.source_revision,
                root_kind: request.root_kind,
                root_index: request.root_index,
                path: &request.path,
                implementation_kind: request.implementation_kind,
            },
            disposition: request.disposition,
        })
        .collect();
    materialize_artifact_selection(&[revision], &inventory, &requests).ok()
}

/// Project one complete accepted generation selection into the domain-separated
/// artifact selection owned by its monolithic image code unit, plus the exact
/// resumable rows that require lowering. No policy is chosen here: every row
/// and disposition is copied from the durable generation carrier.
fn image_artifact_selection_and_requests(
    generation_selection: &[u8],
) -> Option<(Vec<u8>, Vec<OwnedSelectionRequest>)> {
    let selection = inspect_generation_selection(generation_selection).ok()?;
    let revisions = (0..selection.revision_count())
        .map(|index| selection.revision(index))
        .collect::<Option<Vec<_>>>()?;
    let rows = selection
        .rows()
        .map(|row| {
            let row = row.ok()?;
            Some(OwnedSelectionRequest {
                source_revision: row.source_revision,
                root_kind: row.root_kind,
                root_index: row.root_index,
                path: row.path.iter().collect(),
                implementation_kind: row.implementation_kind,
                disposition: row.disposition,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    let specs = rows
        .iter()
        .map(|row| ImplementationSpec {
            source_revision: row.source_revision,
            root_kind: row.root_kind,
            root_index: row.root_index,
            path: &row.path,
            implementation_kind: row.implementation_kind,
        })
        .collect::<Vec<_>>();
    let requests = rows
        .iter()
        .map(|row| SelectionRequest {
            implementation: ImplementationSpec {
                source_revision: row.source_revision,
                root_kind: row.root_kind,
                root_index: row.root_index,
                path: &row.path,
                implementation_kind: row.implementation_kind,
            },
            disposition: row.disposition,
        })
        .collect::<Vec<_>>();
    let artifact = materialize_artifact_selection(&revisions, &specs, &requests).ok()?;
    let resumable = rows
        .into_iter()
        .filter(|row| row.disposition == SelectionDisposition::ResumableRequired)
        .collect();
    Some((artifact, resumable))
}

fn decode_hex_digest(value: &[u8; 64]) -> Option<[u8; 32]> {
    let mut digest = [0; 32];
    for (output, pair) in digest.iter_mut().zip(value.chunks_exact(2)) {
        *output = hex_nibble(pair[0])?.checked_mul(16)? | hex_nibble(pair[1])?;
    }
    Some(digest)
}

#[cfg(test)]
fn empty_resumable_manifest(authority: [u8; 32]) -> Option<Vec<u8>> {
    let selection_bytes =
        birnam_artifact_codec::stackless_selection::encode_artifact_selection(&[], &[]).ok()?;
    let selection = inspect_artifact_selection(&selection_bytes).ok()?;
    resumable_manifest::materialize(authority, selection, &[]).ok()
}

#[derive(Debug, PartialEq, Eq)]
struct CompiledResumablePlan {
    manifest: Vec<u8>,
    entries: Vec<CompiledResumableEntry>,
    named_emissions: Vec<NamedResumableEmission>,
    block_emissions: Vec<BlockResumableEmission>,
}

#[derive(Debug, PartialEq, Eq)]
struct CompiledResumableEntry {
    request: OwnedSelectionRequest,
    selector: Vec<u8>,
    descriptor: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResumablePlanError {
    Revision,
    Selection,
    Operation,
    Compile,
    Manifest,
}

fn resumable_indices(
    requests: &[OwnedSelectionRequest],
) -> Result<BTreeMap<OwnedSelectionRequestKey, (u32, u32)>, ResumablePlanError> {
    let mut keys = requests
        .iter()
        .filter(|request| request.disposition == SelectionDisposition::ResumableRequired)
        .map(OwnedSelectionRequest::key)
        .collect::<Vec<_>>();
    keys.sort();
    let mut manifest_keys = keys.clone();
    manifest_keys.sort_by(|left, right| {
        left.implementation_kind
            .cmp(&right.implementation_kind)
            .then_with(|| left.source_revision.cmp(&right.source_revision))
            .then_with(|| left.root_kind.cmp(&right.root_kind))
            .then_with(|| left.root_index.cmp(&right.root_index))
            .then_with(|| left.path.cmp(&right.path))
    });
    let manifest_indices = manifest_keys
        .into_iter()
        .enumerate()
        .map(|(index, key)| {
            u32::try_from(index)
                .map(|index| (key, index))
                .map_err(|_| ResumablePlanError::Compile)
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    keys.into_iter()
        .enumerate()
        .map(|(index, key)| {
            let index = u32::try_from(index).map_err(|_| ResumablePlanError::Compile)?;
            let manifest_index = *manifest_indices
                .get(&key)
                .ok_or(ResumablePlanError::Manifest)?;
            Ok((key, (index, manifest_index)))
        })
        .collect()
}

fn compile_resumable_plan<'a>(
    metadata: &RevisionMetadata,
    requests: &[OwnedSelectionRequest],
    artifact_selection: &[u8],
    authority: [u8; 32],
    diagnostic_path: &[u8],
    source: &[u8],
    program: &'a Program<'a>,
) -> Result<CompiledResumablePlan, ResumablePlanError> {
    let indices = resumable_indices(requests)?;
    let mut plan = compile_resumable_plan_indexed(
        metadata,
        requests,
        &indices,
        0,
        diagnostic_path,
        source,
        program,
    )?;
    let selection = inspect_artifact_selection(artifact_selection)
        .map_err(|_| ResumablePlanError::Selection)?;
    let entries = plan
        .entries
        .iter()
        .map(|entry| ManifestEntrySpec {
            selection: SelectionSpec {
                source_revision: entry.request.source_revision,
                root_kind: entry.request.root_kind,
                root_index: entry.request.root_index,
                path: &entry.request.path,
                implementation_kind: entry.request.implementation_kind,
                disposition: entry.request.disposition,
            },
            descriptor: &entry.descriptor,
            selector: &entry.selector,
        })
        .collect::<Vec<_>>();
    plan.manifest = resumable_manifest::materialize(authority, selection, &entries)
        .map_err(|_| ResumablePlanError::Manifest)?;
    let manifest = resumable_manifest::inspect(&plan.manifest, authority)
        .map_err(|_| ResumablePlanError::Manifest)?;
    for entry in &plan.entries {
        let expected = indices
            .get(&entry.request.key())
            .map(|(_, manifest_index)| *manifest_index)
            .ok_or(ResumablePlanError::Manifest)?;
        let actual = (0..manifest.entry_count())
            .find(|index| {
                manifest.entry(*index).is_some_and(|row| {
                    row.source_revision == entry.request.source_revision
                        && row.root_kind == entry.request.root_kind
                        && row.root_index == entry.request.root_index
                        && row.path.iter().eq(entry.request.path.iter().copied())
                        && row.implementation_kind == entry.request.implementation_kind
                })
            })
            .and_then(|index| u32::try_from(index).ok())
            .ok_or(ResumablePlanError::Manifest)?;
        if actual != expected {
            return Err(ResumablePlanError::Manifest);
        }
    }
    Ok(plan)
}

fn compile_resumable_plan_indexed<'a>(
    metadata: &RevisionMetadata,
    requests: &[OwnedSelectionRequest],
    indices: &BTreeMap<OwnedSelectionRequestKey, (u32, u32)>,
    synchronous_function_offset: u32,
    diagnostic_path: &[u8],
    source: &[u8],
    program: &'a Program<'a>,
) -> Result<CompiledResumablePlan, ResumablePlanError> {
    let revision =
        decode_hex_digest(&metadata.revision_digest).ok_or(ResumablePlanError::Revision)?;
    let mut resumable_requests = requests
        .iter()
        .filter(|request| request.disposition == SelectionDisposition::ResumableRequired)
        .cloned()
        .collect::<Vec<_>>();
    resumable_requests.sort_by(|left, right| {
        (
            left.source_revision,
            left.root_kind,
            left.root_index,
            &left.path,
            left.implementation_kind,
        )
            .cmp(&(
                right.source_revision,
                right.root_kind,
                right.root_index,
                &right.path,
                right.implementation_kind,
            ))
    });

    enum PreparedKind<'a> {
        Named,
        Block {
            block: &'a birnam_codegen::Block<'a>,
            captures: Vec<Vec<u8>>,
        },
    }

    struct Prepared<'a> {
        request: &'a OwnedSelectionRequest,
        codegen_selection: CodegenSelectionSpec<'a>,
        symbol_index: u32,
        selector: Vec<u8>,
        descriptor: Vec<u8>,
        kind: PreparedKind<'a>,
    }

    let mut synchronous_function_index = synchronous_function_offset;
    let mut placeholder_block_literals = Vec::new();
    for key in metadata
        .inventory
        .iter()
        .filter(|key| key.implementation_kind == InventoryImplementationKind::BlockBody)
    {
        let resolved =
            resolve_block_implementation(program, key).ok_or(ResumablePlanError::Operation)?;
        let selected = resumable_requests.iter().any(|request| {
            request.implementation_kind == SelectionImplementationKind::BlockBody
                && request.root_kind
                    == match key.root_kind {
                        InventoryRootKind::NonMethodProgram => RootKind::NonMethodProgram,
                        InventoryRootKind::MethodOperation => RootKind::MethodOperation,
                    }
                && request.root_index == key.root_index
                && request.path == key.path
        });
        let disposition = if selected {
            Some(BlockLiteralDisposition::Resumable {
                manifest_entry_index: 0,
            })
        } else if key.root_kind == InventoryRootKind::MethodOperation {
            Some(BlockLiteralDisposition::Synchronous {
                function_index: synchronous_function_index,
                language_arity: u32::try_from(resolved.block.params.len())
                    .map_err(|_| ResumablePlanError::Compile)?,
            })
        } else {
            None
        };
        if let Some(disposition) = disposition {
            let portable_program =
                birnam_codegen::portable::encode_block(resolved.block, &resolved.captures)
                    .map_err(|_| ResumablePlanError::Compile)?;
            placeholder_block_literals.push(BlockLiteralSpec {
                block: resolved.block,
                disposition,
                portable_program,
            });
        }
        if key.root_kind == InventoryRootKind::MethodOperation {
            synchronous_function_index = synchronous_function_index
                .checked_add(1)
                .ok_or(ResumablePlanError::Compile)?;
        }
    }

    let mut prepared = Vec::with_capacity(resumable_requests.len());
    for request in &resumable_requests {
        if request.source_revision != revision {
            return Err(ResumablePlanError::Operation);
        }
        let symbol_index = indices
            .get(&request.key())
            .map(|(symbol_index, _)| *symbol_index)
            .ok_or(ResumablePlanError::Manifest)?;
        let codegen_selection = CodegenSelectionSpec {
            source_revision: request.source_revision,
            root_kind: match request.root_kind {
                RootKind::NonMethodProgram => CodegenRootKind::NonMethodProgram,
                RootKind::MethodOperation => CodegenRootKind::MethodOperation,
            },
            root_index: request.root_index,
            path: &request.path,
            implementation_kind: match request.implementation_kind {
                SelectionImplementationKind::NamedAdapter => {
                    CodegenImplementationKind::NamedAdapter
                }
                SelectionImplementationKind::BlockBody => CodegenImplementationKind::BlockBody,
            },
            disposition: CodegenDisposition::ResumableRequired,
        };
        let (selector, descriptor, kind) = match request.implementation_kind {
            SelectionImplementationKind::NamedAdapter => {
                if request.root_kind != RootKind::MethodOperation || !request.path.is_empty() {
                    return Err(ResumablePlanError::Operation);
                }
                let operation = metadata
                    .operations
                    .get(request.root_index as usize)
                    .filter(|operation| operation.install)
                    .ok_or(ResumablePlanError::Operation)?;
                let compiled = compile_named_adapter_v1(
                    &operation.selector,
                    codegen_selection,
                    diagnostic_path,
                    source,
                    symbol_index,
                    0,
                )
                .map_err(|_| ResumablePlanError::Compile)?;
                (
                    operation.selector.clone(),
                    compiled.descriptor,
                    PreparedKind::Named,
                )
            }
            SelectionImplementationKind::BlockBody => {
                let key = ImplementationKey {
                    root_kind: match request.root_kind {
                        RootKind::NonMethodProgram => InventoryRootKind::NonMethodProgram,
                        RootKind::MethodOperation => InventoryRootKind::MethodOperation,
                    },
                    root_index: request.root_index,
                    path: request.path.clone(),
                    implementation_kind: InventoryImplementationKind::BlockBody,
                };
                let resolved = resolve_block_implementation(program, &key)
                    .ok_or(ResumablePlanError::Operation)?;
                let capture_refs = resolved
                    .captures
                    .iter()
                    .map(Vec::as_slice)
                    .collect::<Vec<_>>();
                let compiled = compile_block_with_literals_v1(
                    resolved.block,
                    &capture_refs,
                    codegen_selection,
                    diagnostic_path,
                    source,
                    symbol_index,
                    0,
                    &placeholder_block_literals,
                )
                .map_err(|_| ResumablePlanError::Compile)?;
                (
                    compiled.selector,
                    compiled.descriptor,
                    PreparedKind::Block {
                        block: resolved.block,
                        captures: resolved.captures,
                    },
                )
            }
        };
        prepared.push(Prepared {
            request,
            codegen_selection,
            symbol_index,
            selector,
            descriptor,
            kind,
        });
    }

    let mut synchronous_function_index = synchronous_function_offset;
    let mut block_literals = Vec::new();
    for key in metadata
        .inventory
        .iter()
        .filter(|key| key.implementation_kind == InventoryImplementationKind::BlockBody)
    {
        let resolved =
            resolve_block_implementation(program, key).ok_or(ResumablePlanError::Operation)?;
        let selected = resumable_requests.iter().find(|request| {
            request.implementation_kind == SelectionImplementationKind::BlockBody
                && request.root_kind
                    == match key.root_kind {
                        InventoryRootKind::NonMethodProgram => RootKind::NonMethodProgram,
                        InventoryRootKind::MethodOperation => RootKind::MethodOperation,
                    }
                && request.root_index == key.root_index
                && request.path == key.path
        });
        let disposition = if let Some(request) = selected {
            let manifest_entry_index = indices
                .get(&request.key())
                .map(|(_, manifest_entry_index)| *manifest_entry_index)
                .ok_or(ResumablePlanError::Manifest)?;
            Some(BlockLiteralDisposition::Resumable {
                manifest_entry_index,
            })
        } else if key.root_kind == InventoryRootKind::MethodOperation {
            Some(BlockLiteralDisposition::Synchronous {
                function_index: synchronous_function_index,
                language_arity: u32::try_from(resolved.block.params.len())
                    .map_err(|_| ResumablePlanError::Compile)?,
            })
        } else {
            None
        };
        if let Some(disposition) = disposition {
            let portable_program =
                birnam_codegen::portable::encode_block(resolved.block, &resolved.captures)
                    .map_err(|_| ResumablePlanError::Compile)?;
            block_literals.push(BlockLiteralSpec {
                block: resolved.block,
                disposition,
                portable_program,
            });
        }
        if key.root_kind == InventoryRootKind::MethodOperation {
            synchronous_function_index = synchronous_function_index
                .checked_add(1)
                .ok_or(ResumablePlanError::Compile)?;
        }
    }
    let mut named_emissions = Vec::new();
    let mut block_emissions = Vec::new();
    for prepared in &prepared {
        let manifest_entry_index = indices
            .get(&prepared.request.key())
            .map(|(_, manifest_entry_index)| *manifest_entry_index)
            .ok_or(ResumablePlanError::Manifest)?;
        match &prepared.kind {
            PreparedKind::Named => {
                let compiled = compile_named_adapter_v1(
                    &prepared.selector,
                    prepared.codegen_selection,
                    diagnostic_path,
                    source,
                    prepared.symbol_index,
                    manifest_entry_index,
                )
                .map_err(|_| ResumablePlanError::Compile)?;
                if compiled.descriptor != prepared.descriptor {
                    return Err(ResumablePlanError::Compile);
                }
                named_emissions.push(NamedResumableEmission {
                    operation_index: prepared.request.root_index,
                    symbol_index: prepared.symbol_index,
                    c_source: compiled.c_source,
                });
            }
            PreparedKind::Block { block, captures } => {
                let capture_refs = captures.iter().map(Vec::as_slice).collect::<Vec<_>>();
                let compiled = compile_block_with_literals_v1(
                    block,
                    &capture_refs,
                    prepared.codegen_selection,
                    diagnostic_path,
                    source,
                    prepared.symbol_index,
                    manifest_entry_index,
                    &block_literals,
                )
                .map_err(|_| ResumablePlanError::Compile)?;
                if compiled.descriptor != prepared.descriptor {
                    return Err(ResumablePlanError::Compile);
                }
                block_emissions.push(BlockResumableEmission {
                    key: ImplementationKey {
                        root_kind: match prepared.request.root_kind {
                            RootKind::NonMethodProgram => InventoryRootKind::NonMethodProgram,
                            RootKind::MethodOperation => InventoryRootKind::MethodOperation,
                        },
                        root_index: prepared.request.root_index,
                        path: prepared.request.path.clone(),
                        implementation_kind: InventoryImplementationKind::BlockBody,
                    },
                    symbol_index: prepared.symbol_index,
                    manifest_entry_index,
                    selector: compiled.selector,
                    captures: captures.clone(),
                    c_source: compiled.c_source,
                });
            }
        }
    }
    let entries = prepared
        .into_iter()
        .map(|entry| CompiledResumableEntry {
            request: entry.request.clone(),
            selector: entry.selector,
            descriptor: entry.descriptor,
        })
        .collect();
    Ok(CompiledResumablePlan {
        manifest: Vec::new(),
        entries,
        named_emissions,
        block_emissions,
    })
}

fn rebase_image_resumable_plan(
    plan: &mut CompiledResumablePlan,
    operation_offset: u32,
    non_method_block_offset: u32,
) -> Option<()> {
    for emission in &mut plan.named_emissions {
        emission.operation_index = emission.operation_index.checked_add(operation_offset)?;
    }
    for emission in &mut plan.block_emissions {
        match emission.key.root_kind {
            InventoryRootKind::MethodOperation => {
                emission.key.root_index = emission.key.root_index.checked_add(operation_offset)?;
            }
            InventoryRootKind::NonMethodProgram => {
                let first = emission.key.path.first_mut()?;
                *first = first.checked_add(non_method_block_offset)?;
            }
        }
    }
    Some(())
}

fn non_method_block_count(metadata: &RevisionMetadata) -> Option<u32> {
    metadata
        .inventory
        .iter()
        .filter(|key| {
            key.root_kind == InventoryRootKind::NonMethodProgram
                && key.implementation_kind == InventoryImplementationKind::BlockBody
        })
        .try_fold(0u32, |count, key| {
            key.path
                .first()
                .copied()
                .and_then(|ordinal| ordinal.checked_add(1))
                .map(|next| count.max(next))
        })
}

fn block_function_count(metadata: &RevisionMetadata) -> Option<u32> {
    u32::try_from(
        metadata
            .inventory
            .iter()
            .filter(|key| key.implementation_kind == InventoryImplementationKind::BlockBody)
            .count(),
    )
    .ok()
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn candidate_generated(
    generated: &[u8],
    manifest: &[u8],
    operations: &[RevisionOperation],
    rust_owned_shims: bool,
) -> Vec<u8> {
    // Keep candidate declarations local to the artifact instead of exporting
    // another owner for the same dynamic symbols.
    let generated = if rust_owned_shims {
        rewrite_codegen_shims(generated)
    } else {
        generated.to_vec()
    };
    let validate_binding =
        codegen_shim_name(b"birnam_candidate_validate_binding", rust_owned_shims);
    let mut output = Vec::with_capacity(generated.len() + manifest.len() + 512);
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
    // Preparation is deliberately structural. Running `__ev` here would
    // execute an initializer twice and could leak arbitrary effects before the
    // host safe point. Lowering and the validator have already proved the
    // projected statement set; VM-owned root changes occur exactly once in the
    // rollback-capable activation transaction below.
    output.extend_from_slice(
        b"void* __birnam_candidate_prepare(void* s, void** a, long n){(void)s;(void)a;(void)n;return birnam_true();}\n",
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
            b"birnam_candidate_preparing" => b"birnam_candidate_preparing",
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
        b"birnam_vtable_cache_set" => b"birnam_rust_codegen_vtable_cache_set",
        b"birnam_vtable_cache_get" => b"birnam_rust_codegen_vtable_cache_get",
        b"birnam_portable_code_admit_v1" => b"birnam_rust_codegen_portable_code_admit_v1",
        b"birnam_portable_block_new_v1" => b"birnam_rust_codegen_portable_block_new_v1",
        b"birnam_portable_materialized_block_new_v2" => {
            b"birnam_rust_codegen_portable_materialized_block_new_v2"
        }
        b"birnam_candidate_prepare_mode" => b"birnam_rust_codegen_candidate_prepare_mode",
        b"birnam_candidate_preparing" => b"birnam_rust_codegen_candidate_preparing",
        b"birnam_candidate_validate_binding" => b"birnam_rust_codegen_candidate_validate_binding",
        _ => unreachable!("unknown Codegen shim"),
    }
}

fn rewrite_codegen_shims(generated: &[u8]) -> Vec<u8> {
    let mut rewritten = generated.to_vec();
    const NAMES: [&[u8]; 18] = [
        b"birnam_candidate_validate_binding",
        b"birnam_candidate_prepare_mode",
        b"birnam_candidate_preparing",
        b"birnam_candidate_root_set",
        b"birnam_candidate_root_get",
        b"birnam_vtable_cache_set",
        b"birnam_vtable_cache_get",
        b"birnam_portable_code_admit_v1",
        b"birnam_portable_block_new_v1",
        b"birnam_portable_materialized_block_new_v2",
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

#[derive(Clone)]
struct RevisionOperation {
    target: Vec<u8>,
    selector: Vec<u8>,
    install: bool,
    source: Vec<u8>,
}

#[derive(Clone)]
struct RevisionMetadata {
    name: Vec<u8>,
    requires: Vec<Vec<u8>>,
    #[allow(dead_code)]
    operations: Vec<RevisionOperation>,
    non_method_digest: [u8; 64],
    initializer_residual_digest: [u8; 64],
    method_digest: [u8; 64],
    revision_digest: [u8; 64],
    inventory: Vec<ImplementationKey>,
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
            non_method_spans.push((index, whole_source.to_vec()));
            continue;
        }
        let receiver = skip_cst_wrappers(whole.child(0)?)?;
        let message = skip_cst_wrappers(whole.child(1)?)?;
        let Some((selector, args)) = keyword_selector_args(message, source) else {
            non_method_spans.push((index, whole_source.to_vec()));
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
            non_method_spans.push((index, whole_source.to_vec()));
            continue;
        }
        if receiver.rule() != b"ident"
            || source_span(receiver, source) != Some(b"Reflect".as_slice())
        {
            non_method_spans.push((index, whole_source.to_vec()));
            continue;
        }
        let is_define = (selector == b"defineMethod:on:as:" && args.len() == 3)
            || (selector == b"defineMethod:on:as:doc:" && args.len() == 4);
        let is_remove = selector == b"removeMethod:on:" && args.len() == 2;
        if !is_define && !is_remove {
            non_method_spans.push((index, whole_source.to_vec()));
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
    let normalized = lower_program(tree, source).ok()?;
    let projected_statements = birnam_codegen::resumable::projected_block_bindings(&normalized)
        .into_iter()
        .filter(|binding| binding.captures.is_empty())
        .map(|binding| binding.statement_index)
        .collect::<BTreeSet<_>>();
    let mut non_method_input = b"birnam.non-method.v1\0".to_vec();
    let mut initializer_residual_input = b"birnam.initializer-residual.v1\0".to_vec();
    for (statement_index, span) in non_method_spans {
        append_digest_field(&mut non_method_input, &span);
        if !projected_statements.contains(&statement_index) {
            append_digest_field(&mut initializer_residual_input, &span);
        }
    }
    let non_method_digest = sha256_hex(&non_method_input);
    let initializer_residual_digest = sha256_hex(&initializer_residual_input);

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

    let inventory = implementation_inventory(&normalized);
    Some(RevisionMetadata {
        name,
        requires,
        operations,
        non_method_digest,
        initializer_residual_digest,
        method_digest,
        revision_digest,
        inventory,
    })
}

#[allow(dead_code)]
fn candidate_manifest_matches(
    manifest: CandidateManifest<'_>,
    source: &[u8],
    metadata: &RevisionMetadata,
    artifact_selection: &[u8],
) -> bool {
    if manifest.name != metadata.name
        || manifest.source_digest != sha256_hex(source)
        || manifest.non_method_digest != metadata.non_method_digest
        || manifest.method_digest != metadata.method_digest
        || manifest.revision_digest != metadata.revision_digest
    {
        return false;
    }
    match manifest.version {
        CandidateManifestVersion::V4 => {
            if !artifact_selection_matches_inventory(metadata, artifact_selection) {
                return false;
            }
            let selection_digest = sha256_hex(artifact_selection);
            if manifest.artifact_selection_digest != Some(selection_digest.as_slice())
                || manifest.covered_revision_count() != 1
                || manifest.covered_revision(0) != Some(metadata.revision_digest.as_slice())
            {
                return false;
            }
        }
    }
    if manifest.dependency_count() != metadata.requires.len()
        || metadata
            .requires
            .iter()
            .enumerate()
            .any(|(index, dependency)| manifest.dependency(index) != Some(dependency.as_slice()))
    {
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

fn artifact_selection_matches_inventory(metadata: &RevisionMetadata, bytes: &[u8]) -> bool {
    let Some(revision) = decode_hex_digest(&metadata.revision_digest) else {
        return false;
    };
    let Ok(selection) = inspect_artifact_selection(bytes) else {
        return false;
    };
    if selection.revision_count() != 1
        || selection.revision(0) != Some(revision)
        || selection.row_count() != metadata.inventory.len()
    {
        return false;
    }
    selection
        .rows()
        .zip(&metadata.inventory)
        .all(|(row, expected)| {
            let Ok(row) = row else {
                return false;
            };
            row.source_revision == revision
                && row.root_kind
                    == match expected.root_kind {
                        InventoryRootKind::NonMethodProgram => RootKind::NonMethodProgram,
                        InventoryRootKind::MethodOperation => RootKind::MethodOperation,
                    }
                && row.root_index == expected.root_index
                && row.path.iter().eq(expected.path.iter().copied())
                && row.implementation_kind
                    == match expected.implementation_kind {
                        InventoryImplementationKind::NamedAdapter => {
                            SelectionImplementationKind::NamedAdapter
                        }
                        InventoryImplementationKind::BlockBody => {
                            SelectionImplementationKind::BlockBody
                        }
                    }
        })
}

fn candidate_selection_matches(
    manifest: CandidateManifest<'_>,
    expected: &[u8],
    exported: &[u8],
) -> bool {
    match manifest.version {
        CandidateManifestVersion::V4 => {
            let exported_digest = sha256_hex(exported);
            if expected != exported
                || manifest.artifact_selection_digest != Some(exported_digest.as_slice())
            {
                return false;
            }
            let Ok(selection) = inspect_artifact_selection(exported) else {
                return false;
            };
            if manifest.covered_revision_count() != selection.revision_count() {
                return false;
            }
            (0..selection.revision_count()).all(|index| {
                let Some(manifest_revision) = manifest.covered_revision(index) else {
                    return false;
                };
                let Some(manifest_revision) = decode_hex_digest_slice(manifest_revision) else {
                    return false;
                };
                selection.revision(index) == Some(manifest_revision)
            })
        }
    }
}

fn decode_hex_digest_slice(value: &[u8]) -> Option<[u8; 32]> {
    let value: &[u8; 64] = value.try_into().ok()?;
    decode_hex_digest(value)
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
        if let Some(existing) = modules.iter().find(|existing| {
            unsafe { symbol_bytes(api, existing.name as Oop) }
                .as_deref()
                .is_some_and(|existing_name| existing_name == name.as_slice())
        }) {
            if unsafe { string_bytes(api, existing.source as Oop) }.as_deref()
                == Some(source.as_slice())
            {
                return values[0];
            }
            note_error(b"RustCodegen duplicate module source");
            return api.nilObj;
        }
        modules.push(ModuleSourceEntry {
            name: values[1] as usize,
            source: values[0] as usize,
            artifact_selection: None,
        });
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
    for entry in modules.iter() {
        if unsafe { symbol_bytes(api, entry.name as Oop) }
            .as_deref()
            .is_some_and(|existing| existing == name.as_slice())
        {
            return entry.source as Oop;
        }
    }
    api.nilObj
}

/// Register source exported by a successfully loaded Rust artifact.
///
/// Artifact mode installs precompiled method trampolines directly through the
/// generated ABI, so it does not pass through `recordModuleSource:named:`.
/// Preserve the same durable source projection by admitting the validated
/// source bytes only after the artifact initializer/entry point succeeds.
unsafe fn register_artifact_source(
    api: &IdApi,
    identity: &[u8],
    expected_digest: &[u8],
    source: &[u8],
    artifact_selection: Option<&[u8]>,
) -> bool {
    if sha256_hex(source).as_slice() != expected_digest {
        note_error(b"RustCodegen artifact source digest is incompatible");
        return false;
    }
    // Only module-bearing artifacts contribute to the durable source
    // projection. Image generations and other ad-hoc compile outputs carry
    // the same source export for diagnostics, but deliberately have no
    // revision identity and must not collide with an earlier path-derived
    // artifact entry.
    unsafe { (api.gcSetAuto)(0) };
    struct AutoGc<'a>(&'a IdApi);
    impl Drop for AutoGc<'_> {
        fn drop(&mut self) {
            unsafe { (self.0.gcSetAuto)(1) };
        }
    }
    let _auto_gc = AutoGc(api);
    let name = unsafe { intern_bytes(api, identity) };
    let source_object = unsafe { (api.mkString)(source.as_ptr(), source.len()) };
    let Some((_, metadata)) = (unsafe { parse_revision_metadata(api, source_object) }) else {
        return true;
    };
    if metadata.name != identity {
        note_error(b"RustCodegen artifact source identity is incompatible");
        return false;
    }
    let Ok(mut modules) = MODULE_SOURCES.lock() else {
        note_error(b"RustCodegen module source registry is unavailable");
        return false;
    };
    if let Some(existing) = modules.iter().find(|existing| {
        unsafe { symbol_bytes(api, existing.name as Oop) }
            .as_deref()
            .is_some_and(|existing_name| existing_name == identity)
    }) {
        if unsafe { string_bytes(api, existing.source as Oop) }.as_deref() == Some(source)
            && existing.artifact_selection.as_deref() == artifact_selection
        {
            return true;
        }
        note_error(b"RustCodegen duplicate module source");
        return false;
    }
    modules.push(ModuleSourceEntry {
        name: name as usize,
        source: source_object as usize,
        artifact_selection: artifact_selection.map(<[u8]>::to_vec),
    });
    true
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

/// Establish the first durable DispatchGeneration from the Rust-owned source
/// projection.
///
/// The ordinary `DispatchGeneration>>initializeCurrent` method is authored in
/// Birnam and asks the canonical `Codegen` global for its projection. This
/// selector creates G0 only when Store has no active generation, and refuses to
/// overwrite or reinterpret an existing durable generation.
unsafe extern "C" fn rust_codegen_initialize_current_generation(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }.unwrap_or_else(|| {
        abort("RustCodegen initializeCurrentGeneration received invalid arguments")
    });
    if !values.is_empty() {
        abort("RustCodegen initializeCurrentGeneration expects no arguments");
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

    let durable = unsafe { durable_current_generation(api) };
    if !durable.is_null() && durable != api.nilObj {
        if !unsafe { durable_generation_is_canonical(api, durable) } {
            note_error(b"RustCodegen durable generation is not canonical");
            return api.nilObj;
        }
        let projection = unsafe {
            send(
                api,
                runtime.rust_codegen as Oop,
                INITIAL_GENERATION_PROJECTION_SELECTOR,
                &[],
            )
        };
        let projection_digest = unsafe { vector_item(api, projection, 2) };
        let durable_digest = unsafe {
            send(
                api,
                runtime.store as Oop,
                CURRENT_VALUE_SELECTOR,
                &[durable, intern(api, b"baseDigest\0")],
            )
        };
        if unsafe { string_bytes(api, projection_digest) }
            != unsafe { string_bytes(api, durable_digest) }
        {
            note_error(b"RustCodegen durable generation does not match its projection");
            return api.nilObj;
        }
        if let Ok(mut sealed) = INITIAL_GENERATION_SEALED.lock() {
            *sealed = true;
        } else {
            note_error(b"RustCodegen initial generation seal is unavailable");
            return api.nilObj;
        }
        ACTIVE_GENERATION.store(durable as usize, Ordering::Release);
        return durable;
    }

    let sealed = INITIAL_GENERATION_SEALED
        .lock()
        .map(|value| *value)
        .unwrap_or(true);
    if sealed {
        note_error(b"RustCodegen initial generation is already sealed");
        return api.nilObj;
    }

    let projection = unsafe {
        send(
            api,
            runtime.rust_codegen as Oop,
            INITIAL_GENERATION_PROJECTION_SELECTOR,
            &[],
        )
    };
    if projection.is_null() || projection == api.nilObj {
        note_error(b"RustCodegen initial generation projection is unavailable");
        return api.nilObj;
    }
    let globals = runtime.globals as Oop;
    if globals.is_null() || globals == api.nilObj {
        note_error(b"RustCodegen Globals authority is unavailable");
        return api.nilObj;
    }
    let generation_proto = unsafe {
        send(
            api,
            globals,
            GLOBAL_AT_SELECTOR,
            &[intern(api, DISPATCH_GENERATION_NAME)],
        )
    };
    if generation_proto.is_null() || generation_proto == api.nilObj {
        note_error(b"RustCodegen DispatchGeneration family is unavailable");
        return api.nilObj;
    }
    let generation = unsafe { send0(api, generation_proto, b"clone\0") };
    if generation.is_null() || generation == api.nilObj {
        note_error(b"RustCodegen could not allocate initial generation");
        return api.nilObj;
    }
    let initialized = unsafe {
        send(
            api,
            generation,
            FROM_PROJECTION_SELECTOR,
            &[projection, api.nilObj, api.nilObj],
        )
    };
    if initialized.is_null() || initialized == api.nilObj {
        note_error(b"RustCodegen could not materialize initial generation");
        return api.nilObj;
    }
    if !unsafe { set_durable_current_generation(api, initialized) } {
        note_error(b"RustCodegen could not publish initial generation");
        return api.nilObj;
    }
    if let Ok(mut sealed) = INITIAL_GENERATION_SEALED.lock() {
        *sealed = true;
    } else {
        note_error(b"RustCodegen initial generation seal is unavailable");
        return api.nilObj;
    }
    ACTIVE_GENERATION.store(initialized as usize, Ordering::Release);
    initialized
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
    let current_source = unsafe { vector_item(api, current, 3) };
    let successor_source = unsafe { vector_item(api, successor, 3) };
    let Some((_, current_metadata)) = (unsafe { parse_revision_metadata(api, current_source) })
    else {
        return false;
    };
    let Some((_, successor_metadata)) = (unsafe { parse_revision_metadata(api, successor_source) })
    else {
        return false;
    };
    current_metadata.initializer_residual_digest == successor_metadata.initializer_residual_digest
}

#[derive(Clone)]
struct GenerationModuleProjection {
    name: Vec<u8>,
    revision: [u8; 32],
    source: Vec<u8>,
    metadata: RevisionMetadata,
}

struct CandidateGenerationProjection {
    selection: Vec<u8>,
    authority: [u8; 32],
}

fn sha256_raw(bytes: &[u8]) -> [u8; 32] {
    decode_hex_digest(&sha256_hex(bytes)).expect("sha256_hex always returns lowercase SHA-256")
}

fn push_be_field(output: &mut Vec<u8>, bytes: &[u8]) -> Option<()> {
    output.extend_from_slice(&u32::try_from(bytes.len()).ok()?.to_be_bytes());
    output.extend_from_slice(bytes);
    Some(())
}

unsafe fn generation_modules(
    api: &IdApi,
    source_map: Oop,
) -> Option<Vec<GenerationModuleProjection>> {
    let vector_vt = unsafe { (api.import)(VECTOR_VT_NAME.as_ptr().cast()) };
    let count = unsafe { vector_length(api, source_map, vector_vt) }?;
    let mut modules = Vec::with_capacity(count);
    for index in 0..count {
        let entry = unsafe { vector_item(api, source_map, index) };
        if unsafe { vector_length(api, entry, vector_vt) } != Some(3) {
            return None;
        }
        let name = unsafe { symbol_bytes(api, vector_item(api, entry, 0)) }?;
        let revision_object = unsafe { vector_item(api, entry, 1) };
        if !unsafe { valid_revision(api, revision_object) } {
            return None;
        }
        let source_object = unsafe { vector_item(api, revision_object, 3) };
        let source = unsafe { string_bytes(api, source_object) }?;
        let (_, metadata) = unsafe { parse_revision_metadata(api, source_object) }?;
        if metadata.name != name
            || unsafe { string_bytes(api, vector_item(api, revision_object, 6)) }?
                != metadata.revision_digest
        {
            return None;
        }
        let revision = decode_hex_digest(&metadata.revision_digest)?;
        modules.push(GenerationModuleProjection {
            name,
            revision,
            source,
            metadata,
        });
    }
    modules.sort_by(|left, right| left.name.cmp(&right.name));
    if modules.windows(2).any(|pair| pair[0].name == pair[1].name) {
        return None;
    }
    Some(modules)
}

fn generation_revisions(modules: &[GenerationModuleProjection]) -> Option<Vec<[u8; 32]>> {
    let mut revisions = modules
        .iter()
        .map(|module| module.revision)
        .collect::<Vec<_>>();
    revisions.sort_unstable();
    if revisions.windows(2).any(|pair| pair[0] == pair[1]) {
        return None;
    }
    Some(revisions)
}

unsafe fn registered_generation_modules(api: &IdApi) -> Option<Vec<GenerationModuleProjection>> {
    let entries = MODULE_SOURCES.lock().ok()?.clone();
    let mut modules = entries
        .into_iter()
        .map(|entry| {
            let name = unsafe { symbol_bytes(api, entry.name as Oop) }?;
            let (source, metadata) = unsafe { parse_revision_metadata(api, entry.source as Oop) }?;
            if metadata.name != name {
                return None;
            }
            let revision = decode_hex_digest(&metadata.revision_digest)?;
            Some(GenerationModuleProjection {
                name,
                revision,
                source,
                metadata,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    modules.sort_by(|left, right| left.name.cmp(&right.name));
    if modules.windows(2).any(|pair| pair[0].name == pair[1].name) {
        return None;
    }
    Some(modules)
}

unsafe fn initial_module_authority(
    api: &IdApi,
    identity: &[u8],
    source: &[u8],
    selection: &[u8],
) -> Option<[u8; 32]> {
    let entries = MODULE_SOURCES.lock().ok()?.clone();
    let mut modules = Vec::with_capacity(entries.len().checked_add(1)?);
    let mut overrides = Vec::new();
    for entry in entries {
        let name = unsafe { symbol_bytes(api, entry.name as Oop) }?;
        if name == identity {
            return None;
        }
        let (source, metadata) = unsafe { parse_revision_metadata(api, entry.source as Oop) }?;
        let revision = decode_hex_digest(&metadata.revision_digest)?;
        if let Some(selection) = entry.artifact_selection {
            overrides.push((name.clone(), selection));
        }
        modules.push(GenerationModuleProjection {
            name,
            revision,
            source,
            metadata,
        });
    }
    let source_object = unsafe { (api.mkString)(source.as_ptr(), source.len()) };
    let (source, metadata) = unsafe { parse_revision_metadata(api, source_object) }?;
    if metadata.name != identity || !artifact_selection_matches_inventory(&metadata, selection) {
        return None;
    }
    let revision = decode_hex_digest(&metadata.revision_digest)?;
    modules.push(GenerationModuleProjection {
        name: identity.to_vec(),
        revision,
        source,
        metadata,
    });
    modules.sort_by(|left, right| left.name.cmp(&right.name));
    overrides.push((identity.to_vec(), selection.to_vec()));
    let generation_selection = generation_selection_with_overrides(&modules, &overrides)?;
    dispatch_generation_authority(&modules, &generation_selection)
}

fn all_synchronous_generation_selection(modules: &[GenerationModuleProjection]) -> Option<Vec<u8>> {
    generation_selection_with_overrides(modules, &[])
}

fn generation_selection_with_overrides(
    modules: &[GenerationModuleProjection],
    overrides: &[(Vec<u8>, Vec<u8>)],
) -> Option<Vec<u8>> {
    let revisions = generation_revisions(modules)?;
    let slices = modules
        .iter()
        .map(|module| {
            if let Some((_, selection)) = overrides.iter().find(|(name, _)| name == &module.name) {
                artifact_selection_matches_inventory(&module.metadata, selection)
                    .then(|| selection.clone())
            } else {
                artifact_selection(&module.metadata)
            }
        })
        .collect::<Option<Vec<_>>>()?;
    let inspected = slices
        .iter()
        .map(|slice| inspect_artifact_selection(slice).ok())
        .collect::<Option<Vec<_>>>()?;
    compose_generation_selection(&revisions, &inspected).ok()
}

fn visit_generation_module(
    modules: &[GenerationModuleProjection],
    index: usize,
    state: &mut [u8],
    ordered: &mut Vec<usize>,
) -> bool {
    if state[index] == 2 {
        return true;
    }
    if state[index] == 1 {
        return false;
    }
    state[index] = 1;
    for requirement in &modules[index].metadata.requires {
        if let Ok(dependency) =
            modules.binary_search_by(|module| module.name.as_slice().cmp(requirement))
            && !visit_generation_module(modules, dependency, state, ordered)
        {
            return false;
        }
    }
    state[index] = 2;
    ordered.push(index);
    true
}

fn generation_module_order(modules: &[GenerationModuleProjection]) -> Option<Vec<usize>> {
    let mut state = vec![0u8; modules.len()];
    let mut ordered = Vec::with_capacity(modules.len());
    for index in 0..modules.len() {
        if !visit_generation_module(modules, index, &mut state, &mut ordered) {
            return None;
        }
    }
    Some(ordered)
}

fn named_operation_descriptor_hash(
    module: &GenerationModuleProjection,
    operation_index: usize,
    selection: birnam_artifact_codec::stackless_selection::Selection<'_>,
) -> Option<[u8; 32]> {
    let row = named_operation_selection_row(module, operation_index, selection)?;
    if row.disposition != SelectionDisposition::ResumableRequired {
        return None;
    }
    let path = row.path.iter().collect::<Vec<_>>();
    let compiled = compile_named_adapter_v1(
        &module.metadata.operations.get(operation_index)?.selector,
        CodegenSelectionSpec {
            source_revision: row.source_revision,
            root_kind: CodegenRootKind::MethodOperation,
            root_index: row.root_index,
            path: &path,
            implementation_kind: CodegenImplementationKind::NamedAdapter,
            disposition: CodegenDisposition::ResumableRequired,
        },
        &module.name,
        &module.source,
        u32::try_from(operation_index).ok()?,
        0,
    )
    .ok()?;
    Some(sha256_raw(&compiled.descriptor))
}

fn named_operation_selection_row<'a>(
    module: &GenerationModuleProjection,
    operation_index: usize,
    selection: birnam_artifact_codec::stackless_selection::Selection<'a>,
) -> Option<birnam_artifact_codec::stackless_selection::SelectionRow<'a>> {
    selection.rows().find_map(|row| {
        let row = row.ok()?;
        (row.source_revision == module.revision
            && row.root_kind == RootKind::MethodOperation
            && row.root_index == operation_index as u32
            && row.path.is_empty()
            && row.implementation_kind == SelectionImplementationKind::NamedAdapter)
            .then_some(row)
    })
}

fn dispatch_generation_authority(
    modules: &[GenerationModuleProjection],
    generation_selection: &[u8],
) -> Option<[u8; 32]> {
    dispatch_generation_authority_with(modules, generation_selection, |module, index, selection| {
        named_operation_descriptor_hash(module, index, selection)
    })
}

fn dispatch_generation_authority_with(
    modules: &[GenerationModuleProjection],
    generation_selection: &[u8],
    mut descriptor_hash: impl FnMut(
        &GenerationModuleProjection,
        usize,
        birnam_artifact_codec::stackless_selection::Selection<'_>,
    ) -> Option<[u8; 32]>,
) -> Option<[u8; 32]> {
    let selection = inspect_generation_selection(generation_selection).ok()?;
    let revisions = generation_revisions(modules)?;
    if selection.revision_count() != revisions.len()
        || !(0..selection.revision_count())
            .all(|index| selection.revision(index) == revisions.get(index).copied())
    {
        return None;
    }

    let mut input = b"birnam.dispatch-base.v2\0".to_vec();
    input.extend_from_slice(&u32::try_from(modules.len()).ok()?.to_be_bytes());
    for module in modules {
        push_be_field(&mut input, &module.name)?;
        input.extend_from_slice(&module.revision);
    }
    let operation_count = modules.iter().try_fold(0usize, |count, module| {
        count.checked_add(module.metadata.operations.len())
    })?;
    input.extend_from_slice(&u32::try_from(operation_count).ok()?.to_be_bytes());
    for module_index in generation_module_order(modules)? {
        let module = &modules[module_index];
        for (operation_index, operation) in module.metadata.operations.iter().enumerate() {
            push_be_field(&mut input, &module.name)?;
            push_be_field(&mut input, &operation.target)?;
            push_be_field(&mut input, &operation.selector)?;
            input.push(if operation.install { 1 } else { 2 });
            if operation.install {
                let mut definition = b"birnam.definition.v1\0".to_vec();
                definition.extend_from_slice(&operation.source);
                input.extend_from_slice(&sha256_raw(&definition));
                let row = named_operation_selection_row(module, operation_index, selection)?;
                match row.disposition {
                    SelectionDisposition::ResumableRequired => {
                        let descriptor = descriptor_hash(module, operation_index, selection)?;
                        input.push(2);
                        input.extend_from_slice(&descriptor);
                    }
                    SelectionDisposition::Synchronous => {
                        input.push(1);
                        input.extend_from_slice(&[0; 32]);
                    }
                }
            } else {
                input.extend_from_slice(&[0; 32]);
                input.push(0);
                input.extend_from_slice(&[0; 32]);
            }
        }
    }
    input.extend_from_slice(&sha256_raw(generation_selection));
    Some(sha256_raw(&input))
}

fn project_candidate_generation(
    mut modules: Vec<GenerationModuleProjection>,
    candidate: GenerationModuleProjection,
    predecessor_selection: &[u8],
    artifact_selection: &[u8],
) -> Option<CandidateGenerationProjection> {
    let module_index = modules
        .binary_search_by(|module| module.name.cmp(&candidate.name))
        .ok()?;
    let removed_revision = modules[module_index].revision;
    modules[module_index] = candidate;
    let revisions = generation_revisions(&modules)?;
    let predecessor = inspect_generation_selection(predecessor_selection).ok()?;
    let replacement = inspect_artifact_selection(artifact_selection).ok()?;
    let selection =
        replace_generation_selection(predecessor, &[removed_revision], &revisions, replacement)
            .ok()?;
    let authority = dispatch_generation_authority(&modules, &selection)?;
    Some(CandidateGenerationProjection {
        selection,
        authority,
    })
}

unsafe fn project_open_candidate_generation(
    api: &IdApi,
    predecessor: Oop,
    expected_predecessor_digest: &[u8; 64],
    candidate_source: Oop,
    artifact_selection: &[u8],
) -> Option<CandidateGenerationProjection> {
    if unsafe { durable_generation_base_digest(api, predecessor) }.as_ref()
        != Some(expected_predecessor_digest)
    {
        return None;
    }
    let source_map = unsafe {
        send(
            api,
            runtime().store as Oop,
            CURRENT_VALUE_SELECTOR,
            &[predecessor, intern(api, b"sourceMap\0")],
        )
    };
    let modules = unsafe { generation_modules(api, source_map) }?;
    let predecessor_selection = unsafe { durable_generation_selection(api, predecessor) }?;
    let (source, metadata) = unsafe { parse_revision_metadata(api, candidate_source) }?;
    let revision = decode_hex_digest(&metadata.revision_digest)?;
    project_candidate_generation(
        modules,
        GenerationModuleProjection {
            name: metadata.name.clone(),
            revision,
            source,
            metadata,
        },
        &predecessor_selection,
        artifact_selection,
    )
}

fn generation_digest_with_selection(
    modules: &[GenerationModuleProjection],
    generation_selection: &[u8],
) -> Option<[u8; 64]> {
    let authority = dispatch_generation_authority(modules, generation_selection)?;
    Some(raw_digest_spelling(authority))
}

fn raw_digest_spelling(authority: [u8; 32]) -> [u8; 64] {
    let mut spelling = [0u8; 64];
    for (index, byte) in authority.iter().copied().enumerate() {
        spelling[index * 2] = b"0123456789abcdef"[(byte >> 4) as usize];
        spelling[index * 2 + 1] = b"0123456789abcdef"[(byte & 0x0f) as usize];
    }
    spelling
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

unsafe fn durable_generation_base_digest(api: &IdApi, generation: Oop) -> Option<[u8; 64]> {
    if generation.is_null() || generation == api.nilObj {
        return None;
    }
    let digest = unsafe {
        send(
            api,
            runtime().store as Oop,
            CURRENT_VALUE_SELECTOR,
            &[generation, intern(api, b"baseDigest\0")],
        )
    };
    let digest = unsafe { string_bytes(api, digest) }?;
    let digest: [u8; 64] = digest.try_into().ok()?;
    decode_hex_digest(&digest)?;
    Some(digest)
}

unsafe fn durable_generation_selection(api: &IdApi, generation: Oop) -> Option<Vec<u8>> {
    if generation.is_null() || generation == api.nilObj {
        return None;
    }
    let store = runtime().store as Oop;
    let source_map = unsafe {
        send(
            api,
            store,
            CURRENT_VALUE_SELECTOR,
            &[generation, intern(api, b"sourceMap\0")],
        )
    };
    let modules = unsafe { generation_modules(api, source_map) }?;
    let recorded = unsafe {
        send(
            api,
            store,
            CURRENT_VALUE_SELECTOR,
            &[generation, intern(api, b"generationSelection\0")],
        )
    };
    if !recorded.is_null() && recorded != api.nilObj {
        let bytes = unsafe { string_bytes(api, recorded) }?;
        dispatch_generation_authority(&modules, &bytes)?;
        return Some(bytes);
    }
    // Transitional G0 values predate the S2 carrier. Their only admissible
    // projection is the complete all-synchronous inventory derived from their
    // exact retained SourceRevisions. Once a v2 generation is published the
    // canonicality check below requires the retained bytes.
    all_synchronous_generation_selection(&modules)
}

/// Recover module source pairs from the durable generation projection after an
/// image load. The process-local registry remains useful during boot, but it
/// cannot contain modules that were persisted by another process.
unsafe fn durable_generation_sources(api: &IdApi) -> Option<Vec<(usize, usize)>> {
    let generation = unsafe { durable_current_generation(api) };
    if generation.is_null() || generation == api.nilObj {
        return None;
    }
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
    let mut modules = Vec::with_capacity(count);
    for index in 0..count {
        let entry = unsafe { vector_item(api, source_map, index) };
        if unsafe { vector_length(api, entry, vector_vt) } != Some(3) {
            return None;
        }
        let name = unsafe { vector_item(api, entry, 0) };
        let revision = unsafe { vector_item(api, entry, 1) };
        let source = unsafe { vector_item(api, revision, 3) };
        if unsafe { symbol_bytes(api, name) }.is_none()
            || unsafe { string_bytes(api, source) }.is_none()
        {
            return None;
        }
        modules.push((name as usize, source as usize));
    }
    Some(modules)
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
    let Some(modules) = (unsafe { generation_modules(api, source_map) }) else {
        return false;
    };
    let Some(selection) = (unsafe { durable_generation_selection(api, generation) }) else {
        return false;
    };
    let Some(actual) = generation_digest_with_selection(&modules, &selection) else {
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
    let active = ACTIVE_GENERATION.load(Ordering::Acquire) as Oop;
    active == durable && unsafe { durable_generation_is_canonical(api, durable) }
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
        let left_name = unsafe { symbol_bytes(api, left.name as Oop) }.unwrap_or_default();
        let right_name = unsafe { symbol_bytes(api, right.name as Oop) }.unwrap_or_default();
        left_name.cmp(&right_name)
    });

    let mut entries = Vec::with_capacity(modules.len());
    let mut selection_overrides = Vec::new();
    for entry in modules {
        let name = entry.name as Oop;
        let source = entry.source as Oop;
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
        if let Some(selection) = entry.artifact_selection {
            let Some(name) = (unsafe { symbol_bytes(api, name) }) else {
                note_error(b"RustCodegen module name is malformed");
                return api.nilObj;
            };
            selection_overrides.push((name, selection));
        }
    }
    let source_map = unsafe { make_vector(api, vector_proto, &entries) };
    let Some(modules) = (unsafe { generation_modules(api, source_map) }) else {
        note_error(b"RustCodegen generation source map is malformed");
        return api.nilObj;
    };
    let Some(selection) = generation_selection_with_overrides(&modules, &selection_overrides)
    else {
        note_error(b"RustCodegen generation selection is malformed");
        return api.nilObj;
    };
    let Some(digest) = generation_digest_with_selection(&modules, &selection) else {
        note_error(b"RustCodegen generation projection is malformed");
        return api.nilObj;
    };
    let digest_object = unsafe { (api.mkString)(digest.as_ptr(), digest.len()) };
    let selection_object = unsafe { (api.mkString)(selection.as_ptr(), selection.len()) };
    let tag = unsafe { intern(api, b"generationProjection\0") };
    unsafe {
        make_vector(
            api,
            vector_proto,
            &[tag, source_map, digest_object, selection_object],
        )
    }
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
    let predecessor = unsafe { durable_current_generation(api) };
    if predecessor.is_null() || predecessor == api.nilObj {
        note_error(b"RustCodegen predecessor generation is unavailable");
        return api.nilObj;
    }
    let current_source_map = unsafe {
        send(
            api,
            runtime.store as Oop,
            CURRENT_VALUE_SELECTOR,
            &[predecessor, intern(api, b"sourceMap\0")],
        )
    };
    if current_source_map != values[0] {
        note_error(b"RustCodegen predecessor source map is stale");
        return api.nilObj;
    }
    let (candidate_name, expected_digest, projected_authority, projected_selection, manifest) = {
        let Ok(candidates) = CANDIDATES.lock() else {
            note_error(b"RustCodegen candidate registry is unavailable");
            return api.nilObj;
        };
        let Some(candidate) = candidates
            .iter()
            .find(|candidate| candidate.unit == values[3] as usize)
        else {
            note_error(b"RustCodegen candidate unit is unknown");
            return api.nilObj;
        };
        (
            candidate.name as Oop,
            candidate.expected_predecessor_digest,
            candidate.projected_authority,
            candidate.projected_selection.clone(),
            candidate.resumable_manifest.clone(),
        )
    };
    if candidate_name != values[1] {
        note_error(b"RustCodegen candidate projection name mismatches");
        return api.nilObj;
    }
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
    if unsafe { durable_generation_base_digest(api, predecessor) }.as_ref()
        != Some(&expected_digest)
    {
        note_error(b"RustCodegen candidate predecessor is stale");
        return api.nilObj;
    }
    let Some(projected_authority) = projected_authority else {
        note_error(b"RustCodegen candidate projection is unavailable");
        return api.nilObj;
    };
    if projected_selection.is_empty()
        || (!manifest.is_empty()
            && resumable_manifest::inspect(&manifest, projected_authority).is_err())
    {
        note_error(b"RustCodegen candidate resumable authority mismatches");
        return api.nilObj;
    }
    let digest = raw_digest_spelling(projected_authority);
    let digest_object = unsafe { (api.mkString)(digest.as_ptr(), digest.len()) };
    let selection_object =
        unsafe { (api.mkString)(projected_selection.as_ptr(), projected_selection.len()) };
    let tag = unsafe { intern(api, b"generationProjection\0") };
    unsafe {
        make_vector(
            api,
            vector_proto,
            &[tag, successor, digest_object, selection_object],
        )
    }
}

unsafe fn visit_generation_source(
    api: &IdApi,
    modules: &[ModuleSourceEntry],
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
    let source = modules[index].source as Oop;
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

struct MethodOperationSource {
    sequence: i64,
    tie: i64,
    source: Oop,
}

unsafe fn ordered_method_operation_sources(api: &IdApi) -> Vec<MethodOperationSource> {
    if !METHOD_OP_PRESENT.load(Ordering::Acquire) {
        return Vec::new();
    }
    let store = runtime().store as Oop;
    if store.is_null() || store == api.nilObj {
        return Vec::new();
    }
    let entity = unsafe { intern(api, METHOD_OPS_ENTITY_NAME) };
    let count = unsafe { send(api, store, b"attrCount:\0", &[entity]) };
    if unsafe { (api.isSmallInt)(count) } == 0 {
        return Vec::new();
    }
    let count = unsafe { (api.smallIntValue)(count) }.max(0) as usize;
    let mut sources = Vec::new();
    for index in 0..count {
        let attribute = unsafe {
            send(
                api,
                store,
                b"attrAt:index:\0",
                &[entity, (api.mkSmallInt)(index as i64)],
            )
        };
        let Some(name) = (unsafe { symbol_bytes(api, attribute) }) else {
            continue;
        };
        let Some(rest) = name.strip_prefix(b"__mop_") else {
            continue;
        };
        let Some(split) = rest.iter().position(|byte| *byte == b'_') else {
            continue;
        };
        let Ok(sequence) = std::str::from_utf8(&rest[..split])
            .ok()
            .and_then(|value| value.parse::<i64>().ok())
            .ok_or(())
        else {
            continue;
        };
        let Ok(tie) = std::str::from_utf8(&rest[split + 1..])
            .ok()
            .and_then(|value| value.parse::<i64>().ok())
            .ok_or(())
        else {
            continue;
        };
        let source = unsafe { send(api, store, CURRENT_VALUE_SELECTOR, &[entity, attribute]) };
        if source.is_null()
            || source == api.nilObj
            || unsafe { (api.vtableOf)(source) } != api.string_vt
        {
            continue;
        }
        sources.push(MethodOperationSource {
            sequence,
            tie,
            source,
        });
    }
    sources.sort_by_key(|entry| (entry.sequence, entry.tie));
    sources
}

unsafe fn ordered_generation_sources(api: &IdApi) -> Option<Vec<Oop>> {
    let Ok(module_guard) = MODULE_SOURCES.lock() else {
        return None;
    };
    let mut modules = module_guard.clone();
    drop(module_guard);
    if let Some(durable_modules) = unsafe { durable_generation_sources(api) } {
        for (name, source) in durable_modules {
            if let Some(existing) = modules.iter_mut().find(|existing| {
                (unsafe { symbol_bytes(api, existing.name as Oop) })
                    == (unsafe { symbol_bytes(api, name as Oop) })
            }) {
                existing.source = source;
            } else {
                modules.push(ModuleSourceEntry {
                    name,
                    source,
                    artifact_selection: None,
                });
            }
        }
    }
    modules.sort_by(|left, right| {
        let left_name = unsafe { symbol_bytes(api, left.name as Oop) }.unwrap_or_default();
        let right_name = unsafe { symbol_bytes(api, right.name as Oop) }.unwrap_or_default();
        left_name.cmp(&right_name)
    });
    let names = modules
        .iter()
        .map(|entry| unsafe { symbol_bytes(api, entry.name as Oop) })
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
    if values.len() != 1 && values.len() != 2 {
        abort("RustCodegen image compilation expects a code path and optional manifest path");
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
    let recovery_generation_available = IMAGE_DISPATCH_RECOVERY.load(Ordering::Acquire)
        && RUNTIME_PHASE.load(Ordering::Acquire) == PHASE_IMAGE_RECOVERY
        && unsafe { durable_generation_is_canonical(api, durable_current_generation(api)) };
    if !unsafe { image_generation_matches_durable(api) } && !recovery_generation_available {
        note_error(b"RustCodegen image generation is stale");
        return api.nilObj;
    }
    let generation = unsafe { durable_current_generation(api) };
    let source_map = unsafe {
        send(
            api,
            runtime.store as Oop,
            CURRENT_VALUE_SELECTOR,
            &[generation, intern(api, b"sourceMap\0")],
        )
    };
    let Some(modules) = (unsafe { generation_modules(api, source_map) }) else {
        note_error(b"RustCodegen durable image source map is malformed");
        return api.nilObj;
    };
    let Some(module_order) = generation_module_order(&modules) else {
        note_error(b"RustCodegen image source ordering failed");
        return api.nilObj;
    };
    if modules.is_empty() {
        note_error(b"RustCodegen image has no generation sources");
        return api.nilObj;
    }
    let Some(generation_selection) = (unsafe { durable_generation_selection(api, generation) })
    else {
        note_error(b"RustCodegen image generation selection is unavailable");
        return api.nilObj;
    };
    let Some(authority) = dispatch_generation_authority(&modules, &generation_selection) else {
        note_error(b"RustCodegen image generation selection is not authoritative");
        return api.nilObj;
    };
    let Some((artifact_selection, mut resumable_requests)) =
        image_artifact_selection_and_requests(&generation_selection)
    else {
        note_error(b"RustCodegen image artifact selection could not be projected");
        return api.nilObj;
    };
    resumable_requests.sort_by_key(OwnedSelectionRequest::key);
    let Ok(indices) = resumable_indices(&resumable_requests) else {
        note_error(b"RustCodegen image resumable index space overflowed");
        return api.nilObj;
    };

    let mut program = Vec::new();
    let mut entries = Vec::new();
    let mut named_resumables = Vec::new();
    let mut block_resumables = Vec::new();
    let mut operation_offset = 0u32;
    let mut non_method_block_offset = 0u32;
    let mut block_function_offset = 0u32;
    for module_index in module_order {
        let module = &modules[module_index];
        let bytes = &module.source;
        program.extend_from_slice(&bytes);
        let mut end = bytes.len();
        while end > 0 && bytes[end - 1].is_ascii_whitespace() {
            end -= 1;
        }
        if end == 0 || bytes[end - 1] != b';' {
            program.extend_from_slice(b";\n");
        } else {
            program.push(b'\n');
        }
        let module_requests = resumable_requests
            .iter()
            .filter(|request| request.source_revision == module.revision)
            .cloned()
            .collect::<Vec<_>>();
        if !module_requests.is_empty() {
            let source_object = unsafe { (api.mkString)(bytes.as_ptr(), bytes.len()) };
            let lang = unsafe { (api.import)(LANG_NAME.as_ptr().cast()) };
            let cst = unsafe { send(api, lang, PARSE_SELECTOR, &[source_object]) };
            let cst_vt = unsafe { (api.import)(PEG_CST_VT_NAME.as_ptr().cast()) };
            if cst.is_null() || cst == api.nilObj || unsafe { (api.vtableOf)(cst) != cst_vt } {
                note_error(b"RustCodegen selected image source could not be parsed");
                return api.nilObj;
            }
            let Some(tree) = (unsafe { cst_tree(api, cst, 0) }) else {
                note_error(b"RustCodegen selected image CST is invalid");
                return api.nilObj;
            };
            let Some(tree) = rewrite_embedded(tree, bytes, b".") else {
                note_error(b"RustCodegen selected image embedded source is unavailable");
                return api.nilObj;
            };
            let Ok(normalized) = lower_program(&tree, bytes) else {
                note_error(b"RustCodegen selected image source could not be lowered");
                return api.nilObj;
            };
            let Ok(mut plan) = compile_resumable_plan_indexed(
                &module.metadata,
                &module_requests,
                &indices,
                block_function_offset,
                &module.name,
                bytes,
                &normalized,
            ) else {
                note_error(b"RustCodegen selected image resumable lowering failed");
                return api.nilObj;
            };
            if rebase_image_resumable_plan(&mut plan, operation_offset, non_method_block_offset)
                .is_none()
            {
                note_error(b"RustCodegen image resumable key space overflowed");
                return api.nilObj;
            }
            entries.extend(plan.entries);
            named_resumables.extend(plan.named_emissions);
            block_resumables.extend(plan.block_emissions);
        }
        let Some(operation_count) = u32::try_from(module.metadata.operations.len()).ok() else {
            note_error(b"RustCodegen image operation space overflowed");
            return api.nilObj;
        };
        let Some(next_operation_offset) = operation_offset.checked_add(operation_count) else {
            note_error(b"RustCodegen image operation space overflowed");
            return api.nilObj;
        };
        operation_offset = next_operation_offset;
        let Some(next_non_method_offset) = non_method_block_count(&module.metadata)
            .and_then(|count| non_method_block_offset.checked_add(count))
        else {
            note_error(b"RustCodegen image non-method block space overflowed");
            return api.nilObj;
        };
        non_method_block_offset = next_non_method_offset;
        let Some(next_block_function_offset) = block_function_count(&module.metadata)
            .and_then(|count| block_function_offset.checked_add(count))
        else {
            note_error(b"RustCodegen image block function space overflowed");
            return api.nilObj;
        };
        block_function_offset = next_block_function_offset;
    }
    for operation in unsafe { ordered_method_operation_sources(api) } {
        let Some(bytes) = (unsafe { string_bytes(api, operation.source) }) else {
            note_error(b"RustCodegen image method source is not a String");
            return api.nilObj;
        };
        program.extend_from_slice(&bytes);
        program.extend_from_slice(b";\n");
    }
    let source_object = unsafe { (api.mkString)(program.as_ptr(), program.len()) };
    let _ = self_object;
    entries.sort_by_key(|entry| entry.request.key());
    let selection = match inspect_artifact_selection(&artifact_selection) {
        Ok(selection) => selection,
        Err(_) => {
            note_error(b"RustCodegen image artifact selection is invalid");
            return api.nilObj;
        }
    };
    let manifest_entries = entries
        .iter()
        .map(|entry| ManifestEntrySpec {
            selection: SelectionSpec {
                source_revision: entry.request.source_revision,
                root_kind: entry.request.root_kind,
                root_index: entry.request.root_index,
                path: &entry.request.path,
                implementation_kind: entry.request.implementation_kind,
                disposition: entry.request.disposition,
            },
            descriptor: &entry.descriptor,
            selector: &entry.selector,
        })
        .collect::<Vec<_>>();
    let Ok(manifest) = resumable_manifest::materialize(authority, selection, &manifest_entries)
    else {
        note_error(b"RustCodegen image resumable manifest could not be materialized");
        return api.nilObj;
    };
    let emissions_present = !named_resumables.is_empty() || !block_resumables.is_empty();
    let Some(emitted) = (unsafe {
        emit_image_recovery_source(
            api,
            source_object,
            emissions_present.then_some(named_resumables),
            emissions_present.then_some(block_resumables),
        )
    }) else {
        note_error(b"RustCodegen could not lower selected image source");
        return api.nilObj;
    };
    let result = unsafe {
        write_emitted_artifact(
            api,
            source_object,
            values[0],
            emitted,
            true,
            Some(&artifact_selection),
            Some(&manifest),
        )
    };
    if result == api.nilObj || values.len() == 1 {
        return result;
    }
    let Some(manifest_path) = (unsafe { string_bytes(api, values[1]) }) else {
        note_error(b"RustCodegen image manifest path was not a String");
        return api.nilObj;
    };
    let manifest_path = Path::new(OsStr::from_bytes(&manifest_path));
    if fs::write(manifest_path, &manifest).is_err() {
        if let Some(code_path) = unsafe { string_bytes(api, values[0]) } {
            let _ = fs::remove_file(Path::new(OsStr::from_bytes(&code_path)));
        }
        note_error(b"RustCodegen could not write the image resumable manifest");
        return api.nilObj;
    }
    result
}

/// Recompile the durable generation's exact accepted projection and map the
/// fresh unit during source recovery. Unlike ordinary mapped recovery, this
/// path does not trust or execute the image's captured native bytes.
unsafe extern "C" fn rust_codegen_rebuild_image_code(
    self_object: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    if count != 0 {
        abort("RustCodegen rebuildImageCode expects no arguments");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    clear_error();
    if !IMAGE_DISPATCH_RECOVERY.load(Ordering::Acquire)
        || RUNTIME_PHASE.load(Ordering::Acquire) != PHASE_IMAGE_RECOVERY
    {
        note_error(b"RustCodegen image code rebuild requires image recovery");
        return api.nilObj;
    }
    let sequence = IMAGE_CODE_LOAD_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let mut stem = std::env::temp_dir().into_os_string().into_vec();
    if !stem.ends_with(b"/") {
        stem.push(b'/');
    }
    stem.extend_from_slice(b"birnam-image-rebuild-");
    stem.extend_from_slice(std::process::id().to_string().as_bytes());
    stem.push(b'-');
    stem.extend_from_slice(sequence.to_string().as_bytes());
    let mut code_path = stem.clone();
    code_path.extend_from_slice(b".dylib");
    let mut manifest_path = stem;
    manifest_path.extend_from_slice(b".manifest");
    struct RebuiltPaths<'a> {
        code: &'a [u8],
        manifest: &'a [u8],
    }
    impl Drop for RebuiltPaths<'_> {
        fn drop(&mut self) {
            let _ = fs::remove_file(Path::new(OsStr::from_bytes(self.code)));
            let _ = fs::remove_file(Path::new(OsStr::from_bytes(self.manifest)));
        }
    }
    let _paths = RebuiltPaths {
        code: &code_path,
        manifest: &manifest_path,
    };
    let code_object = unsafe { (api.mkString)(code_path.as_ptr(), code_path.len()) };
    let manifest_object = unsafe { (api.mkString)(manifest_path.as_ptr(), manifest_path.len()) };
    let compiled = unsafe {
        rust_codegen_compile_image_code(self_object, [code_object, manifest_object].as_ptr(), 2)
    };
    if compiled.is_null() || compiled == api.nilObj {
        note_error(b"RustCodegen image code rebuild failed");
        return api.nilObj;
    }
    let Ok(manifest) = fs::read(Path::new(OsStr::from_bytes(&manifest_path))) else {
        note_error(b"RustCodegen rebuilt image manifest is unavailable");
        return api.nilObj;
    };
    let generation = unsafe { durable_current_generation(api) };
    let Some(digest) = (unsafe { durable_generation_base_digest(api, generation) }) else {
        note_error(b"RustCodegen rebuilt image authority is unavailable");
        return api.nilObj;
    };
    let Some(authority) = decode_hex_digest(&digest) else {
        note_error(b"RustCodegen rebuilt image authority is invalid");
        return api.nilObj;
    };
    if resumable_manifest::inspect(&manifest, authority).is_err() {
        note_error(b"RustCodegen rebuilt image manifest is invalid");
        return api.nilObj;
    }
    unsafe {
        rust_codegen_load_impl(
            self_object,
            &code_object,
            1,
            ResumableLoadPolicy::Expected(ResumableLoadExpectation {
                authority,
                manifest: &manifest,
            }),
        )
    }
}

#[derive(Clone, Copy)]
struct ResumableLoadExpectation<'a> {
    authority: [u8; 32],
    manifest: &'a [u8],
}

#[derive(Clone, Copy)]
enum ResumableLoadPolicy<'a> {
    None,
    Expected(ResumableLoadExpectation<'a>),
    InitialModule,
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
    named_resumables: Option<Vec<NamedResumableEmission>>,
    block_resumables: Option<Vec<BlockResumableEmission>>,
    projected_statement_indices: Option<&[usize]>,
) -> Option<EmittedSource> {
    let source = unsafe { string_bytes(api, source_object) }?;
    let lang = unsafe { (api.import)(LANG_NAME.as_ptr().cast()) };
    let cst = unsafe { send(api, lang, PARSE_SELECTOR, &[source_object]) };
    let cst_vt = unsafe { (api.import)(PEG_CST_VT_NAME.as_ptr().cast()) };
    if cst.is_null() || cst == api.nilObj || unsafe { (api.vtableOf)(cst) != cst_vt } {
        unsafe { note_syntax_error(api, lang, source_object) };
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
        Err(error) => {
            if error == LowerError::ThreadedSpecialForm {
                note_error(b"RustCodegen source-sensitive special forms cannot use => threading");
            } else {
                note_error(b"RustCodegen could not lower the source");
            }
            return None;
        }
    };
    let artifact_salt = candidate_salt.or_else(|| {
        artifact.then(|| {
            let digest = sha256_hex(&source);
            u64::from_be_bytes(digest[..8].try_into().unwrap())
        })
    });
    let mut emitter = match (artifact_salt, named_resumables, block_resumables) {
        (Some(salt), Some(named), blocks) => Emitter::new_selected_resumable_candidate_artifact(
            salt,
            named,
            blocks.unwrap_or_default(),
        )
        .ok()?,
        (Some(salt), None, Some(blocks)) => {
            Emitter::new_selected_resumable_candidate_artifact(salt, Vec::new(), blocks).ok()?
        }
        (Some(salt), None, None) => Emitter::new_candidate_artifact(salt),
        (None, Some(_), _) | (None, _, Some(_)) => return None,
        (None, None, None) => Emitter::new_for_artifact(artifact),
    };
    if let Some(indices) = projected_statement_indices {
        if emitter
            .emit_projected_candidate_program(&program, indices)
            .is_err()
        {
            note_error(b"RustCodegen could not emit the projected initializer");
            return None;
        }
    } else if persist_top_level {
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
    let baked_globals = emitter.baked_globals().to_vec();
    let (function_defs, body) = emitter.finish_parts();
    let mut unit = TranslationUnit::new(artifact);
    for name in &baked_globals {
        unit.add_baked_global(name);
    }
    unit.push_function(&function_defs);
    unit.push_body(&body);
    Some(EmittedSource {
        bytes: unit.finish(),
        baked_globals,
    })
}

unsafe fn emit_image_recovery_source(
    api: &IdApi,
    source_object: Oop,
    named_resumables: Option<Vec<NamedResumableEmission>>,
    block_resumables: Option<Vec<BlockResumableEmission>>,
) -> Option<EmittedSource> {
    let source = unsafe { string_bytes(api, source_object) }?;
    let lang = unsafe { (api.import)(LANG_NAME.as_ptr().cast()) };
    let cst = unsafe { send(api, lang, PARSE_SELECTOR, &[source_object]) };
    let cst_vt = unsafe { (api.import)(PEG_CST_VT_NAME.as_ptr().cast()) };
    if cst.is_null() || cst == api.nilObj || unsafe { (api.vtableOf)(cst) != cst_vt } {
        unsafe { note_syntax_error(api, lang, source_object) };
        return None;
    }
    let tree = unsafe { cst_tree(api, cst, 0) }?;
    let tree = rewrite_embedded(tree, &source, b".")?;
    let program = lower_program(&tree, &source).ok()?;
    let digest = sha256_hex(&source);
    let salt = u64::from_be_bytes(digest[..8].try_into().ok()?);
    let mut emitter = Emitter::new_selected_resumable_candidate_artifact(
        salt,
        named_resumables.unwrap_or_default(),
        block_resumables.unwrap_or_default(),
    )
    .ok()?;
    if emitter.emit_image_recovery_program(&program).is_err() {
        note_error(b"RustCodegen could not emit the image recovery projection");
        return None;
    }
    let baked_globals = emitter.baked_globals().to_vec();
    let (function_defs, body) = emitter.finish_parts();
    let mut unit = TranslationUnit::new(true);
    for name in &baked_globals {
        unit.add_baked_global(name);
    }
    unit.push_function(&function_defs);
    unit.push_body(&body);
    Some(EmittedSource {
        bytes: unit.finish(),
        baked_globals,
    })
}

struct EmittedSource {
    bytes: Vec<u8>,
    baked_globals: Vec<Vec<u8>>,
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
    let Some(output) = (unsafe {
        emit_source(
            api, values[0], false, false, None, false, None, None, None, None,
        )
    }) else {
        note_error(b"RustCodegen could not lower the source");
        return api.nilObj;
    };
    // SAFETY: mkString copies the generated bytes before this call returns.
    unsafe { (api.mkString)(output.bytes.as_ptr(), output.bytes.len()) }
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
    let name_symbol = unsafe { intern_bytes(api, &metadata.name) };
    let require_symbols = metadata
        .requires
        .iter()
        .map(|requirement| unsafe { intern_bytes(api, requirement) })
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
    unsafe { source_revision_from_metadata(api, values[0], &metadata) }
}

unsafe fn source_revision_from_metadata(
    api: &IdApi,
    source_object: Oop,
    metadata: &RevisionMetadata,
) -> Oop {
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
    let name_symbol = unsafe { intern_bytes(api, &metadata.name) };
    let require_symbols = metadata
        .requires
        .iter()
        .map(|requirement| unsafe { intern_bytes(api, requirement) })
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
        source_object,
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
            None,
            None,
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
    let c_source = unsafe { (api.mkString)(source.bytes.as_ptr(), source.bytes.len()) };
    if unsafe { send(api, unit, COMPILE_SELECTOR, &[c_source]) } == api.nilObj
        || !unsafe { bind_global_shims(api, unit) }
        || !unsafe { bind_baked_global_shims(api, unit, &source.baked_globals) }
        || unsafe { send0(api, unit, BIND_RUNTIME_SELECTOR) } == api.nilObj
        || unsafe { send0(api, unit, RELOCATE_SELECTOR) } == api.nilObj
        || !unsafe { bind_jit_code_generation_identity(api, unit) }
    {
        unsafe { dispose_unit(api, unit) };
        note_error(b"RustCodegen generated C failed to compile");
        return api.nilObj;
    }
    let entry_name = unsafe { (api.mkString)(b"__ev".as_ptr(), 4) };
    let entry_selector = unsafe { intern(api, ENTRY_SELECTOR) };
    if !source.bytes.windows(5).any(|window| window == b"__blk") {
        let symbol = unsafe { send(api, unit, SYMBOL_SELECTOR, &[entry_name]) };
        let Some(address) = (unsafe { foreign_address_value(api, symbol) }) else {
            unsafe { dispose_unit(api, unit) };
            note_error(b"RustCodegen could not resolve generated entry point");
            return api.nilObj;
        };
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

fn issue_materialization_namespace() -> Option<u32> {
    NEXT_MATERIALIZATION_NAMESPACE
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
            current.checked_add(1).filter(|next| *next < 1 << 31)
        })
        .ok()
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value != 0)
}

fn next_materialization_age() -> u64 {
    NEXT_MATERIALIZATION_AGE.fetch_add(1, Ordering::AcqRel)
}

fn cached_materialization(
    disposition: MaterializationDisposition,
    digest: [u8; 32],
    program: &[u8],
) -> Option<(Oop, Oop)> {
    let mut entries = MATERIALIZATIONS.lock().ok()?;
    let entry = entries.iter_mut().find(|entry| {
        entry.ready
            && entry.disposition == disposition
            && entry.digest == digest
            && entry.program == program
    })?;
    entry.age = next_materialization_age();
    Some((entry.vtable as Oop, entry.unit as Oop))
}

fn stage_materialization(
    disposition: MaterializationDisposition,
    digest: [u8; 32],
    program: Vec<u8>,
    unit: Oop,
    generated_bytes: usize,
) -> bool {
    let Ok(mut entries) = MATERIALIZATIONS.lock() else {
        return false;
    };
    while entries.len() >= MAX_MATERIALIZATION_CACHE_ENTRIES
        || entries
            .iter()
            .try_fold(0usize, |total, entry| {
                total.checked_add(entry.program.len())
            })
            .and_then(|total| total.checked_add(program.len()))
            .is_none_or(|total| total > MAX_MATERIALIZATION_CACHE_PROGRAM_BYTES)
        || entries
            .iter()
            .try_fold(0usize, |total, entry| {
                total.checked_add(entry.generated_bytes)
            })
            .and_then(|total| total.checked_add(generated_bytes))
            .is_none_or(|total| total > MAX_MATERIALIZATION_C_SOURCE_BYTES)
    {
        let Some((index, _)) = entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.ready)
            .min_by_key(|(_, entry)| entry.age)
        else {
            return false;
        };
        entries.remove(index);
    }
    entries.push(MaterializationEntry {
        disposition,
        digest,
        program,
        vtable: 0,
        unit: unit as usize,
        generated_bytes,
        age: next_materialization_age(),
        ready: false,
    });
    true
}

fn set_staged_materialization_vtable(unit: Oop, vtable: Oop) -> bool {
    let Ok(mut entries) = MATERIALIZATIONS.lock() else {
        return false;
    };
    let Some(entry) = entries
        .iter_mut()
        .find(|entry| !entry.ready && entry.unit == unit as usize)
    else {
        return false;
    };
    entry.vtable = vtable as usize;
    true
}

fn commit_staged_materialization(unit: Oop) -> bool {
    let Ok(mut entries) = MATERIALIZATIONS.lock() else {
        return false;
    };
    let Some(entry) = entries
        .iter_mut()
        .find(|entry| !entry.ready && entry.unit == unit as usize && entry.vtable != 0)
    else {
        return false;
    };
    entry.ready = true;
    entry.age = next_materialization_age();
    true
}

fn remove_materialization(unit: Oop) {
    let removed = if let Ok(mut entries) = MATERIALIZATIONS.lock() {
        let before = entries.len();
        entries.retain(|entry| entry.unit != unit as usize);
        before != entries.len()
    } else {
        false
    };
    if removed {
        let _ = take_vtable_cache_roots(unit as usize as u64);
    }
}

unsafe fn compile_synchronous_materialization(
    api: &IdApi,
    program: &[u8],
    digest: [u8; 32],
) -> Option<(Oop, Oop)> {
    let namespace = issue_materialization_namespace()?;
    let compiled = compile_synchronous_block_v1(program, namespace).ok()?;
    if compiled.c_source.len() > MAX_MATERIALIZATION_C_SOURCE_BYTES {
        return None;
    }
    let runtime = runtime();
    let tcc = runtime.tcc as Oop;
    if tcc.is_null() {
        return None;
    }
    let unit = unsafe { send0(api, tcc, NEW_SELECTOR) };
    if unit.is_null() || unit == api.nilObj {
        return None;
    }
    let c_source = unsafe { (api.mkString)(compiled.c_source.as_ptr(), compiled.c_source.len()) };
    if unsafe { send(api, unit, COMPILE_SELECTOR, &[c_source]) } == api.nilObj
        || !unsafe { bind_global_shims(api, unit) }
        || unsafe { send0(api, unit, BIND_RUNTIME_SELECTOR) } == api.nilObj
        || unsafe { send0(api, unit, RELOCATE_SELECTOR) } == api.nilObj
        || !unsafe { bind_jit_code_generation_identity(api, unit) }
    {
        unsafe { dispose_unit(api, unit) };
        return None;
    }
    let entry_name =
        unsafe { (api.mkString)(compiled.entry_symbol.as_ptr(), compiled.entry_symbol.len()) };
    let symbol = unsafe { send(api, unit, SYMBOL_SELECTOR, &[entry_name]) };
    let Some(address) = (unsafe { foreign_address_value(api, symbol) }) else {
        unsafe { dispose_unit(api, unit) };
        return None;
    };
    if !stage_materialization(
        MaterializationDisposition::Synchronous,
        digest,
        program.to_vec(),
        unit,
        compiled.c_source.len(),
    ) {
        unsafe { dispose_unit(api, unit) };
        return None;
    }
    if address == 0 {
        remove_materialization(unit);
        unsafe { dispose_unit(api, unit) };
        return None;
    }
    let block_family = unsafe { (api.import)(c"BlockVt".as_ptr()) };
    let vtable = if block_family.is_null() || block_family == api.nilObj {
        ptr::null_mut()
    } else {
        unsafe { (api.proto)(block_family) }
    };
    if vtable.is_null() || !set_staged_materialization_vtable(unit, vtable) {
        remove_materialization(unit);
        unsafe { dispose_unit(api, unit) };
        return None;
    }
    let selector = unsafe { intern_bytes(api, &compiled.selector) };
    let entry: Method = unsafe { core::mem::transmute(address) };
    unsafe {
        (api.method)(vtable, selector, entry);
        (api.setLayout)(vtable, ptr::null(), 0, SLOT_OOP);
    }
    if unsafe { send0(api, unit, DISPOSABLE_SELECTOR) } == api.nilObj {
        remove_materialization(unit);
        return None;
    }
    Some((vtable, unit))
}

struct PortableResumablePlan {
    programs: Vec<u8>,
    bindings: Vec<u8>,
    selection: Vec<u8>,
    manifest: Vec<u8>,
    authority: [u8; 32],
    c_source: Vec<u8>,
    root_manifest_entry_index: u32,
}

struct PortableNamedPlan {
    programs: Vec<u8>,
    bindings: Vec<u8>,
    selection: Vec<u8>,
    manifest: Vec<u8>,
    authority: [u8; 32],
    c_source: Vec<u8>,
}

struct PortableImagePlan {
    programs: Vec<u8>,
    bindings: Vec<u8>,
    selection: Vec<u8>,
    manifest: Vec<u8>,
    authority: [u8; 32],
    c_source: Vec<u8>,
    binding_count: u32,
    program_count: u32,
}

fn portable_image_plan(
    programs: &[u8],
    bindings: &[u8],
    selection: &[u8],
) -> Option<PortableImagePlan> {
    let program_inventory = portable_inventory::inspect(programs).ok()?;
    let binding_inventory = code_binding_inventory::inspect(bindings).ok()?;
    let selection_view = materialization_selection::inspect(selection, programs, bindings).ok()?;
    let binding_count = binding_inventory.binding_count;
    let program_count = program_inventory.program_count;
    if selection_view.row_count != binding_count.checked_add(program_count)? {
        return None;
    }
    let mut named = Vec::with_capacity(binding_count as usize);
    for index in 0..binding_count {
        let row = selection_view.row(index as usize)?;
        named.push(compile_portable_materialization_named_v1(row, index, index).ok()?);
    }
    let mut blocks = Vec::with_capacity(program_count as usize);
    for program_index in 0..program_count {
        let program = program_inventory.program(program_index as usize)?;
        let inspected = birnam_artifact::portable_program::inspect(program).ok()?;
        let mut literals = Vec::new();
        for node_index in 0..inspected.node_count as usize {
            let node = inspected.node(node_index)?;
            if node.semantic_kind()
                != Some(birnam_artifact::portable_program::NodeKind::BlockLiteral)
            {
                continue;
            }
            let nested_index = (0..program_count as usize)
                .find(|index| program_inventory.program(*index) == Some(node.data))?;
            literals.push(PortableBlockLiteralSpec {
                node_index: u32::try_from(node_index).ok()?,
                disposition: BlockLiteralDisposition::Resumable {
                    manifest_entry_index: binding_count
                        .checked_add(u32::try_from(nested_index).ok()?)?,
                },
                portable_program: node.data.to_vec(),
                retain_materialization_owner: true,
            });
        }
        let row_index = binding_count.checked_add(program_index)?;
        blocks.push(
            compile_portable_materialization_block_v1(
                program,
                selection_view.row(row_index as usize)?,
                row_index,
                row_index,
                &literals,
            )
            .ok()?,
        );
    }
    let authority = materialization_selection::authority(programs, bindings, selection).ok()?;
    let mut manifest_entries = Vec::with_capacity(selection_view.row_count as usize);
    for (index, entry) in named.iter().enumerate() {
        manifest_entries.push(portable_resumable_manifest::EntrySpec {
            selection_row_index: u32::try_from(index).ok()?,
            driver_argument_count: 0,
            lowering_version: resumable_descriptor::LOWERING_VERSION_V1,
            descriptor: &entry.descriptor,
        });
    }
    for (index, entry) in blocks.iter().enumerate() {
        manifest_entries.push(portable_resumable_manifest::EntrySpec {
            selection_row_index: binding_count.checked_add(u32::try_from(index).ok()?)?,
            driver_argument_count: entry.driver_argument_count,
            lowering_version: resumable_descriptor::LOWERING_VERSION_V1,
            descriptor: &entry.descriptor,
        });
    }
    let manifest =
        portable_resumable_manifest::materialize(authority, &selection_view, &manifest_entries)
            .ok()?;
    let mut translation_unit = TranslationUnit::new(false);
    translation_unit.push_function(resumable_registration_c::registration_prelude_v2());
    fn push_exported_bytes(unit: &mut TranslationUnit, name: &str, bytes: &[u8]) {
        let mut declaration = format!("const unsigned char {name}[]={{").into_bytes();
        for (index, byte) in bytes.iter().enumerate() {
            if index != 0 {
                declaration.push(b',');
            }
            declaration.extend_from_slice(byte.to_string().as_bytes());
        }
        declaration.extend_from_slice(
            format!(
                "}};\nconst unsigned long long {name}_size={}ULL;\n",
                bytes.len()
            )
            .as_bytes(),
        );
        unit.push_function(&declaration);
    }
    push_exported_bytes(
        &mut translation_unit,
        "__birnam_portable_program_inventory_v1",
        programs,
    );
    push_exported_bytes(
        &mut translation_unit,
        "__birnam_code_binding_inventory_v1",
        bindings,
    );
    push_exported_bytes(
        &mut translation_unit,
        "__birnam_code_materialization_selection_v1",
        selection,
    );
    push_exported_bytes(
        &mut translation_unit,
        "__birnam_portable_resumable_manifest_v1",
        &manifest,
    );
    translation_unit.push_function(b"void* __birnam_materialization_owner;\n");
    translation_unit.push_function(
        b"void* __birnam_resolve_block_vtable_v1(unsigned int manifest_entry_index);\n",
    );
    for entry in &named {
        translation_unit.push_function(&entry.c_source);
    }
    for entry in &blocks {
        translation_unit.push_function(&entry.c_source);
    }
    let mut prepare_blocks =
        b"unsigned int __birnam_prepare_portable_image_blocks_v1(void){\n".to_vec();
    for index in 0..program_count {
        let row = binding_count.checked_add(index)?;
        prepare_blocks.extend_from_slice(
            format!("if(__birnam_resumable_prepare_{row}((void*)0)!=0U)return 1U;\n").as_bytes(),
        );
    }
    prepare_blocks.extend_from_slice(b"return 0U;}\n");
    translation_unit.push_function(&prepare_blocks);
    let mut prepare_binding = b"unsigned int __birnam_prepare_portable_image_binding_v1(unsigned int index,void* implementation){(void)implementation;switch(index){\n".to_vec();
    for index in 0..binding_count {
        prepare_binding.extend_from_slice(
            format!("case {index}U:return __birnam_resumable_prepare_{index}(implementation);\n")
                .as_bytes(),
        );
    }
    prepare_binding.extend_from_slice(b"default:return 1U;}}\n");
    translation_unit.push_function(&prepare_binding);
    let mut binding_entry = b"unsigned long long __birnam_portable_image_binding_entry_address_v1(unsigned int index){switch(index){\n".to_vec();
    for index in 0..binding_count {
        binding_entry.extend_from_slice(format!("case {index}U:return (unsigned long long)(unsigned long)&__birnam_resumable_thunk_{index};\n").as_bytes());
    }
    binding_entry.extend_from_slice(b"default:return 0ULL;}}\n");
    translation_unit.push_function(&binding_entry);
    if !blocks.is_empty() {
        let resolver_specs = blocks
            .iter()
            .map(|entry| entry.resolver_spec())
            .collect::<Vec<_>>();
        translation_unit.push_function(
            &resumable_resolver_c::emit_block_vtable_resolver_v1(&resolver_specs).ok()?,
        );
    }
    translation_unit.push_body(b"return birnam_nil();\n");
    Some(PortableImagePlan {
        programs: programs.to_vec(),
        bindings: bindings.to_vec(),
        selection: selection.to_vec(),
        manifest,
        authority,
        c_source: translation_unit.finish(),
        binding_count,
        program_count,
    })
}

type PortableBegin = unsafe extern "C" fn(
    *const u8,
    usize,
    *const u8,
    usize,
    *const u8,
    usize,
    *const u8,
    usize,
    *const u8,
    u64,
    u64,
) -> u32;
type PortableClose = unsafe extern "C" fn() -> u32;
type PortableBeginContext = unsafe extern "C" fn(u64, u64) -> u32;
type PortableEndContext = unsafe extern "C" fn() -> u32;
type PortablePrepareBlocks = unsafe extern "C" fn() -> u32;
type PortablePrepareBinding = unsafe extern "C" fn(u32, Oop) -> u32;
type PortableBindingEntryAddress = unsafe extern "C" fn(u32) -> u64;
type PortableResolveBlockVtable = unsafe extern "C" fn(u32) -> Oop;
type StagedBlockVtable = unsafe extern "C" fn(*const u8, usize, u32) -> Oop;

unsafe fn portable_native_symbol(
    api: &IdApi,
    unit: Oop,
    handle: usize,
    name: &[u8],
) -> Option<usize> {
    if !unit.is_null() {
        return unsafe { unit_symbol_address(api, unit, name) };
    }
    let name = CString::new(name).ok()?;
    let symbol = unsafe { (api.dlsym)(handle as *mut c_void, name.as_ptr()) };
    (!symbol.is_null()).then_some(symbol as usize)
}

unsafe extern "C" fn portable_mapped_unit_finalize(owner: Oop) {
    if owner.is_null() {
        return;
    }
    let payload = owner.cast::<PortableMappedUnitPayload>();
    let handle = unsafe { (*payload).handle };
    let generation = unsafe { (*payload).code_generation_identity };
    unsafe {
        (*payload).handle = 0;
        (*payload).code_generation_identity = 0;
    }
    if handle == 0 {
        return;
    }
    let api = unsafe { api(runtime()) };
    if unsafe { retire_runtime_generation_v1(api, generation) } != RuntimeRetirement::Retired {
        abort("portable mapped image generation remained live at finalization");
    }
    if unsafe { dlclose(handle as *mut c_void) } != 0 {
        abort("portable mapped image unit could not be unmapped");
    }
}

unsafe fn make_portable_mapped_owner(api: &IdApi, handle: usize, generation: u64) -> Option<Oop> {
    let owner = unsafe {
        (api.alloc)(
            runtime().portable_mapped_unit_vt as Oop,
            size_of::<PortableMappedUnitPayload>(),
        )
    };
    if owner.is_null() {
        return None;
    }
    unsafe {
        owner
            .cast::<PortableMappedUnitPayload>()
            .write(PortableMappedUnitPayload {
                handle,
                code_generation_identity: generation,
            });
    }
    Some(owner)
}

unsafe fn release_uncommitted_portable_owner(
    api: &IdApi,
    carrier_kind: PortableImageCarrierKind,
    owner: Oop,
    handle: usize,
) {
    match carrier_kind {
        PortableImageCarrierKind::Rebuilt => unsafe { dispose_unit(api, owner) },
        PortableImageCarrierKind::Mapped => {
            if !owner.is_null() {
                let payload = owner.cast::<PortableMappedUnitPayload>();
                unsafe {
                    (*payload).handle = 0;
                    (*payload).code_generation_identity = 0;
                }
            }
            if handle != 0 {
                let _ = unsafe { dlclose(handle as *mut c_void) };
            }
        }
    }
}

unsafe fn begin_portable_image_stage(
    api: &IdApi,
    plan: PortableImagePlan,
    carrier_kind: PortableImageCarrierKind,
    unit: Oop,
    handle: usize,
    owner: Oop,
    generated_bytes: usize,
) -> Option<Oop> {
    if PORTABLE_IMAGE_STAGE.lock().ok()?.is_some() {
        return None;
    }
    let generation = match carrier_kind {
        PortableImageCarrierKind::Rebuilt => u64::try_from(unit as usize).ok()?,
        PortableImageCarrierKind::Mapped => u64::try_from(handle).ok()?,
    };
    if generation == 0 || generation > i64::MAX as u64 {
        return None;
    }
    let lease = issue_code_generation_lease()?;
    let identity_address =
        unsafe { portable_native_symbol(api, unit, handle, b"__birnam_code_generation_identity") }?;
    unsafe { (identity_address as *mut u64).write(generation) };
    let owner_address =
        unsafe { portable_native_symbol(api, unit, handle, b"__birnam_materialization_owner") }?;
    unsafe { (owner_address as *mut Oop).write(owner) };
    if carrier_kind == PortableImageCarrierKind::Rebuilt {
        let identity = unsafe { (api.mkSmallInt)(generation as i64) };
        if unsafe { send(api, unit, OWN_CODE_GENERATION_SELECTOR, &[identity]) } == api.nilObj {
            return None;
        }
    }
    let begin_address =
        unsafe { floor_symbol(api, b"birnam_id_begin_portable_resumable_unit_v1\0") }?;
    let finish_address = unsafe { floor_symbol(api, b"birnam_id_finish_resumable_unit_v1\0") }?;
    let cancel_address = unsafe { floor_symbol(api, b"birnam_id_cancel_resumable_unit_v1\0") }?;
    let staged_vtable_address =
        unsafe { floor_symbol(api, b"birnam_id_staged_portable_block_vtable_v1\0") }?;
    let begin: PortableBegin = unsafe { core::mem::transmute(begin_address) };
    let finish: PortableClose = unsafe { core::mem::transmute(finish_address) };
    let cancel: PortableClose = unsafe { core::mem::transmute(cancel_address) };
    let begin_context_address = unsafe {
        portable_native_symbol(api, unit, handle, b"__birnam_resumable_context_begin_v1")
    }?;
    let end_context =
        unsafe { portable_native_symbol(api, unit, handle, b"__birnam_resumable_context_end_v1") }?;
    let prepare_blocks_address = unsafe {
        portable_native_symbol(
            api,
            unit,
            handle,
            b"__birnam_prepare_portable_image_blocks_v1",
        )
    }?;
    let prepare_binding = unsafe {
        portable_native_symbol(
            api,
            unit,
            handle,
            b"__birnam_prepare_portable_image_binding_v1",
        )
    }?;
    let binding_entry_address = unsafe {
        portable_native_symbol(
            api,
            unit,
            handle,
            b"__birnam_portable_image_binding_entry_address_v1",
        )
    }?;
    let resolve = (plan.program_count != 0)
        .then(|| unsafe {
            portable_native_symbol(api, unit, handle, b"__birnam_resolve_block_vtable_v1")
        })
        .flatten();
    if plan.program_count != 0 && resolve.is_none() {
        return None;
    }
    if unsafe {
        begin(
            plan.programs.as_ptr(),
            plan.programs.len(),
            plan.bindings.as_ptr(),
            plan.bindings.len(),
            plan.selection.as_ptr(),
            plan.selection.len(),
            plan.manifest.as_ptr(),
            plan.manifest.len(),
            plan.authority.as_ptr(),
            generation,
            lease,
        )
    } != 0
    {
        return None;
    }
    let begin_context: PortableBeginContext =
        unsafe { core::mem::transmute(begin_context_address) };
    let prepare_blocks: PortablePrepareBlocks =
        unsafe { core::mem::transmute(prepare_blocks_address) };
    if unsafe { begin_context(generation, lease) } != 0 || unsafe { prepare_blocks() } != 0 {
        let end: PortableEndContext = unsafe { core::mem::transmute(end_context) };
        let _ = unsafe { end() };
        let _ = unsafe { cancel() };
        return None;
    }
    let staged_vtable: StagedBlockVtable = unsafe { core::mem::transmute(staged_vtable_address) };
    for program_index in 0..plan.program_count {
        let manifest_index = plan
            .binding_count
            .checked_add(program_index)
            .expect("portable plan row count was already bounded");
        let resolve: PortableResolveBlockVtable =
            unsafe { core::mem::transmute(resolve.expect("nonempty programs require resolver")) };
        let resolved = unsafe { resolve(manifest_index) };
        if resolved.is_null()
            || unsafe {
                staged_vtable(
                    plan.authority.as_ptr(),
                    plan.authority.len(),
                    manifest_index,
                )
            } != resolved
        {
            let end: PortableEndContext = unsafe { core::mem::transmute(end_context) };
            let _ = unsafe { end() };
            let _ = unsafe { cancel() };
            return None;
        }
    }
    *PORTABLE_IMAGE_STAGE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(PortableImageStage {
        carrier_kind,
        owner: owner as usize,
        handle,
        generation,
        authority: plan.authority,
        bindings: plan.bindings,
        binding_count: plan.binding_count,
        program_count: plan.program_count,
        generated_bytes,
        finish: finish as usize,
        cancel: cancel as usize,
        end_context,
        prepare_binding,
        binding_entry_address,
        committed: false,
        prepared_bindings: 0,
        installed_bindings: 0,
    });
    Some(owner)
}

unsafe fn mapped_export_matches(api: &IdApi, handle: usize, name: &[u8], expected: &[u8]) -> bool {
    let Ok(name_c) = CString::new(name) else {
        return false;
    };
    let mut size_name = name.to_vec();
    size_name.extend_from_slice(b"_size");
    let Ok(size_name) = CString::new(size_name) else {
        return false;
    };
    let bytes = unsafe { (api.dlsym)(handle as *mut c_void, name_c.as_ptr()) };
    let size = unsafe { (api.dlsym)(handle as *mut c_void, size_name.as_ptr()) };
    if bytes.is_null() || size.is_null() {
        return false;
    }
    let size = unsafe { ptr::read_unaligned(size.cast::<u64>()) };
    usize::try_from(size).ok() == Some(expected.len())
        && unsafe { core::slice::from_raw_parts(bytes.cast::<u8>(), expected.len()) } == expected
}

unsafe fn compile_portable_image_code_bytes(
    api: &IdApi,
    programs: &[u8],
    bindings: &[u8],
    selection: &[u8],
) -> Option<Vec<u8>> {
    let plan = portable_image_plan(programs, bindings, selection)?;
    if plan.c_source.len() > MAX_MATERIALIZATION_C_SOURCE_BYTES {
        return None;
    }
    let sequence = IMAGE_CODE_LOAD_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "birnam-portable-image-{}-{sequence}.dylib",
        std::process::id()
    ));
    let path_bytes = path.as_os_str().as_bytes();
    let source = unsafe { (api.mkString)(plan.c_source.as_ptr(), plan.c_source.len()) };
    let path_oop = unsafe { (api.mkString)(path_bytes.as_ptr(), path_bytes.len()) };
    let kind = unsafe { intern(api, b"dll\0") };
    let result = unsafe {
        send(
            api,
            runtime().tcc as Oop,
            TCC_COMPILE_OUTPUT_SELECTOR,
            &[source, path_oop, kind],
        )
    };
    if result == api.nilObj {
        let _ = fs::remove_file(&path);
        return None;
    }
    let bytes = fs::read(&path).ok();
    let _ = fs::remove_file(&path);
    bytes.filter(|bytes| !bytes.is_empty())
}

unsafe extern "C" fn rust_codegen_compile_portable_image_code(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustCodegen portable image compile received invalid arguments"));
    if values.len() != 3 {
        abort("RustCodegen portable image compile expects programs, bindings, and selection");
    }
    let api = unsafe { api(runtime()) };
    let Some(programs) = (unsafe { string_bytes(api, values[0]) }) else {
        return api.nilObj;
    };
    let Some(bindings) = (unsafe { string_bytes(api, values[1]) }) else {
        return api.nilObj;
    };
    let Some(selection) = (unsafe { string_bytes(api, values[2]) }) else {
        return api.nilObj;
    };
    let Some(code) =
        (unsafe { compile_portable_image_code_bytes(api, &programs, &bindings, &selection) })
    else {
        note_error(b"RustCodegen could not compile portable image Code");
        return api.nilObj;
    };
    unsafe { (api.mkString)(code.as_ptr(), code.len()) }
}

unsafe extern "C" fn rust_codegen_begin_portable_image_rebuild(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustCodegen portable image rebuild received invalid arguments"));
    if values.len() != 3 {
        abort("RustCodegen portable image rebuild expects programs, bindings, and selection");
    }
    let api = unsafe { api(runtime()) };
    let (Some(programs), Some(bindings), Some(selection)) = (
        unsafe { string_bytes(api, values[0]) },
        unsafe { string_bytes(api, values[1]) },
        unsafe { string_bytes(api, values[2]) },
    ) else {
        return api.nilObj;
    };
    let Some(plan) = portable_image_plan(&programs, &bindings, &selection) else {
        return api.nilObj;
    };
    if plan.c_source.len() > MAX_MATERIALIZATION_C_SOURCE_BYTES {
        return api.nilObj;
    }
    let unit = unsafe { send0(api, runtime().tcc as Oop, NEW_SELECTOR) };
    if unit.is_null() || unit == api.nilObj {
        return api.nilObj;
    }
    let source = unsafe { (api.mkString)(plan.c_source.as_ptr(), plan.c_source.len()) };
    if unsafe { send(api, unit, COMPILE_SELECTOR, &[source]) } == api.nilObj
        || !unsafe { bind_global_shims(api, unit) }
        || unsafe { send0(api, unit, BIND_RUNTIME_SELECTOR) } == api.nilObj
        || unsafe { send0(api, unit, RELOCATE_SELECTOR) } == api.nilObj
    {
        unsafe { dispose_unit(api, unit) };
        return api.nilObj;
    }
    let generated_bytes = plan.c_source.len();
    let owner = unsafe {
        begin_portable_image_stage(
            api,
            plan,
            PortableImageCarrierKind::Rebuilt,
            unit,
            0,
            unit,
            generated_bytes,
        )
    };
    if owner.is_none() {
        unsafe { dispose_unit(api, unit) };
    }
    owner.unwrap_or(api.nilObj)
}

unsafe extern "C" fn rust_codegen_begin_portable_image_mapped(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustCodegen portable image mapping received invalid arguments"));
    if values.len() != 4 {
        abort("RustCodegen portable image mapping expects Code, programs, bindings, and selection");
    }
    let api = unsafe { api(runtime()) };
    let (Some(code), Some(programs), Some(bindings), Some(selection)) = (
        unsafe { string_bytes(api, values[0]) },
        unsafe { string_bytes(api, values[1]) },
        unsafe { string_bytes(api, values[2]) },
        unsafe { string_bytes(api, values[3]) },
    ) else {
        return api.nilObj;
    };
    let Some(plan) = portable_image_plan(&programs, &bindings, &selection) else {
        return api.nilObj;
    };
    if !candidate_macho_safe(&code) {
        note_error(b"RustCodegen portable image Code violates the native mapping policy");
        return api.nilObj;
    }
    let sequence = IMAGE_CODE_LOAD_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "birnam-portable-map-{}-{sequence}.dylib",
        std::process::id()
    ));
    let wrote = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .and_then(|mut file| file.write_all(&code));
    if wrote.is_err() {
        return api.nilObj;
    }
    let Ok(path_c) = CString::new(path.as_os_str().as_bytes()) else {
        let _ = fs::remove_file(&path);
        return api.nilObj;
    };
    #[cfg(target_os = "macos")]
    const RTLD_NOW_LOCAL: c_int = 2 | 4;
    #[cfg(not(target_os = "macos"))]
    const RTLD_NOW_LOCAL: c_int = 2;
    let handle = unsafe { (api.dlopen)(path_c.as_ptr(), RTLD_NOW_LOCAL) as usize };
    let _ = fs::remove_file(&path);
    if handle == 0
        || !unsafe {
            mapped_export_matches(
                api,
                handle,
                b"__birnam_portable_program_inventory_v1",
                &programs,
            ) && mapped_export_matches(
                api,
                handle,
                b"__birnam_code_binding_inventory_v1",
                &bindings,
            ) && mapped_export_matches(
                api,
                handle,
                b"__birnam_code_materialization_selection_v1",
                &selection,
            ) && mapped_export_matches(
                api,
                handle,
                b"__birnam_portable_resumable_manifest_v1",
                &plan.manifest,
            )
        }
    {
        if handle != 0 {
            let _ = unsafe { dlclose(handle as *mut c_void) };
        }
        return api.nilObj;
    }
    let generation = handle as u64;
    let Some(owner) = (unsafe { make_portable_mapped_owner(api, handle, generation) }) else {
        let _ = unsafe { dlclose(handle as *mut c_void) };
        return api.nilObj;
    };
    let result = unsafe {
        begin_portable_image_stage(
            api,
            plan,
            PortableImageCarrierKind::Mapped,
            ptr::null_mut(),
            handle,
            owner,
            code.len(),
        )
    };
    if result.is_none() {
        unsafe {
            release_uncommitted_portable_owner(api, PortableImageCarrierKind::Mapped, owner, handle)
        };
    }
    result.unwrap_or(api.nilObj)
}

unsafe extern "C" fn rust_codegen_portable_image_block_vtable(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustCodegen portable image vtable received invalid arguments"));
    if values.len() != 1 {
        abort("RustCodegen portable image vtable expects one program index");
    }
    let api = unsafe { api(runtime()) };
    if unsafe { (api.isSmallInt)(values[0]) } == 0 {
        return api.nilObj;
    }
    let raw = unsafe { (api.smallIntValue)(values[0]) };
    let Ok(program_index) = u32::try_from(raw) else {
        return api.nilObj;
    };
    let Ok(stage) = PORTABLE_IMAGE_STAGE.lock() else {
        return api.nilObj;
    };
    let Some(stage) = stage.as_ref() else {
        return api.nilObj;
    };
    if stage.committed || program_index >= stage.program_count {
        return api.nilObj;
    }
    let Some(manifest_index) = stage.binding_count.checked_add(program_index) else {
        return api.nilObj;
    };
    let Some(address) =
        (unsafe { floor_symbol(api, b"birnam_id_staged_portable_block_vtable_v1\0") })
    else {
        return api.nilObj;
    };
    let resolve: StagedBlockVtable = unsafe { core::mem::transmute(address) };
    let vtable = unsafe {
        resolve(
            stage.authority.as_ptr(),
            stage.authority.len(),
            manifest_index,
        )
    };
    if vtable.is_null() { api.nilObj } else { vtable }
}

unsafe extern "C" fn rust_codegen_prepare_portable_image_binding(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }.unwrap_or_else(|| {
        abort("RustCodegen portable image binding prepare received invalid arguments")
    });
    if values.len() != 2 {
        abort("RustCodegen portable image binding prepare expects index and implementation");
    }
    let api = unsafe { api(runtime()) };
    if unsafe { (api.isSmallInt)(values[0]) } == 0 {
        return api.nilObj;
    }
    let Ok(index) = u32::try_from(unsafe { (api.smallIntValue)(values[0]) }) else {
        return api.nilObj;
    };
    let Ok(mut stage) = PORTABLE_IMAGE_STAGE.lock() else {
        return api.nilObj;
    };
    let Some(stage) = stage.as_mut() else {
        return api.nilObj;
    };
    if stage.committed || index != stage.prepared_bindings || index >= stage.binding_count {
        return api.nilObj;
    }
    let prepare: PortablePrepareBinding = unsafe { core::mem::transmute(stage.prepare_binding) };
    if unsafe { prepare(index, values[1]) } != 0 {
        return api.nilObj;
    }
    stage.prepared_bindings += 1;
    api.trueObj
}

unsafe extern "C" fn rust_codegen_finish_portable_image_unit(
    _self_object: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    if count != 0 {
        abort("RustCodegen finishPortableImageUnit expects no arguments");
    }
    let api = unsafe { api(runtime()) };
    let Some(mut stage) = PORTABLE_IMAGE_STAGE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take()
    else {
        return api.nilObj;
    };
    if stage.committed || stage.prepared_bindings != stage.binding_count {
        *PORTABLE_IMAGE_STAGE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(stage);
        return api.nilObj;
    }
    let end: PortableEndContext = unsafe { core::mem::transmute(stage.end_context) };
    let finish: PortableClose = unsafe { core::mem::transmute(stage.finish) };
    let cancel: PortableClose = unsafe { core::mem::transmute(stage.cancel) };
    let context_ok = unsafe { end() } == 0;
    let committed = context_ok && unsafe { finish() } == 0;
    if !committed {
        if !context_ok && unsafe { cancel() } != 0 {
            abort("portable image staged-unit compensation failed");
        }
        unsafe {
            release_uncommitted_portable_owner(
                api,
                stage.carrier_kind,
                stage.owner as Oop,
                stage.handle,
            )
        };
        return api.nilObj;
    }
    let owner = stage.owner as Oop;
    match stage.carrier_kind {
        PortableImageCarrierKind::Rebuilt => {
            if unsafe { send0(api, owner, DISPOSABLE_SELECTOR) } == api.nilObj {
                abort("committed portable rebuilt unit could not become disposable");
            }
        }
        PortableImageCarrierKind::Mapped => unsafe {
            (api.setFinalizer)(owner, portable_mapped_unit_finalize)
        },
    }
    stage.committed = true;
    if stage.binding_count != 0 {
        *PORTABLE_IMAGE_STAGE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(stage);
    }
    api.trueObj
}

unsafe extern "C" fn rust_codegen_cancel_portable_image_unit(
    _self_object: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    if count != 0 {
        abort("RustCodegen cancelPortableImageUnit expects no arguments");
    }
    let api = unsafe { api(runtime()) };
    let stage = {
        let Ok(mut slot) = PORTABLE_IMAGE_STAGE.lock() else {
            return api.nilObj;
        };
        if slot.as_ref().is_some_and(|stage| stage.committed) {
            return api.nilObj;
        }
        slot.take()
    };
    let Some(stage) = stage else {
        return api.nilObj;
    };
    let end: PortableEndContext = unsafe { core::mem::transmute(stage.end_context) };
    let cancel: PortableClose = unsafe { core::mem::transmute(stage.cancel) };
    let _ = unsafe { end() };
    if unsafe { cancel() } != 0 {
        abort("portable image staged-unit compensation failed");
    }
    unsafe {
        release_uncommitted_portable_owner(
            api,
            stage.carrier_kind,
            stage.owner as Oop,
            stage.handle,
        )
    };
    api.trueObj
}

unsafe extern "C" fn rust_codegen_install_portable_image_binding(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }.unwrap_or_else(|| {
        abort("RustCodegen portable image binding install received invalid arguments")
    });
    if values.len() != 4 {
        abort(
            "RustCodegen portable image binding install expects index, target, implementation, and provenance",
        );
    }
    let api = unsafe { api(runtime()) };
    if unsafe { (api.isSmallInt)(values[0]) } == 0 {
        return api.nilObj;
    }
    let Ok(index) = u32::try_from(unsafe { (api.smallIntValue)(values[0]) }) else {
        return api.nilObj;
    };
    let (selector_bytes, method, owner, generation, authority, generated_bytes) = {
        let Ok(stage) = PORTABLE_IMAGE_STAGE.lock() else {
            return api.nilObj;
        };
        let Some(stage) = stage.as_ref() else {
            return api.nilObj;
        };
        if !stage.committed || index != stage.installed_bindings || index >= stage.binding_count {
            return api.nilObj;
        }
        let Ok(bindings) = code_binding_inventory::inspect(&stage.bindings) else {
            return api.nilObj;
        };
        let Some(binding) = bindings.binding_spec(index as usize) else {
            return api.nilObj;
        };
        let entry: PortableBindingEntryAddress =
            unsafe { core::mem::transmute(stage.binding_entry_address) };
        let Ok(method) = usize::try_from(unsafe { entry(index) }) else {
            return api.nilObj;
        };
        if method == 0 {
            return api.nilObj;
        }
        (
            binding.selector.to_vec(),
            method,
            stage.owner,
            stage.generation,
            stage.authority,
            stage.generated_bytes,
        )
    };
    let target = values[1];
    let implementation = values[2];
    let provenance = values[3];
    let selector = unsafe { intern_bytes(api, &selector_bytes) };
    let vtable = unsafe { (api.ensurePrivateVtable)(target) };
    if vtable.is_null() {
        return api.nilObj;
    }
    {
        let Ok(mut bindings) = CODE_BINDINGS.lock() else {
            return api.nilObj;
        };
        let current_count = bindings
            .iter()
            .filter(|binding| binding.lifecycle == CodeBindingLifecycle::Current)
            .count();
        let replacing = bindings.iter().any(|binding| {
            binding.lifecycle == CodeBindingLifecycle::Current
                && binding.target == target as usize
                && binding.selector == selector as usize
        });
        // Image recovery runs on the Runtime owner thread, so no competing
        // publisher can consume this reservation between the two lock scopes.
        // Keep every fallible registry operation ahead of Store publication.
        if (current_count >= MAX_CURRENT_CODE_BINDINGS && !replacing)
            || bindings.try_reserve(1).is_err()
        {
            return api.nilObj;
        }
    }
    let stored = unsafe {
        send(
            api,
            runtime().store as Oop,
            b"addMethod:on:with:\0",
            &[selector, vtable, method as Oop],
        )
    };
    if stored == api.nilObj {
        return api.nilObj;
    }
    let predecessors = {
        let Ok(mut bindings) = CODE_BINDINGS.lock() else {
            abort("portable image binding registry became unavailable after Store publication");
        };
        let mut predecessors = Vec::new();
        for binding in bindings.iter_mut() {
            if binding.lifecycle == CodeBindingLifecycle::Current
                && binding.target == target as usize
                && binding.selector == selector as usize
            {
                binding.lifecycle = CodeBindingLifecycle::Retiring;
                predecessors.push(binding.generation);
            }
        }
        bindings.push(CodeBindingEntry {
            target: target as usize,
            vtable: vtable as usize,
            selector: selector as usize,
            implementation: implementation as usize,
            provenance: provenance as usize,
            unit: owner,
            generation,
            method,
            authority,
            generated_bytes,
            lifecycle: CodeBindingLifecycle::Current,
        });
        predecessors
    };
    unsafe { request_code_binding_retirements(api, &predecessors) };
    let mut slot = PORTABLE_IMAGE_STAGE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let stage = slot
        .as_mut()
        .unwrap_or_else(|| abort("portable image binding stage disappeared"));
    stage.installed_bindings += 1;
    if stage.installed_bindings == stage.binding_count {
        *slot = None;
    }
    vtable
}

fn portable_named_plan(selector: &[u8], _provenance: Option<&[u8]>) -> Option<PortableNamedPlan> {
    let programs = portable_inventory::encode_closure(&[]).ok()?;
    // HeapRef ordinals are local structural names for the two live Oops. The
    // CodeBinding registry compares those Oops directly; image-v8 later emits
    // their actual canonical Field locations. Provenance is nonsemantic.
    let bindings = code_binding_inventory::encode(&[code_binding_inventory::BindingSpec {
        target: code_binding_inventory::Field::HeapRef(0),
        selector,
        implementation: code_binding_inventory::Field::HeapRef(1),
        // Provenance is excluded from executable authority. The live registry
        // retains the Oop, while image-v8 later assigns its actual HeapRef.
        provenance: code_binding_inventory::Field::Nil,
    }])
    .ok()?;
    let selection = materialization_selection::encode(
        &programs,
        &bindings,
        &[materialization_selection::Disposition::Resumable],
        &[],
    )
    .ok()?;
    let selection_view =
        materialization_selection::inspect(&selection, &programs, &bindings).ok()?;
    let compiled = compile_portable_materialization_named_v1(selection_view.row(0)?, 0, 0).ok()?;
    let authority = materialization_selection::authority(&programs, &bindings, &selection).ok()?;
    let manifest = portable_resumable_manifest::materialize(
        authority,
        &selection_view,
        &[portable_resumable_manifest::EntrySpec {
            selection_row_index: 0,
            driver_argument_count: 0,
            lowering_version: resumable_descriptor::LOWERING_VERSION_V1,
            descriptor: &compiled.descriptor,
        }],
    )
    .ok()?;
    let mut translation_unit = TranslationUnit::new(false);
    translation_unit.push_function(resumable_registration_c::registration_prelude_v2());
    translation_unit.push_function(&compiled.c_source);
    translation_unit.push_function(
        b"unsigned int __birnam_prepare_portable_named_v1(void* implementation){return __birnam_resumable_prepare_0(implementation);}\n",
    );
    translation_unit.push_function(
        b"unsigned long long __birnam_portable_named_entry_address_v1(void){return (unsigned long long)(unsigned long)&__birnam_resumable_thunk_0;}\n",
    );
    translation_unit.push_body(b"return birnam_nil();\n");
    Some(PortableNamedPlan {
        programs,
        bindings,
        selection,
        manifest,
        authority,
        c_source: translation_unit.finish(),
    })
}

fn portable_resumable_plan(program: &[u8]) -> Option<PortableResumablePlan> {
    let programs = portable_inventory::encode_closure(&[program]).ok()?;
    let inventory = portable_inventory::inspect(&programs).ok()?;
    let root_program_index = (0..inventory.program_count as usize)
        .find(|index| inventory.program(*index) == Some(program))?;
    let bindings = code_binding_inventory::encode(&[]).ok()?;
    // `materializeResumable:` is the explicit policy producer for this
    // source-less unit. Its complete transitive Block closure is resumable;
    // canonical Node occurrences select the corresponding inventory row.
    let block_dispositions =
        vec![materialization_selection::Disposition::Resumable; inventory.program_count as usize];
    let selection =
        materialization_selection::encode(&programs, &bindings, &[], &block_dispositions).ok()?;
    let selection_view =
        materialization_selection::inspect(&selection, &programs, &bindings).ok()?;
    let mut compiled = Vec::with_capacity(inventory.program_count as usize);
    for program_index in 0..inventory.program_count as usize {
        let program_bytes = inventory.program(program_index)?;
        let inspected = birnam_artifact::portable_program::inspect(program_bytes).ok()?;
        let mut literals = Vec::new();
        for node_index in 0..inspected.node_count as usize {
            let node = inspected.node(node_index)?;
            if node.semantic_kind()
                != Some(birnam_artifact::portable_program::NodeKind::BlockLiteral)
            {
                continue;
            }
            let nested_index = (0..inventory.program_count as usize)
                .find(|index| inventory.program(*index) == Some(node.data))?;
            literals.push(PortableBlockLiteralSpec {
                node_index: u32::try_from(node_index).ok()?,
                disposition: BlockLiteralDisposition::Resumable {
                    manifest_entry_index: u32::try_from(nested_index).ok()?,
                },
                portable_program: node.data.to_vec(),
                retain_materialization_owner: true,
            });
        }
        let index = u32::try_from(program_index).ok()?;
        compiled.push(
            compile_portable_materialization_block_v1(
                program_bytes,
                selection_view.row(program_index)?,
                index,
                index,
                &literals,
            )
            .ok()?,
        );
    }
    let authority = materialization_selection::authority(&programs, &bindings, &selection).ok()?;
    let manifest_entries = compiled
        .iter()
        .enumerate()
        .map(|(index, entry)| portable_resumable_manifest::EntrySpec {
            selection_row_index: u32::try_from(index).expect("bounded inventory index"),
            driver_argument_count: entry.driver_argument_count,
            lowering_version: resumable_descriptor::LOWERING_VERSION_V1,
            descriptor: &entry.descriptor,
        })
        .collect::<Vec<_>>();
    let manifest =
        portable_resumable_manifest::materialize(authority, &selection_view, &manifest_entries)
            .ok()?;
    let mut translation_unit = TranslationUnit::new(false);
    translation_unit.push_function(resumable_registration_c::registration_prelude_v2());
    translation_unit.push_function(b"void* __birnam_materialization_owner;\n");
    translation_unit.push_function(
        b"void* __birnam_resolve_block_vtable_v1(unsigned int manifest_entry_index);\n",
    );
    for entry in &compiled {
        translation_unit.push_function(&entry.c_source);
    }
    let mut prepare_all = b"unsigned int __birnam_prepare_portable_resumables_v1(void){\n".to_vec();
    for index in 0..inventory.program_count {
        prepare_all.extend_from_slice(b"if(__birnam_resumable_prepare_");
        prepare_all.extend_from_slice(index.to_string().as_bytes());
        prepare_all.extend_from_slice(b"((void*)0)!=0U)return 1U;\n");
    }
    prepare_all.extend_from_slice(b"return 0U;\n}\n");
    translation_unit.push_function(&prepare_all);
    let resolver_specs = compiled
        .iter()
        .map(|entry| entry.resolver_spec())
        .collect::<Vec<_>>();
    translation_unit
        .push_function(&resumable_resolver_c::emit_block_vtable_resolver_v1(&resolver_specs).ok()?);
    translation_unit.push_body(b"return birnam_nil();\n");
    Some(PortableResumablePlan {
        programs,
        bindings,
        selection,
        manifest,
        authority,
        c_source: translation_unit.finish(),
        root_manifest_entry_index: u32::try_from(root_program_index).ok()?,
    })
}

unsafe fn unit_symbol_address(api: &IdApi, unit: Oop, name: &[u8]) -> Option<usize> {
    let name = unsafe { (api.mkString)(name.as_ptr(), name.len()) };
    let symbol = unsafe { send(api, unit, SYMBOL_SELECTOR, &[name]) };
    unsafe { foreign_address_value(api, symbol) }.filter(|address| *address != 0)
}

#[allow(clippy::too_many_arguments)]
fn stage_code_binding(
    target: Oop,
    vtable: Oop,
    selector: Oop,
    implementation: Oop,
    provenance: Oop,
    unit: Oop,
    generation: u64,
    authority: [u8; 32],
    generated_bytes: usize,
) -> bool {
    let Ok(mut bindings) = CODE_BINDINGS.lock() else {
        return false;
    };
    let current_count = bindings
        .iter()
        .filter(|binding| binding.lifecycle == CodeBindingLifecycle::Current)
        .count();
    let replacing = bindings.iter().any(|binding| {
        binding.lifecycle == CodeBindingLifecycle::Current
            && binding.target == target as usize
            && binding.selector == selector as usize
    });
    if (current_count >= MAX_CURRENT_CODE_BINDINGS && !replacing)
        || bindings.iter().any(|binding| {
            binding.lifecycle == CodeBindingLifecycle::Staging
                && binding.target == target as usize
                && binding.selector == selector as usize
        })
    {
        return false;
    }
    bindings.push(CodeBindingEntry {
        target: target as usize,
        vtable: vtable as usize,
        selector: selector as usize,
        implementation: implementation as usize,
        provenance: provenance as usize,
        unit: unit as usize,
        generation,
        method: 0,
        authority,
        generated_bytes,
        lifecycle: CodeBindingLifecycle::Staging,
    });
    true
}

fn set_staged_code_binding_method(unit: Oop, method: usize) -> bool {
    let Ok(mut bindings) = CODE_BINDINGS.lock() else {
        return false;
    };
    let Some(binding) = bindings.iter_mut().find(|binding| {
        binding.unit == unit as usize && binding.lifecycle == CodeBindingLifecycle::Staging
    }) else {
        return false;
    };
    if method == 0 {
        return false;
    }
    binding.method = method;
    true
}

fn remove_staged_code_binding(unit: Oop) {
    if let Ok(mut bindings) = CODE_BINDINGS.lock() {
        bindings.retain(|binding| {
            binding.unit != unit as usize || binding.lifecycle != CodeBindingLifecycle::Staging
        });
    }
}

fn retire_staged_code_binding(unit: Oop) -> Option<u64> {
    let mut bindings = CODE_BINDINGS.lock().ok()?;
    let binding = bindings.iter_mut().find(|binding| {
        binding.unit == unit as usize && binding.lifecycle == CodeBindingLifecycle::Staging
    })?;
    binding.lifecycle = CodeBindingLifecycle::Retiring;
    Some(binding.generation)
}

fn publish_staged_code_binding(unit: Oop) -> Option<Vec<u64>> {
    let mut bindings = CODE_BINDINGS.lock().ok()?;
    let staged_index = bindings.iter().position(|binding| {
        binding.unit == unit as usize
            && binding.lifecycle == CodeBindingLifecycle::Staging
            && binding.method != 0
    })?;
    let target = bindings[staged_index].target;
    let selector = bindings[staged_index].selector;
    let mut predecessors = Vec::new();
    for (index, binding) in bindings.iter_mut().enumerate() {
        if index != staged_index
            && binding.lifecycle == CodeBindingLifecycle::Current
            && binding.target == target
            && binding.selector == selector
        {
            binding.lifecycle = CodeBindingLifecycle::Retiring;
            predecessors.push(binding.generation);
        }
    }
    bindings[staged_index].lifecycle = CodeBindingLifecycle::Current;
    Some(predecessors)
}

fn withdraw_current_code_binding(target: Oop, selector: Oop) -> Vec<u64> {
    let Ok(mut bindings) = CODE_BINDINGS.lock() else {
        return Vec::new();
    };
    let mut generations = Vec::new();
    for binding in bindings.iter_mut() {
        if binding.lifecycle == CodeBindingLifecycle::Current
            && binding.target == target as usize
            && binding.selector == selector as usize
        {
            binding.lifecycle = CodeBindingLifecycle::Retiring;
            generations.push(binding.generation);
        }
    }
    generations
}

fn update_current_code_binding_provenance(target: Oop, selector: Oop, provenance: Oop) -> bool {
    let Ok(mut bindings) = CODE_BINDINGS.lock() else {
        return false;
    };
    let Some(binding) = bindings.iter_mut().find(|binding| {
        binding.lifecycle == CodeBindingLifecycle::Current
            && binding.target == target as usize
            && binding.selector == selector as usize
    }) else {
        return false;
    };
    binding.provenance = provenance as usize;
    true
}

fn current_code_binding_implementation(vtable: Oop, selector: Oop) -> Option<Oop> {
    CODE_BINDINGS.lock().ok().and_then(|bindings| {
        bindings
            .iter()
            .find(|binding| {
                binding.lifecycle == CodeBindingLifecycle::Current
                    && binding.vtable == vtable as usize
                    && binding.selector == selector as usize
            })
            .map(|binding| binding.implementation as Oop)
    })
}

unsafe fn request_code_binding_retirements(api: &IdApi, generations: &[u64]) {
    for generation in generations.iter().copied() {
        let _ = unsafe { retire_runtime_generation_v1(api, generation) };
    }
}

unsafe fn reap_code_bindings(api: &IdApi) -> usize {
    let generations = CODE_BINDINGS
        .lock()
        .ok()
        .map(|bindings| {
            bindings
                .iter()
                .filter(|binding| binding.lifecycle == CodeBindingLifecycle::Retiring)
                .map(|binding| binding.generation)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut retired = Vec::new();
    for generation in generations {
        if unsafe { retire_runtime_generation_v1(api, generation) } == RuntimeRetirement::Retired {
            retired.push(generation);
        }
    }
    if retired.is_empty() {
        return 0;
    }
    let removed = CODE_BINDINGS
        .lock()
        .ok()
        .map(|mut bindings| {
            let before = bindings.len();
            bindings.retain(|binding| {
                binding.lifecycle != CodeBindingLifecycle::Retiring
                    || !retired.contains(&binding.generation)
            });
            before - bindings.len()
        })
        .unwrap_or(0);
    // Removed rows release their disposable Unit Oops. Collection runs only
    // after Runtime has forgotten every native address in those generations.
    let _ = unsafe { (api.gcCollect)() };
    removed
}

#[allow(clippy::too_many_arguments)]
unsafe fn compile_portable_named_materialization(
    api: &IdApi,
    target: Oop,
    vtable: Oop,
    selector: Oop,
    selector_bytes: &[u8],
    implementation: Oop,
    provenance: Oop,
) -> Option<(Oop, Oop)> {
    type BeginPortable = unsafe extern "C" fn(
        *const u8,
        usize,
        *const u8,
        usize,
        *const u8,
        usize,
        *const u8,
        usize,
        *const u8,
        u64,
        u64,
    ) -> u32;
    type CloseStage = unsafe extern "C" fn() -> u32;
    type BeginContext = unsafe extern "C" fn(u64, u64) -> u32;
    type EndContext = unsafe extern "C" fn() -> u32;
    type Prepare = unsafe extern "C" fn(Oop) -> u32;
    type EntryAddress = unsafe extern "C" fn() -> u64;

    let provenance_bytes = unsafe { string_bytes(api, provenance) };
    let plan = portable_named_plan(selector_bytes, provenance_bytes.as_deref())?;
    if plan.c_source.len() > MAX_MATERIALIZATION_C_SOURCE_BYTES {
        return None;
    }
    let begin_address =
        unsafe { floor_symbol(api, b"birnam_id_begin_portable_resumable_unit_v1\0") }?;
    let finish_address = unsafe { floor_symbol(api, b"birnam_id_finish_resumable_unit_v1\0") }?;
    let cancel_address = unsafe { floor_symbol(api, b"birnam_id_cancel_resumable_unit_v1\0") }?;
    let unit = unsafe { send0(api, runtime().tcc as Oop, NEW_SELECTOR) };
    if unit.is_null() || unit == api.nilObj {
        return None;
    }
    let generation = u64::try_from(unit as usize).ok()?;
    if generation == 0 || generation > i64::MAX as u64 {
        unsafe { dispose_unit(api, unit) };
        return None;
    }
    if !stage_code_binding(
        target,
        vtable,
        selector,
        implementation,
        provenance,
        unit,
        generation,
        plan.authority,
        plan.c_source.len(),
    ) {
        unsafe { dispose_unit(api, unit) };
        return None;
    }
    let source = unsafe { (api.mkString)(plan.c_source.as_ptr(), plan.c_source.len()) };
    if unsafe { send(api, unit, COMPILE_SELECTOR, &[source]) } == api.nilObj
        || !unsafe { bind_global_shims(api, unit) }
        || unsafe { send0(api, unit, BIND_RUNTIME_SELECTOR) } == api.nilObj
        || unsafe { send0(api, unit, RELOCATE_SELECTOR) } == api.nilObj
        || !unsafe { bind_jit_code_generation_identity(api, unit) }
    {
        remove_staged_code_binding(unit);
        unsafe { dispose_unit(api, unit) };
        return None;
    }
    let identity = unsafe { (api.mkSmallInt)(generation as i64) };
    if unsafe { send(api, unit, OWN_CODE_GENERATION_SELECTOR, &[identity]) } == api.nilObj {
        remove_staged_code_binding(unit);
        unsafe { dispose_unit(api, unit) };
        return None;
    }
    let Some(lease) = issue_code_generation_lease() else {
        remove_staged_code_binding(unit);
        unsafe { dispose_unit(api, unit) };
        return None;
    };
    let begin: BeginPortable = unsafe { core::mem::transmute(begin_address) };
    let finish: CloseStage = unsafe { core::mem::transmute(finish_address) };
    let cancel: CloseStage = unsafe { core::mem::transmute(cancel_address) };
    if unsafe {
        begin(
            plan.programs.as_ptr(),
            plan.programs.len(),
            plan.bindings.as_ptr(),
            plan.bindings.len(),
            plan.selection.as_ptr(),
            plan.selection.len(),
            plan.manifest.as_ptr(),
            plan.manifest.len(),
            plan.authority.as_ptr(),
            generation,
            lease,
        )
    } != 0
    {
        remove_staged_code_binding(unit);
        unsafe { dispose_unit(api, unit) };
        return None;
    }
    let staged = (|| unsafe {
        let begin_context_address =
            unit_symbol_address(api, unit, b"__birnam_resumable_context_begin_v1");
        let end_context_address =
            unit_symbol_address(api, unit, b"__birnam_resumable_context_end_v1");
        let prepare_address = unit_symbol_address(api, unit, b"__birnam_prepare_portable_named_v1");
        let entry_address =
            unit_symbol_address(api, unit, b"__birnam_portable_named_entry_address_v1");
        let begin_context: BeginContext = core::mem::transmute(begin_context_address?);
        let end_context: EndContext = core::mem::transmute(end_context_address?);
        let prepare: Prepare = core::mem::transmute(prepare_address?);
        let entry_address: EntryAddress = core::mem::transmute(entry_address?);
        let method = usize::try_from(entry_address())
            .ok()
            .filter(|address| *address != 0)?;
        if begin_context(generation, lease) != 0 {
            return None;
        }
        if prepare(implementation) != 0 {
            let _ = end_context();
            return None;
        }
        let end_status = end_context();
        if end_status != 0 || !set_staged_code_binding_method(unit, method) {
            return None;
        }
        Some(method as Oop)
    })();
    let Some(method) = staged else {
        if unsafe { cancel() } != 0 {
            abort("portable named staged-unit compensation failed");
        }
        remove_staged_code_binding(unit);
        unsafe { dispose_unit(api, unit) };
        return None;
    };
    if unsafe { finish() } != 0 || unsafe { send0(api, unit, DISPOSABLE_SELECTOR) } == api.nilObj {
        let generation = retire_staged_code_binding(unit).unwrap_or(generation);
        unsafe { request_code_binding_retirements(api, &[generation]) };
        return None;
    }
    Some((unit, method))
}

#[allow(clippy::too_many_arguments)]
unsafe fn compile_resumable_materialized_block(
    api: &IdApi,
    code_object: Oop,
    captures: &[Oop],
    program: &[u8],
    digest: [u8; 32],
) -> Option<Oop> {
    type BeginPortable = unsafe extern "C" fn(
        *const u8,
        usize,
        *const u8,
        usize,
        *const u8,
        usize,
        *const u8,
        usize,
        *const u8,
        u64,
        u64,
    ) -> u32;
    type CloseStage = unsafe extern "C" fn() -> u32;
    type BeginContext = unsafe extern "C" fn(u64, u64) -> u32;
    type EndContext = unsafe extern "C" fn() -> u32;
    type PrepareAll = unsafe extern "C" fn() -> u32;
    type ResolveBlockVtable = unsafe extern "C" fn(u32) -> Oop;

    let begin_address =
        unsafe { floor_symbol(api, b"birnam_id_begin_portable_resumable_unit_v1\0") }?;
    let finish_address = unsafe { floor_symbol(api, b"birnam_id_finish_resumable_unit_v1\0") }?;
    let cancel_address = unsafe { floor_symbol(api, b"birnam_id_cancel_resumable_unit_v1\0") }?;
    let plan = portable_resumable_plan(program)?;
    if plan.c_source.len() > MAX_MATERIALIZATION_C_SOURCE_BYTES {
        return None;
    }
    let tcc = runtime().tcc as Oop;
    if tcc.is_null() {
        return None;
    }
    let unit = unsafe { send0(api, tcc, NEW_SELECTOR) };
    if unit.is_null() || unit == api.nilObj {
        return None;
    }
    let source = unsafe { (api.mkString)(plan.c_source.as_ptr(), plan.c_source.len()) };
    if unsafe { send(api, unit, COMPILE_SELECTOR, &[source]) } == api.nilObj
        || !unsafe { bind_global_shims(api, unit) }
        || unsafe { send0(api, unit, BIND_RUNTIME_SELECTOR) } == api.nilObj
        || unsafe { send0(api, unit, RELOCATE_SELECTOR) } == api.nilObj
        || !unsafe { bind_jit_code_generation_identity(api, unit) }
        || !stage_materialization(
            MaterializationDisposition::Resumable,
            digest,
            program.to_vec(),
            unit,
            plan.c_source.len(),
        )
    {
        unsafe { dispose_unit(api, unit) };
        return None;
    }
    let Ok(code_generation_identity) = u64::try_from(unit as usize) else {
        remove_materialization(unit);
        unsafe { dispose_unit(api, unit) };
        return None;
    };
    let Some(code_generation_lease) = issue_code_generation_lease() else {
        remove_materialization(unit);
        unsafe { dispose_unit(api, unit) };
        return None;
    };
    if code_generation_identity > i64::MAX as u64 {
        remove_materialization(unit);
        unsafe { dispose_unit(api, unit) };
        return None;
    }
    let identity = unsafe { (api.mkSmallInt)(code_generation_identity as i64) };
    if unsafe { send(api, unit, OWN_CODE_GENERATION_SELECTOR, &[identity]) } == api.nilObj {
        remove_materialization(unit);
        unsafe { dispose_unit(api, unit) };
        return None;
    }

    let begin: BeginPortable = unsafe { core::mem::transmute(begin_address) };
    let finish: CloseStage = unsafe { core::mem::transmute(finish_address) };
    let cancel: CloseStage = unsafe { core::mem::transmute(cancel_address) };
    if unsafe {
        begin(
            plan.programs.as_ptr(),
            plan.programs.len(),
            plan.bindings.as_ptr(),
            plan.bindings.len(),
            plan.selection.as_ptr(),
            plan.selection.len(),
            plan.manifest.as_ptr(),
            plan.manifest.len(),
            plan.authority.as_ptr(),
            code_generation_identity,
            code_generation_lease,
        )
    } != 0
    {
        remove_materialization(unit);
        unsafe { dispose_unit(api, unit) };
        return None;
    }

    let staged =
        (|| unsafe {
            let begin_context: BeginContext = core::mem::transmute(unit_symbol_address(
                api,
                unit,
                b"__birnam_resumable_context_begin_v1",
            )?);
            let end_context: EndContext = core::mem::transmute(unit_symbol_address(
                api,
                unit,
                b"__birnam_resumable_context_end_v1",
            )?);
            let resolve_block_vtable: ResolveBlockVtable = core::mem::transmute(
                unit_symbol_address(api, unit, b"__birnam_resolve_block_vtable_v1")?,
            );
            let owner_address = unit_symbol_address(api, unit, b"__birnam_materialization_owner")?;
            let prepare_all: PrepareAll = core::mem::transmute(unit_symbol_address(
                api,
                unit,
                b"__birnam_prepare_portable_resumables_v1",
            )?);
            // The generated word is a private physical carrier, never durable
            // authority. Every nested Block copies this Unit into its traced v2
            // carrier before it can escape the currently executing receiver.
            (owner_address as *mut Oop).write(unit);
            if begin_context(code_generation_identity, code_generation_lease) != 0 {
                return None;
            }
            if prepare_all() != 0 {
                let _ = end_context();
                return None;
            }
            if end_context() != 0 {
                return None;
            }
            let vtable = resolve_block_vtable(plan.root_manifest_entry_index);
            if vtable.is_null() || !set_staged_materialization_vtable(unit, vtable) {
                return None;
            }
            let capability = portable_code_api_v2(api)?;
            let block = (capability.new_materialized_block)(
                vtable,
                code_object,
                captures.as_ptr(),
                captures.len(),
                unit,
            );
            if block.is_null() || block == api.nilObj || birnam_rust_codegen_is_error(block) != 0 {
                return None;
            }
            Some(block)
        })();
    let Some(block) = staged else {
        if unsafe { cancel() } != 0 {
            abort("portable resumable staged-unit compensation failed");
        }
        remove_materialization(unit);
        unsafe { dispose_unit(api, unit) };
        return None;
    };
    if unsafe { finish() } != 0
        || unsafe { send0(api, unit, DISPOSABLE_SELECTOR) } == api.nilObj
        || !commit_staged_materialization(unit)
    {
        remove_materialization(unit);
        unsafe { dispose_unit(api, unit) };
        return None;
    }
    Some(block)
}

unsafe extern "C" fn rust_codegen_materialize_synchronous(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustCodegen materializeSynchronous: received invalid arguments"));
    if values.len() != 2 {
        abort("RustCodegen materializeSynchronous:captures: expects two arguments");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    clear_error();
    let code_object = values[0];
    let captures_object = values[1];
    let Some(program) = (unsafe { copy_code_object_program(api, code_object) }) else {
        note_error(b"RustCodegen materialization needs an admitted CodeObject");
        return api.nilObj;
    };
    let Some((_, capture_count)) = birnam_codegen::portable::program_signature(&program).ok()
    else {
        note_error(b"RustCodegen materialization rejected PortableProgram semantics");
        return api.nilObj;
    };
    let vector_vtable = unsafe { (api.import)(VECTOR_VT_NAME.as_ptr().cast()) };
    if unsafe { vector_length(api, captures_object, vector_vtable) }
        != usize::try_from(capture_count).ok()
    {
        note_error(b"RustCodegen materialization capture count disagrees with CodeObject");
        return api.nilObj;
    }
    let digest = sha256_raw(&program);
    let (vtable, unit, staged) =
        match cached_materialization(MaterializationDisposition::Synchronous, digest, &program) {
            Some((vtable, unit)) => (vtable, unit, false),
            None => {
                let Some((vtable, unit)) =
                    (unsafe { compile_synchronous_materialization(api, &program, digest) })
                else {
                    note_error(b"RustCodegen could not compile PortableProgram materialization");
                    return api.nilObj;
                };
                (vtable, unit, true)
            }
        };
    let captures = (0..capture_count as usize)
        .map(|index| unsafe { vector_item(api, captures_object, index) })
        .collect::<Vec<_>>();
    let Some(capability) = (unsafe { portable_code_api_v2(api) }) else {
        if staged {
            remove_materialization(unit);
        }
        note_error(b"RustCodegen PortableCodeApiV2 is unavailable");
        return api.nilObj;
    };
    let block = unsafe {
        (capability.new_materialized_block)(
            vtable,
            code_object,
            captures.as_ptr(),
            captures.len(),
            unit,
        )
    };
    if block.is_null() || block == api.nilObj || unsafe { birnam_rust_codegen_is_error(block) } != 0
    {
        if staged {
            remove_materialization(unit);
        }
        return block;
    }
    if staged && !commit_staged_materialization(unit) {
        remove_materialization(unit);
        note_error(b"RustCodegen could not publish PortableProgram materialization");
        return api.nilObj;
    }
    block
}

unsafe extern "C" fn rust_codegen_materialize_resumable(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustCodegen materializeResumable: received invalid arguments"));
    if values.len() != 2 {
        abort("RustCodegen materializeResumable:captures: expects two arguments");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    clear_error();
    let code_object = values[0];
    let captures_object = values[1];
    let Some(program) = (unsafe { copy_code_object_program(api, code_object) }) else {
        note_error(b"RustCodegen resumable materialization needs an admitted CodeObject");
        return api.nilObj;
    };
    let Some((_, capture_count)) = birnam_codegen::portable::program_signature(&program).ok()
    else {
        note_error(b"RustCodegen resumable materialization rejected PortableProgram semantics");
        return api.nilObj;
    };
    let vector_vtable = unsafe { (api.import)(VECTOR_VT_NAME.as_ptr().cast()) };
    if unsafe { vector_length(api, captures_object, vector_vtable) }
        != usize::try_from(capture_count).ok()
    {
        note_error(b"RustCodegen resumable capture count disagrees with CodeObject");
        return api.nilObj;
    }
    let captures = (0..capture_count as usize)
        .map(|index| unsafe { vector_item(api, captures_object, index) })
        .collect::<Vec<_>>();
    let digest = sha256_raw(&program);
    if let Some((vtable, unit)) =
        cached_materialization(MaterializationDisposition::Resumable, digest, &program)
    {
        let Some(capability) = (unsafe { portable_code_api_v2(api) }) else {
            note_error(b"RustCodegen PortableCodeApiV2 is unavailable");
            return api.nilObj;
        };
        let block = unsafe {
            (capability.new_materialized_block)(
                vtable,
                code_object,
                captures.as_ptr(),
                captures.len(),
                unit,
            )
        };
        if block.is_null()
            || block == api.nilObj
            || unsafe { birnam_rust_codegen_is_error(block) } != 0
        {
            return block;
        }
        return block;
    }
    let Some(block) = (unsafe {
        compile_resumable_materialized_block(api, code_object, &captures, &program, digest)
    }) else {
        note_error(b"RustCodegen could not compile portable resumable materialization");
        return api.nilObj;
    };
    block
}

unsafe extern "C" fn rust_codegen_materialization_count(
    _self_object: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    if count != 0 {
        abort("RustCodegen materializationCount expects no arguments");
    }
    let value = MATERIALIZATIONS
        .lock()
        .map(|entries| entries.iter().filter(|entry| entry.ready).count())
        .unwrap_or(0);
    let api = unsafe { api(runtime()) };
    unsafe { (api.mkSmallInt)(value as i64) }
}

unsafe extern "C" fn rust_codegen_current_code_binding_count(
    _self_object: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    if count != 0 {
        abort("RustCodegen currentCodeBindingCount expects no arguments");
    }
    let value = CODE_BINDINGS
        .lock()
        .map(|bindings| {
            bindings
                .iter()
                .filter(|binding| {
                    binding.lifecycle == CodeBindingLifecycle::Current
                        && binding.authority != [0; 32]
                })
                .count()
        })
        .unwrap_or(0);
    let api = unsafe { api(runtime()) };
    unsafe { (api.mkSmallInt)(value as i64) }
}

unsafe extern "C" fn rust_codegen_retiring_code_binding_count(
    _self_object: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    if count != 0 {
        abort("RustCodegen retiringCodeBindingCount expects no arguments");
    }
    let value = CODE_BINDINGS
        .lock()
        .map(|bindings| {
            bindings
                .iter()
                .filter(|binding| binding.lifecycle == CodeBindingLifecycle::Retiring)
                .count()
        })
        .unwrap_or(0);
    let api = unsafe { api(runtime()) };
    unsafe { (api.mkSmallInt)(value as i64) }
}

unsafe extern "C" fn rust_codegen_code_binding_generated_bytes(
    _self_object: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    if count != 0 {
        abort("RustCodegen codeBindingGeneratedBytes expects no arguments");
    }
    let value = CODE_BINDINGS
        .lock()
        .ok()
        .and_then(|bindings| {
            let mut generations = BTreeMap::<u64, usize>::new();
            for binding in bindings.iter() {
                generations
                    .entry(binding.generation)
                    .and_modify(|bytes| *bytes = (*bytes).max(binding.generated_bytes))
                    .or_insert(binding.generated_bytes);
            }
            generations
                .into_values()
                .try_fold(0usize, usize::checked_add)
        })
        .unwrap_or(usize::MAX);
    let api = unsafe { api(runtime()) };
    i64::try_from(value)
        .ok()
        .map(|value| unsafe { (api.mkSmallInt)(value) })
        .unwrap_or(api.nilObj)
}

unsafe extern "C" fn rust_codegen_reap_code_bindings(
    _self_object: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    if count != 0 {
        abort("RustCodegen reapCodeBindings expects no arguments");
    }
    let api = unsafe { api(runtime()) };
    let removed = unsafe { reap_code_bindings(api) };
    unsafe { (api.mkSmallInt)(removed as i64) }
}

/// Return a point-in-time semantic snapshot of the current CodeBindings.
///
/// This is a private K5 image-writer seam, not public compilation policy. Each
/// row is `[target, selector, implementation, provenance]`; publication
/// generations, Methods, units, and other process-local carrier state are
/// deliberately absent. Automatic collection is disabled while copied Oops are
/// between the rooted registry and the returned rooted Vector graph.
unsafe extern "C" fn rust_codegen_portable_image_bindings(
    _self_object: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    if count != 0 {
        abort("RustCodegen portableImageBindings expects no arguments");
    }
    let api = unsafe { api(runtime()) };
    unsafe { (api.gcSetAuto)(0) };
    struct AutoGc<'a>(&'a IdApi);
    impl Drop for AutoGc<'_> {
        fn drop(&mut self) {
            unsafe { (self.0.gcSetAuto)(1) };
        }
    }
    let _auto_gc = AutoGc(api);
    let rows = CODE_BINDINGS
        .lock()
        .ok()
        .map(|bindings| {
            bindings
                .iter()
                .filter(|binding| binding.lifecycle == CodeBindingLifecycle::Current)
                .map(|binding| {
                    [
                        binding.target as Oop,
                        binding.selector as Oop,
                        binding.implementation as Oop,
                        binding.provenance as Oop,
                    ]
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let vector = unsafe { (api.import)(VECTOR_NAME.as_ptr().cast()) };
    if vector.is_null() || vector == api.nilObj {
        return api.nilObj;
    }
    let mut encoded = Vec::with_capacity(rows.len());
    for row in rows {
        let row = unsafe { make_vector(api, vector, &row) };
        if row.is_null() || row == api.nilObj {
            return api.nilObj;
        }
        encoded.push(row);
    }
    unsafe { make_vector(api, vector, &encoded) }
}

unsafe extern "C" fn rust_codegen_evict_materializations(
    _self_object: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    if count != 0 {
        abort("RustCodegen evictMaterializations expects no arguments");
    }
    let (removed, generations) = MATERIALIZATIONS
        .lock()
        .map(|mut entries| {
            let before = entries.len();
            let mut generations = Vec::new();
            entries.retain(|entry| {
                if entry.ready {
                    generations.push((entry.disposition, entry.unit as u64));
                    false
                } else {
                    true
                }
            });
            (before - entries.len(), generations)
        })
        .unwrap_or_default();
    // The repository cache lease and the generated vtable/code-object cache
    // lease end together. Live Block headers still trace their private vtable,
    // whose carrier traces the Unit; an evicted-but-live unit can therefore
    // rebuild a missing nested cache row without becoming a hidden root.
    for (disposition, generation) in generations {
        let _ = take_vtable_cache_roots(generation);
        if disposition == MaterializationDisposition::Resumable {
            let api = unsafe { api(runtime()) };
            // A live Block makes this first attempt reject after preserving
            // the generation for a later GC safe-point retry. A quiescent unit
            // retires immediately. Either result leaves no repository root.
            let _ = unsafe { retire_runtime_generation_v1(api, generation) };
        }
    }
    let api = unsafe { api(runtime()) };
    unsafe { (api.mkSmallInt)(removed as i64) }
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
            None,
            None,
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
    let c_source = unsafe { (api.mkString)(source.bytes.as_ptr(), source.bytes.len()) };
    if unsafe { send(api, unit, COMPILE_SELECTOR, &[c_source]) } == api.nilObj
        || !unsafe { bind_global_shims(api, unit) }
        || !unsafe { bind_baked_global_shims(api, unit, &source.baked_globals) }
        || unsafe { send0(api, unit, BIND_RUNTIME_SELECTOR) } == api.nilObj
        || unsafe { send0(api, unit, RELOCATE_SELECTOR) } == api.nilObj
        || !unsafe { bind_jit_code_generation_identity(api, unit) }
    {
        unsafe { dispose_unit(api, unit) };
        note_error(b"RustCodegen generated C failed to compile");
        return api.nilObj;
    }
    let entry_name = unsafe { (api.mkString)(b"__ev".as_ptr(), 4) };
    let symbol = unsafe { send(api, unit, SYMBOL_SELECTOR, &[entry_name]) };
    let Some(address) = (unsafe { foreign_address_value(api, symbol) }) else {
        unsafe { dispose_unit(api, unit) };
        note_error(b"RustCodegen could not resolve generated entry point");
        return api.nilObj;
    };
    // SAFETY: `symbol:` returns the generated Method-ABI entry point and the
    // unit remains relocated but uninstalled for this direct invocation.
    let entry: Method = unsafe { core::mem::transmute(address) };
    let result = unsafe { entry(api.nilObj, ptr::null(), 0) };
    // A persisting source can install blocks as methods or store them in
    // globals. Those closures point into this TCC unit, so keep the unit live
    // exactly as the native evaluator does for any program containing `__blk`.
    // One-shot programs with no blocks have no native code reachable after the
    // entry returns and may be reclaimed immediately.
    if source.bytes.windows(5).any(|window| window == b"__blk") {
        retain_unit(unit);
    } else {
        unsafe { dispose_unit(api, unit) };
    }
    let _ = self_object;
    result
}

unsafe extern "C" fn rust_codegen_compile_output(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    unsafe { rust_codegen_compile_output_impl(pointer, count, false, false, false, None) }
}

unsafe extern "C" fn rust_codegen_compile_output_root(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    unsafe { rust_codegen_compile_output_impl(pointer, count, true, false, false, None) }
}

unsafe extern "C" fn rust_codegen_compile_output_selection(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    unsafe { rust_codegen_compile_output_impl(pointer, count, false, false, true, None) }
}

/// Compile one eager Module AOT whose selected implementations belong to G0.
/// The complete initial source inventory must already be registered, allowing
/// the resumable manifest to bind to the same authority G0 will later publish.
unsafe extern "C" fn rust_codegen_compile_initial_module_selection(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }.unwrap_or_else(|| {
        abort("RustCodegen compileInitialModule:output:selection: received invalid arguments")
    });
    if values.len() != 3 {
        abort("RustCodegen compileInitialModule:output:selection: expects source/output/selection");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    clear_error();
    let parsed_requests = match unsafe { parse_selection_requests(api, values[2]) } {
        Ok(requests) => requests,
        Err(_) => {
            note_error(b"RustCodegen initial module selection request is invalid");
            return api.nilObj;
        }
    };
    let Some((source_bytes, metadata)) = (unsafe { parse_revision_metadata(api, values[0]) })
    else {
        note_error(b"RustCodegen initial module source is not indexable");
        return api.nilObj;
    };
    let Some(requests) = expand_selection_requests(&metadata, parsed_requests) else {
        note_error(b"RustCodegen initial module selection could not be expanded");
        return api.nilObj;
    };
    let Some(selection) = artifact_selection_with_requests(&metadata, &requests) else {
        note_error(b"RustCodegen initial module selection mismatches its inventory");
        return api.nilObj;
    };
    let Some(mut modules) = (unsafe { registered_generation_modules(api) }) else {
        note_error(b"RustCodegen initial generation source inventory is malformed");
        return api.nilObj;
    };
    let Some(revision) = decode_hex_digest(&metadata.revision_digest) else {
        note_error(b"RustCodegen initial module revision is invalid");
        return api.nilObj;
    };
    if let Some(existing) = modules.iter().find(|module| module.name == metadata.name) {
        if existing.revision != revision {
            note_error(b"RustCodegen initial module inventory revision mismatches");
            return api.nilObj;
        }
    } else {
        modules.push(GenerationModuleProjection {
            name: metadata.name.clone(),
            revision,
            source: source_bytes.clone(),
            metadata: metadata.clone(),
        });
        modules.sort_by(|left, right| left.name.cmp(&right.name));
    }
    let Some(generation_selection) = generation_selection_with_overrides(
        &modules,
        &[(metadata.name.clone(), selection.clone())],
    ) else {
        note_error(b"RustCodegen initial generation selection is malformed");
        return api.nilObj;
    };
    let Some(authority) = dispatch_generation_authority(&modules, &generation_selection) else {
        note_error(b"RustCodegen initial generation authority is unavailable");
        return api.nilObj;
    };
    let lang = unsafe { (api.import)(LANG_NAME.as_ptr().cast()) };
    let cst = unsafe { send(api, lang, PARSE_SELECTOR, &[values[0]]) };
    let cst_vt = unsafe { (api.import)(PEG_CST_VT_NAME.as_ptr().cast()) };
    if cst.is_null() || cst == api.nilObj || unsafe { (api.vtableOf)(cst) } != cst_vt {
        note_error(b"RustCodegen initial module source could not be parsed");
        return api.nilObj;
    }
    let Some(tree) = (unsafe { cst_tree(api, cst, 0) }) else {
        note_error(b"RustCodegen initial module CST is invalid");
        return api.nilObj;
    };
    let Some(tree) = rewrite_embedded(tree, &source_bytes, b".") else {
        note_error(b"RustCodegen initial module embedded source is unavailable");
        return api.nilObj;
    };
    let Ok(normalized) = lower_program(&tree, &source_bytes) else {
        note_error(b"RustCodegen initial module source could not be lowered");
        return api.nilObj;
    };
    let projected_bindings = birnam_codegen::resumable::projected_block_bindings(&normalized);
    let mut projected_statements = projected_bindings
        .iter()
        .filter(|binding| binding.captures.is_empty())
        .map(|binding| binding.statement_index)
        .collect::<Vec<_>>();
    projected_statements.sort_unstable();
    projected_statements.dedup();
    let path = unsafe { string_bytes(api, values[1]) }.unwrap_or_default();
    let plan = match compile_resumable_plan(
        &metadata,
        &requests,
        &selection,
        authority,
        &path,
        &source_bytes,
        &normalized,
    ) {
        Ok(plan) => plan,
        Err(error) => {
            let message =
                format!("RustCodegen initial module resumable lowering failed: {error:?}");
            note_error(message.as_bytes());
            return api.nilObj;
        }
    };
    let source_digest = sha256_hex(&source_bytes);
    let artifact_salt = u64::from_be_bytes(source_digest[..8].try_into().unwrap());
    let Some(generated) = (unsafe {
        emit_source(
            api,
            values[0],
            true,
            false,
            None,
            false,
            Some(artifact_salt),
            Some(plan.named_emissions),
            Some(plan.block_emissions),
            (!projected_statements.is_empty()).then_some(projected_statements.as_slice()),
        )
    }) else {
        note_error(b"RustCodegen initial module source could not be emitted");
        return api.nilObj;
    };
    unsafe {
        write_emitted_artifact(
            api,
            values[0],
            values[1],
            generated,
            false,
            Some(&selection),
            Some(&plan.manifest),
        )
    }
}

unsafe fn write_emitted_artifact(
    api: &IdApi,
    source_object: Oop,
    output_path: Oop,
    source: EmittedSource,
    image_artifact: bool,
    explicit_selection: Option<&[u8]>,
    resumable_manifest: Option<&[u8]>,
) -> Oop {
    let tcc = runtime().tcc as Oop;
    if tcc.is_null() {
        note_error(b"RustCodegen Tcc is unavailable");
        return api.nilObj;
    }
    let Some(source_bytes) = (unsafe { string_bytes(api, source_object) }) else {
        note_error(b"RustCodegen source was not a String");
        return api.nilObj;
    };
    let Some(path_bytes) = (unsafe { string_bytes(api, output_path) }) else {
        note_error(b"RustCodegen output path was not a String");
        return api.nilObj;
    };
    let (identity, requires, native_requires) = if image_artifact {
        let native_requires = unsafe { artifact_metadata(api, source_object) }
            .map(|(_, native_requires)| native_requires)
            .unwrap_or_default();
        (artifact_identity(&path_bytes), Vec::new(), native_requires)
    } else {
        match unsafe { artifact_metadata(api, source_object) } {
            Some((Some((identity, requires)), native_requires)) => {
                (identity, requires, native_requires)
            }
            Some((None, native_requires)) => {
                (artifact_identity(&path_bytes), Vec::new(), native_requires)
            }
            None => (artifact_identity(&path_bytes), Vec::new(), Vec::new()),
        }
    };
    let source_digest = sha256_hex(&source_bytes);
    let revision_digest = unsafe { parse_revision_metadata(api, source_object) }
        .map(|(_, metadata)| metadata.revision_digest)
        .unwrap_or(source_digest);
    let artifact = artifact_source(
        &source_bytes,
        &identity,
        &source_digest,
        &revision_digest,
        &requires,
        &native_requires,
        &source.bytes,
        !image_artifact,
        explicit_selection,
        resumable_manifest,
    );
    let c_source = unsafe { (api.mkString)(artifact.as_ptr(), artifact.len()) };
    let kind = unsafe { intern(api, b"dll\0") };
    let result = unsafe {
        send(
            api,
            tcc,
            TCC_COMPILE_OUTPUT_SELECTOR,
            &[c_source, output_path, kind],
        )
    };
    if result == api.nilObj {
        note_error(b"RustCodegen could not write the compiled dylib");
        api.nilObj
    } else {
        output_path
    }
}

unsafe fn rust_codegen_compile_output_impl(
    pointer: *const Oop,
    count: usize,
    source_path_is_root: bool,
    image_artifact: bool,
    selection_bearing: bool,
    resumable_manifest: Option<&[u8]>,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustCodegen compile:output: received invalid arguments"));
    if (selection_bearing && values.len() != 3)
        || (!selection_bearing && values.len() != 2 && values.len() != 3)
    {
        abort("RustCodegen compile:output: expects source/output or source/output/source path");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    clear_error();
    // A selection-bearing candidate AOT unit is a prebuilt generation candidate,
    // not an ordinary eager module. Route it through the same envelope,
    // predecessor projection, initializer transaction, and activation path as
    // CompiledUnit compilation. This keeps delivery form from creating a
    // second authority model: callers open/map/activate the resulting path.
    if selection_bearing {
        let Some((_, metadata)) = (unsafe { parse_revision_metadata(api, values[0]) }) else {
            note_error(b"RustCodegen selection-bearing source is not indexable");
            return api.nilObj;
        };
        let name = unsafe { intern_bytes(api, &metadata.name) };
        let candidate_args = [values[0], name, values[1], values[2]];
        return unsafe {
            rust_codegen_compile_candidate_impl(candidate_args.as_ptr(), candidate_args.len(), true)
        };
    }
    let source_path = if values.len() == 3 && !selection_bearing {
        unsafe { string_bytes(api, values[2]) }
    } else {
        None
    };
    if values.len() == 3 && !selection_bearing && source_path.is_none() {
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
            None,
            None,
            None,
        )
    }) else {
        note_error(b"RustCodegen could not lower the source");
        return api.nilObj;
    };
    unsafe {
        write_emitted_artifact(
            api,
            values[0],
            values[1],
            source,
            image_artifact,
            None,
            resumable_manifest,
        )
    }
}

unsafe extern "C" fn rust_codegen_compile_candidate(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    unsafe { rust_codegen_compile_candidate_impl(pointer, count, false) }
}

unsafe extern "C" fn rust_codegen_compile_candidate_selection(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    unsafe { rust_codegen_compile_candidate_impl(pointer, count, true) }
}

unsafe fn rust_codegen_compile_candidate_impl(
    pointer: *const Oop,
    count: usize,
    selection_bearing: bool,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustCodegen candidate compilation received invalid arguments"));
    let expected = if selection_bearing { 4 } else { 3 };
    if values.len() != expected {
        abort("RustCodegen candidate compilation received the wrong argument count");
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
    let parsed_requests = if selection_bearing {
        match unsafe { parse_selection_requests(api, values[3]) } {
            Ok(requests) => requests,
            Err(_) => {
                note_error(b"RustCodegen candidate selection request is invalid");
                return api.nilObj;
            }
        }
    } else {
        ParsedSelectionRequests::Exact(Vec::new())
    };
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
    let Some(requests) = expand_selection_requests(&metadata, parsed_requests) else {
        note_error(b"RustCodegen whole-unit selection could not be expanded");
        return api.nilObj;
    };
    let predecessor = unsafe { durable_current_generation(api) };
    if predecessor.is_null()
        || predecessor == api.nilObj
        || !unsafe { durable_generation_is_canonical(api, predecessor) }
    {
        note_error(b"RustCodegen candidate predecessor generation is unavailable");
        return api.nilObj;
    }
    let Some(expected_predecessor_digest) =
        (unsafe { durable_generation_base_digest(api, predecessor) })
    else {
        note_error(b"RustCodegen candidate predecessor digest is unavailable");
        return api.nilObj;
    };
    let Some(selection) = artifact_selection_with_requests(&metadata, &requests) else {
        note_error(b"RustCodegen candidate selection does not match its implementation inventory");
        return api.nilObj;
    };
    let source_map = unsafe {
        send(
            api,
            runtime.store as Oop,
            CURRENT_VALUE_SELECTOR,
            &[predecessor, intern(api, b"sourceMap\0")],
        )
    };
    let Some(modules) = (unsafe { generation_modules(api, source_map) }) else {
        note_error(b"RustCodegen candidate predecessor source map is invalid");
        return api.nilObj;
    };
    let Some(predecessor_selection) = (unsafe { durable_generation_selection(api, predecessor) })
    else {
        note_error(b"RustCodegen candidate predecessor selection is unavailable");
        return api.nilObj;
    };
    let Some(revision) = decode_hex_digest(&metadata.revision_digest) else {
        note_error(b"RustCodegen candidate revision digest is invalid");
        return api.nilObj;
    };
    let Some(projected) = project_candidate_generation(
        modules,
        GenerationModuleProjection {
            name: metadata.name.clone(),
            revision,
            source: source_bytes.clone(),
            metadata: metadata.clone(),
        },
        &predecessor_selection,
        &selection,
    ) else {
        note_error(b"RustCodegen candidate successor generation cannot be projected");
        return api.nilObj;
    };
    let has_resumables = requests
        .iter()
        .any(|request| request.disposition == SelectionDisposition::ResumableRequired);
    let lang = unsafe { (api.import)(LANG_NAME.as_ptr().cast()) };
    let cst = unsafe { send(api, lang, PARSE_SELECTOR, &[values[0]]) };
    let cst_vt = unsafe { (api.import)(PEG_CST_VT_NAME.as_ptr().cast()) };
    if cst.is_null() || cst == api.nilObj || unsafe { (api.vtableOf)(cst) != cst_vt } {
        note_error(b"RustCodegen candidate source could not be parsed");
        return api.nilObj;
    }
    let Some(tree) = (unsafe { cst_tree(api, cst, 0) }) else {
        note_error(b"RustCodegen candidate CST is invalid");
        return api.nilObj;
    };
    let Some(tree) = rewrite_embedded(tree, &source_bytes, b".") else {
        note_error(b"RustCodegen candidate embedded source is unavailable");
        return api.nilObj;
    };
    let Ok(normalized) = lower_program(&tree, &source_bytes) else {
        note_error(b"RustCodegen candidate source could not be lowered");
        return api.nilObj;
    };
    let projected_bindings = birnam_codegen::resumable::projected_block_bindings(&normalized);
    for request in requests.iter().filter(|request| {
        request.disposition == SelectionDisposition::ResumableRequired
            && request.root_kind == RootKind::NonMethodProgram
            && request.implementation_kind == SelectionImplementationKind::BlockBody
    }) {
        let Some(binding) = projected_bindings.iter().find(|binding| {
            binding.key.root_index == request.root_index && binding.key.path == request.path
        }) else {
            note_error(
                b"RustCodegen selected nonmethod closure is not a projected top-level binding",
            );
            return api.nilObj;
        };
        if !binding.captures.is_empty() {
            note_error(b"RustCodegen projected top-level closure has lexical captures");
            return api.nilObj;
        }
    }
    let mut projected_statements = projected_bindings
        .iter()
        .filter(|binding| binding.captures.is_empty())
        .map(|binding| binding.statement_index)
        .collect::<Vec<_>>();
    projected_statements.sort_unstable();
    projected_statements.dedup();
    let (resumable_manifest, named_resumables, block_resumables) = if has_resumables {
        let plan = match compile_resumable_plan(
            &metadata,
            &requests,
            &selection,
            projected.authority,
            &path_bytes,
            &source_bytes,
            &normalized,
        ) {
            Ok(plan) => plan,
            Err(error) => {
                let message =
                    format!("RustCodegen selected resumable candidate lowering failed: {error:?}");
                note_error(message.as_bytes());
                return api.nilObj;
            }
        };
        (
            plan.manifest,
            Some(plan.named_emissions),
            Some(plan.block_emissions),
        )
    } else {
        let Ok(selection_view) = inspect_artifact_selection(&selection) else {
            note_error(b"RustCodegen candidate selection is invalid");
            return api.nilObj;
        };
        let Ok(manifest) =
            resumable_manifest::materialize(projected.authority, selection_view, &[])
        else {
            note_error(b"RustCodegen candidate resumable manifest cannot be materialized");
            return api.nilObj;
        };
        (manifest, None, None)
    };

    let program_object = if projected_statements.is_empty() {
        let mut program = Vec::new();
        for operation in &metadata.operations {
            program.extend_from_slice(&operation.source);
            program.extend_from_slice(b";\n");
        }
        unsafe { (api.mkString)(program.as_ptr(), program.len()) }
    } else {
        values[0]
    };
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
            named_resumables,
            block_resumables,
            (!projected_statements.is_empty()).then_some(projected_statements.as_slice()),
        )
    }) else {
        note_error(b"RustCodegen candidate source could not be emitted");
        return api.nilObj;
    };
    let native_requires = unsafe { artifact_metadata(api, values[0]) }
        .map(|(_, native_requires)| native_requires)
        .unwrap_or_default();
    let selection_digest = sha256_hex(&selection);
    let manifest = candidate_manifest(&metadata, &source_digest, &selection_digest);
    let rust_owned_shims = unsafe { (api.isExported)(CODEGEN_NAME.as_ptr().cast()) } == 0;
    let candidate_generated = candidate_generated(
        &generated.bytes,
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
        true,
        Some(&selection),
        Some(&resumable_manifest),
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
    sidecar.extend_from_slice(b"\npredecessor=");
    sidecar.extend_from_slice(&expected_predecessor_digest);
    sidecar.extend_from_slice(b"\nmanifest=");
    sidecar.extend_from_slice(&manifest);
    sidecar.extend_from_slice(b"\nselection=");
    sidecar.extend_from_slice(selection.len().to_string().as_bytes());
    sidecar.push(b'\n');
    sidecar.extend_from_slice(&selection);
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
/// can run. This mirrors the Codegen check byte-for-byte at the contract
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
    if !candidate_manifest_matches(manifest, &source, &metadata, envelope.artifact_selection) {
        note_error(b"RustCodegen candidate metadata mismatches source");
        return api.nilObj;
    }
    let artifact_selection = envelope.artifact_selection.to_vec();
    let Ok(expected_predecessor_digest) =
        <[u8; 64]>::try_from(envelope.expected_predecessor_digest)
    else {
        note_error(b"RustCodegen candidate predecessor digest is invalid");
        return api.nilObj;
    };
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
    let expected_predecessor_object = unsafe {
        (api.mkString)(
            expected_predecessor_digest.as_ptr(),
            expected_predecessor_digest.len(),
        )
    };
    let requires = unsafe { candidate_manifest_vector(api, &metadata) };
    let source_revision = unsafe { source_revision_from_metadata(api, source_object, &metadata) };
    if source_revision.is_null() || source_revision == api.nilObj {
        note_error(b"RustCodegen candidate source revision is unavailable");
        return api.nilObj;
    }
    let facts = [
        (b"unitName\0".as_slice(), name),
        (b"kind\0".as_slice(), unsafe {
            intern(api, b"candidateArtifact\0")
        }),
        (b"sourceHash\0".as_slice(), source_hash),
        (b"authoredSource\0".as_slice(), source_object),
        (b"sourceRevision\0".as_slice(), source_revision),
        (b"nonMethodDigest\0".as_slice(), non_method),
        (b"methodDigest\0".as_slice(), method),
        (b"revisionDigest\0".as_slice(), revision),
        (b"artifactChecksum\0".as_slice(), checksum_object),
        (
            b"expectedBaseDigest\0".as_slice(),
            expected_predecessor_object,
        ),
        (b"platform\0".as_slice(), unsafe {
            intern(api, b"aarch64-macos\0")
        }),
        (b"runtimeAbi\0".as_slice(), unsafe {
            intern_bytes(api, ARTIFACT_ABI)
        }),
        (b"candidateAbi\0".as_slice(), unsafe {
            (api.mkSmallInt)(i64::from(CANDIDATE_ABI))
        }),
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
            artifact_selection,
            resumable_manifest: Vec::new(),
            expected_predecessor_digest,
            projected_authority: None,
            projected_selection: Vec::new(),
            requires: metadata.requires.clone(),
            operations,
            initializer_writes: Vec::new(),
            handle: 0,
            validate: 0,
            prepare: 0,
            activate: 0,
            rollback: 0,
            prepared: 0,
            active: false,
            installed_method_ever: false,
            all_installed_methods_resumable: true,
            predecessor: 0,
            rollback_leases: Vec::new(),
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

type BeginResumableUnitV1 = unsafe extern "C" fn(*const u8, usize, *const u8, u64, u64) -> u32;
type CloseResumableUnitV1 = unsafe extern "C" fn() -> u32;
type BeginArtifactContextV1 = unsafe extern "C" fn(u64, u64) -> u32;
type EndArtifactContextV1 = unsafe extern "C" fn() -> u32;
type TryAcquireStacklessGenerationLeaseV1 =
    unsafe extern "C" fn(Oop, *mut StacklessGenerationLeaseV1) -> u32;
type ReleaseStacklessGenerationLeaseV1 =
    unsafe extern "C" fn(*const StacklessGenerationLeaseV1) -> u32;
type RetireStacklessCodeGenerationV1 = unsafe extern "C" fn(u64) -> u32;

struct PendingRollbackLeasesV1 {
    release: ReleaseStacklessGenerationLeaseV1,
    leases: Vec<StacklessGenerationLeaseV1>,
}

impl PendingRollbackLeasesV1 {
    fn into_inner(mut self) -> Vec<StacklessGenerationLeaseV1> {
        core::mem::take(&mut self.leases)
    }
}

impl Drop for PendingRollbackLeasesV1 {
    fn drop(&mut self) {
        for lease in self.leases.drain(..).rev() {
            // SAFETY: every carrier was issued by the same selected floor and
            // has not been released or transferred into candidate state.
            if unsafe { (self.release)(core::ptr::addr_of!(lease)) } != 0 {
                abort("rollback generation lease compensation failed");
            }
        }
    }
}

unsafe fn acquire_rollback_leases_v1(
    api: &IdApi,
    operations: &[CandidateOperationState],
) -> Result<Option<PendingRollbackLeasesV1>, &'static [u8]> {
    let methods = operations
        .iter()
        .filter(|operation| operation.prior_present)
        .map(|operation| operation.prior_method)
        .collect::<BTreeSet<_>>();
    if methods.is_empty() {
        return Ok(None);
    }
    let acquire_address = unsafe {
        floor_symbol(
            api,
            b"birnam_id_try_acquire_stackless_generation_lease_v1\0",
        )
    }
    .ok_or(b"RustCodegen rollback generation lease acquire seam is unavailable".as_slice())?;
    let release_address =
        unsafe { floor_symbol(api, b"birnam_id_release_stackless_generation_lease_v1\0") }.ok_or(
            b"RustCodegen rollback generation lease release seam is unavailable".as_slice(),
        )?;
    let acquire: TryAcquireStacklessGenerationLeaseV1 =
        unsafe { core::mem::transmute(acquire_address) };
    let release: ReleaseStacklessGenerationLeaseV1 =
        unsafe { core::mem::transmute(release_address) };
    let mut pending = PendingRollbackLeasesV1 {
        release,
        leases: Vec::new(),
    };
    for method in methods {
        let mut lease = StacklessGenerationLeaseV1::default();
        match unsafe { acquire(method as Oop, core::ptr::addr_of_mut!(lease)) } {
            0 if lease.token_generation != 0 => pending.leases.push(lease),
            2 => {}
            _ => return Err(b"RustCodegen rollback generation lease acquisition failed"),
        }
    }
    Ok(Some(pending))
}

unsafe fn release_rollback_leases_v1(api: &IdApi, leases: &mut Vec<StacklessGenerationLeaseV1>) {
    if leases.is_empty() {
        return;
    }
    let address =
        unsafe { floor_symbol(api, b"birnam_id_release_stackless_generation_lease_v1\0") }
            .unwrap_or_else(|| abort("rollback generation lease release seam disappeared"));
    let release: ReleaseStacklessGenerationLeaseV1 = unsafe { core::mem::transmute(address) };
    for lease in leases.drain(..).rev() {
        if unsafe { release(core::ptr::addr_of!(lease)) } != 0 {
            abort("rollback generation lease release failed");
        }
    }
}

unsafe fn installed_methods_are_resumable_v1(
    api: &IdApi,
    operations: &[CandidateOperationState],
) -> bool {
    let methods = operations
        .iter()
        .filter(|operation| operation.install && operation.post_present)
        .map(|operation| operation.post_method)
        .collect::<BTreeSet<_>>();
    if methods.is_empty() {
        return true;
    }
    let Some(acquire_address) = (unsafe {
        floor_symbol(
            api,
            b"birnam_id_try_acquire_stackless_generation_lease_v1\0",
        )
    }) else {
        return false;
    };
    let Some(release_address) =
        (unsafe { floor_symbol(api, b"birnam_id_release_stackless_generation_lease_v1\0") })
    else {
        return false;
    };
    let acquire: TryAcquireStacklessGenerationLeaseV1 =
        unsafe { core::mem::transmute(acquire_address) };
    let release: ReleaseStacklessGenerationLeaseV1 =
        unsafe { core::mem::transmute(release_address) };
    for method in methods {
        let mut lease = StacklessGenerationLeaseV1::default();
        if unsafe { acquire(method as Oop, core::ptr::addr_of_mut!(lease)) } != 0
            || lease.token_generation == 0
        {
            return false;
        }
        if unsafe { release(core::ptr::addr_of!(lease)) } != 0 {
            abort("resumable method eligibility lease release failed");
        }
    }
    true
}

struct ActiveResumableStageV1 {
    finish: CloseResumableUnitV1,
    cancel: CloseResumableUnitV1,
    artifact_end: Option<EndArtifactContextV1>,
}

impl ActiveResumableStageV1 {
    unsafe fn close(self, accept: bool) -> bool {
        let context_ok = self
            .artifact_end
            .is_none_or(|artifact_end| unsafe { artifact_end() } == 0);
        let close = if accept && context_ok {
            self.finish
        } else {
            self.cancel
        };
        (unsafe { close() }) == 0 && accept && context_ok
    }
}

unsafe fn begin_resumable_stage_v1(
    api: &IdApi,
    handle: *mut c_void,
    manifest: &[u8],
    authority: [u8; 32],
) -> Result<ActiveResumableStageV1, &'static [u8]> {
    let admitted = resumable_manifest::inspect(manifest, authority)
        .map_err(|_| b"RustCodegen resumable manifest is invalid".as_slice())?;
    let begin_address = unsafe { floor_symbol(api, b"birnam_id_begin_resumable_unit_v1\0") }
        .ok_or(b"RustCodegen resumable staging begin seam is unavailable".as_slice())?;
    let finish_address = unsafe { floor_symbol(api, b"birnam_id_finish_resumable_unit_v1\0") }
        .ok_or(b"RustCodegen resumable staging finish seam is unavailable".as_slice())?;
    let cancel_address = unsafe { floor_symbol(api, b"birnam_id_cancel_resumable_unit_v1\0") }
        .ok_or(b"RustCodegen resumable staging cancel seam is unavailable".as_slice())?;
    let code_generation_identity = handle as usize as u64;
    let code_generation_lease = issue_code_generation_lease()
        .ok_or(b"RustCodegen code-generation lease space is exhausted".as_slice())?;
    if code_generation_identity == 0 {
        return Err(b"RustCodegen code-generation identity is invalid");
    }
    let begin: BeginResumableUnitV1 = unsafe { core::mem::transmute(begin_address) };
    let finish: CloseResumableUnitV1 = unsafe { core::mem::transmute(finish_address) };
    let cancel: CloseResumableUnitV1 = unsafe { core::mem::transmute(cancel_address) };
    if unsafe {
        begin(
            manifest.as_ptr(),
            manifest.len(),
            authority.as_ptr(),
            code_generation_identity,
            code_generation_lease,
        )
    } != 0
    {
        return Err(b"RustCodegen resumable staged unit was rejected");
    }
    let artifact_end = if admitted.entry_count() == 0 {
        None
    } else {
        let begin_context_symbol =
            unsafe { (api.dlsym)(handle, c"__birnam_resumable_context_begin_v1".as_ptr()) };
        let end_context_symbol =
            unsafe { (api.dlsym)(handle, c"__birnam_resumable_context_end_v1".as_ptr()) };
        if begin_context_symbol.is_null() || end_context_symbol.is_null() {
            if unsafe { cancel() } != 0 {
                abort("resumable staged unit compensation failed");
            }
            return Err(b"RustCodegen artifact resumable context exports are unavailable");
        }
        let begin_context: BeginArtifactContextV1 =
            unsafe { core::mem::transmute(begin_context_symbol) };
        let end_context: EndArtifactContextV1 = unsafe { core::mem::transmute(end_context_symbol) };
        if unsafe { begin_context(code_generation_identity, code_generation_lease) } != 0 {
            if unsafe { cancel() } != 0 {
                abort("resumable staged unit compensation failed");
            }
            return Err(b"RustCodegen artifact resumable context begin was rejected");
        }
        Some(end_context)
    };
    Ok(ActiveResumableStageV1 {
        finish,
        cancel,
        artifact_end,
    })
}

fn issue_code_generation_lease() -> Option<u64> {
    NEXT_CODE_GENERATION_LEASE
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
            current.checked_add(1)
        })
        .ok()
        .filter(|lease| *lease != 0)
}

fn issue_prepared_token(api: &IdApi) -> Option<(usize, Oop)> {
    let value = NEXT_PREPARED_TOKEN
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
            current
                .checked_add(1)
                .filter(|next| *next <= i64::MAX as u64)
        })
        .ok()
        .filter(|value| *value != 0)?;
    let token = unsafe { (api.mkSmallInt)(value as i64) };
    Some((usize::try_from(value).ok()?, token))
}

unsafe fn prepared_token_value(api: &IdApi, token: Oop) -> Option<usize> {
    if unsafe { (api.isSmallInt)(token) } == 0 {
        return None;
    }
    usize::try_from(unsafe { (api.smallIntValue)(token) }).ok()
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
    // Rust-only stack this is the first safe point to claim Store callbacks.
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
        unsafe { reap_retained_generations(api) };
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

// The host phase bridge is a frozen C ABI. Rust keeps the language-level
// selectors above on the Codegen object, then exposes these process-local
// adapters so the host does not need to know which Codegen floor owns them.
#[unsafe(export_name = "birnam_codegen_enter_running")]
pub unsafe extern "C" fn birnam_codegen_enter_running() -> c_int {
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    (unsafe { rust_codegen_enter_running(api.nilObj, ptr::null(), 0) } == api.trueObj) as c_int
}

#[unsafe(export_name = "birnam_codegen_begin_quiescence")]
pub unsafe extern "C" fn birnam_codegen_begin_quiescence() -> c_int {
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    (unsafe { rust_codegen_begin_quiescence(api.nilObj, ptr::null(), 0) } == api.trueObj) as c_int
}

#[unsafe(export_name = "birnam_codegen_end_quiescence")]
pub unsafe extern "C" fn birnam_codegen_end_quiescence() -> c_int {
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    (unsafe { rust_codegen_end_quiescence(api.nilObj, ptr::null(), 0) } == api.trueObj) as c_int
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
    // The canonical Codegen provider owns Store callback registration.
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
    let Some(callback) = (unsafe {
        borrowed_address(
            api,
            rust_codegen_authorize_method_mutation as *const () as usize,
        )
    }) else {
        return;
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

    let (
        author_path,
        checksum,
        manifest,
        artifact_selection,
        candidate_source,
        expected_predecessor_digest,
        requires,
        existing_handle,
    ) = {
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
            candidate.artifact_selection.clone(),
            candidate.source as Oop,
            candidate.expected_predecessor_digest,
            candidate.requires.clone(),
            candidate.handle,
        )
    };
    if existing_handle != 0 {
        let _ = unsafe { assert_candidate_fact(api, runtime, values[0], b"mapped\0", api.trueObj) };
        return values[0];
    }

    if !requires.is_empty() {
        let module = unsafe { global_by_name(api, runtime, b"Module") };
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
    // the already-loaded native rung set, matching the mapper.
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
    if !unsafe { bind_mapped_code_generation_identity(api, handle, handle as usize as u64) } {
        return unsafe {
            map_candidate_failure(
                api,
                handle,
                b"RustCodegen candidate cache identity is unavailable",
            )
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
    let Ok(parsed_manifest) = inspect_manifest(&manifest) else {
        return unsafe {
            map_candidate_failure(api, handle, b"RustCodegen candidate manifest is invalid")
        };
    };
    match parsed_manifest.version {
        CandidateManifestVersion::V4 => {
            let selection_symbol =
                unsafe { (api.dlsym)(handle, c"__birnam_artifact_selection_v1".as_ptr()) };
            let selection_size_symbol =
                unsafe { (api.dlsym)(handle, c"__birnam_artifact_selection_v1_size".as_ptr()) };
            if selection_symbol.is_null() || selection_size_symbol.is_null() {
                return unsafe {
                    map_candidate_failure(
                        api,
                        handle,
                        b"RustCodegen candidate selection is unavailable",
                    )
                };
            }
            let selection_size =
                unsafe { ptr::read_unaligned(selection_size_symbol.cast::<u64>()) };
            let Ok(selection_size) = usize::try_from(selection_size) else {
                return unsafe {
                    map_candidate_failure(
                        api,
                        handle,
                        b"RustCodegen candidate selection is invalid",
                    )
                };
            };
            if selection_size > MAX_CANONICAL_VALUE_BYTES
                || selection_size != artifact_selection.len()
            {
                return unsafe {
                    map_candidate_failure(
                        api,
                        handle,
                        b"RustCodegen candidate selection mismatches",
                    )
                };
            }
            let exported_selection = unsafe {
                core::slice::from_raw_parts(selection_symbol.cast::<u8>(), selection_size)
            };
            if !candidate_selection_matches(
                parsed_manifest,
                &artifact_selection,
                exported_selection,
            ) {
                return unsafe {
                    map_candidate_failure(
                        api,
                        handle,
                        b"RustCodegen candidate selection mismatches",
                    )
                };
            }
        }
    }
    let predecessor = unsafe { durable_current_generation(api) };
    let Some(projected) = (unsafe {
        project_open_candidate_generation(
            api,
            predecessor,
            &expected_predecessor_digest,
            candidate_source,
            &artifact_selection,
        )
    }) else {
        return unsafe {
            map_candidate_failure(api, handle, b"RustCodegen candidate predecessor is stale")
        };
    };
    let resumable_symbol =
        unsafe { (api.dlsym)(handle, c"__birnam_resumable_manifest_v1".as_ptr()) };
    let resumable_size_symbol =
        unsafe { (api.dlsym)(handle, c"__birnam_resumable_manifest_v1_size".as_ptr()) };
    if resumable_symbol.is_null() || resumable_size_symbol.is_null() {
        return unsafe {
            map_candidate_failure(
                api,
                handle,
                b"RustCodegen candidate resumable manifest is unavailable",
            )
        };
    }
    let resumable_size = unsafe { ptr::read_unaligned(resumable_size_symbol.cast::<u64>()) };
    let Ok(resumable_size) = usize::try_from(resumable_size) else {
        return unsafe {
            map_candidate_failure(
                api,
                handle,
                b"RustCodegen candidate resumable manifest is invalid",
            )
        };
    };
    if resumable_size > resumable_manifest::MAX_MANIFEST_BYTES {
        return unsafe {
            map_candidate_failure(
                api,
                handle,
                b"RustCodegen candidate resumable manifest is invalid",
            )
        };
    }
    let resumable_bytes =
        unsafe { core::slice::from_raw_parts(resumable_symbol.cast::<u8>(), resumable_size) };
    if resumable_manifest::inspect(resumable_bytes, projected.authority).is_err() {
        return unsafe {
            map_candidate_failure(
                api,
                handle,
                b"RustCodegen candidate resumable authority mismatches",
            )
        };
    }
    let resumable_bytes = resumable_bytes.to_vec();
    let projected_authority = projected.authority;
    let projected_selection = projected.selection;
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

    // SAFETY: the generated candidate ABI exports these exact function
    // signatures, and all calls occur while the dlopen handle is retained.
    // All immutable manifest and selection admission above completes before
    // this first execution of candidate-owned code.
    let init: unsafe extern "C" fn() = unsafe { core::mem::transmute(init_symbol) };
    let validate: Method = unsafe { core::mem::transmute(validate_symbol) };
    unsafe { init() };
    if unsafe { validate(api.nilObj, ptr::null(), 0) } != api.trueObj {
        return unsafe {
            map_candidate_failure(api, handle, b"RustCodegen candidate validation failed")
        };
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
    candidate.resumable_manifest = resumable_bytes;
    candidate.projected_authority = Some(projected_authority);
    candidate.projected_selection = projected_selection;
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
        let store = runtime().store as Oop;
        let basis = unsafe { send0(api, store, b"basis\0") };
        let planned_basis = plan.basis as Oop;
        let basis_matches = unsafe { (api.isSmallInt)(basis) } != 0
            && unsafe { (api.isSmallInt)(planned_basis) } != 0
            && unsafe { (api.smallIntValue)(basis) == (api.smallIntValue)(planned_basis) };
        return durable == plan.expected_generation as Oop && basis_matches;
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
    let Some((token_value, token)) = issue_prepared_token(api) else {
        note_error(b"RustCodegen prepared token allocation failed");
        return api.nilObj;
    };
    let Ok(mut prepared_states) = PREPARED.lock() else {
        note_error(b"RustCodegen prepared registry is unavailable");
        return api.nilObj;
    };
    if let Ok(mut candidates) = CANDIDATES.lock()
        && let Some(candidate) = candidates
            .iter_mut()
            .find(|candidate| candidate.unit == values[0] as usize)
    {
        candidate.prepared = token_value;
        candidate.operations = operations;
        prepared_states.push(PreparedState {
            token: token_value,
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
    let Some(token) = (unsafe { prepared_token_value(api, values[0]) }) else {
        note_error(b"RustCodegen prepared token is invalid");
        return api.nilObj;
    };
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
    let (
        activate,
        handle,
        resumable_manifest_bytes,
        candidate_source,
        artifact_selection,
        expected_predecessor_digest,
        rollback_operations,
    ) = {
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
        (
            candidate.activate,
            candidate.handle as *mut c_void,
            candidate.resumable_manifest.clone(),
            candidate.source as Oop,
            candidate.artifact_selection.clone(),
            candidate.expected_predecessor_digest,
            candidate.operations.clone(),
        )
    };
    if handle.is_null() || resumable_manifest_bytes.is_empty() {
        note_error(b"RustCodegen candidate resumable staging is unavailable");
        return api.nilObj;
    }
    let authority = if plan.enforce_lifecycle {
        let digest = unsafe {
            send(
                api,
                runtime.store as Oop,
                CURRENT_VALUE_SELECTOR,
                &[
                    plan.successor_generation as Oop,
                    intern(api, b"baseDigest\0"),
                ],
            )
        };
        let Some(digest) = (unsafe { string_bytes(api, digest) }) else {
            note_error(b"RustCodegen successor generation authority is unavailable");
            return api.nilObj;
        };
        let Ok(digest) = <[u8; 64]>::try_from(digest.as_slice()) else {
            note_error(b"RustCodegen successor generation authority is invalid");
            return api.nilObj;
        };
        let Some(authority) = decode_hex_digest(&digest) else {
            note_error(b"RustCodegen successor generation authority is invalid");
            return api.nilObj;
        };
        authority
    } else {
        let predecessor = unsafe { durable_current_generation(api) };
        let Some(projected) = (unsafe {
            project_open_candidate_generation(
                api,
                predecessor,
                &expected_predecessor_digest,
                candidate_source,
                &artifact_selection,
            )
        }) else {
            note_error(b"RustCodegen candidate predecessor is stale");
            return api.nilObj;
        };
        projected.authority
    };
    let rollback_leases = match unsafe { acquire_rollback_leases_v1(api, &rollback_operations) } {
        Ok(leases) => leases,
        Err(message) => {
            note_error(message);
            return api.nilObj;
        }
    };
    let stage = match unsafe {
        begin_resumable_stage_v1(api, handle, &resumable_manifest_bytes, authority)
    } {
        Ok(stage) => stage,
        Err(message) => {
            note_error(message);
            return api.nilObj;
        }
    };
    // SAFETY: the hook is retained by the mapped candidate handle.
    let activate: Method = unsafe { core::mem::transmute(activate) };
    if !begin_initializer_transaction(unit) {
        let _ = unsafe { stage.close(false) };
        note_error(b"RustCodegen initializer transaction is unavailable");
        return api.nilObj;
    }
    MOP_MUTATION_IN_PROGRESS.store(true, Ordering::Release);
    let result = unsafe { activate(api.nilObj, ptr::null(), 0) };
    MOP_MUTATION_IN_PROGRESS.store(false, Ordering::Release);
    let Some(initializer_writes) = take_initializer_transaction(unit) else {
        let _ = unsafe { stage.close(false) };
        abort("candidate initializer transaction disappeared");
    };
    let error_vt = unsafe { (api.import)(ERROR_VT_NAME.as_ptr().cast()) };
    let accepted = result != api.nilObj
        && (error_vt.is_null()
            || error_vt == api.nilObj
            || unsafe { (api.vtableOf)(result) } != error_vt);
    if !accepted && !unsafe { restore_initializer_writes(api, &initializer_writes) } {
        let _ = unsafe { stage.close(false) };
        abort("candidate initializer compensation failed");
    }
    if !unsafe { stage.close(accepted) } {
        if accepted && !unsafe { restore_initializer_writes(api, &initializer_writes) } {
            abort("candidate initializer compensation failed after registration rejection");
        }
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
    let installed_method = operations.iter().any(|operation| operation.install);
    let all_installed_methods_resumable =
        unsafe { installed_methods_are_resumable_v1(api, &operations) };
    if let Ok(mut candidates) = CANDIDATES.lock() {
        let index = candidates
            .iter()
            .position(|candidate| candidate.unit == unit);
        if let Some(index) = index {
            candidates[index].active = true;
            candidates[index].prepared = 0;
            candidates[index].operations = operations;
            candidates[index].initializer_writes = initializer_writes;
            candidates[index].installed_method_ever |= installed_method;
            candidates[index].all_installed_methods_resumable &= all_installed_methods_resumable;
            candidates[index].predecessor = prior;
            candidates[index].rollback_leases = rollback_leases
                .map(PendingRollbackLeasesV1::into_inner)
                .unwrap_or_default();
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

unsafe fn candidate_initializer_writes_match_post(
    api: &IdApi,
    writes: &[CandidateGlobalMutation],
) -> bool {
    writes
        .iter()
        .all(|write| (unsafe { current_global_raw(api, write.key as Oop) }) as usize == write.post)
}

/// Return the effective operation ledger for a generation root. Newer
/// generations shadow the same target/selector pair in their predecessors;
/// untouched predecessor operations remain rollback obligations of the root.
fn effective_candidate_operations(unit: usize) -> Vec<CandidateOperationState> {
    let Ok(candidates) = CANDIDATES.lock() else {
        return Vec::new();
    };
    let mut result = Vec::new();
    let mut cursor = unit;
    while cursor != 0 {
        let Some(candidate) = candidates.iter().find(|candidate| candidate.unit == cursor) else {
            break;
        };
        for operation in &candidate.operations {
            if result.iter().any(|existing: &CandidateOperationState| {
                existing.target_name == operation.target_name
                    && existing.selector == operation.selector
            }) {
                continue;
            }
            result.push(operation.clone());
        }
        cursor = candidate.predecessor;
    }
    result
}

/// Return the active projected-global ledger with the same successor-shadowing
/// rule as dispatch operations.
fn effective_candidate_initializer_writes(unit: usize) -> Vec<CandidateGlobalMutation> {
    let Ok(candidates) = CANDIDATES.lock() else {
        return Vec::new();
    };
    let mut result = Vec::new();
    let mut cursor = unit;
    while cursor != 0 {
        let Some(candidate) = candidates.iter().find(|candidate| candidate.unit == cursor) else {
            break;
        };
        for write in &candidate.initializer_writes {
            if result
                .iter()
                .any(|existing: &CandidateGlobalMutation| existing.name == write.name)
            {
                continue;
            }
            result.push(write.clone());
        }
        cursor = candidate.predecessor;
    }
    result
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
    let active = {
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
        candidate.active
    };
    if !active {
        return values[0];
    }
    let operations = effective_candidate_operations(values[0] as usize);
    let initializer_writes = effective_candidate_initializer_writes(values[0] as usize);
    if unsafe { candidate_operations_match_post(api, runtime, &operations) }
        && unsafe { candidate_initializer_writes_match_post(api, &initializer_writes) }
    {
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
    release_initializer_compensations(&initializer_writes);
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

/// Adopt the reflected composed generation created by the pure-Birnam image
/// lifecycle policy and rebuild its process-local binding ledgers from the
/// dispatch dictionary that image recovery just established.
///
/// Every source-map row must name a distinct `#composed` CompiledUnit. Those
/// units are mechanism roots only: their durable facts own lifecycle policy,
/// while this cache records the exact target/vtable/direct-method state needed
/// for later drift checks and candidates staged over the recovered image.
unsafe extern "C" fn rust_codegen_adopt_composed_generation(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }.unwrap_or_else(|| {
        abort("RustCodegen adoptComposedGeneration: received invalid arguments")
    });
    if values.len() != 1 {
        abort("RustCodegen adoptComposedGeneration: expects one generation");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    clear_error();
    if !IMAGE_DISPATCH_RECOVERY.load(Ordering::Acquire)
        || RUNTIME_PHASE.load(Ordering::Acquire) != PHASE_IMAGE_RECOVERY
    {
        note_error(b"RustCodegen composed generation adoption requires image recovery");
        return api.nilObj;
    }
    let generation = values[0];
    if generation.is_null()
        || generation == api.nilObj
        || unsafe { durable_current_generation(api) } != generation
        || !unsafe { durable_generation_is_canonical(api, generation) }
    {
        note_error(b"RustCodegen composed generation is not the durable current generation");
        return api.nilObj;
    }
    let store = runtime.store as Oop;
    let source_map = unsafe {
        send(
            api,
            store,
            CURRENT_VALUE_SELECTOR,
            &[generation, intern(api, b"sourceMap\0")],
        )
    };
    let Some(modules) = (unsafe { generation_modules(api, source_map) }) else {
        note_error(b"RustCodegen composed generation source map is malformed");
        return api.nilObj;
    };
    let vector_vt = unsafe { (api.import)(VECTOR_VT_NAME.as_ptr().cast()) };
    let Some(entry_count) = (unsafe { vector_length(api, source_map, vector_vt) }) else {
        note_error(b"RustCodegen composed generation source map is malformed");
        return api.nilObj;
    };
    if entry_count != modules.len() {
        note_error(b"RustCodegen composed generation source map disagrees with its modules");
        return api.nilObj;
    }
    let Some(expected_digest) = (unsafe { durable_generation_base_digest(api, generation) }) else {
        note_error(b"RustCodegen composed generation digest is unavailable");
        return api.nilObj;
    };
    let mut units = BTreeSet::new();
    let mut composed = Vec::with_capacity(entry_count);
    for (index, module) in modules.into_iter().enumerate() {
        let entry = unsafe { vector_item(api, source_map, index) };
        let unit = unsafe { vector_item(api, entry, 2) };
        if unit.is_null() || unit == api.nilObj || !units.insert(unit as usize) {
            note_error(b"RustCodegen composed generation units are missing or duplicated");
            return api.nilObj;
        }
        let name = unsafe { vector_item(api, entry, 0) };
        let source = unsafe { vector_item(api, vector_item(api, entry, 1), 3) };
        let mut operations = Vec::with_capacity(module.metadata.operations.len());
        for operation in &module.metadata.operations {
            let target = unsafe { global_by_name(api, runtime, &operation.target) };
            if target.is_null() || target == api.nilObj {
                // The portable stream also contains bootstrap/private targets
                // that are not eligible for candidate management. They remain
                // reproducible code, but are outside the transient lifecycle
                // ledger by the same stable-owned-target rule used at staging.
                continue;
            }
            let vtable = unsafe { (api.vtableOf)(target) };
            if vtable.is_null() || unsafe { (api.vtableOwner)(vtable) } != target {
                continue;
            }
            let selector = unsafe { intern_bytes(api, &operation.selector) };
            let method =
                unsafe { send(api, store, STORE_RAW_CURRENT_SELECTOR, &[vtable, selector]) };
            let present = !method.is_null();
            operations.push(CandidateOperationState {
                target_name: operation.target.clone(),
                selector: operation.selector.clone(),
                install: operation.install,
                target: target as usize,
                vtable: vtable as usize,
                prior_method: method as usize,
                prior_present: present,
                post_method: method as usize,
                post_present: present,
            });
        }
        composed.push(CandidateState {
            unit: unit as usize,
            name: name as usize,
            source: source as usize,
            author_path: Vec::new(),
            checksum: [0; 64],
            source_digest: sha256_hex(&module.source),
            non_method_digest: module.metadata.non_method_digest,
            method_digest: module.metadata.method_digest,
            revision_digest: module.metadata.revision_digest,
            manifest: Vec::new(),
            artifact_selection: Vec::new(),
            resumable_manifest: Vec::new(),
            expected_predecessor_digest: expected_digest,
            projected_authority: None,
            projected_selection: Vec::new(),
            requires: module.metadata.requires,
            operations,
            initializer_writes: Vec::new(),
            handle: 0,
            validate: 0,
            prepare: 0,
            activate: 0,
            rollback: 0,
            prepared: 0,
            active: true,
            installed_method_ever: false,
            all_installed_methods_resumable: true,
            predecessor: 0,
            rollback_leases: Vec::new(),
        });
    }
    let Ok(mut candidates) = CANDIDATES.lock() else {
        note_error(b"RustCodegen candidate registry is unavailable");
        return api.nilObj;
    };
    candidates.retain(|candidate| !units.contains(&candidate.unit));
    candidates.extend(composed);
    ACTIVE_GENERATION.store(generation as usize, Ordering::Release);
    generation
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
    if let Ok(mut mapped) = MAPPED_IMAGE_RESUMABLES.lock() {
        *mapped = None;
    } else {
        abort("mapped image resumable registry is unavailable");
    }
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
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    IMAGE_DISPATCH_RECOVERY.store(false, Ordering::Release);
    if let Ok(mut mapped) = MAPPED_IMAGE_RESUMABLES.lock() {
        *mapped = None;
    } else {
        abort("mapped image resumable registry is unavailable");
    }
    let previous = IMAGE_PREVIOUS_PHASE.load(Ordering::Acquire);
    let _ = RUNTIME_PHASE.compare_exchange(
        PHASE_IMAGE_RECOVERY,
        previous,
        Ordering::AcqRel,
        Ordering::Acquire,
    );
    api.trueObj
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
    let (name, prepared, active, predecessor) = {
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
        )
    };
    let operations = effective_candidate_operations(values[0] as usize);
    let initializer_writes = effective_candidate_initializer_writes(values[0] as usize);
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
            release_initializer_compensations(&initializer_writes);
            note_error(b"RustCodegen candidate dispatch drifted");
            return api.nilObj;
        }
    }
    if !unsafe { candidate_initializer_writes_match_post(api, &initializer_writes) } {
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
        release_initializer_compensations(&initializer_writes);
        note_error(b"RustCodegen candidate initializer projection drifted");
        return api.nilObj;
    }
    let Some((token_value, token)) = issue_prepared_token(api) else {
        note_error(b"RustCodegen prepared token allocation failed");
        return api.nilObj;
    };
    let Ok(mut prepared_states) = PREPARED.lock() else {
        note_error(b"RustCodegen prepared registry is unavailable");
        return api.nilObj;
    };
    if let Ok(mut candidates) = CANDIDATES.lock()
        && let Some(candidate) = candidates
            .iter_mut()
            .find(|candidate| candidate.unit == values[0] as usize)
    {
        candidate.prepared = token_value;
        prepared_states.push(PreparedState {
            token: token_value,
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
    let Some(token) = (unsafe { prepared_token_value(api, values[0]) }) else {
        note_error(b"RustCodegen prepared token is invalid");
        return api.nilObj;
    };
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
    let (operations, initializer_writes) = {
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
        (
            candidate.operations.clone(),
            candidate.initializer_writes.clone(),
        )
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
    if !unsafe { restore_initializer_writes(api, &initializer_writes) } {
        abort("candidate initializer rollback compensation failed");
    }
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
    let mut rollback_leases = Vec::new();
    if let Ok(mut candidates) = CANDIDATES.lock() {
        let index = candidates
            .iter()
            .position(|candidate| candidate.unit == unit);
        if let Some(index) = index {
            candidates[index].active = false;
            candidates[index].prepared = 0;
            rollback_leases = core::mem::take(&mut candidates[index].rollback_leases);
            if prior != 0
                && let Some(previous) = candidates
                    .iter_mut()
                    .find(|candidate| candidate.unit == prior)
            {
                previous.active = true;
            }
        }
    }
    unsafe { release_rollback_leases_v1(api, &mut rollback_leases) };
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
    let Some(token) = (unsafe { prepared_token_value(api, values[0]) }) else {
        note_error(b"RustCodegen prepared token is invalid");
        return api.nilObj;
    };
    let unit = {
        let Ok(prepared) = PREPARED.lock() else {
            note_error(b"RustCodegen prepared registry is unavailable");
            return api.nilObj;
        };
        let Some(state) = prepared.iter().find(|state| state.token == token) else {
            note_error(b"RustCodegen prepared token is unknown during discard");
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
            note_error(b"RustCodegen candidate prepared token mismatches during discard");
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

unsafe extern "C" fn rust_codegen_load(self_object: Oop, pointer: *const Oop, count: usize) -> Oop {
    unsafe { rust_codegen_load_impl(self_object, pointer, count, ResumableLoadPolicy::None) }
}

unsafe extern "C" fn rust_codegen_load_initial_module(
    self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    unsafe {
        rust_codegen_load_impl(
            self_object,
            pointer,
            count,
            ResumableLoadPolicy::InitialModule,
        )
    }
}

unsafe fn rust_codegen_load_impl(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
    resumable_policy: ResumableLoadPolicy<'_>,
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
    if !unsafe { bind_mapped_code_generation_identity(api, handle, handle as usize as u64) } {
        return unsafe {
            map_candidate_failure(
                api,
                handle,
                b"RustCodegen artifact cache identity is unavailable",
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
    // process-width length symbol for the lifetime of the retained handle.
    // Older artifacts only export the descriptor array, so derive its framed
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
    let artifact_source = {
        let source_symbol = unsafe { (api.dlsym)(handle, c"__birnam_artifact_source".as_ptr()) };
        let source_len_symbol =
            unsafe { (api.dlsym)(handle, c"__birnam_artifact_source_len".as_ptr()) };
        if source_symbol.is_null() && source_len_symbol.is_null() {
            None
        } else if source_symbol.is_null() || source_len_symbol.is_null() {
            return unsafe {
                map_candidate_failure(
                    api,
                    handle,
                    b"RustCodegen artifact source export is incomplete",
                )
            };
        } else {
            let source_len = unsafe { *(source_len_symbol.cast::<usize>()) };
            Some(unsafe { core::slice::from_raw_parts(source_symbol.cast::<u8>(), source_len) })
        }
    };
    let selection_symbol =
        unsafe { (api.dlsym)(handle, c"__birnam_artifact_selection_v1".as_ptr()) };
    let selection_size_symbol =
        unsafe { (api.dlsym)(handle, c"__birnam_artifact_selection_v1_size".as_ptr()) };
    if selection_symbol.is_null() != selection_size_symbol.is_null() {
        return unsafe {
            map_candidate_failure(
                api,
                handle,
                b"RustCodegen artifact selection export is incomplete",
            )
        };
    }
    let exported_selection = if selection_symbol.is_null() {
        None
    } else {
        let size = unsafe { ptr::read_unaligned(selection_size_symbol.cast::<u64>()) };
        let Ok(size) = usize::try_from(size) else {
            return unsafe {
                map_candidate_failure(api, handle, b"RustCodegen artifact selection is invalid")
            };
        };
        if size > MAX_CANONICAL_VALUE_BYTES {
            return unsafe {
                map_candidate_failure(api, handle, b"RustCodegen artifact selection is too large")
            };
        }
        Some(unsafe { core::slice::from_raw_parts(selection_symbol.cast::<u8>(), size) })
    };
    let manifest_symbol =
        unsafe { (api.dlsym)(handle, c"__birnam_resumable_manifest_v1".as_ptr()) };
    let manifest_size_symbol =
        unsafe { (api.dlsym)(handle, c"__birnam_resumable_manifest_v1_size".as_ptr()) };
    if manifest_symbol.is_null() != manifest_size_symbol.is_null() {
        return unsafe {
            map_candidate_failure(
                api,
                handle,
                b"RustCodegen artifact resumable manifest export is incomplete",
            )
        };
    }
    let exported_manifest = if manifest_symbol.is_null() {
        None
    } else {
        let manifest_size = unsafe { ptr::read_unaligned(manifest_size_symbol.cast::<u64>()) };
        let Ok(manifest_size) = usize::try_from(manifest_size) else {
            return unsafe {
                map_candidate_failure(
                    api,
                    handle,
                    b"RustCodegen artifact resumable manifest length is invalid",
                )
            };
        };
        if manifest_size > resumable_manifest::MAX_MANIFEST_BYTES {
            return unsafe {
                map_candidate_failure(
                    api,
                    handle,
                    b"RustCodegen artifact resumable manifest is too large",
                )
            };
        }
        Some(unsafe { core::slice::from_raw_parts(manifest_symbol.cast::<u8>(), manifest_size) })
    };
    let initial_expectation = if matches!(resumable_policy, ResumableLoadPolicy::InitialModule) {
        let (Some(source), Some(selection)) = (artifact_source, exported_selection) else {
            return unsafe {
                map_candidate_failure(
                    api,
                    handle,
                    b"RustCodegen initial module source or selection is unavailable",
                )
            };
        };
        if sha256_hex(source).as_slice() != parsed_descriptor.source_digest {
            return unsafe {
                map_candidate_failure(api, handle, b"RustCodegen initial module source mismatches")
            };
        }
        let Some(authority) = (unsafe {
            initial_module_authority(api, parsed_descriptor.identity, source, selection)
        }) else {
            return unsafe {
                map_candidate_failure(
                    api,
                    handle,
                    b"RustCodegen initial module generation projection is invalid",
                )
            };
        };
        Some(authority)
    } else {
        None
    };
    let expected = match resumable_policy {
        ResumableLoadPolicy::None => None,
        ResumableLoadPolicy::Expected(expected) => Some(expected),
        ResumableLoadPolicy::InitialModule => {
            let Some(manifest) = exported_manifest else {
                return unsafe {
                    map_candidate_failure(
                        api,
                        handle,
                        b"RustCodegen initial module resumable manifest is missing",
                    )
                };
            };
            Some(ResumableLoadExpectation {
                authority: initial_expectation.expect("initial authority was projected"),
                manifest,
            })
        }
    };
    let resumable_manifest_shape = match (expected, exported_manifest) {
        (None, None) => None,
        (None, Some(_)) => {
            return unsafe {
                map_candidate_failure(
                    api,
                    handle,
                    b"RustCodegen artifact resumable manifest was not expected",
                )
            };
        }
        (Some(_), None) => {
            return unsafe {
                map_candidate_failure(
                    api,
                    handle,
                    b"RustCodegen artifact resumable manifest is missing",
                )
            };
        }
        (Some(expected), Some(exported)) => {
            if exported != expected.manifest {
                return unsafe {
                    map_candidate_failure(
                        api,
                        handle,
                        b"RustCodegen artifact resumable manifest mismatches",
                    )
                };
            }
            let Ok(manifest) = resumable_manifest::inspect(exported, expected.authority) else {
                return unsafe {
                    map_candidate_failure(
                        api,
                        handle,
                        b"RustCodegen artifact resumable manifest is invalid",
                    )
                };
            };
            Some((
                manifest.entry_count(),
                (0..manifest.entry_count()).any(|index| {
                    manifest.entry(index).is_some_and(|entry| {
                        entry.implementation_kind == SelectionImplementationKind::BlockBody
                    })
                }),
            ))
        }
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
    // Host-capability loading is the explicit process-local UI escape hatch
    // used by a recovered image. It must not mutate the sealed application
    // generation or require that the UI unit's ordinary application-module
    // dependencies were part of that image.
    // Ordinary Module load: remains dependency-checked and fail-closed.
    let host_capability_loading = HOST_CAPABILITY_LOADING.load(Ordering::Acquire);
    if parsed_descriptor.requires.count() != 0 && !host_capability_loading {
        let module = unsafe { global_by_name(api, runtime, b"Module") };
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
            let loaded = unsafe { send(api, module, MODULE_IS_LOADED_SELECTOR, &[requirement]) };
            if loaded != api.trueObj {
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
    if INITIAL_GENERATION_SEALED
        .lock()
        .ok()
        .is_some_and(|sealed| *sealed)
        && !HOST_CAPABILITY_LOADING.load(Ordering::Acquire)
        && !IMAGE_DISPATCH_RECOVERY.load(Ordering::Acquire)
    {
        return unsafe { error_value(api, b"moduleIntroductionNotAvailable") };
    }
    type BeginResumableUnit = unsafe extern "C" fn(*const u8, usize, *const u8, u64, u64) -> u32;
    type CloseResumableUnit = unsafe extern "C" fn() -> u32;
    type BeginArtifactContext = unsafe extern "C" fn(u64, u64) -> u32;
    type EndArtifactContext = unsafe extern "C" fn() -> u32;
    struct ResumableStage {
        finish: CloseResumableUnit,
        cancel: CloseResumableUnit,
        artifact_end: Option<EndArtifactContext>,
        code_generation_identity: u64,
    }

    let resumable_stage = if let Some((entry_count, _)) = resumable_manifest_shape {
        let expected = expected.expect("entry count requires expectation");
        let manifest = exported_manifest.expect("entry count requires manifest export");
        let Some(begin_address) =
            (unsafe { floor_symbol(api, b"birnam_id_begin_resumable_unit_v1\0") })
        else {
            return unsafe {
                map_candidate_failure(
                    api,
                    handle,
                    b"RustCodegen resumable staging begin seam is unavailable",
                )
            };
        };
        let Some(finish_address) =
            (unsafe { floor_symbol(api, b"birnam_id_finish_resumable_unit_v1\0") })
        else {
            return unsafe {
                map_candidate_failure(
                    api,
                    handle,
                    b"RustCodegen resumable staging finish seam is unavailable",
                )
            };
        };
        let Some(cancel_address) =
            (unsafe { floor_symbol(api, b"birnam_id_cancel_resumable_unit_v1\0") })
        else {
            return unsafe {
                map_candidate_failure(
                    api,
                    handle,
                    b"RustCodegen resumable staging cancel seam is unavailable",
                )
            };
        };
        let code_generation_identity = handle as usize as u64;
        let Some(code_generation_lease) = issue_code_generation_lease() else {
            return unsafe {
                map_candidate_failure(
                    api,
                    handle,
                    b"RustCodegen code-generation lease space is exhausted",
                )
            };
        };
        if code_generation_identity == 0 {
            return unsafe {
                map_candidate_failure(
                    api,
                    handle,
                    b"RustCodegen code-generation identity is invalid",
                )
            };
        }
        let begin: BeginResumableUnit = unsafe { core::mem::transmute(begin_address) };
        let finish: CloseResumableUnit = unsafe { core::mem::transmute(finish_address) };
        let cancel: CloseResumableUnit = unsafe { core::mem::transmute(cancel_address) };
        if unsafe {
            begin(
                manifest.as_ptr(),
                manifest.len(),
                expected.authority.as_ptr(),
                code_generation_identity,
                code_generation_lease,
            )
        } != 0
        {
            return unsafe {
                map_candidate_failure(
                    api,
                    handle,
                    b"RustCodegen resumable staged unit was rejected",
                )
            };
        }
        let artifact_end = if entry_count == 0 {
            None
        } else {
            let begin_context_symbol =
                unsafe { (api.dlsym)(handle, c"__birnam_resumable_context_begin_v1".as_ptr()) };
            let end_context_symbol =
                unsafe { (api.dlsym)(handle, c"__birnam_resumable_context_end_v1".as_ptr()) };
            if begin_context_symbol.is_null() || end_context_symbol.is_null() {
                if unsafe { cancel() } != 0 {
                    abort("resumable staged unit compensation failed");
                }
                return unsafe {
                    map_candidate_failure(
                        api,
                        handle,
                        b"RustCodegen artifact resumable context exports are unavailable",
                    )
                };
            }
            let begin_context: BeginArtifactContext =
                unsafe { core::mem::transmute(begin_context_symbol) };
            let end_context: EndArtifactContext =
                unsafe { core::mem::transmute(end_context_symbol) };
            if unsafe { begin_context(code_generation_identity, code_generation_lease) } != 0 {
                if unsafe { cancel() } != 0 {
                    abort("resumable staged unit compensation failed");
                }
                return unsafe {
                    map_candidate_failure(
                        api,
                        handle,
                        b"RustCodegen artifact resumable context begin was rejected",
                    )
                };
            }
            Some(end_context)
        };
        Some(ResumableStage {
            finish,
            cancel,
            artifact_end,
            code_generation_identity,
        })
    } else {
        None
    };

    unsafe { init() };
    let result = unsafe { entry(api.nilObj, ptr::null(), 0) };
    let artifact_context_ok = resumable_stage
        .as_ref()
        .and_then(|stage| stage.artifact_end)
        .is_none_or(|end| unsafe { end() } == 0);
    let error_vt = unsafe { (api.import)(ERROR_VT_NAME.as_ptr().cast()) };
    let is_error = !error_vt.is_null()
        && error_vt != api.nilObj
        && unsafe { (api.vtableOf)(result) } == error_vt;
    if let Some(stage) = resumable_stage {
        if result == api.nilObj || is_error || !artifact_context_ok {
            if unsafe { (stage.cancel)() } != 0 {
                abort("resumable staged unit compensation failed");
            }
            if result == api.nilObj || is_error {
                unsafe { dlclose(handle) };
                return result;
            }
            return unsafe {
                map_candidate_failure(
                    api,
                    handle,
                    b"RustCodegen artifact resumable context end was rejected",
                )
            };
        }
        if unsafe { (stage.finish)() } != 0 {
            return unsafe {
                map_candidate_failure(
                    api,
                    handle,
                    b"RustCodegen resumable staged unit did not commit",
                )
            };
        }
        let expected = expected.expect("staged unit requires expectation");
        let Some(matches_address) =
            (unsafe { floor_symbol(api, b"birnam_id_stackless_resumable_manifest_matches_v1\0") })
        else {
            abort("committed resumable manifest agreement seam disappeared");
        };
        let matches: unsafe extern "C" fn(*const u8, usize, u64, *const u8, usize) -> u32 =
            unsafe { core::mem::transmute(matches_address) };
        if unsafe {
            matches(
                expected.authority.as_ptr(),
                expected.authority.len(),
                stage.code_generation_identity,
                expected.manifest.as_ptr(),
                expected.manifest.len(),
            )
        } != 1
        {
            abort("committed resumable manifest disagrees with the mapped artifact");
        }
        if IMAGE_DISPATCH_RECOVERY.load(Ordering::Acquire) {
            let (_, has_block_entries) =
                resumable_manifest_shape.expect("staged unit requires manifest shape");
            let resolver = if has_block_entries {
                let resolver =
                    unsafe { (api.dlsym)(handle, c"__birnam_resolve_block_vtable_v1".as_ptr()) };
                if resolver.is_null() {
                    abort("committed block manifest lost its artifact resolver");
                }
                resolver as usize
            } else {
                0
            };
            let Ok(mut mapped) = MAPPED_IMAGE_RESUMABLES.lock() else {
                abort("mapped image resumable registry is unavailable");
            };
            if mapped.is_some() {
                abort("more than one image resumable unit was mapped in one recovery");
            }
            let _ = resolver;
            *mapped = Some(MappedImageResumables);
        }
    } else if result == api.nilObj || is_error {
        if let Ok(mut handles) = LIVE_ARTIFACT_HANDLES.lock() {
            handles.push(handle as usize);
        }
        return result;
    }
    if let Ok(mut handles) = LIVE_ARTIFACT_HANDLES.lock() {
        handles.push(handle as usize);
    }
    if let Some(source) = artifact_source
        && !IMAGE_DISPATCH_RECOVERY.load(Ordering::Acquire)
    {
        // Current image-v8 recovery has already restored this exact source
        // projection into Store. The mapped/rebuilt unit must materialize its
        // methods, but re-registering the source would create a fresh durable
        // generation identity on every image lifetime.
        if !unsafe {
            register_artifact_source(
                api,
                parsed_descriptor.identity,
                parsed_descriptor.source_digest,
                source,
                matches!(resumable_policy, ResumableLoadPolicy::InitialModule)
                    .then_some(exported_selection)
                    .flatten(),
            )
        } {
            return api.nilObj;
        }
    }
    result
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PhysicalDisposition {
    Disposed,
    Retained(&'static [u8]),
}

fn take_candidate_roots(generation: u64) -> Vec<CandidateRoot> {
    let Ok(mut roots) = CANDIDATE_ROOTS.lock() else {
        return Vec::new();
    };
    let mut retained = Vec::with_capacity(roots.len());
    let mut taken = Vec::new();
    for root in roots.drain(..) {
        if root.generation == generation {
            taken.push(root);
        } else {
            retained.push(root);
        }
    }
    *roots = retained;
    taken
}

fn take_vtable_cache_roots(generation: u64) -> Vec<VtableCacheRoot> {
    let Ok(mut roots) = VTABLE_CACHE_ROOTS.lock() else {
        return Vec::new();
    };
    let mut retained = Vec::with_capacity(roots.len());
    let mut taken = Vec::new();
    for root in roots.drain(..) {
        if root.generation == generation {
            taken.push(root);
        } else {
            retained.push(root);
        }
    }
    *roots = retained;
    taken
}

unsafe fn restore_live_vtable_cache_roots(api: &IdApi, roots: Vec<VtableCacheRoot>) {
    let Ok(mut cache) = VTABLE_CACHE_ROOTS.lock() else {
        return;
    };
    for root in roots {
        let value = root.value as Oop;
        // vtableParent is an exact registry lookup and does not dereference an
        // untrusted payload. A null result means GC reclaimed the cache value;
        // generated code will rebuild that slot on its next use.
        if !value.is_null() && !(unsafe { (api.vtableParent)(value) }).is_null() {
            cache.push(root);
        }
    }
}

fn candidate_mapped_bytes(candidate: &CandidateState) -> usize {
    fs::metadata(Path::new(OsStr::from_bytes(&candidate.author_path)))
        .ok()
        .and_then(|metadata| usize::try_from(metadata.len()).ok())
        .unwrap_or(0)
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum RuntimeRetirement {
    Rejected,
    Retired,
    Deferred,
}

unsafe fn retire_runtime_generation_v1(
    api: &IdApi,
    code_generation_identity: u64,
) -> RuntimeRetirement {
    let Some(address) =
        (unsafe { floor_symbol(api, b"birnam_id_retire_stackless_code_generation_v1\0") })
    else {
        return RuntimeRetirement::Rejected;
    };
    let retire: RetireStacklessCodeGenerationV1 = unsafe { core::mem::transmute(address) };
    match unsafe { retire(code_generation_identity) } {
        1 => RuntimeRetirement::Retired,
        2 => RuntimeRetirement::Deferred,
        _ => RuntimeRetirement::Rejected,
    }
}

unsafe fn reap_retained_generations(api: &IdApi) {
    let mut pending = RETAINED_GENERATIONS
        .lock()
        .ok()
        .map(|mut retained| core::mem::take(&mut *retained))
        .unwrap_or_default();
    let mut still_retained = Vec::new();
    for mut entry in pending.drain(..) {
        if !entry.runtime_retired {
            let cache_roots = take_vtable_cache_roots(entry.handle as u64);
            let _ = unsafe { (api.gcCollect)() };
            if !entry.runtime_retirement_eligible {
                unsafe { restore_live_vtable_cache_roots(api, cache_roots) };
                still_retained.push(entry);
                continue;
            }
            match unsafe { retire_runtime_generation_v1(api, entry.handle as u64) } {
                RuntimeRetirement::Retired => {}
                RuntimeRetirement::Deferred => {
                    still_retained.push(entry);
                    continue;
                }
                RuntimeRetirement::Rejected => {
                    unsafe { restore_live_vtable_cache_roots(api, cache_roots) };
                    still_retained.push(entry);
                    continue;
                }
            }
            entry.runtime_retired = true;
        }
        if unsafe { dlclose(entry.handle as *mut c_void) } == 0 {
            unsafe { release_rollback_leases_v1(api, &mut entry.rollback_leases) };
        } else {
            still_retained.push(entry);
        }
    }
    if let Ok(mut retained) = RETAINED_GENERATIONS.lock() {
        retained.extend(still_retained);
    }
}

unsafe fn dispose_or_retain_candidate(
    candidate: &mut CandidateState,
    mapped_bytes: usize,
) -> PhysicalDisposition {
    if candidate.handle == 0 {
        return PhysicalDisposition::Disposed;
    }
    if candidate.installed_method_ever {
        let api = unsafe { api(runtime()) };
        let _candidate_roots = take_candidate_roots(candidate.handle as u64);
        let cache_roots = take_vtable_cache_roots(candidate.handle as u64);
        let _ = unsafe { (api.gcCollect)() };
        let retirement = if candidate.all_installed_methods_resumable {
            unsafe { retire_runtime_generation_v1(api, candidate.handle as u64) }
        } else {
            RuntimeRetirement::Rejected
        };
        let runtime_retired = retirement == RuntimeRetirement::Retired;
        if runtime_retired && unsafe { dlclose(candidate.handle as *mut c_void) } == 0 {
            unsafe { release_rollback_leases_v1(api, &mut candidate.rollback_leases) };
            return PhysicalDisposition::Disposed;
        }
        if retirement == RuntimeRetirement::Rejected {
            unsafe { restore_live_vtable_cache_roots(api, cache_roots) };
        }
        if let Ok(mut retained) = RETAINED_GENERATIONS.lock() {
            retained.push(RetainedGeneration {
                unit: candidate.unit,
                handle: candidate.handle,
                mapped_bytes,
                runtime_retirement_eligible: candidate.all_installed_methods_resumable,
                runtime_retired,
                rollback_leases: core::mem::take(&mut candidate.rollback_leases),
            });
        }
        return PhysicalDisposition::Retained(if runtime_retired {
            b"disposalFailed\0"
        } else {
            b"codeGenerationPinned\0"
        });
    }
    // SAFETY: this mapping never installed a method and is detached from the
    // candidate registry, so no vtable or lifecycle callback can reach it.
    let _candidate_roots = take_candidate_roots(candidate.handle as u64);
    let cache_roots = take_vtable_cache_roots(candidate.handle as u64);
    if unsafe { dlclose(candidate.handle as *mut c_void) } == 0 {
        let api = unsafe { api(runtime()) };
        unsafe { release_rollback_leases_v1(api, &mut candidate.rollback_leases) };
        PhysicalDisposition::Disposed
    } else {
        let api = unsafe { api(runtime()) };
        unsafe { restore_live_vtable_cache_roots(api, cache_roots) };
        if let Ok(mut retained) = RETAINED_GENERATIONS.lock() {
            retained.push(RetainedGeneration {
                unit: candidate.unit,
                handle: candidate.handle,
                mapped_bytes,
                runtime_retirement_eligible: true,
                runtime_retired: true,
                rollback_leases: core::mem::take(&mut candidate.rollback_leases),
            });
        }
        PhysicalDisposition::Retained(b"disposalFailed\0")
    }
}

unsafe fn make_discard_receipt(
    api: &IdApi,
    generation: Oop,
    disposition: PhysicalDisposition,
) -> Oop {
    let runtime = runtime();
    let receipt = unsafe {
        (api.alloc)(
            runtime.discard_receipt_vt as Oop,
            size_of::<DiscardReceiptPayload>(),
        )
    };
    if receipt.is_null() {
        return api.nilObj;
    }
    let (physical, reason) = match disposition {
        PhysicalDisposition::Disposed => (unsafe { intern(api, b"disposed\0") }, api.nilObj),
        PhysicalDisposition::Retained(reason) => (unsafe { intern(api, b"retained\0") }, unsafe {
            intern(api, reason)
        }),
    };
    // SAFETY: receipt owns the exact four-Oop payload declared above.
    unsafe {
        receipt
            .cast::<DiscardReceiptPayload>()
            .write(DiscardReceiptPayload {
                generation,
                logical: intern(api, b"discarded\0"),
                physical,
                reason,
            })
    };
    receipt
}

unsafe fn discard_receipt_payload(receiver: Oop) -> Option<&'static DiscardReceiptPayload> {
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    if receiver.is_null()
        || unsafe { (api.vtableOf)(receiver) } != runtime.discard_receipt_vt as Oop
        || unsafe { (api.objectPayloadSize)(receiver) } != size_of::<DiscardReceiptPayload>()
    {
        return None;
    }
    // SAFETY: the canonical vtable and exact payload extent were validated.
    Some(unsafe { &*receiver.cast::<DiscardReceiptPayload>() })
}

unsafe extern "C" fn discard_receipt_generation(
    receiver: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    let api = unsafe { api(runtime()) };
    if count != 0 {
        return api.nilObj;
    }
    unsafe { discard_receipt_payload(receiver) }
        .map(|payload| payload.generation)
        .unwrap_or(api.nilObj)
}

unsafe extern "C" fn discard_receipt_logical(
    receiver: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    let api = unsafe { api(runtime()) };
    if count != 0 {
        return api.nilObj;
    }
    unsafe { discard_receipt_payload(receiver) }
        .map(|payload| payload.logical)
        .unwrap_or(api.nilObj)
}

unsafe extern "C" fn discard_receipt_physical(
    receiver: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    let api = unsafe { api(runtime()) };
    if count != 0 {
        return api.nilObj;
    }
    unsafe { discard_receipt_payload(receiver) }
        .map(|payload| payload.physical)
        .unwrap_or(api.nilObj)
}

unsafe extern "C" fn discard_receipt_reason(
    receiver: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    let api = unsafe { api(runtime()) };
    if count != 0 {
        return api.nilObj;
    }
    unsafe { discard_receipt_payload(receiver) }
        .map(|payload| payload.reason)
        .unwrap_or(api.nilObj)
}

unsafe extern "C" fn rust_codegen_retained_generation_count(
    _receiver: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    let api = unsafe { api(runtime()) };
    if count != 0 {
        return api.nilObj;
    }
    let value = RETAINED_GENERATIONS
        .lock()
        .ok()
        .map(|retained| retained.iter().filter(|entry| entry.handle != 0).count())
        .unwrap_or(0)
        .min(i64::MAX as usize) as i64;
    unsafe { (api.mkSmallInt)(value) }
}

unsafe extern "C" fn rust_codegen_retained_generation_bytes(
    _receiver: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    let api = unsafe { api(runtime()) };
    if count != 0 {
        return api.nilObj;
    }
    let value = RETAINED_GENERATIONS
        .lock()
        .ok()
        .map(|retained| {
            retained.iter().fold(0usize, |total, entry| {
                total.saturating_add(entry.mapped_bytes)
            })
        })
        .unwrap_or(0)
        .min(i64::MAX as usize) as i64;
    unsafe { (api.mkSmallInt)(value) }
}

unsafe fn floor_usize_diagnostic(api: &IdApi, symbol: &[u8]) -> Option<usize> {
    let address = unsafe { floor_symbol(api, symbol) }?;
    let read: unsafe extern "C" fn() -> usize = unsafe { core::mem::transmute(address) };
    Some(unsafe { read() })
}

unsafe extern "C" fn rust_codegen_live_vtable_count(
    _receiver: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    let api = unsafe { api(runtime()) };
    if count != 0 {
        return api.nilObj;
    }
    unsafe { (api.mkSmallInt)((api.vtableCount)().min(i64::MAX as usize) as i64) }
}

unsafe fn floor_diagnostic_method(count: usize, symbol: &[u8]) -> Oop {
    let api = unsafe { api(runtime()) };
    if count != 0 {
        return api.nilObj;
    }
    let Some(value) = (unsafe { floor_usize_diagnostic(api, symbol) }) else {
        return api.nilObj;
    };
    unsafe { (api.mkSmallInt)(value.min(i64::MAX as usize) as i64) }
}

unsafe extern "C" fn rust_codegen_live_symbol_count(
    _receiver: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    unsafe { floor_diagnostic_method(count, b"birnam_id_live_symbol_count_v1\0") }
}

unsafe extern "C" fn rust_codegen_live_symbol_name_bytes(
    _receiver: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    unsafe { floor_diagnostic_method(count, b"birnam_id_live_symbol_name_bytes_v1\0") }
}

unsafe extern "C" fn rust_codegen_live_heap_bytes(
    _receiver: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    unsafe { floor_diagnostic_method(count, b"birnam_id_live_heap_bytes_v1\0") }
}

unsafe extern "C" fn rust_codegen_discard_candidate(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustCodegen discardCandidate: received invalid arguments"));
    if values.len() != 1 {
        abort("RustCodegen discardCandidate: expects one unit");
    }
    let api = unsafe { api(runtime()) };
    let unit = values[0] as usize;
    let Some(mut candidate) = (|| {
        let mut candidates = CANDIDATES.lock().ok()?;
        let index = candidates
            .iter()
            .position(|candidate| candidate.unit == unit)?;
        if candidates[index].active || candidates[index].prepared != 0 {
            return None;
        }
        Some(candidates.remove(index))
    })() else {
        let blocked = CANDIDATES.lock().ok().is_some_and(|candidates| {
            candidates.iter().any(|candidate| {
                candidate.unit == unit && (candidate.active || candidate.prepared != 0)
            })
        });
        if blocked {
            note_error(b"RustCodegen candidate cannot be discarded while active or prepared");
        } else {
            note_error(b"RustCodegen candidate unit is unknown");
        }
        return api.nilObj;
    };
    let mapped_bytes = candidate_mapped_bytes(&candidate);
    let remove_file = |path: &[u8]| match fs::remove_file(Path::new(OsStr::from_bytes(path))) {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
        Err(_) => false,
    };
    let author_removed = remove_file(&candidate.author_path);
    let mut sidecar = candidate.author_path.clone();
    sidecar.extend_from_slice(b".candidate");
    let sidecar_removed = remove_file(&sidecar);
    if !author_removed || !sidecar_removed {
        if let Ok(mut candidates) = CANDIDATES.lock() {
            candidates.push(candidate);
        }
        note_error(b"RustCodegen candidate files could not be discarded");
        return api.nilObj;
    }
    if !(unsafe {
        assert_candidate_fact(
            api,
            runtime(),
            values[0],
            b"state\0",
            intern(api, b"discarded\0"),
        )
    }) {
        if let Ok(mut candidates) = CANDIDATES.lock() {
            candidates.push(candidate);
        }
        note_error(b"RustCodegen candidate Store is unavailable");
        return api.nilObj;
    }
    let _ = release_initializer_compensation(candidate.unit);
    let disposition = unsafe { dispose_or_retain_candidate(&mut candidate, mapped_bytes) };
    unsafe { make_discard_receipt(api, values[0], disposition) }
}

unsafe extern "C" fn rust_codegen_release_candidate(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustCodegen releaseCandidate: received invalid arguments"));
    if values.len() != 1 {
        abort("RustCodegen releaseCandidate: expects one unit");
    }
    let api = unsafe { api(runtime()) };
    let unit = values[0] as usize;
    let Some(mut candidate) = (|| {
        let mut candidates = CANDIDATES.lock().ok()?;
        let index = candidates
            .iter()
            .position(|candidate| candidate.unit == unit)?;
        if candidates[index].active || candidates[index].prepared != 0 {
            return None;
        }
        Some(candidates.remove(index))
    })() else {
        if let Ok(candidates) = CANDIDATES.lock()
            && candidates.iter().any(|candidate| {
                candidate.unit == unit && (candidate.active || candidate.prepared != 0)
            })
        {
            note_error(b"RustCodegen candidate cannot be released while active or prepared");
        }
        return if unit == 0 { api.nilObj } else { values[0] };
    };
    let mapped_bytes = candidate_mapped_bytes(&candidate);
    let _ = release_initializer_compensation(candidate.unit);
    let _ = unsafe { dispose_or_retain_candidate(&mut candidate, mapped_bytes) };
    let _ = unsafe {
        assert_candidate_fact(
            api,
            runtime(),
            values[0],
            b"state\0",
            intern(api, b"discarded\0"),
        )
    };
    values[0]
}

unsafe fn reflect_symbol_name(api: &IdApi, value: Oop) -> Option<Vec<u8>> {
    if value.is_null() || unsafe { (api.vtableOf)(value) } != api.symbol_vt {
        return None;
    }
    let pointer = unsafe { (api.symbolName)(value) };
    if pointer.is_null() {
        return None;
    }
    Some(unsafe { CStr::from_ptr(pointer) }.to_bytes().to_vec())
}

unsafe fn reflect_exported_symbol(api: &IdApi, value: Oop) -> Oop {
    if value.is_null() {
        return api.nilObj;
    }
    let name = unsafe { (api.exportedNameOf)(value) };
    if name.is_null() {
        return api.nilObj;
    }
    let name = unsafe { CStr::from_ptr(name) }.to_owned();
    unsafe { (api.intern)(name.as_ptr()) }
}

unsafe fn reflect_map_put(api: &IdApi, map: Oop, key: &[u8], value: Oop) -> Oop {
    let key = unsafe { intern(api, key) };
    unsafe { send(api, map, b"at:put:\0", &[key, value]) }
}

unsafe fn reflect_new_map(api: &IdApi) -> Oop {
    let map = unsafe { (api.import)(b"Map\0".as_ptr().cast()) };
    if map.is_null() || map == api.nilObj {
        return api.nilObj;
    }
    unsafe { send0(api, map, NEW_SELECTOR) }
}

unsafe fn reflect_method_source_for_target_selector(
    api: &IdApi,
    target: Oop,
    selector: Oop,
) -> Oop {
    let Some(target_bytes) = (unsafe { symbol_bytes(api, target) }) else {
        return api.nilObj;
    };
    let Some(selector_bytes) = (unsafe { symbol_bytes(api, selector) }) else {
        return api.nilObj;
    };
    let Some(sources) = (unsafe { ordered_generation_sources(api) }) else {
        return api.nilObj;
    };
    let mut found = api.nilObj;
    for source in sources {
        let Some((_source_bytes, metadata)) = (unsafe { parse_revision_metadata(api, source) })
        else {
            continue;
        };
        for operation in metadata.operations {
            if operation.install
                && operation.target == target_bytes
                && operation.selector == selector_bytes
            {
                found =
                    unsafe { (api.mkString)(operation.source.as_ptr(), operation.source.len()) };
            }
        }
    }
    found
}

unsafe fn reflect_method_target_name(api: &IdApi, vt: Oop, global_rows: &[ReflectFactRow]) -> Oop {
    let owner = unsafe { (api.vtableOwner)(vt) };
    if !owner.is_null() {
        let exported = unsafe { reflect_exported_symbol(api, owner) };
        if exported != api.nilObj {
            return exported;
        }
        if let Some(row) = global_rows.iter().find(|row| row.value == owner) {
            return row.attribute;
        }
    }
    let exported = unsafe { reflect_exported_symbol(api, vt) };
    if exported != api.nilObj {
        return exported;
    }
    global_rows
        .iter()
        .find(|row| row.value == vt)
        .map_or(api.nilObj, |row| row.attribute)
}

unsafe fn reflect_method_descriptor(
    api: &IdApi,
    runtime: &Runtime,
    vt: Oop,
    selector: Oop,
    target: Oop,
) -> Oop {
    let descriptor = unsafe { reflect_new_map(api) };
    if descriptor == api.nilObj {
        return api.nilObj;
    }
    let owner = unsafe { (api.vtableOwner)(vt) };
    let owner = if owner.is_null() { api.nilObj } else { owner };
    let mirror = unsafe {
        send(
            api,
            runtime.store as Oop,
            CURRENT_VALUE_SELECTOR,
            &[vt, selector],
        )
    };
    let method = if let Some(implementation) = current_code_binding_implementation(vt, selector) {
        implementation
    } else if mirror == api.nilObj {
        unsafe { intern(api, b"primitive\0") }
    } else if mirror == unsafe { intern(api, b"primitive\0") }
        || unsafe { (api.objectPayloadSize)(mirror) } != 0
    {
        mirror
    } else {
        unsafe { intern(api, b"compiled\0") }
    };
    let descriptor = unsafe { reflect_map_put(api, descriptor, b"selector\0", selector) };
    let descriptor = unsafe { reflect_map_put(api, descriptor, b"owner\0", owner) };
    let descriptor = unsafe { reflect_map_put(api, descriptor, b"target\0", target) };
    unsafe { reflect_map_put(api, descriptor, b"method\0", method) }
}

#[derive(Clone, Copy)]
struct ReflectFactRow {
    entity: Oop,
    attribute: Oop,
    value: Oop,
}

struct ReflectFactCollectorState {
    target: Oop,
    rows: Vec<ReflectFactRow>,
}

unsafe fn collect_reflect_facts(api: &IdApi, target: Oop) -> Vec<ReflectFactRow> {
    let runtime = runtime();
    let state = Box::new(ReflectFactCollectorState {
        target,
        rows: Vec::new(),
    });
    let state_pointer = Box::into_raw(state);
    let collector = unsafe { (api.alloc)(runtime.adapter_collector_vt as Oop, size_of::<usize>()) };
    if collector.is_null() {
        // SAFETY: no Store call was made, so ownership remains local.
        unsafe { drop(Box::from_raw(state_pointer)) };
        return Vec::new();
    }
    // The collector callback runs synchronously inside Store.  Disable
    // automatic collection for the entire call so the raw state pointer and
    // the Oops copied into its rows cannot be invalidated between callbacks.
    unsafe { *(collector as *mut usize) = state_pointer as usize };
    let _ = unsafe { send(api, runtime.store as Oop, b"liveTriplesDo:\0", &[collector]) };
    let state = unsafe { Box::from_raw(state_pointer) };
    state.rows
}

unsafe extern "C" fn rust_reflect_vtable_of(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustReflect vtableOf: received invalid arguments"));
    if values.len() != 1 {
        abort("RustReflect vtableOf: expects one value");
    }
    unsafe { (api(runtime()).vtableOf)(values[0]) }
}

unsafe extern "C" fn rust_reflect_name_of(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustReflect nameOf: received invalid arguments"));
    if values.len() != 1 {
        abort("RustReflect nameOf: expects one value");
    }
    let api = unsafe { api(runtime()) };
    let Some(name) = (unsafe { reflect_symbol_name(api, values[0]) }) else {
        note_error(b"RustReflect nameOf: expects a symbol");
        let error = unsafe { (api.import)(c"Error".as_ptr()) };
        if error.is_null() || error == api.nilObj {
            return api.nilObj;
        }
        let kind = unsafe { intern(api, b"notASymbol\0") };
        let message = unsafe {
            (api.mkString)(
                b"nameOf: expects a symbol".as_ptr(),
                b"nameOf: expects a symbol".len(),
            )
        };
        return unsafe { send(api, error, b"kind:message:\0", &[kind, message]) };
    };
    unsafe { (api.mkString)(name.as_ptr(), name.len()) }
}

unsafe extern "C" fn rust_reflect_global_named(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustReflect globalNamed: received invalid arguments"));
    if values.len() != 1 {
        abort("RustReflect globalNamed: expects one symbol");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    let Some(name) = (unsafe { reflect_symbol_name(api, values[0]) }) else {
        note_error(b"RustReflect globalNamed: expects a symbol");
        return api.nilObj;
    };
    let name =
        CString::new(name).unwrap_or_else(|_| abort("Symbol name unexpectedly contained NUL"));
    let imported = unsafe { (api.import)(name.as_ptr()) };
    if !imported.is_null() && imported != api.nilObj {
        return imported;
    }
    unsafe { send(api, runtime.globals as Oop, b"at:\0", &[values[0]]) }
}

unsafe extern "C" fn rust_reflect_all_global_names(
    _self_object: Oop,
    _pointer: *const Oop,
    count: usize,
) -> Oop {
    let api = unsafe { api(runtime()) };
    if count != 0 {
        return api.nilObj;
    }
    let Some(count_address) = (unsafe { floor_symbol(api, b"birnam_id_export_count_v1\0") }) else {
        return api.nilObj;
    };
    let Some(name_address) = (unsafe { floor_symbol(api, b"birnam_id_export_name_at_v1\0") })
    else {
        return api.nilObj;
    };
    let export_count: unsafe extern "C" fn() -> usize =
        unsafe { core::mem::transmute(count_address) };
    let export_name_at: unsafe extern "C" fn(usize) -> Oop =
        unsafe { core::mem::transmute(name_address) };
    let vector = unsafe { (api.import)(b"Vector\0".as_ptr().cast()) };
    if vector.is_null() || vector == api.nilObj {
        return api.nilObj;
    }
    unsafe { (api.gcSetAuto)(0) };
    let names = (0..unsafe { export_count() })
        .filter_map(|index| {
            let name = unsafe { export_name_at(index) };
            (!name.is_null()).then_some(name)
        })
        .collect::<Vec<_>>();
    let result = unsafe { make_vector(api, vector, &names) };
    unsafe { (api.gcSetAuto)(1) };
    result
}

unsafe extern "C" fn rust_reflect_export_vtable(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustReflect exportVtableOf:as: received invalid arguments"));
    if values.len() != 2 {
        abort("RustReflect exportVtableOf:as: expects value and symbol");
    }
    let api = unsafe { api(runtime()) };
    let Some(name) = (unsafe { reflect_symbol_name(api, values[1]) }) else {
        note_error(b"RustReflect exportVtableOf:as: expects a symbol");
        return api.nilObj;
    };
    let name =
        CString::new(name).unwrap_or_else(|_| abort("Symbol name unexpectedly contained NUL"));
    let vt = unsafe { (api.vtableOf)(values[0]) };
    if vt.is_null() || unsafe { (api.vtableOwner)(vt) } != values[0] {
        note_error(b"RustReflect exportVtableOf:as: target has no stable vtable");
        return api.nilObj;
    }
    if unsafe { (api.isExported)(name.as_ptr()) } != 0 {
        return if unsafe { (api.import)(name.as_ptr()) } == vt {
            values[0]
        } else {
            note_error(b"RustReflect exportVtableOf:as: name conflict");
            api.nilObj
        };
    }
    if unsafe { (api.exportObj)(name.as_ptr(), vt) }.is_null() {
        note_error(b"RustReflect exportVtableOf:as: export failed");
        return api.nilObj;
    }
    values[0]
}

unsafe extern "C" fn rust_reflect_export_object(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustReflect exportObject:as: received invalid arguments"));
    if values.len() != 2 {
        abort("RustReflect exportObject:as: expects value and symbol");
    }
    let api = unsafe { api(runtime()) };
    let Some(name) = (unsafe { reflect_symbol_name(api, values[1]) }) else {
        note_error(b"RustReflect exportObject:as: expects a symbol");
        return api.nilObj;
    };
    let name =
        CString::new(name).unwrap_or_else(|_| abort("Symbol name unexpectedly contained NUL"));
    if unsafe { (api.isExported)(name.as_ptr()) } != 0 {
        return if unsafe { (api.import)(name.as_ptr()) } == values[0] {
            values[0]
        } else {
            note_error(b"RustReflect exportObject:as: name conflict");
            api.nilObj
        };
    }
    if unsafe { (api.exportObj)(name.as_ptr(), values[0]) }.is_null() {
        note_error(b"RustReflect exportObject:as: export failed");
        return api.nilObj;
    }
    values[0]
}

unsafe fn reflect_install_method(
    api: &IdApi,
    selector: Oop,
    target: Oop,
    implementation: Oop,
    provenance: Oop,
) -> Oop {
    let _ = unsafe { reap_code_bindings(api) };
    let Some(selector_bytes) = (unsafe { reflect_symbol_name(api, selector) }) else {
        note_error(b"RustReflect defineMethod: expects a selector symbol");
        return api.nilObj;
    };
    let vt = unsafe { (api.ensurePrivateVtable)(target) };
    if vt.is_null() {
        note_error(b"RustReflect defineMethod: target has no vtable");
        return api.nilObj;
    }
    let Some((unit, method)) = (unsafe {
        compile_portable_named_materialization(
            api,
            target,
            vt,
            selector,
            &selector_bytes,
            implementation,
            provenance,
        )
    }) else {
        note_error(b"RustReflect defineMethod: portable NamedAdapter materialization failed");
        return api.nilObj;
    };
    let result = unsafe {
        send(
            api,
            runtime().store as Oop,
            b"addMethod:on:with:\0",
            &[selector, vt, method],
        )
    };
    if result == api.nilObj {
        let generation = retire_staged_code_binding(unit)
            .unwrap_or_else(|| abort("published NamedAdapter lost its staged CodeBinding"));
        unsafe { request_code_binding_retirements(api, &[generation]) };
        note_error(b"RustReflect defineMethod: Store rejected method");
        return api.nilObj;
    }
    let predecessors = publish_staged_code_binding(unit)
        .unwrap_or_else(|| abort("published NamedAdapter lost its staged CodeBinding"));
    unsafe { request_code_binding_retirements(api, &predecessors) };
    vt
}

unsafe extern "C" fn rust_reflect_define_method(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustReflect defineMethod:on:as: received invalid arguments"));
    if values.len() != 3 {
        abort("RustReflect defineMethod:on:as: expects selector, target, and block");
    }
    let api = unsafe { api(runtime()) };
    unsafe { reflect_install_method(api, values[0], values[1], values[2], api.nilObj) }
}

unsafe extern "C" fn rust_reflect_remove_method(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustReflect removeMethod:on: received invalid arguments"));
    if values.len() != 2 {
        abort("RustReflect removeMethod:on: expects selector and target");
    }
    let result = unsafe { birnam_rust_codegen_remove_method(values[1], values[0], ptr::null(), 0) };
    if result == unsafe { api(runtime()).nilObj } {
        note_error(b"RustReflect removeMethod:on: Store rejected method");
    } else {
        let api = unsafe { api(runtime()) };
        let generations = withdraw_current_code_binding(values[1], values[0]);
        unsafe { request_code_binding_retirements(api, &generations) };
    }
    result
}

unsafe extern "C" fn rust_reflect_define_method_sourced(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustReflect sourced define received invalid arguments"));
    if values.len() != 4 {
        abort("RustReflect defineMethod:on:as:source: expects four arguments");
    }
    let api = unsafe { api(runtime()) };
    unsafe { reflect_install_method(api, values[0], values[1], values[2], values[3]) }
}

unsafe extern "C" fn rust_reflect_define_method_doc_sourced(
    self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustReflect doc define received invalid arguments"));
    if values.len() != 5 {
        abort("RustReflect defineMethod:on:as:doc:source: expects five arguments");
    }
    let api = unsafe { api(runtime()) };
    // Preserve the late-bound documentation hook. The source adapter sends
    // the four-argument form so the Birnam stdlib can retain the receiver
    // prototype and docstring rather than silently dropping the doc payload.
    let result = unsafe { send(api, self_object, b"defineMethod:on:as:doc:\0", &values[..4]) };
    if result != api.nilObj {
        let _ = update_current_code_binding_provenance(values[1], values[0], values[4]);
    }
    result
}

unsafe extern "C" fn rust_reflect_remove_method_sourced(
    self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustReflect sourced remove received invalid arguments"));
    if values.len() != 3 {
        abort("RustReflect removeMethod:on:source: expects three arguments");
    }
    unsafe { rust_reflect_remove_method(self_object, values.as_ptr(), 2) }
}

unsafe extern "C" fn rust_reflect_all_methods(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustReflect allMethodsOf: received invalid arguments"));
    if values.len() != 1 {
        abort("RustReflect allMethodsOf: expects one value");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    unsafe { (api.gcSetAuto)(0) };
    let result = (|| {
        let vector = unsafe { (api.import)(b"Vector\0".as_ptr().cast()) };
        if vector.is_null() || vector == api.nilObj {
            return api.nilObj;
        }
        let globals = unsafe { intern(api, GLOBALS_ENTITY_NAME) };
        let global_rows = unsafe { collect_reflect_facts(api, globals) };
        let mut entries = Vec::new();
        let mut seen = Vec::<Oop>::new();
        let mut vt = unsafe { (api.vtableOf)(values[0]) };
        while !vt.is_null() {
            let selector_count = 0..;
            for index in selector_count {
                let selector = unsafe { (api.vtableSelectorAt)(vt, index) };
                if selector.is_null() {
                    break;
                }
                if seen.contains(&selector) {
                    continue;
                }
                seen.push(selector);
                let target = unsafe { reflect_method_target_name(api, vt, &global_rows) };
                entries
                    .push(unsafe { reflect_method_descriptor(api, runtime, vt, selector, target) });
            }
            vt = unsafe { (api.vtableParent)(vt) };
        }
        unsafe { make_vector(api, vector, &entries) }
    })();
    unsafe { (api.gcSetAuto)(1) };
    result
}

unsafe extern "C" fn rust_reflect_method_source(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustReflect methodSource: received invalid arguments"));
    if values.len() != 1 {
        abort("RustReflect methodSource: expects one descriptor");
    }
    let api = unsafe { api(runtime()) };
    let source = unsafe { send(api, values[0], b"at:\0", &[intern(api, b"source\0")]) };
    if source != api.nilObj {
        return source;
    }
    let target = unsafe { send(api, values[0], b"at:\0", &[intern(api, b"target\0")]) };
    let selector = unsafe { send(api, values[0], b"at:\0", &[intern(api, b"selector\0")]) };
    if target == api.nilObj || selector == api.nilObj {
        return api.nilObj;
    }
    unsafe { reflect_method_source_for_target_selector(api, target, selector) }
}

unsafe extern "C" fn rust_reflect_facts_of(
    _self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustReflect factsOf: received invalid arguments"));
    if values.len() != 1 {
        abort("RustReflect factsOf: expects one entity");
    }
    let runtime = runtime();
    let api = unsafe { api(runtime) };
    unsafe { (api.gcSetAuto)(0) };
    let result = (|| {
        let vector = unsafe { (api.import)(b"Vector\0".as_ptr().cast()) };
        if vector.is_null() || vector == api.nilObj {
            return api.nilObj;
        }
        let mut facts = Vec::new();
        for row in unsafe { collect_reflect_facts(api, values[0]) } {
            let map = unsafe { reflect_new_map(api) };
            let map = unsafe { reflect_map_put(api, map, b"entity\0", row.entity) };
            let map = unsafe { reflect_map_put(api, map, b"attribute\0", row.attribute) };
            let map = unsafe { reflect_map_put(api, map, b"value\0", row.value) };
            facts.push(map);
        }
        unsafe { make_vector(api, vector, &facts) }
    })();
    unsafe { (api.gcSetAuto)(1) };
    result
}

unsafe extern "C" fn rust_reflect_collect_fact(
    self_object: Oop,
    pointer: *const Oop,
    count: usize,
) -> Oop {
    if count != 3 {
        abort("RustReflectFactCollector value:value:value: expects three arguments");
    }
    let values = unsafe { borrowed_oop_args(pointer, count) }
        .unwrap_or_else(|| abort("RustReflectFactCollector received invalid arguments"));
    if self_object.is_null() {
        return unsafe { api(runtime()) }.nilObj;
    }
    // SAFETY: collect_reflect_facts installs a valid state pointer in the raw
    // collector slot for the synchronous Store traversal.
    let state_pointer = unsafe { *(self_object as *const usize) };
    if state_pointer == 0 {
        return unsafe { api(runtime()) }.nilObj;
    }
    let state = unsafe { &mut *(state_pointer as *mut ReflectFactCollectorState) };
    if values[0] == state.target {
        state.rows.push(ReflectFactRow {
            entity: values[0],
            attribute: values[1],
            value: values[2],
        });
    }
    unsafe { api(runtime()) }.nilObj
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
        let adapter_reloading =
            unsafe { (host_api.isExported)(ADAPTER_CODEGEN_NAME.as_ptr().cast()) != 0 };
        let (adapter_codegen_vt, adapter_codegen) = if adapter_reloading {
            (
                unsafe { (host_api.import)(ADAPTER_CODEGEN_VT_NAME.as_ptr().cast()) },
                unsafe { (host_api.import)(ADAPTER_CODEGEN_NAME.as_ptr().cast()) },
            )
        } else {
            let vt = unsafe { (host_api.proto)(host_api.object_vt) };
            let object = unsafe { (host_api.alloc)(vt, 0) };
            (vt, object)
        };
        let (adapter_reflect_vt, adapter_reflect) = if adapter_reloading {
            (
                unsafe { (host_api.import)(ADAPTER_REFLECT_VT_NAME.as_ptr().cast()) },
                unsafe { (host_api.import)(ADAPTER_REFLECT_NAME.as_ptr().cast()) },
            )
        } else {
            let vt = unsafe { (host_api.proto)(host_api.object_vt) };
            let object = unsafe { (host_api.alloc)(vt, 0) };
            (vt, object)
        };
        let adapter_collector_vt = if adapter_reloading {
            unsafe { (host_api.import)(ADAPTER_COLLECTOR_VT_NAME.as_ptr().cast()) }
        } else {
            unsafe { (host_api.proto)(host_api.object_vt) }
        };
        let receipt_reloading =
            unsafe { (host_api.isExported)(DISCARD_RECEIPT_NAME.as_ptr().cast()) != 0 };
        let (discard_receipt_vt, discard_receipt) = if receipt_reloading {
            (
                unsafe { (host_api.import)(DISCARD_RECEIPT_VT_NAME.as_ptr().cast()) },
                unsafe { (host_api.import)(DISCARD_RECEIPT_NAME.as_ptr().cast()) },
            )
        } else {
            let vt = unsafe { (host_api.proto)(host_api.object_vt) };
            let object = unsafe { (host_api.alloc)(vt, 0) };
            (vt, object)
        };
        let portable_mapped_unit_vt = if unsafe {
            (host_api.isExported)(PORTABLE_MAPPED_UNIT_VT_NAME.as_ptr().cast()) != 0
        } {
            unsafe { (host_api.import)(PORTABLE_MAPPED_UNIT_VT_NAME.as_ptr().cast()) }
        } else {
            unsafe { (host_api.proto)(host_api.object_vt) }
        };
        if adapter_codegen_vt.is_null()
            || adapter_codegen.is_null()
            || adapter_reflect_vt.is_null()
            || adapter_reflect.is_null()
            || adapter_collector_vt.is_null()
            || discard_receipt_vt.is_null()
            || discard_receipt.is_null()
            || portable_mapped_unit_vt.is_null()
        {
            abort("RustCodegen adapter exports were incomplete");
        }
        unsafe {
            (host_api.setLayout)(adapter_collector_vt, ptr::null(), 0, birnam_abi::SLOT_RAW);
            (host_api.setLayout)(discard_receipt_vt, ptr::null(), 0, birnam_abi::SLOT_OOP);
            (host_api.setLayout)(portable_mapped_unit_vt, ptr::null(), 0, SLOT_RAW);
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
            adapter_codegen_vt: adapter_codegen_vt as usize,
            adapter_codegen: adapter_codegen as usize,
            adapter_reflect_vt: adapter_reflect_vt as usize,
            adapter_reflect: adapter_reflect as usize,
            adapter_collector_vt: adapter_collector_vt as usize,
            discard_receipt_vt: discard_receipt_vt as usize,
            discard_receipt: discard_receipt as usize,
            portable_mapped_unit_vt: portable_mapped_unit_vt as usize,
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
        (api.exportObj)(
            PORTABLE_MAPPED_UNIT_VT_NAME.as_ptr().cast(),
            runtime.portable_mapped_unit_vt as Oop,
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
            intern(api, INITIALIZE_CURRENT_GENERATION_SELECTOR),
            rust_codegen_initialize_current_generation,
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
            intern(api, COMPILE_IMAGE_CODE_MANIFEST_SELECTOR),
            rust_codegen_compile_image_code,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, REBUILD_IMAGE_CODE_SELECTOR),
            rust_codegen_rebuild_image_code,
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
            intern(api, ADOPT_COMPOSED_GENERATION_SELECTOR),
            rust_codegen_adopt_composed_generation,
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
            intern(api, MATERIALIZE_SYNCHRONOUS_SELECTOR),
            rust_codegen_materialize_synchronous,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, MATERIALIZE_RESUMABLE_SELECTOR),
            rust_codegen_materialize_resumable,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, MATERIALIZATION_COUNT_SELECTOR),
            rust_codegen_materialization_count,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, EVICT_MATERIALIZATIONS_SELECTOR),
            rust_codegen_evict_materializations,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, CURRENT_CODE_BINDING_COUNT_SELECTOR),
            rust_codegen_current_code_binding_count,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, RETIRING_CODE_BINDING_COUNT_SELECTOR),
            rust_codegen_retiring_code_binding_count,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, CODE_BINDING_GENERATED_BYTES_SELECTOR),
            rust_codegen_code_binding_generated_bytes,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, REAP_CODE_BINDINGS_SELECTOR),
            rust_codegen_reap_code_bindings,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, PORTABLE_IMAGE_BINDINGS_SELECTOR),
            rust_codegen_portable_image_bindings,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, COMPILE_PORTABLE_IMAGE_CODE_SELECTOR),
            rust_codegen_compile_portable_image_code,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, BEGIN_PORTABLE_IMAGE_REBUILD_SELECTOR),
            rust_codegen_begin_portable_image_rebuild,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, BEGIN_PORTABLE_IMAGE_MAPPED_SELECTOR),
            rust_codegen_begin_portable_image_mapped,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, PORTABLE_IMAGE_BLOCK_VTABLE_SELECTOR),
            rust_codegen_portable_image_block_vtable,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, PREPARE_PORTABLE_IMAGE_BINDING_SELECTOR),
            rust_codegen_prepare_portable_image_binding,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, FINISH_PORTABLE_IMAGE_UNIT_SELECTOR),
            rust_codegen_finish_portable_image_unit,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, CANCEL_PORTABLE_IMAGE_UNIT_SELECTOR),
            rust_codegen_cancel_portable_image_unit,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, INSTALL_PORTABLE_IMAGE_BINDING_SELECTOR),
            rust_codegen_install_portable_image_binding,
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
            intern(api, COMPILE_OUTPUT_SELECTION_SELECTOR),
            rust_codegen_compile_output_selection,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, COMPILE_INITIAL_MODULE_SELECTION_SELECTOR),
            rust_codegen_compile_initial_module_selection,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, COMPILE_CANDIDATE_SELECTOR),
            rust_codegen_compile_candidate,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, COMPILE_CANDIDATE_SELECTION_SELECTOR),
            rust_codegen_compile_candidate_selection,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, LOAD_SELECTOR),
            rust_codegen_load,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, LOAD_INITIAL_MODULE_SELECTOR),
            rust_codegen_load_initial_module,
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
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, RETAINED_GENERATION_COUNT_SELECTOR),
            rust_codegen_retained_generation_count,
        );
        (api.method)(
            runtime.rust_codegen_vt as Oop,
            intern(api, RETAINED_GENERATION_BYTES_SELECTOR),
            rust_codegen_retained_generation_bytes,
        );
        for (selector, method) in [
            (
                LIVE_VTABLE_COUNT_SELECTOR,
                rust_codegen_live_vtable_count as birnam_abi::Method,
            ),
            (LIVE_SYMBOL_COUNT_SELECTOR, rust_codegen_live_symbol_count),
            (
                LIVE_SYMBOL_NAME_BYTES_SELECTOR,
                rust_codegen_live_symbol_name_bytes,
            ),
            (LIVE_HEAP_BYTES_SELECTOR, rust_codegen_live_heap_bytes),
        ] {
            (api.method)(
                runtime.rust_codegen_vt as Oop,
                intern(api, selector),
                method,
            );
        }
        for (selector, method) in [
            (
                b"generation\0".as_slice(),
                discard_receipt_generation as birnam_abi::Method,
            ),
            (
                b"logical\0".as_slice(),
                discard_receipt_logical as birnam_abi::Method,
            ),
            (
                b"physical\0".as_slice(),
                discard_receipt_physical as birnam_abi::Method,
            ),
            (
                b"reason\0".as_slice(),
                discard_receipt_reason as birnam_abi::Method,
            ),
        ] {
            (api.method)(
                runtime.discard_receipt_vt as Oop,
                intern(api, selector),
                method,
            );
        }
        macro_rules! adapter_method {
            ($selector:expr, $method:expr) => {
                (api.method)(
                    runtime.adapter_codegen_vt as Oop,
                    intern(api, $selector),
                    $method,
                );
            };
        }
        // Exact frozen Codegen selector projection.  Rust-only conveniences
        // (emit:, compile:, image preflight helpers, and phase aliases) remain
        // on RustCodegen and are deliberately not copied into this vtable.
        adapter_method!(b"eval:\0", rust_codegen_eval);
        adapter_method!(b"evalPersisting:\0", rust_codegen_eval_persisting);
        adapter_method!(b"eval:from:\0", rust_codegen_eval);
        adapter_method!(b"evalPersisting:from:\0", rust_codegen_eval_persisting);
        adapter_method!(
            MATERIALIZE_SYNCHRONOUS_SELECTOR,
            rust_codegen_materialize_synchronous
        );
        adapter_method!(
            MATERIALIZE_RESUMABLE_SELECTOR,
            rust_codegen_materialize_resumable
        );
        adapter_method!(
            MATERIALIZATION_COUNT_SELECTOR,
            rust_codegen_materialization_count
        );
        adapter_method!(
            EVICT_MATERIALIZATIONS_SELECTOR,
            rust_codegen_evict_materializations
        );
        adapter_method!(
            CURRENT_CODE_BINDING_COUNT_SELECTOR,
            rust_codegen_current_code_binding_count
        );
        adapter_method!(
            RETIRING_CODE_BINDING_COUNT_SELECTOR,
            rust_codegen_retiring_code_binding_count
        );
        adapter_method!(
            CODE_BINDING_GENERATED_BYTES_SELECTOR,
            rust_codegen_code_binding_generated_bytes
        );
        adapter_method!(REAP_CODE_BINDINGS_SELECTOR, rust_codegen_reap_code_bindings);
        adapter_method!(b"compile:output:\0", rust_codegen_compile_output);
        adapter_method!(b"compile:output:from:\0", rust_codegen_compile_output);
        adapter_method!(b"compile:output:root:\0", rust_codegen_compile_output_root);
        adapter_method!(
            COMPILE_OUTPUT_SELECTION_SELECTOR,
            rust_codegen_compile_output_selection
        );
        adapter_method!(
            COMPILE_INITIAL_MODULE_SELECTION_SELECTOR,
            rust_codegen_compile_initial_module_selection
        );
        adapter_method!(SOURCE_REVISION_SELECTOR, rust_codegen_source_revision);
        adapter_method!(MODULE_METADATA_SELECTOR, rust_codegen_module_metadata);
        adapter_method!(EMBEDDED_FILES_SELECTOR, rust_codegen_embedded_files);
        adapter_method!(
            RECORD_MODULE_SOURCE_SELECTOR,
            rust_codegen_record_module_source
        );
        adapter_method!(
            INITIAL_GENERATION_PROJECTION_SELECTOR,
            rust_codegen_initial_generation_projection
        );
        adapter_method!(
            SEAL_INITIAL_GENERATION_SELECTOR,
            rust_codegen_seal_initial_generation
        );
        adapter_method!(
            SUCCESSOR_GENERATION_PROJECTION_SELECTOR,
            rust_codegen_successor_generation_projection
        );
        adapter_method!(MODULE_SOURCE_FOR_SELECTOR, rust_codegen_module_source_for);
        adapter_method!(COMPILE_CANDIDATE_SELECTOR, rust_codegen_compile_candidate);
        adapter_method!(
            COMPILE_CANDIDATE_SELECTION_SELECTOR,
            rust_codegen_compile_candidate_selection
        );
        adapter_method!(INSPECT_CANDIDATE_SELECTOR, rust_codegen_inspect_candidate);
        adapter_method!(MAP_CANDIDATE_SELECTOR, rust_codegen_map_candidate);
        adapter_method!(CANDIDATE_MAPPED_SELECTOR, rust_codegen_candidate_mapped);
        adapter_method!(
            CANDIDATE_CONTENT_PATH_SELECTOR,
            rust_codegen_candidate_content_path
        );
        adapter_method!(b"discardCandidate:\0", rust_codegen_discard_candidate);
        adapter_method!(b"releaseCandidate:\0", rust_codegen_release_candidate);
        adapter_method!(PREPARE_ACTIVATION_SELECTOR, rust_codegen_prepare_activation);
        adapter_method!(
            PREPARE_ACTIVATION_PRIOR_SELECTOR,
            rust_codegen_prepare_activation
        );
        adapter_method!(
            PREPARE_ACTIVATION_PLAN_SELECTOR,
            rust_codegen_prepare_activation
        );
        adapter_method!(COMMIT_PREPARED_SELECTOR, rust_codegen_commit_prepared);
        adapter_method!(PREPARE_ROLLBACK_SELECTOR, rust_codegen_prepare_rollback);
        adapter_method!(
            PREPARE_ROLLBACK_PLAN_SELECTOR,
            rust_codegen_prepare_rollback
        );
        adapter_method!(
            COMMIT_PREPARED_ROLLBACK_SELECTOR,
            rust_codegen_commit_prepared_rollback
        );
        adapter_method!(DISCARD_PREPARED_SELECTOR, rust_codegen_discard_prepared);
        adapter_method!(
            RECONCILE_CANDIDATE_SELECTOR,
            rust_codegen_reconcile_candidate
        );
        adapter_method!(
            RECONCILE_ALL_CANDIDATES_SELECTOR,
            rust_codegen_reconcile_all_candidates
        );
        adapter_method!(
            ADOPT_COMPOSED_GENERATION_SELECTOR,
            rust_codegen_adopt_composed_generation
        );
        adapter_method!(
            MANAGED_METHOD_CHANGED_SELECTOR,
            rust_codegen_managed_method_changed
        );
        adapter_method!(
            AUTHORIZE_METHOD_MUTATION_SELECTOR,
            rust_codegen_authorize_method_mutation_method
        );
        adapter_method!(
            BEGIN_IMAGE_RECOVERY_SELECTOR,
            rust_codegen_begin_image_dispatch_recovery
        );
        adapter_method!(
            END_IMAGE_RECOVERY_SELECTOR,
            rust_codegen_end_image_dispatch_recovery
        );
        adapter_method!(COMPILE_IMAGE_CODE_SELECTOR, rust_codegen_compile_image_code);
        adapter_method!(
            COMPILE_IMAGE_CODE_MANIFEST_SELECTOR,
            rust_codegen_compile_image_code
        );
        adapter_method!(REBUILD_IMAGE_CODE_SELECTOR, rust_codegen_rebuild_image_code);
        adapter_method!(GENERATION_SOURCES_SELECTOR, rust_codegen_generation_sources);
        adapter_method!(
            GENERATION_METHOD_SOURCES_SELECTOR,
            rust_codegen_generation_method_sources
        );
        adapter_method!(
            CURRENT_GENERATION_DIGEST_SELECTOR,
            rust_codegen_current_generation_digest
        );
        adapter_method!(REPLAY_ALIENS_SELECTOR, rust_codegen_replay_aliens);
        adapter_method!(
            DERIVE_IMAGE_GLOBALS_SELECTOR,
            rust_codegen_derive_image_globals
        );
        adapter_method!(b"lastError\0", rust_codegen_last_error);
        adapter_method!(
            RETAINED_GENERATION_COUNT_SELECTOR,
            rust_codegen_retained_generation_count
        );
        adapter_method!(
            RETAINED_GENERATION_BYTES_SELECTOR,
            rust_codegen_retained_generation_bytes
        );
        adapter_method!(LIVE_VTABLE_COUNT_SELECTOR, rust_codegen_live_vtable_count);
        adapter_method!(LIVE_SYMBOL_COUNT_SELECTOR, rust_codegen_live_symbol_count);
        adapter_method!(
            LIVE_SYMBOL_NAME_BYTES_SELECTOR,
            rust_codegen_live_symbol_name_bytes
        );
        adapter_method!(LIVE_HEAP_BYTES_SELECTOR, rust_codegen_live_heap_bytes);
        adapter_method!(LOAD_SELECTOR, rust_codegen_load);
        adapter_method!(
            LOAD_INITIAL_MODULE_SELECTOR,
            rust_codegen_load_initial_module
        );
        adapter_method!(
            LOAD_HOST_CAPABILITY_SELECTOR,
            rust_codegen_load_host_capability
        );

        macro_rules! reflect_method {
            ($selector:expr, $method:expr) => {
                (api.method)(
                    runtime.adapter_reflect_vt as Oop,
                    intern(api, $selector),
                    $method,
                );
            };
        }
        reflect_method!(b"defineMethod:on:as:\0", rust_reflect_define_method);
        reflect_method!(b"removeMethod:on:\0", rust_reflect_remove_method);
        reflect_method!(
            b"removeMethod:on:source:\0",
            rust_reflect_remove_method_sourced
        );
        reflect_method!(
            b"defineMethod:on:as:source:\0",
            rust_reflect_define_method_sourced
        );
        reflect_method!(
            b"defineMethod:on:as:doc:source:\0",
            rust_reflect_define_method_doc_sourced
        );
        reflect_method!(b"vtableOf:\0", rust_reflect_vtable_of);
        reflect_method!(b"exportVtableOf:as:\0", rust_reflect_export_vtable);
        reflect_method!(b"exportObject:as:\0", rust_reflect_export_object);
        reflect_method!(b"nameOf:\0", rust_reflect_name_of);
        reflect_method!(b"globalNamed:\0", rust_reflect_global_named);
        reflect_method!(b"allGlobalNames\0", rust_reflect_all_global_names);
        reflect_method!(b"allMethodsOf:\0", rust_reflect_all_methods);
        reflect_method!(b"methodSource:\0", rust_reflect_method_source);
        reflect_method!(b"factsOf:\0", rust_reflect_facts_of);
        (api.method)(
            runtime.adapter_collector_vt as Oop,
            intern(api, b"value:value:value:\0"),
            rust_reflect_collect_fact,
        );
        let _ = (api.exportObj)(
            ADAPTER_CODEGEN_VT_NAME.as_ptr().cast(),
            runtime.adapter_codegen_vt as Oop,
        );
        let _ = (api.exportObj)(
            ADAPTER_CODEGEN_NAME.as_ptr().cast(),
            runtime.adapter_codegen as Oop,
        );
        let _ = (api.exportObj)(
            ADAPTER_REFLECT_VT_NAME.as_ptr().cast(),
            runtime.adapter_reflect_vt as Oop,
        );
        let _ = (api.exportObj)(
            ADAPTER_REFLECT_NAME.as_ptr().cast(),
            runtime.adapter_reflect as Oop,
        );
        let _ = (api.exportObj)(
            ADAPTER_COLLECTOR_VT_NAME.as_ptr().cast(),
            runtime.adapter_collector_vt as Oop,
        );
        if (api.isExported)(DISCARD_RECEIPT_NAME.as_ptr().cast()) == 0 {
            let _ = (api.exportObj)(
                DISCARD_RECEIPT_VT_NAME.as_ptr().cast(),
                runtime.discard_receipt_vt as Oop,
            );
            let _ = (api.exportObj)(
                DISCARD_RECEIPT_NAME.as_ptr().cast(),
                runtime.discard_receipt as Oop,
            );
        }
        if canonical_exports_requested() {
            for (name, value) in [
                (CANONICAL_CODEGEN_VT_NAME, runtime.adapter_codegen_vt as Oop),
                (CANONICAL_CODEGEN_NAME, runtime.adapter_codegen as Oop),
                (CANONICAL_REFLECT_VT_NAME, runtime.adapter_reflect_vt as Oop),
                (CANONICAL_REFLECT_NAME, runtime.adapter_reflect as Oop),
                (
                    CANONICAL_COLLECTOR_VT_NAME,
                    runtime.adapter_collector_vt as Oop,
                ),
            ] {
                if (api.isExported)(name.as_ptr().cast()) != 0 {
                    if (api.import)(name.as_ptr().cast()) != value {
                        abort("RustCodegen canonical export conflicts with an existing authority");
                    }
                } else if (api.exportObj)(name.as_ptr().cast(), value).is_null() {
                    abort("RustCodegen canonical export failed");
                }
            }
        }
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
pub extern "C" fn birnam_symbol_borrow_v1() -> core::ffi::c_int {
    1
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
        CompiledResumablePlan, GenerationModuleProjection, IDENTITY, ImplementationKey,
        InventoryImplementationKind, InventoryRootKind, OwnedSelectionRequest,
        ParsedSelectionRequests, RevisionMetadata, RevisionOperation, RootKind,
        SelectionDisposition, SelectionImplementationKind, artifact_selection,
        artifact_selection_with_requests, artifact_source, birnam_codegen_rust_identity,
        birnam_rust_codegen_candidate_root_set, birnam_rust_codegen_vtable_cache_get,
        birnam_rust_codegen_vtable_cache_set, candidate_manifest, candidate_manifest_matches,
        candidate_selection_matches, compile_resumable_plan, decode_hex_digest,
        dispatch_generation_authority_with, empty_resumable_manifest, expand_selection_requests,
        image_artifact_selection_and_requests, inspect_artifact_selection,
        inspect_generation_selection, inspect_manifest, portable_image_plan, portable_named_plan,
        portable_resumable_plan, rebase_image_resumable_plan, resumable_indices,
        resumable_manifest, selector_arity, sha256_hex, take_candidate_roots,
        take_vtable_cache_roots,
    };
    use birnam_artifact::portable_program::{NodeSpec, ProgramSpec, StatementSpec, encode};
    use birnam_codegen::{
        Block, BlockResumableEmission, Expr, NamedResumableEmission, Program, Statement,
    };
    use std::borrow::Cow;
    use std::ffi::CStr;
    use std::process::Command;

    #[test]
    fn source_less_resumable_plan_covers_nested_closure_and_is_c11_clean() {
        let nested = encode(ProgramSpec {
            parameters: &[],
            captures: &[b"held"],
            lets: &[],
            statements: &[StatementSpec::Expression { root_node: 0 }],
            nodes: &[NodeSpec::LexicalRead(0)],
        })
        .unwrap();
        let outer = encode(ProgramSpec {
            parameters: &[b"held"],
            captures: &[],
            lets: &[],
            statements: &[StatementSpec::Expression { root_node: 0 }],
            nodes: &[NodeSpec::BlockLiteral {
                program: &nested,
                captures: &[0],
            }],
        })
        .unwrap();
        let plan = portable_resumable_plan(&outer).expect("complete portable closure compiles");
        let selection = birnam_artifact::materialization_selection::inspect(
            &plan.selection,
            &plan.programs,
            &plan.bindings,
        )
        .unwrap();
        let manifest = birnam_artifact::portable_resumable_manifest::inspect(
            &plan.manifest,
            plan.authority,
            &selection,
        )
        .unwrap();
        assert_eq!(manifest.entry_count(), 2);
        assert!(
            plan.c_source
                .windows(b"birnam_portable_materialized_block_new_v2".len())
                .any(|window| window == b"birnam_portable_materialized_block_new_v2")
        );

        let path = std::env::temp_dir().join(format!(
            "birnam-portable-resumable-plan-{}.c",
            std::process::id()
        ));
        std::fs::write(&path, &plan.c_source).unwrap();
        let output = Command::new("cc")
            .args([
                "-std=c11",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-Wno-unused-function",
                "-fsyntax-only",
            ])
            .arg(&path)
            .output()
            .unwrap();
        let _ = std::fs::remove_file(path);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn source_less_named_plan_is_portable_and_c11_clean() {
        let plan = portable_named_plan(b"at:put:", Some(b"definition provenance"))
            .expect("portable named adapter compiles");
        let selection = birnam_artifact::materialization_selection::inspect(
            &plan.selection,
            &plan.programs,
            &plan.bindings,
        )
        .unwrap();
        let row = selection.row(0).unwrap();
        assert_eq!(
            row.kind,
            birnam_artifact::materialization_selection::SubjectKind::NamedAdapter
        );
        assert_eq!(row.language_arity, 2);
        let manifest = birnam_artifact::portable_resumable_manifest::inspect(
            &plan.manifest,
            plan.authority,
            &selection,
        )
        .unwrap();
        assert_eq!(manifest.entry_count(), 1);
        assert!(
            plan.c_source
                .windows(b"__birnam_prepare_portable_named_v1".len())
                .any(|window| window == b"__birnam_prepare_portable_named_v1")
        );

        let path = std::env::temp_dir().join(format!(
            "birnam-portable-named-plan-{}.c",
            std::process::id()
        ));
        std::fs::write(&path, &plan.c_source).unwrap();
        let output = Command::new("cc")
            .args([
                "-std=c11",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-Wno-unused-function",
                "-fsyntax-only",
            ])
            .arg(&path)
            .output()
            .unwrap();
        let _ = std::fs::remove_file(path);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn portable_image_plan_exports_exact_authority_inputs_and_is_c11_clean() {
        let program = encode(ProgramSpec {
            parameters: &[],
            captures: &[],
            lets: &[],
            statements: &[StatementSpec::Expression { root_node: 0 }],
            nodes: &[NodeSpec::Integer(7)],
        })
        .unwrap();
        let programs = birnam_artifact::portable_inventory::encode_closure(&[&program]).unwrap();
        let bindings = birnam_artifact::code_binding_inventory::encode(&[]).unwrap();
        let selection = birnam_artifact::materialization_selection::encode(
            &programs,
            &bindings,
            &[],
            &[birnam_artifact::materialization_selection::Disposition::Resumable],
        )
        .unwrap();
        let plan = portable_image_plan(&programs, &bindings, &selection)
            .expect("complete portable image compiles");
        assert_eq!(plan.programs, programs);
        assert_eq!(plan.bindings, bindings);
        assert_eq!(plan.selection, selection);
        for name in [
            b"__birnam_portable_program_inventory_v1".as_slice(),
            b"__birnam_code_binding_inventory_v1".as_slice(),
            b"__birnam_code_materialization_selection_v1".as_slice(),
            b"__birnam_portable_resumable_manifest_v1".as_slice(),
            b"__birnam_prepare_portable_image_blocks_v1".as_slice(),
            b"__birnam_resolve_block_vtable_v1".as_slice(),
        ] {
            assert!(
                plan.c_source
                    .windows(name.len())
                    .any(|window| window == name),
                "missing {}",
                String::from_utf8_lossy(name)
            );
        }
        let path = std::env::temp_dir().join(format!(
            "birnam-portable-image-plan-{}.c",
            std::process::id()
        ));
        std::fs::write(&path, &plan.c_source).unwrap();
        let output = Command::new("cc")
            .args([
                "-std=c11",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-Wno-unused-function",
                "-fsyntax-only",
            ])
            .arg(&path)
            .output()
            .unwrap();
        let _ = std::fs::remove_file(path);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn selector_arity_counts_binary_operators_as_one_argument() {
        assert_eq!(selector_arity(b"printString"), 0);
        assert_eq!(selector_arity(b"_private"), 0);
        assert_eq!(selector_arity(b"at:put:"), 2);
        assert_eq!(selector_arity(b"|>"), 1);
        assert_eq!(selector_arity(b"+"), 1);
        assert_eq!(selector_arity(b"~="), 1);
    }

    #[test]
    fn identity_is_a_terminated_static_string() {
        // SAFETY: the export points to immutable crate storage for process life.
        let identity = unsafe { CStr::from_ptr(birnam_codegen_rust_identity()) };
        assert_eq!(identity.to_bytes_with_nul(), IDENTITY);
    }

    #[test]
    fn generated_vtable_caches_are_generation_owned_and_withdrawable() {
        const LEFT: u64 = 0xf001_f001_f001_f001;
        const RIGHT: u64 = 0xf002_f002_f002_f002;
        let left = core::ptr::NonNull::<u8>::dangling().as_ptr().cast();
        let right = core::ptr::NonNull::<u16>::dangling().as_ptr().cast();
        unsafe {
            assert_eq!(birnam_rust_codegen_vtable_cache_set(LEFT, 7, left), left);
            assert_eq!(birnam_rust_codegen_vtable_cache_set(RIGHT, 7, right), right);
            assert_eq!(birnam_rust_codegen_vtable_cache_get(LEFT, 7), left);
            assert_eq!(birnam_rust_codegen_vtable_cache_get(RIGHT, 7), right);
        }

        let withdrawn = take_vtable_cache_roots(LEFT);
        assert_eq!(withdrawn.len(), 1);
        assert_eq!(withdrawn[0].generation, LEFT);
        assert!(unsafe { birnam_rust_codegen_vtable_cache_get(LEFT, 7) }.is_null());
        assert_eq!(
            unsafe { birnam_rust_codegen_vtable_cache_get(RIGHT, 7) },
            right
        );
        assert_eq!(take_vtable_cache_roots(RIGHT).len(), 1);
    }

    #[test]
    fn generated_candidate_roots_are_generation_owned_and_withdrawable() {
        const LEFT: u64 = 0xf003_f003_f003_f003;
        const RIGHT: u64 = 0xf004_f004_f004_f004;
        let left = core::ptr::NonNull::<u32>::dangling().as_ptr().cast();
        let right = core::ptr::NonNull::<u64>::dangling().as_ptr().cast();
        unsafe {
            assert_eq!(birnam_rust_codegen_candidate_root_set(LEFT, 3, left), left);
            assert_eq!(
                birnam_rust_codegen_candidate_root_set(RIGHT, 3, right),
                right
            );
        }

        let withdrawn = take_candidate_roots(LEFT);
        assert_eq!(withdrawn.len(), 1);
        assert_eq!(withdrawn[0].generation, LEFT);
        assert_eq!(withdrawn[0].value, left as usize);
        let remaining = take_candidate_roots(RIGHT);
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].generation, RIGHT);
        assert_eq!(remaining[0].value, right as usize);
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
            true,
            None,
            None,
        );
        assert!(
            output
                .windows(b"const unsigned char __birnam_artifact_descriptor[] = {".len())
                .any(|window| window == b"const unsigned char __birnam_artifact_descriptor[] = {")
        );
    }

    #[test]
    fn image_artifact_exports_the_exact_authority_bound_manifest() {
        let manifest = empty_resumable_manifest([0x42; 32]).unwrap();
        let output = artifact_source(
            b"7",
            b"image-generation",
            &sha256_hex(b"7"),
            &sha256_hex(b"7"),
            &[],
            &[],
            b"void* __ev(void* self, void** args, long n){}",
            false,
            None,
            Some(&manifest),
        );
        assert!(
            output
                .windows(b"__birnam_resumable_manifest_v1[]".len())
                .any(|window| window == b"__birnam_resumable_manifest_v1[]")
        );
        let size = format!(
            "__birnam_resumable_manifest_v1_size = {}ULL",
            manifest.len()
        );
        assert!(
            output
                .windows(size.len())
                .any(|window| window == size.as_bytes())
        );
    }

    #[test]
    fn resumable_plan_composes_named_and_block_manifest_indices() {
        let revision = [0x31; 32];
        let metadata = RevisionMetadata {
            name: b"candidate".to_vec(),
            requires: Vec::new(),
            operations: vec![RevisionOperation {
                target: b"Integer".to_vec(),
                selector: b"answer".to_vec(),
                install: true,
                source: b"[Reflect defineMethod: #answer on: Integer as: [:self | 71]]".to_vec(),
            }],
            non_method_digest: sha256_hex(b"non-method"),
            initializer_residual_digest: sha256_hex(b"initializer-residual"),
            method_digest: sha256_hex(b"method"),
            revision_digest: *b"3131313131313131313131313131313131313131313131313131313131313131",
            inventory: vec![
                ImplementationKey {
                    root_kind: InventoryRootKind::MethodOperation,
                    root_index: 0,
                    path: Vec::new(),
                    implementation_kind: InventoryImplementationKind::NamedAdapter,
                },
                ImplementationKey {
                    root_kind: InventoryRootKind::MethodOperation,
                    root_index: 0,
                    path: vec![0],
                    implementation_kind: InventoryImplementationKind::BlockBody,
                },
            ],
        };
        let request = OwnedSelectionRequest {
            source_revision: revision,
            root_kind: RootKind::MethodOperation,
            root_index: 0,
            path: Vec::new(),
            implementation_kind: SelectionImplementationKind::NamedAdapter,
            disposition: SelectionDisposition::ResumableRequired,
        };
        let selection = artifact_selection_with_requests(&metadata, &[request.clone()]).unwrap();
        let source = b"[Reflect defineMethod: #answer on: Integer as: [:self | 71]]";
        let normalized = Program {
            body: vec![Statement::Expr(Expr::Send {
                receiver: Box::new(Expr::Global(b"Reflect")),
                selector: Cow::Borrowed(b"defineMethod:on:as:"),
                args: vec![
                    Expr::Symbol(b"answer"),
                    Expr::Global(b"Integer"),
                    Expr::Block(Box::new(Block {
                        params: vec![b"self"],
                        body: vec![Statement::Expr(Expr::SmallInt(71))],
                    })),
                ],
                source,
            })],
        };
        let plan = compile_resumable_plan(
            &metadata,
            &[request.clone()],
            &selection,
            [0x52; 32],
            b"candidate.birnam",
            source,
            &normalized,
        )
        .unwrap();
        let manifest = resumable_manifest::inspect(&plan.manifest, [0x52; 32]).unwrap();
        assert_eq!(manifest.entry_count(), 1);
        assert_eq!(plan.named_emissions.len(), 1);
        assert!(plan.block_emissions.is_empty());
        assert_eq!(plan.named_emissions[0].operation_index, 0);
        assert!(
            plan.named_emissions[0]
                .c_source
                .windows(b"e.manifest_entry_index=0U".len())
                .any(|window| window == b"e.manifest_entry_index=0U")
        );

        let block_request = OwnedSelectionRequest {
            implementation_kind: SelectionImplementationKind::BlockBody,
            path: vec![0],
            ..request.clone()
        };
        let block_selection =
            artifact_selection_with_requests(&metadata, &[block_request.clone()]).unwrap();
        let block_plan = compile_resumable_plan(
            &metadata,
            &[block_request],
            &block_selection,
            [0x52; 32],
            b"candidate.birnam",
            source,
            &normalized,
        )
        .unwrap();
        assert!(block_plan.named_emissions.is_empty());
        assert_eq!(block_plan.block_emissions.len(), 1);
        assert_eq!(block_plan.block_emissions[0].key.path, vec![0]);
        assert_eq!(block_plan.block_emissions[0].manifest_entry_index, 0);
        assert!(
            block_plan.block_emissions[0]
                .c_source
                .windows(b"e.manifest_entry_index=0U".len())
                .any(|window| window == b"e.manifest_entry_index=0U")
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
            initializer_residual_digest:
                *b"1111111111111111111111111111111111111111111111111111111111111111",
            method_digest: *b"abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789",
            revision_digest: *b"fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210",
            inventory: vec![ImplementationKey {
                root_kind: InventoryRootKind::MethodOperation,
                root_index: 0,
                path: Vec::new(),
                implementation_kind: InventoryImplementationKind::NamedAdapter,
            }],
        };
        let selection = artifact_selection(&metadata).unwrap();
        let selection_digest = sha256_hex(&selection);
        let manifest = candidate_manifest(&metadata, &sha256_hex(source), &selection_digest);
        let parsed = inspect_manifest(&manifest).unwrap();
        assert!(candidate_manifest_matches(
            parsed, source, &metadata, &selection
        ));
        assert!(candidate_selection_matches(parsed, &selection, &selection));

        let mut changed_selection = selection.clone();
        let last = changed_selection.len() - 1;
        changed_selection[last] ^= 1;
        assert!(!candidate_selection_matches(
            parsed,
            &selection,
            &changed_selection
        ));
    }

    #[test]
    fn owned_selection_requests_materialize_exactly_and_reject_duplicates_or_foreign_scope() {
        let metadata = RevisionMetadata {
            name: b"candidate".to_vec(),
            requires: Vec::new(),
            operations: vec![RevisionOperation {
                target: b"Target".to_vec(),
                selector: b"answer:".to_vec(),
                install: true,
                source: b"operation".to_vec(),
            }],
            non_method_digest: *b"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            initializer_residual_digest:
                *b"1111111111111111111111111111111111111111111111111111111111111111",
            method_digest: *b"abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789",
            revision_digest: *b"fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210",
            inventory: vec![ImplementationKey {
                root_kind: InventoryRootKind::MethodOperation,
                root_index: 0,
                path: Vec::new(),
                implementation_kind: InventoryImplementationKind::NamedAdapter,
            }],
        };
        let revision = decode_hex_digest(&metadata.revision_digest).unwrap();
        let synchronous = OwnedSelectionRequest {
            source_revision: revision,
            root_kind: RootKind::MethodOperation,
            root_index: 0,
            path: Vec::new(),
            implementation_kind: SelectionImplementationKind::NamedAdapter,
            disposition: SelectionDisposition::Synchronous,
        };
        assert_eq!(
            artifact_selection_with_requests(&metadata, std::slice::from_ref(&synchronous)),
            artifact_selection(&metadata),
            "an explicit synchronous row is byte-identical to the sparse default"
        );

        let resumable = OwnedSelectionRequest {
            disposition: SelectionDisposition::ResumableRequired,
            ..synchronous.clone()
        };
        let bytes = artifact_selection_with_requests(&metadata, &[resumable.clone()]).unwrap();
        let selection = inspect_artifact_selection(&bytes).unwrap();
        assert_eq!(
            selection.rows().next().unwrap().unwrap().disposition,
            SelectionDisposition::ResumableRequired
        );
        assert!(
            artifact_selection_with_requests(&metadata, &[resumable.clone(), resumable.clone()])
                .is_none()
        );
        assert!(
            artifact_selection_with_requests(
                &metadata,
                &[OwnedSelectionRequest {
                    source_revision: [0x44; 32],
                    ..resumable
                }],
            )
            .is_none()
        );
    }

    #[test]
    fn whole_unit_request_expands_to_exact_resumable_inventory_rows() {
        let metadata = RevisionMetadata {
            name: b"control".to_vec(),
            requires: Vec::new(),
            operations: vec![RevisionOperation {
                target: b"Boolean".to_vec(),
                selector: b"ifTrue:".to_vec(),
                install: true,
                source: b"operation".to_vec(),
            }],
            non_method_digest: sha256_hex(b"non-method"),
            initializer_residual_digest: sha256_hex(b"initializer-residual"),
            method_digest: sha256_hex(b"method"),
            revision_digest: *b"3131313131313131313131313131313131313131313131313131313131313131",
            inventory: vec![
                ImplementationKey {
                    root_kind: InventoryRootKind::MethodOperation,
                    root_index: 0,
                    path: Vec::new(),
                    implementation_kind: InventoryImplementationKind::NamedAdapter,
                },
                ImplementationKey {
                    root_kind: InventoryRootKind::MethodOperation,
                    root_index: 0,
                    path: vec![0],
                    implementation_kind: InventoryImplementationKind::BlockBody,
                },
            ],
        };

        let requests =
            expand_selection_requests(&metadata, ParsedSelectionRequests::WholeUnitResumable)
                .unwrap();
        assert_eq!(requests.len(), metadata.inventory.len());
        assert!(requests.iter().all(|request| {
            request.source_revision == [0x31; 32]
                && request.disposition == SelectionDisposition::ResumableRequired
        }));
        assert_eq!(requests[0].root_kind, RootKind::MethodOperation);
        assert_eq!(requests[0].root_index, 0);
        assert!(requests[0].path.is_empty());
        assert_eq!(
            requests[0].implementation_kind,
            SelectionImplementationKind::NamedAdapter
        );
        assert_eq!(requests[1].path, vec![0]);
        assert_eq!(
            requests[1].implementation_kind,
            SelectionImplementationKind::BlockBody
        );

        let selection = artifact_selection_with_requests(&metadata, &requests).unwrap();
        assert!(
            inspect_artifact_selection(&selection)
                .unwrap()
                .rows()
                .all(|row| row.unwrap().disposition == SelectionDisposition::ResumableRequired)
        );
    }

    #[test]
    fn resumable_symbol_and_manifest_indices_use_their_distinct_canonical_orders() {
        let revision = [0x44; 32];
        let requests = vec![
            OwnedSelectionRequest {
                source_revision: revision,
                root_kind: RootKind::NonMethodProgram,
                root_index: 0,
                path: vec![0],
                implementation_kind: SelectionImplementationKind::BlockBody,
                disposition: SelectionDisposition::ResumableRequired,
            },
            OwnedSelectionRequest {
                source_revision: revision,
                root_kind: RootKind::MethodOperation,
                root_index: 0,
                path: Vec::new(),
                implementation_kind: SelectionImplementationKind::NamedAdapter,
                disposition: SelectionDisposition::ResumableRequired,
            },
            OwnedSelectionRequest {
                source_revision: revision,
                root_kind: RootKind::MethodOperation,
                root_index: 0,
                path: vec![0],
                implementation_kind: SelectionImplementationKind::BlockBody,
                disposition: SelectionDisposition::ResumableRequired,
            },
        ];
        let indices = resumable_indices(&requests).unwrap();
        assert_eq!(indices[&requests[0].key()], (0, 1));
        assert_eq!(indices[&requests[1].key()], (1, 0));
        assert_eq!(indices[&requests[2].key()], (2, 2));
    }

    #[test]
    fn image_projection_preserves_the_complete_generation_policy_without_reselection() {
        let generation =
            include_bytes!("../../../abi/fixtures/stackless-s2/generation-selection-ab.bin");
        let accepted = inspect_generation_selection(generation).unwrap();
        let (artifact_bytes, requests) = image_artifact_selection_and_requests(generation).unwrap();
        let artifact = inspect_artifact_selection(&artifact_bytes).unwrap();

        assert_eq!(artifact.revision_count(), accepted.revision_count());
        assert_eq!(artifact.row_count(), accepted.row_count());
        for (artifact_row, accepted_row) in artifact.rows().zip(accepted.rows()) {
            assert_eq!(artifact_row.unwrap(), accepted_row.unwrap());
        }
        let required = accepted
            .rows()
            .filter(|row| {
                row.as_ref()
                    .is_ok_and(|row| row.disposition == SelectionDisposition::ResumableRequired)
            })
            .count();
        assert_eq!(requests.len(), required);
        assert!(
            requests
                .iter()
                .all(|request| { request.disposition == SelectionDisposition::ResumableRequired })
        );
    }

    #[test]
    fn image_plan_rebases_module_local_syntax_keys_without_changing_manifest_indices() {
        let mut plan = CompiledResumablePlan {
            manifest: Vec::new(),
            entries: Vec::new(),
            named_emissions: vec![NamedResumableEmission {
                operation_index: 1,
                symbol_index: 4,
                c_source: Vec::new(),
            }],
            block_emissions: vec![
                BlockResumableEmission {
                    key: ImplementationKey {
                        root_kind: InventoryRootKind::MethodOperation,
                        root_index: 2,
                        path: vec![0],
                        implementation_kind: InventoryImplementationKind::BlockBody,
                    },
                    symbol_index: 5,
                    manifest_entry_index: 5,
                    selector: b"value".to_vec(),
                    captures: Vec::new(),
                    c_source: Vec::new(),
                },
                BlockResumableEmission {
                    key: ImplementationKey {
                        root_kind: InventoryRootKind::NonMethodProgram,
                        root_index: 0,
                        path: vec![3, 1],
                        implementation_kind: InventoryImplementationKind::BlockBody,
                    },
                    symbol_index: 6,
                    manifest_entry_index: 6,
                    selector: b"value".to_vec(),
                    captures: Vec::new(),
                    c_source: Vec::new(),
                },
            ],
        };

        rebase_image_resumable_plan(&mut plan, 5, 7).unwrap();

        assert_eq!(plan.named_emissions[0].operation_index, 6);
        assert_eq!(plan.block_emissions[0].key.root_index, 7);
        assert_eq!(plan.block_emissions[1].key.path, [10, 1]);
        assert_eq!(plan.block_emissions[0].manifest_entry_index, 5);
        assert_eq!(plan.block_emissions[1].manifest_entry_index, 6);
    }

    #[test]
    fn dispatch_base_v2_reproduces_the_frozen_non_circular_trace() {
        let revision_digest = *b"0b0261b15fce452514549a5369bb41624215eaa4e6469a3c370495bdffdaa8a1";
        let module = GenerationModuleProjection {
            name: b"trace".to_vec(),
            revision: decode_hex_digest(&revision_digest).unwrap(),
            source: b"run => self\nblock => [ self ]\n".to_vec(),
            metadata: RevisionMetadata {
                name: b"trace".to_vec(),
                requires: Vec::new(),
                operations: vec![RevisionOperation {
                    target: b"Trace".to_vec(),
                    selector: b"run".to_vec(),
                    install: true,
                    source: b"run => self".to_vec(),
                }],
                non_method_digest: [b'0'; 64],
                initializer_residual_digest: [b'1'; 64],
                method_digest: [b'0'; 64],
                revision_digest,
                inventory: Vec::new(),
            },
        };
        let descriptor =
            decode_hex_digest(b"d670335de8efd9c6db228e19a70affb3fe738e0864dee726dde273928a5171cf")
                .unwrap();
        let generation_selection = include_bytes!(
            "../../../abi/fixtures/stackless-s2/generation-selection-construction-trace.bin"
        );
        let authority = dispatch_generation_authority_with(
            &[module],
            generation_selection,
            |_module, index, _selection| (index == 0).then_some(descriptor),
        )
        .unwrap();
        assert_eq!(
            authority,
            decode_hex_digest(b"7d1a817173f4b47139581aaf2996009012ae398f1e251a3bfe5e75498c5f823c")
                .unwrap()
        );
    }
}
