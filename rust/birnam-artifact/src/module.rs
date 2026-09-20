//! Rust artifact-inspector dylib shell over the allocation-free parser.

use crate as parser;
use crate::Descriptor as ParsedDescriptor;
use birnam_abi as abi;
use core::ffi::{c_char, c_void};
use core::mem::size_of;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{fs, io::Read};

const ARTIFACT_NAME: &[u8] = b"Artifact\0";
const ARTIFACT_VT_NAME: &[u8] = b"ArtifactVt\0";
const DESCRIPTOR_VT_NAME: &[u8] = b"ArtifactDescriptorVt\0";
const VECTOR_NAME: &[u8] = b"Vector\0";
const NEW_SELECTOR: &[u8] = b"new\0";
const ADD_SELECTOR: &[u8] = b"add:\0";

#[repr(C)]
struct DescriptorObject {
    kind: abi::Oop,
    identity: abi::Oop,
    artifact_abi: abi::Oop,
    target: abi::Oop,
    source_digest: abi::Oop,
    revision_digest: abi::Oop,
    path: abi::Oop,
    requires: abi::Oop,
    native_requires: abi::Oop,
    capabilities: abi::Oop,
    requires_count: abi::Oop,
    native_requires_count: abi::Oop,
    capabilities_count: abi::Oop,
}

struct Runtime {
    api: usize,
    roots: usize,
    exports: usize,
    imports: usize,
    presence: usize,
    artifact_vt: usize,
    artifact_obj: usize,
    descriptor_vt: usize,
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();
static ROOT_REGISTERED: OnceLock<()> = OnceLock::new();
static LAST_FAILURE: AtomicUsize = AtomicUsize::new(0);

unsafe extern "C" fn mark_runtime_roots(mark: abi::MarkFn) {
    if let Some(runtime) = RUNTIME.get() {
        for value in [
            runtime.artifact_vt,
            runtime.artifact_obj,
            runtime.descriptor_vt,
        ] {
            unsafe { mark(value as abi::Oop) };
        }
    }
    let failure = LAST_FAILURE.load(Ordering::Acquire);
    if failure != 0 {
        unsafe { mark(failure as abi::Oop) };
    }
}

fn abort(message: &str) -> ! {
    eprintln!("birnam artifact inspector: {message}");
    std::process::abort();
}

fn runtime() -> &'static Runtime {
    RUNTIME
        .get()
        .unwrap_or_else(|| abort("module_init was not called"))
}

fn api(state: &Runtime) -> &'static abi::IdApi {
    // SAFETY: module_init stores the process-owned, live IdApi table and the
    // host keeps that floor dylib loaded for the process lifetime.
    unsafe { &*(state.api as *const abi::IdApi) }
}

fn as_oop(value: usize) -> abi::Oop {
    value as abi::Oop
}

fn oop_address(value: abi::Oop) -> usize {
    value as usize
}

fn c_string(bytes: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(bytes.len() + 1);
    result.extend_from_slice(bytes);
    result.push(0);
    result
}

unsafe fn intern(api: &abi::IdApi, bytes: &[u8]) -> abi::Oop {
    let value = c_string(bytes);
    // SAFETY: `value` is NUL-terminated and alive for this synchronous call.
    unsafe { (api.intern)(value.as_ptr().cast::<c_char>()) }
}

unsafe fn import(imports: *const birnam_abi::namespace_import::ApiV1, name: &[u8]) -> abi::Oop {
    debug_assert!(name.last() == Some(&0));
    // SAFETY: all module names are static NUL-terminated strings.
    unsafe { birnam_abi::namespace_import::import(imports, name.as_ptr().cast::<c_char>()) }
}

unsafe fn fail(api: &abi::IdApi, name: &[u8]) -> abi::Oop {
    let value = unsafe { intern(api, name) };
    LAST_FAILURE.store(oop_address(value), Ordering::Release);
    api.nilObj
}

unsafe fn failure(api: &abi::IdApi) -> abi::Oop {
    let value = LAST_FAILURE.load(Ordering::Acquire);
    if value == 0 {
        api.nilObj
    } else {
        as_oop(value)
    }
}

unsafe fn string_bytes<'a>(_api: &abi::IdApi, value: abi::Oop) -> Option<&'a [u8]> {
    let mut length = 0usize;
    // SAFETY: the caller supplies an object known to be a String by the vtable
    // check in the public method; the IdApi returns its stable byte payload.
    let pointer = unsafe { (birnam_abi::string::bytes)(value, &mut length) };
    // SAFETY: IdApi's stringBytes contract is the same borrowed byte contract
    // used by the ABI crate; zero length accepts a null pointer.
    unsafe { abi::borrowed_bytes(pointer, length) }
}

unsafe fn mk_string(_api: &abi::IdApi, bytes: &[u8]) -> abi::Oop {
    // SAFETY: the bytes remain readable for this synchronous call and a zero
    // length slice carries the ABI's permitted null/count shape.
    unsafe { (birnam_abi::string::make)(bytes.as_ptr(), bytes.len()) }
}

fn list_field(bytes: &[u8], index: usize) -> Option<&[u8]> {
    let mut position = 0usize;
    for current in 0..=index {
        let end = position.checked_add(4)?;
        let length = u32::from_le_bytes(bytes.get(position..end)?.try_into().ok()?) as usize;
        position = end;
        let value_end = position.checked_add(length)?;
        let value = bytes.get(position..value_end)?;
        position = value_end;
        if current == index {
            return Some(value);
        }
    }
    None
}

fn path_from_bytes(bytes: &[u8]) -> &Path {
    Path::new(OsStr::from_bytes(bytes))
}

fn read_artifact(path: &Path) -> Result<Vec<u8>, ()> {
    let file = fs::File::open(path).map_err(|_| ())?;
    if file.metadata().map_err(|_| ())?.len() > parser::MAX_ARTIFACT_BYTES as u64 {
        return Err(());
    }
    let mut bytes = Vec::new();
    file.take((parser::MAX_ARTIFACT_BYTES as u64) + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ())?;
    if bytes.len() > parser::MAX_ARTIFACT_BYTES {
        return Err(());
    }
    Ok(bytes)
}

fn native_requirements_present(descriptor: ParsedDescriptor<'_>, path: &[u8]) -> bool {
    let artifact_path = path_from_bytes(path);
    let directory = artifact_path.parent().unwrap_or_else(|| Path::new("."));
    descriptor.native_requires.iter().all(|requirement| {
        !requirement.is_empty()
            && !requirement.contains(&0)
            && !requirement.contains(&b'/')
            && fs::metadata(directory.join(OsStr::from_bytes(requirement))).is_ok()
    })
}

fn parser_failure(api: &abi::IdApi, error: parser::Error) -> abi::Oop {
    let diagnostic = match error {
        parser::Error::AbiMismatch => b"artifactAbiMismatch".as_slice(),
        parser::Error::TargetMismatch => b"artifactTargetMismatch".as_slice(),
        parser::Error::ArtifactTooLarge
        | parser::Error::MissingDescriptor
        | parser::Error::MultipleDescriptors
        | parser::Error::TruncatedDescriptor
        | parser::Error::InvalidDescriptor
        | parser::Error::ListTooLarge
        | parser::Error::EmptyIdentity => b"artifactDescriptionMissing".as_slice(),
    };
    // SAFETY: the API and diagnostic are process-owned and the C string is
    // created for the duration of the synchronous intern call.
    unsafe { fail(api, diagnostic) }
}

unsafe fn descriptor(value: abi::Oop) -> *mut DescriptorObject {
    // SAFETY: every descriptor object is allocated with this exact payload
    // shape by `materialize` and only its methods receive the vtable.
    value.cast::<DescriptorObject>()
}

unsafe fn value_at(
    state: &Runtime,
    object: *mut DescriptorObject,
    kind: ListKind,
    index: usize,
) -> abi::Oop {
    let api = api(state);
    if kind == ListKind::Roots {
        return if index == 0 {
            unsafe { (*object).identity }
        } else {
            api.nilObj
        };
    }
    let (bytes_object, count) = unsafe {
        match kind {
            ListKind::Roots => unreachable!(),
            ListKind::Requires => ((*object).requires, (*object).requires_count),
            ListKind::NativeRequires => {
                ((*object).native_requires, (*object).native_requires_count)
            }
            ListKind::Capabilities => ((*object).capabilities, (*object).capabilities_count),
        }
    };
    // SAFETY: descriptor list objects and count carriers were created by this
    // module; calls are synchronous and do not retain the borrowed slice.
    let count = unsafe { (birnam_abi::integer::value)(count) };
    if index >= count as usize {
        return api.nilObj;
    }
    // SAFETY: list objects were created from valid parser wire bytes.
    let bytes = unsafe { string_bytes(api, bytes_object) }.unwrap_or(&[]);
    let Some(value) = list_field(bytes, index) else {
        return api.nilObj;
    };
    if kind == ListKind::NativeRequires {
        // SAFETY: the value points into a String object owned by the floor and
        // remains valid while mkString copies it.
        unsafe { mk_string(api, value) }
    } else {
        // SAFETY: intern copies the temporary NUL-terminated spelling during
        // this call.
        unsafe { intern(api, value) }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ListKind {
    Roots,
    Requires,
    NativeRequires,
    Capabilities,
}

unsafe fn vector_of(state: &Runtime, receiver: abi::Oop, kind: ListKind) -> abi::Oop {
    let api = api(state);
    // SAFETY: Vector is the existing Birnam collection protocol; all selector
    // strings are static and NUL-terminated.
    let vector = unsafe { import(execution_imports(), VECTOR_NAME) };
    if vector.is_null() {
        return api.nilObj;
    }
    let new_selector = unsafe { intern(api, &NEW_SELECTOR[..NEW_SELECTOR.len() - 1]) };
    let add_selector = unsafe { intern(api, &ADD_SELECTOR[..ADD_SELECTOR.len() - 1]) };
    let mut result = unsafe { (api.send)(vector, new_selector, core::ptr::null(), 0) };
    let object = unsafe { descriptor(receiver) };
    let count = match kind {
        ListKind::Roots => 1,
        ListKind::Requires => unsafe { (birnam_abi::integer::value)((*object).requires_count) as usize },
        ListKind::NativeRequires => unsafe {
            (birnam_abi::integer::value)((*object).native_requires_count) as usize
        },
        ListKind::Capabilities => unsafe {
            (birnam_abi::integer::value)((*object).capabilities_count) as usize
        },
    };
    for index in 0..count {
        let value = unsafe { value_at(state, object, kind, index) };
        result = unsafe { (api.send)(result, add_selector, &value, 1) };
    }
    result
}

unsafe extern "C" fn inspect_method(
    _receiver: abi::Oop,
    arguments: *const abi::Oop,
    count: usize,
) -> abi::Oop {
    let state = runtime();
    let api = api(state);
    // SAFETY: IdApi method arguments obey the null/count contract.
    let Some(arguments) = (unsafe { abi::borrowed_oop_args(arguments, count) }) else {
        return unsafe { fail(api, b"artifactPathInvalid") };
    };
    if arguments.len() != 1 || unsafe { (api.vtableOf)(arguments[0]) } != unsafe { birnam_abi::string::behavior() } {
        return unsafe { fail(api, b"artifactPathInvalid") };
    }
    // SAFETY: arguments[0] is a String according to the vtable check.
    let Some(path_bytes) = (unsafe { string_bytes(api, arguments[0]) }) else {
        return unsafe { fail(api, b"artifactPathInvalid") };
    };
    let path = path_from_bytes(path_bytes);
    let bytes = match read_artifact(path) {
        Ok(bytes) => bytes,
        Err(()) => return unsafe { fail(api, b"artifactUnavailable") },
    };
    let parsed = match parser::inspect(&bytes) {
        Ok(parsed) => parsed,
        Err(error) => return parser_failure(api, error),
    };
    if !native_requirements_present(parsed, path_bytes) {
        return unsafe { fail(api, b"nativeRequirementUnavailable") };
    }
    materialize(state, parsed, path_bytes)
}

fn materialize(state: &Runtime, parsed: ParsedDescriptor<'_>, path: &[u8]) -> abi::Oop {
    let api = api(state);
    // SAFETY: the descriptor vtable has exactly the 13 Oop slots declared in
    // DescriptorObject and the floor owns the resulting allocation.
    let result = unsafe { (api.alloc)(as_oop(state.descriptor_vt), size_of::<DescriptorObject>()) };
    if result.is_null() {
        abort("descriptor allocation returned null");
    }
    let object = result.cast::<DescriptorObject>();
    let requires = parsed.requires.encoded_bytes();
    let native_requires = parsed.native_requires.encoded_bytes();
    let capabilities = parsed.capabilities.encoded_bytes();
    // SAFETY: all fields are written before the object is published; Oop
    // values come from this IdApi and bytes are copied by the floor helpers.
    unsafe {
        (*object) = DescriptorObject {
            kind: intern(api, parsed.kind),
            identity: intern(api, parsed.identity),
            artifact_abi: mk_string(api, parsed.artifact_abi),
            target: mk_string(api, parsed.target),
            source_digest: mk_string(api, parsed.source_digest),
            revision_digest: mk_string(api, parsed.revision_digest),
            path: mk_string(api, path),
            requires: mk_string(api, requires),
            native_requires: mk_string(api, native_requires),
            capabilities: mk_string(api, capabilities),
            requires_count: (birnam_abi::integer::make)(parsed.requires.count() as i64),
            native_requires_count: (birnam_abi::integer::make)(parsed.native_requires.count() as i64),
            capabilities_count: (birnam_abi::integer::make)(parsed.capabilities.count() as i64),
        };
    }
    LAST_FAILURE.store(oop_address(api.nilObj), Ordering::Release);
    result
}

unsafe extern "C" fn last_failure_method(
    _receiver: abi::Oop,
    _arguments: *const abi::Oop,
    _count: usize,
) -> abi::Oop {
    unsafe { failure(api(runtime())) }
}

unsafe extern "C" fn kind_method(
    receiver: abi::Oop,
    _arguments: *const abi::Oop,
    _count: usize,
) -> abi::Oop {
    unsafe { (*descriptor(receiver)).kind }
}

unsafe extern "C" fn identity_method(
    receiver: abi::Oop,
    _arguments: *const abi::Oop,
    _count: usize,
) -> abi::Oop {
    unsafe { (*descriptor(receiver)).identity }
}

unsafe extern "C" fn artifact_abi_method(
    receiver: abi::Oop,
    _arguments: *const abi::Oop,
    _count: usize,
) -> abi::Oop {
    unsafe { (*descriptor(receiver)).artifact_abi }
}

unsafe extern "C" fn target_method(
    receiver: abi::Oop,
    _arguments: *const abi::Oop,
    _count: usize,
) -> abi::Oop {
    unsafe { (*descriptor(receiver)).target }
}

unsafe extern "C" fn source_digest_method(
    receiver: abi::Oop,
    _arguments: *const abi::Oop,
    _count: usize,
) -> abi::Oop {
    unsafe { (*descriptor(receiver)).source_digest }
}

unsafe extern "C" fn revision_digest_method(
    receiver: abi::Oop,
    _arguments: *const abi::Oop,
    _count: usize,
) -> abi::Oop {
    unsafe { (*descriptor(receiver)).revision_digest }
}

unsafe extern "C" fn path_method(
    receiver: abi::Oop,
    _arguments: *const abi::Oop,
    _count: usize,
) -> abi::Oop {
    unsafe { (*descriptor(receiver)).path }
}

unsafe fn count_of(object: *mut DescriptorObject, kind: ListKind) -> usize {
    let _api = api(runtime());
    match kind {
        ListKind::Roots => 1,
        ListKind::Requires => (unsafe { (birnam_abi::integer::value)((*object).requires_count) }) as usize,
        ListKind::NativeRequires => {
            (unsafe { (birnam_abi::integer::value)((*object).native_requires_count) }) as usize
        }
        ListKind::Capabilities => {
            (unsafe { (birnam_abi::integer::value)((*object).capabilities_count) }) as usize
        }
    }
}

unsafe fn list_method(receiver: abi::Oop, kind: ListKind) -> abi::Oop {
    unsafe { vector_of(runtime(), receiver, kind) }
}

unsafe fn count_method(receiver: abi::Oop, kind: ListKind) -> abi::Oop {
    let _api = api(runtime());
    unsafe { (birnam_abi::integer::make)(count_of(descriptor(receiver), kind) as i64) }
}

unsafe fn item_method(
    receiver: abi::Oop,
    arguments: *const abi::Oop,
    count: usize,
    kind: ListKind,
) -> abi::Oop {
    let state = runtime();
    let api = api(state);
    // SAFETY: IdApi method arguments obey the null/count contract.
    let Some(arguments) = (unsafe { abi::borrowed_oop_args(arguments, count) }) else {
        return api.nilObj;
    };
    if arguments.len() != 1 || unsafe { (api.vtableOf)(arguments[0]) } != unsafe { birnam_abi::integer::behavior() } {
        return api.nilObj;
    }
    let index = unsafe { (birnam_abi::integer::value)(arguments[0]) };
    if index < 0 {
        return api.nilObj;
    }
    unsafe { value_at(state, descriptor(receiver), kind, index as usize) }
}

unsafe extern "C" fn roots_method(
    receiver: abi::Oop,
    _arguments: *const abi::Oop,
    _count: usize,
) -> abi::Oop {
    unsafe { list_method(receiver, ListKind::Roots) }
}

unsafe extern "C" fn roots_count_method(
    receiver: abi::Oop,
    _arguments: *const abi::Oop,
    _count: usize,
) -> abi::Oop {
    unsafe { count_method(receiver, ListKind::Roots) }
}

unsafe extern "C" fn root_at_method(
    receiver: abi::Oop,
    arguments: *const abi::Oop,
    count: usize,
) -> abi::Oop {
    unsafe { item_method(receiver, arguments, count, ListKind::Roots) }
}

unsafe extern "C" fn requires_method(
    receiver: abi::Oop,
    _arguments: *const abi::Oop,
    _count: usize,
) -> abi::Oop {
    unsafe { list_method(receiver, ListKind::Requires) }
}

unsafe extern "C" fn requires_count_method(
    receiver: abi::Oop,
    _arguments: *const abi::Oop,
    _count: usize,
) -> abi::Oop {
    unsafe { count_method(receiver, ListKind::Requires) }
}

unsafe extern "C" fn require_at_method(
    receiver: abi::Oop,
    arguments: *const abi::Oop,
    count: usize,
) -> abi::Oop {
    unsafe { item_method(receiver, arguments, count, ListKind::Requires) }
}

unsafe extern "C" fn native_requires_method(
    receiver: abi::Oop,
    _arguments: *const abi::Oop,
    _count: usize,
) -> abi::Oop {
    unsafe { list_method(receiver, ListKind::NativeRequires) }
}

unsafe extern "C" fn native_requires_count_method(
    receiver: abi::Oop,
    _arguments: *const abi::Oop,
    _count: usize,
) -> abi::Oop {
    unsafe { count_method(receiver, ListKind::NativeRequires) }
}

unsafe extern "C" fn native_require_at_method(
    receiver: abi::Oop,
    arguments: *const abi::Oop,
    count: usize,
) -> abi::Oop {
    unsafe { item_method(receiver, arguments, count, ListKind::NativeRequires) }
}

unsafe extern "C" fn capabilities_method(
    receiver: abi::Oop,
    _arguments: *const abi::Oop,
    _count: usize,
) -> abi::Oop {
    unsafe { list_method(receiver, ListKind::Capabilities) }
}

unsafe extern "C" fn capabilities_count_method(
    receiver: abi::Oop,
    _arguments: *const abi::Oop,
    _count: usize,
) -> abi::Oop {
    unsafe { count_method(receiver, ListKind::Capabilities) }
}

unsafe extern "C" fn capability_at_method(
    receiver: abi::Oop,
    arguments: *const abi::Oop,
    count: usize,
) -> abi::Oop {
    unsafe { item_method(receiver, arguments, count, ListKind::Capabilities) }
}

fn register_methods(state: &Runtime) {
    let api = api(state);
    let layout = [abi::SLOT_OOP; 13];
    // SAFETY: the vtable and selector/method pointers are process-owned and
    // registration is synchronous through the frozen ABI.
    unsafe {
        (api.setLayout)(
            as_oop(state.descriptor_vt),
            layout.as_ptr(),
            layout.len(),
            abi::SLOT_RAW,
        );
        (api.method)(
            as_oop(state.artifact_vt),
            intern(api, b"inspect:"),
            inspect_method,
        );
        (api.method)(
            as_oop(state.artifact_vt),
            intern(api, b"lastFailure"),
            last_failure_method,
        );
        (api.method)(
            as_oop(state.descriptor_vt),
            intern(api, b"kind"),
            kind_method,
        );
        (api.method)(
            as_oop(state.descriptor_vt),
            intern(api, b"identity"),
            identity_method,
        );
        (api.method)(
            as_oop(state.descriptor_vt),
            intern(api, b"artifactAbi"),
            artifact_abi_method,
        );
        (api.method)(
            as_oop(state.descriptor_vt),
            intern(api, b"target"),
            target_method,
        );
        (api.method)(
            as_oop(state.descriptor_vt),
            intern(api, b"roots"),
            roots_method,
        );
        (api.method)(
            as_oop(state.descriptor_vt),
            intern(api, b"rootsCount"),
            roots_count_method,
        );
        (api.method)(
            as_oop(state.descriptor_vt),
            intern(api, b"rootAt:"),
            root_at_method,
        );
        (api.method)(
            as_oop(state.descriptor_vt),
            intern(api, b"requires"),
            requires_method,
        );
        (api.method)(
            as_oop(state.descriptor_vt),
            intern(api, b"requiresCount"),
            requires_count_method,
        );
        (api.method)(
            as_oop(state.descriptor_vt),
            intern(api, b"requireAt:"),
            require_at_method,
        );
        (api.method)(
            as_oop(state.descriptor_vt),
            intern(api, b"nativeRequires"),
            native_requires_method,
        );
        (api.method)(
            as_oop(state.descriptor_vt),
            intern(api, b"nativeRequiresCount"),
            native_requires_count_method,
        );
        (api.method)(
            as_oop(state.descriptor_vt),
            intern(api, b"nativeRequireAt:"),
            native_require_at_method,
        );
        (api.method)(
            as_oop(state.descriptor_vt),
            intern(api, b"capabilities"),
            capabilities_method,
        );
        (api.method)(
            as_oop(state.descriptor_vt),
            intern(api, b"capabilitiesCount"),
            capabilities_count_method,
        );
        (api.method)(
            as_oop(state.descriptor_vt),
            intern(api, b"capabilityAt:"),
            capability_at_method,
        );
        (api.method)(
            as_oop(state.descriptor_vt),
            intern(api, b"sourceDigest"),
            source_digest_method,
        );
        (api.method)(
            as_oop(state.descriptor_vt),
            intern(api, b"revisionDigest"),
            revision_digest_method,
        );
        (api.method)(
            as_oop(state.descriptor_vt),
            intern(api, b"path"),
            path_method,
        );
    }
}

pub unsafe fn bring_up(api_pointer: *mut abi::IdApi, presence: *const birnam_abi::namespace_presence::ApiV1, imports: *const birnam_abi::namespace_import::ApiV1, exports: *const birnam_abi::namespace_export::ApiV1, roots: *const birnam_abi::root_registry::ApiV1) {
    if api_pointer.is_null() {
        abort("module_init received a null IdApi");
    }
    let state = RUNTIME.get_or_init(|| {
        // SAFETY: module_init's caller supplies the live floor table.
        let api = unsafe { &*api_pointer };
        let reloading = unsafe { birnam_abi::namespace_presence::contains(presence, ARTIFACT_NAME.as_ptr().cast()) != 0 };
        let (artifact_vt, artifact_obj, descriptor_vt) = if reloading {
            (
                unsafe { import(imports, ARTIFACT_VT_NAME) },
                unsafe { import(imports, ARTIFACT_NAME) },
                unsafe { import(imports, DESCRIPTOR_VT_NAME) },
            )
        } else {
            let artifact_vt = unsafe { (api.proto)(api.object_vt) };
            let artifact_obj = unsafe { (api.alloc)(artifact_vt, 0) };
            let descriptor_vt = unsafe { (api.proto)(api.object_vt) };
            (artifact_vt, artifact_obj, descriptor_vt)
        };
        Runtime {
            api: api_pointer as usize,
            roots: roots as usize,
            exports: exports as usize,
            imports: imports as usize,
            presence: presence as usize,
            artifact_vt: oop_address(artifact_vt),
            artifact_obj: oop_address(artifact_obj),
            descriptor_vt: oop_address(descriptor_vt),
        }
    });
    register_methods(state);
    let api = api(state);
    let exports = state.exports as *const birnam_abi::namespace_export::ApiV1;
    ROOT_REGISTERED.get_or_init(|| unsafe {
        birnam_abi::root_registry::register(state.roots as *const birnam_abi::root_registry::ApiV1, mark_runtime_roots);
    });
    if LAST_FAILURE.load(Ordering::Acquire) == 0 {
        LAST_FAILURE.store(oop_address(api.nilObj), Ordering::Release);
    }
    if !(unsafe { birnam_abi::namespace_presence::contains(execution_presence(), ARTIFACT_NAME.as_ptr().cast()) != 0 }) {
        // SAFETY: the selected export publishes the three singleton objects exactly as
        // the native inspector does on its first load.
        unsafe {
            let _ = birnam_abi::namespace_export::export(exports, ARTIFACT_VT_NAME.as_ptr().cast(), as_oop(state.artifact_vt));
            let _ = birnam_abi::namespace_export::export(exports, ARTIFACT_NAME.as_ptr().cast(), as_oop(state.artifact_obj));
            let _ = birnam_abi::namespace_export::export(exports,
                DESCRIPTOR_VT_NAME.as_ptr().cast(),
                as_oop(state.descriptor_vt),
            );
        }
    }
}

/// Declare that this module treats IdApi Symbol-name pointers as borrowed.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> core::ffi::c_int {
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(
    api_pointer: *mut abi::IdApi,
    _state: *mut c_void, presence: *const birnam_abi::namespace_presence::ApiV1, imports: *const birnam_abi::namespace_import::ApiV1, exports: *const birnam_abi::namespace_export::ApiV1, _vtables: *const birnam_abi::vtable_enumeration::ApiV2, _observation: *const birnam_abi::method_observation::ApiV1, roots: *const birnam_abi::root_registry::ApiV1, _settings: *const birnam_abi::gc_settings::ApiV1) -> *mut c_void {
    // SAFETY: the host owns the IdApi pointer and calls this entrypoint before
    // dispatching any artifact protocol method.
    unsafe { bring_up(api_pointer, presence, imports, exports, roots) };
    core::ptr::null_mut()
}

fn execution_presence() -> *const birnam_abi::namespace_presence::ApiV1 { RUNTIME.get().expect("module not initialized").presence as *const birnam_abi::namespace_presence::ApiV1 }

fn execution_imports() -> *const birnam_abi::namespace_import::ApiV1 { RUNTIME.get().expect("module not initialized").imports as *const birnam_abi::namespace_import::ApiV1 }
