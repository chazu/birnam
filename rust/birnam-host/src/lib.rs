//! Pure executable-relative runtime-resource resolution for the Rust host.
//!
//! This is deliberately not a loader and does not inspect the filesystem. It
//! only reproduces the installed Birnam layout and the three narrow resource
//! overrides accepted by the existing Zig host. Loading a floor or any higher
//! rung remains a later host boundary.

use std::env;
use std::ffi::{CStr, CString};
use std::io;
use std::marker::PhantomData;
use std::path::{Component, Path, PathBuf};
use std::ptr::NonNull;

use birnam_abi::{IdApi, IdInitFn};

#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Overrides<'a> {
    pub lib_dir: Option<&'a Path>,
    pub id_dylib: Option<&'a Path>,
    pub rust_workspace_dir: Option<&'a Path>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resources {
    pub prefix: PathBuf,
    pub lib_dir: PathBuf,
    pub source_dir: PathBuf,
    pub rust_workspace_dir: PathBuf,
    pub guide_dir: PathBuf,
    pub id_dylib: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EarlyAction {
    Continue,
    Help,
    Version,
}

/// Classify the pure CLI exits that must run before any runtime-resource or
/// floor access. Remaining arguments stay opaque until the host dispatch slice
/// ports the project/REPL command grammar.
pub fn classify_early_args<T: AsRef<std::ffi::OsStr>>(args: &[T]) -> EarlyAction {
    match args.first().map(AsRef::as_ref).and_then(|arg| arg.to_str()) {
        Some("--help" | "-h") => EarlyAction::Help,
        Some("--version" | "-V") => EarlyAction::Version,
        _ => EarlyAction::Continue,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FloorError {
    Read {
        path: PathBuf,
        source: io::ErrorKind,
    },
    DigestMismatch {
        expected: String,
        actual: String,
    },
    EmbeddedNul,
    OpenFailed,
    MissingIdInit,
    NullApi,
    MissingIdentity,
    IdentityMismatch {
        expected: String,
        actual: String,
    },
}

/// A successfully initialized floor. The dynamic-library handle is retained
/// for the lifetime of this value and deliberately never closed: the floor
/// owns process-wide heap/GC state and may retain function pointers in modules.
pub struct LoadedFloor {
    handle: NonNull<std::ffi::c_void>,
    api: NonNull<IdApi>,
    // Keep this type !Send/!Sync until host threading and the floor owner-thread
    // protocol are explicitly ported. The marker has no runtime state.
    _owner_thread: PhantomData<std::rc::Rc<()>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleError {
    EmbeddedNul,
    OpenFailed { path: PathBuf },
    MissingModuleInit { path: PathBuf },
}

/// The native module order is part of the host contract: Store precedes
/// Codegen, and every later rung sees the same canonical floor/API table.
pub const SCRIPT_MODULES: &[&str] = &[
    "libbirnam_id_artifact",
    "libbirnam_store",
    "libbirnam_id_peg",
    "libbirnam_id_lang",
    "libbirnam_id_tcc",
    "libbirnam_id_codegen",
    "libbirnam_id_prelude",
    "libbirnam_id_collection",
    "libbirnam_id_error",
    "libbirnam_id_isolate",
    "libbirnam_id_relational",
    "libbirnam_id_alien",
    "libbirnam_id_image",
];

/// Source-backed self-hosted modules. `modules` is intentionally separate:
/// the manager must be installed first because it implements the dependency
/// registration and ordering protocol used by the remaining modules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelfHostedModule {
    pub name: &'static str,
    pub source: &'static [u8],
}

pub const SELF_HOSTED_MODULES: &[SelfHostedModule] = &[
    SelfHostedModule {
        name: "stdlib",
        source: include_bytes!("../../../src/id/stdlib.birnam"),
    },
    SelfHostedModule {
        name: "compiledUnit",
        source: include_bytes!("../../../src/id/compiled-unit.birnam"),
    },
    SelfHostedModule {
        name: "toml",
        source: include_bytes!("../../../src/id/toml.birnam"),
    },
    SelfHostedModule {
        name: "config",
        source: include_bytes!("../../../src/id/config.birnam"),
    },
    SelfHostedModule {
        name: "appConfig",
        source: include_bytes!("../../../src/id/app-config.birnam"),
    },
    SelfHostedModule {
        name: "isolate",
        source: include_bytes!("../../../src/id/isolate.birnam"),
    },
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BootstrapOrderError {
    UnknownSelfHostedDependency { module: String, dependency: String },
    Cycle { module: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapSourceError {
    RecordRejected,
    EvaluationRejected,
}

/// Scan the first `requires: #[...]` declaration using the same deliberately
/// small source convention as the Zig host. Dependencies outside the
/// self-hosted inventory are native/kernel modules and are already loaded.
pub fn self_hosted_requires(source: &[u8]) -> Vec<String> {
    let Some(requires_at) = source
        .windows(b"requires:".len())
        .position(|window| window == b"requires:")
    else {
        return Vec::new();
    };
    let rest = &source[requires_at + b"requires:".len()..];
    let Some(vector_at) = rest.windows(2).position(|window| window == b"#[") else {
        return Vec::new();
    };
    let mut result = Vec::new();
    let mut index = vector_at + 2;
    while index < rest.len() && rest[index] != b']' {
        if rest[index] == b'#' {
            let start = index + 1;
            let mut end = start;
            while end < rest.len() && (rest[end].is_ascii_alphanumeric() || rest[end] == b'_') {
                end += 1;
            }
            if end > start {
                result.push(String::from_utf8_lossy(&rest[start..end]).into_owned());
            }
            index = end;
        } else {
            index += 1;
        }
    }
    result
}

/// Return the stable dependency order used by self-hosted boot. Ties retain
/// the declared inventory order, while native/kernel dependencies are treated
/// as already satisfied by `LoadedStack`.
pub fn self_hosted_boot_order() -> Result<Vec<&'static str>, BootstrapOrderError> {
    let mut dependencies: Vec<Vec<String>> = SELF_HOSTED_MODULES
        .iter()
        .map(|module| self_hosted_requires(module.source))
        .collect();
    for (index, module) in SELF_HOSTED_MODULES.iter().enumerate() {
        dependencies[index].retain(|dependency| {
            SELF_HOSTED_MODULES
                .iter()
                .any(|candidate| candidate.name == dependency)
        });
        for dependency in self_hosted_requires(module.source) {
            if !SELF_HOSTED_MODULES
                .iter()
                .any(|candidate| candidate.name == dependency)
                && dependency != "modules"
            {
                // The source declaration names a non-self-hosted module. Keep
                // the distinction explicit so a typo cannot silently reorder
                // the self-hosted layer.
                let is_kernel = SCRIPT_MODULES.iter().any(|name| {
                    name.strip_prefix("libbirnam_id_")
                        .is_some_and(|kernel| kernel == dependency)
                        || (*name == "libbirnam_store" && dependency == "store")
                });
                if !is_kernel {
                    return Err(BootstrapOrderError::UnknownSelfHostedDependency {
                        module: module.name.to_owned(),
                        dependency,
                    });
                }
            }
        }
    }

    let mut order = Vec::with_capacity(SELF_HOSTED_MODULES.len());
    while order.len() < SELF_HOSTED_MODULES.len() {
        let mut progress = false;
        for (index, module) in SELF_HOSTED_MODULES.iter().enumerate() {
            if order.contains(&module.name)
                || !dependencies[index]
                    .iter()
                    .all(|dependency| order.contains(&dependency.as_str()))
            {
                continue;
            }
            order.push(module.name);
            progress = true;
        }
        if !progress {
            let module = SELF_HOSTED_MODULES
                .iter()
                .find(|module| !order.contains(&module.name))
                .expect("incomplete order must have a remaining module");
            return Err(BootstrapOrderError::Cycle {
                module: module.name.to_owned(),
            });
        }
    }
    Ok(order)
}

fn send_is_error(api: &IdApi, value: birnam_abi::Oop) -> bool {
    if unsafe { (api.isExported)(c"Error".as_ptr()) } == 0 {
        return false;
    }
    let is_error = unsafe { (api.intern)(c"isError".as_ptr()) };
    unsafe { (api.send)(value, is_error, std::ptr::null(), 0) == api.trueObj }
}

/// Execute one embedded self-hosted module through the frozen Codegen
/// protocol. This deliberately exposes only source recovery: selecting a
/// precompiled dylib, checking its load result, and deciding whether to fall
/// back to this helper remain explicit host policy at the next layer.
///
/// # Safety
///
/// `api` must be the live table returned by the selected floor, `codegen` must
/// be a live Codegen object from that table, and both must remain valid for
/// the duration of the synchronous sends.
pub unsafe fn record_and_eval_source(
    api: &IdApi,
    codegen: birnam_abi::Oop,
    module: &SelfHostedModule,
) -> Result<birnam_abi::Oop, BootstrapSourceError> {
    let name = CString::new(module.name).map_err(|_| BootstrapSourceError::RecordRejected)?;
    let source = unsafe { (api.mkString)(module.source.as_ptr(), module.source.len()) };
    let name_symbol = unsafe { (api.intern)(name.as_ptr()) };
    let record_selector = unsafe { (api.intern)(c"recordModuleSource:named:".as_ptr()) };
    let recorded =
        unsafe { (api.send)(codegen, record_selector, [source, name_symbol].as_ptr(), 2) };
    if send_is_error(api, recorded) {
        return Err(BootstrapSourceError::RecordRejected);
    }
    let eval_selector = unsafe { (api.intern)(c"eval:".as_ptr()) };
    let evaluated = unsafe { (api.send)(codegen, eval_selector, [source].as_ptr(), 1) };
    if send_is_error(api, evaluated) {
        return Err(BootstrapSourceError::EvaluationRejected);
    }
    Ok(evaluated)
}

/// Process-lifetime native stack owned by the host. Higher rungs are opened
/// through `IdApi`; successful handles are retained because methods/finalizers
/// may retain code pointers after `module_init` returns.
pub struct LoadedStack {
    pub floor: LoadedFloor,
    modules: Vec<NonNull<std::ffi::c_void>>,
}

impl LoadedStack {
    pub fn new(floor: LoadedFloor) -> Self {
        Self {
            floor,
            modules: Vec::new(),
        }
    }

    pub fn module_count(&self) -> usize {
        self.modules.len()
    }

    /// Load one higher rung through the selected floor's dynamic-loader API.
    /// No host-side `dlopen` is used after the floor has initialized.
    pub fn load_module(&mut self, path: &Path) -> Result<(), ModuleError> {
        let path_bytes = CString::new(path_bytes(path)).map_err(|_| ModuleError::EmbeddedNul)?;
        // SAFETY: the floor API is process-owned and the C string lives across
        // this synchronous call.
        let handle = unsafe { (self.floor.api().dlopen)(path_bytes.as_ptr(), RTLD_LAZY) };
        let Some(handle) = NonNull::new(handle) else {
            return Err(ModuleError::OpenFailed {
                path: path.to_path_buf(),
            });
        };
        let init_name = c"module_init";
        // SAFETY: the handle came from the selected floor's successful dlopen.
        let init = unsafe { (self.floor.api().dlsym)(handle.as_ptr(), init_name.as_ptr().cast()) };
        let Some(init) = NonNull::new(init) else {
            return Err(ModuleError::MissingModuleInit {
                path: path.to_path_buf(),
            });
        };
        // SAFETY: module_init is the frozen higher-rung ABI and the floor table
        // remains live for the module's initialization call.
        let init: birnam_abi::ModuleInitFn = unsafe { std::mem::transmute(init.as_ptr()) };
        unsafe {
            init(
                self.floor.api() as *const IdApi as *mut IdApi,
                std::ptr::null_mut(),
            )
        };
        self.modules.push(handle);
        Ok(())
    }

    /// Resolve and load the canonical native stack in the same order as the
    /// Zig host. Store/TCC retain their exact override paths; every other rung
    /// comes from the one executable-relative runtime library directory.
    pub fn load_script_modules(&mut self, resources: &Resources) -> Result<(), ModuleError> {
        let store_override = env::var_os("BIRNAM_STORE_DYLIB").map(PathBuf::from);
        let tcc_override = env::var_os("BIRNAM_TCC_DYLIB").map(PathBuf::from);
        for name in SCRIPT_MODULES {
            let path = if *name == "libbirnam_store" {
                store_override
                    .as_deref()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| resources.lib_dir.join("libbirnam_store.dylib"))
            } else if *name == "libbirnam_id_tcc" {
                tcc_override
                    .as_deref()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| resources.lib_dir.join("libbirnam_id_tcc.dylib"))
            } else {
                resources.lib_dir.join(format!("{name}.dylib"))
            };
            self.load_module(&path)?;
        }
        Ok(())
    }
}

const RTLD_LAZY: i32 = 1;

impl LoadedFloor {
    pub fn api(&self) -> &IdApi {
        // SAFETY: `id_init` returned a process-owned, non-null table and the
        // successful loader retains its dylib handle for this value's lifetime.
        unsafe { self.api.as_ref() }
    }

    pub fn handle(&self) -> NonNull<std::ffi::c_void> {
        self.handle
    }
}

/// Load and initialize one selected floor after optional digest/identity
/// checks. This is the only raw loader seam in the Rust host; higher rungs are
/// loaded through the returned IdApi in later R7 slices.
pub fn load_floor(
    path: &Path,
    expected_identity: Option<&str>,
    expected_digest: Option<&str>,
) -> Result<LoadedFloor, FloorError> {
    let bytes = std::fs::read(path).map_err(|error| FloorError::Read {
        path: path.to_path_buf(),
        source: error.kind(),
    })?;
    if let Some(expected) = expected_digest {
        let actual = String::from_utf8_lossy(&sha256_hex(&bytes)).into_owned();
        if actual != expected {
            return Err(FloorError::DigestMismatch {
                expected: expected.to_owned(),
                actual,
            });
        }
    }
    let path = CString::new(path_bytes(path)).map_err(|_| FloorError::EmbeddedNul)?;
    // SAFETY: the path is NUL-terminated and remains live for the call.
    let handle = unsafe { system_dlopen(path.as_ptr(), RTLD_NOW | RTLD_LOCAL) };
    let Some(handle) = NonNull::new(handle) else {
        return Err(FloorError::OpenFailed);
    };
    let init_name = c"id_init";
    // SAFETY: `handle` came from a successful `dlopen`; the symbol name is a
    // static NUL-terminated string.
    let init = unsafe { system_dlsym(handle.as_ptr(), init_name.as_ptr().cast()) };
    let Some(init) = NonNull::new(init) else {
        unsafe { system_dlclose(handle.as_ptr()) };
        return Err(FloorError::MissingIdInit);
    };
    if let Some(expected) = expected_identity {
        let identity_name = c"birnam_id_floor_identity";
        // SAFETY: the handle is live and the static symbol name is valid.
        let identity_symbol =
            unsafe { system_dlsym(handle.as_ptr(), identity_name.as_ptr().cast()) };
        let Some(identity_symbol) = NonNull::new(identity_symbol) else {
            unsafe { system_dlclose(handle.as_ptr()) };
            return Err(FloorError::MissingIdentity);
        };
        // SAFETY: the floor export is the frozen `fn() -> *const char` identity.
        let identity_fn: unsafe extern "C" fn() -> *const std::ffi::c_char =
            unsafe { std::mem::transmute(identity_symbol.as_ptr()) };
        let actual = unsafe { CStr::from_ptr(identity_fn()) }
            .to_string_lossy()
            .into_owned();
        if actual != expected {
            unsafe { system_dlclose(handle.as_ptr()) };
            return Err(FloorError::IdentityMismatch {
                expected: expected.to_owned(),
                actual,
            });
        }
    }
    // SAFETY: `init` is the frozen `IdInitFn` export and the dylib remains live.
    let init: IdInitFn = unsafe { std::mem::transmute(init.as_ptr()) };
    let api = NonNull::new(unsafe { init() }).ok_or_else(|| {
        unsafe { system_dlclose(handle.as_ptr()) };
        FloorError::NullApi
    })?;
    Ok(LoadedFloor {
        handle,
        api,
        _owner_thread: PhantomData,
    })
}

/// Load the selected floor using the same optional environment checks as the
/// existing Zig host. Missing variables preserve the historical no-check mode.
pub fn load_selected_floor(path: &Path) -> Result<LoadedFloor, FloorError> {
    let identity = env::var("BIRNAM_ID_FLOOR_IDENTITY").ok();
    let digest = env::var("BIRNAM_ID_FLOOR_SHA256").ok();
    load_floor(path, identity.as_deref(), digest.as_deref())
}

const RTLD_NOW: i32 = 2;
const RTLD_LOCAL: i32 = 4;

unsafe extern "C" {
    #[link_name = "dlopen"]
    fn system_dlopen(path: *const std::ffi::c_char, mode: i32) -> *mut std::ffi::c_void;
    #[link_name = "dlsym"]
    fn system_dlsym(
        handle: *mut std::ffi::c_void,
        name: *const std::ffi::c_char,
    ) -> *mut std::ffi::c_void;
    #[link_name = "dlclose"]
    fn system_dlclose(handle: *mut std::ffi::c_void) -> i32;
}

fn path_bytes(path: &Path) -> Vec<u8> {
    #[cfg(unix)]
    {
        path.as_os_str().as_bytes().to_vec()
    }
    #[cfg(not(unix))]
    {
        path.to_string_lossy().as_bytes().to_vec()
    }
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
            *word = u32::from_be_bytes(chunk[index * 4..index * 4 + 4].try_into().unwrap());
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

/// Resolve one installed layout from the directory containing the executable.
///
/// The resolver normalizes paths lexically and never requires a resource to
/// exist. This keeps startup deterministic for diagnostics and preserves the
/// host's ability to report one missing resource at the actual load boundary.
pub fn resolve_from_exe_dir(exe_dir: &Path, overrides: Overrides<'_>) -> io::Result<Resources> {
    let exe_dir = absolute_normalized(exe_dir)?;
    let prefix = exe_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| exe_dir.clone());
    let lib_dir = override_or(overrides.lib_dir, &prefix.join("lib"))?;
    let source_dir = normalize_absolute(&prefix.join("share/birnam/src/id"))?;
    let rust_workspace_dir = override_or(
        overrides.rust_workspace_dir,
        &prefix.join("share/birnam/src"),
    )?;
    let guide_dir = normalize_absolute(&prefix.join("share/birnam/docs/guide"))?;
    let id_dylib = override_or(overrides.id_dylib, &lib_dir.join("libbirnam_id.dylib"))?;

    Ok(Resources {
        prefix,
        lib_dir,
        source_dir,
        rust_workspace_dir,
        guide_dir,
        id_dylib,
    })
}

/// Resolve the production layout from the current executable and the same
/// narrow environment overrides accepted by the Zig host.
pub fn resolve() -> io::Result<Resources> {
    let executable = env::current_exe()?;
    let lib_dir = env::var_os("BIRNAM_LIB_DIR").map(PathBuf::from);
    let id_dylib = env::var_os("BIRNAM_ID_DYLIB").map(PathBuf::from);
    let rust_workspace_dir = env::var_os("BIRNAM_RUST_WORKSPACE_DIR").map(PathBuf::from);
    resolve_from_exe_dir(
        executable.parent().unwrap_or_else(|| Path::new(".")),
        Overrides {
            lib_dir: lib_dir.as_deref(),
            id_dylib: id_dylib.as_deref(),
            rust_workspace_dir: rust_workspace_dir.as_deref(),
        },
    )
}

fn override_or(override_path: Option<&Path>, default_path: &Path) -> io::Result<PathBuf> {
    match override_path {
        Some(path) => normalize_absolute(path),
        None => normalize_absolute(default_path),
    }
}

fn absolute_normalized(path: &Path) -> io::Result<PathBuf> {
    if path.is_absolute() {
        normalize_absolute(path)
    } else {
        normalize_absolute(&env::current_dir()?.join(path))
    }
}

fn normalize_absolute(path: &Path) -> io::Result<PathBuf> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()?.join(path)
    };
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(Path::new(std::path::MAIN_SEPARATOR_STR)),
            Component::CurDir => {}
            Component::ParentDir => {
                if normalized.file_name().is_some() {
                    normalized.pop();
                }
            }
            Component::Normal(part) => normalized.push(part),
        }
    }
    if !normalized.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "runtime resource path did not resolve absolute",
        ));
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_one_normalized_executable_relative_layout() {
        let resources = resolve_from_exe_dir(Path::new("/opt/birnam/bin"), Overrides::default())
            .expect("layout should resolve");
        assert_eq!(resources.prefix, Path::new("/opt/birnam"));
        assert_eq!(resources.lib_dir, Path::new("/opt/birnam/lib"));
        assert_eq!(
            resources.source_dir,
            Path::new("/opt/birnam/share/birnam/src/id")
        );
        assert_eq!(
            resources.rust_workspace_dir,
            Path::new("/opt/birnam/share/birnam/src")
        );
        assert_eq!(
            resources.guide_dir,
            Path::new("/opt/birnam/share/birnam/docs/guide")
        );
        assert_eq!(
            resources.id_dylib,
            Path::new("/opt/birnam/lib/libbirnam_id.dylib")
        );
    }

    #[test]
    fn overrides_remain_narrow_and_exact() {
        let lib_dir = Path::new("/tmp/runtime/lib/../lib");
        let id_dylib = Path::new("/tmp/floor/custom-id.dylib");
        let workspace = Path::new("/tmp/native-workspace/../native-workspace");
        let resources = resolve_from_exe_dir(
            Path::new("/opt/birnam/bin"),
            Overrides {
                lib_dir: Some(lib_dir),
                id_dylib: Some(id_dylib),
                rust_workspace_dir: Some(workspace),
            },
        )
        .expect("overrides should resolve");
        assert_eq!(resources.lib_dir, Path::new("/tmp/runtime/lib"));
        assert_eq!(resources.id_dylib, Path::new("/tmp/floor/custom-id.dylib"));
        assert_eq!(
            resources.rust_workspace_dir,
            Path::new("/tmp/native-workspace")
        );
        assert_eq!(
            resources.source_dir,
            Path::new("/opt/birnam/share/birnam/src/id")
        );
    }

    #[test]
    fn a_lib_override_supplies_the_default_id_dylib() {
        let resources = resolve_from_exe_dir(
            Path::new("/opt/birnam/bin"),
            Overrides {
                lib_dir: Some(Path::new("/tmp/runtime-lib")),
                ..Overrides::default()
            },
        )
        .expect("lib override should resolve");
        assert_eq!(
            resources.id_dylib,
            Path::new("/tmp/runtime-lib/libbirnam_id.dylib")
        );
    }

    #[test]
    fn lexical_normalization_does_not_require_files_to_exist() {
        let resources = resolve_from_exe_dir(
            Path::new("/tmp/nonexistent/../birnam/bin"),
            Overrides::default(),
        )
        .expect("lexical paths should resolve without filesystem access");
        assert_eq!(resources.prefix, Path::new("/tmp/birnam"));
    }

    #[test]
    fn sha256_matches_the_floor_digest_format() {
        assert_eq!(
            sha256_hex(b"abc"),
            *b"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn missing_floor_is_rejected_before_dlopen() {
        assert!(matches!(
            load_floor(Path::new("/tmp/birnam-host-no-floor.dylib"), None, None),
            Err(FloorError::Read { .. })
        ));
    }

    #[test]
    fn pure_help_and_version_exits_precede_resource_resolution() {
        assert_eq!(classify_early_args(&["--help"]), EarlyAction::Help);
        assert_eq!(classify_early_args(&["-V"]), EarlyAction::Version);
        assert_eq!(
            classify_early_args(&["project.birnam"]),
            EarlyAction::Continue
        );
        assert_eq!(classify_early_args::<&str>(&[]), EarlyAction::Continue);
    }

    #[test]
    fn embedded_self_hosted_sources_preserve_declared_dependencies() {
        let compiled_unit = SELF_HOSTED_MODULES
            .iter()
            .find(|module| module.name == "compiledUnit")
            .expect("compiledUnit source is embedded");
        assert_eq!(
            self_hosted_requires(compiled_unit.source),
            ["modules", "stdlib"]
        );
        let config = SELF_HOSTED_MODULES
            .iter()
            .find(|module| module.name == "config")
            .expect("config source is embedded");
        assert_eq!(self_hosted_requires(config.source), ["toml"]);
    }

    #[test]
    fn self_hosted_boot_order_is_stable_and_dependency_safe() {
        assert_eq!(
            self_hosted_boot_order().expect("embedded graph is acyclic"),
            [
                "stdlib",
                "compiledUnit",
                "toml",
                "config",
                "appConfig",
                "isolate"
            ]
        );
    }

    #[test]
    fn dependency_scanner_ignores_sources_without_a_declaration() {
        assert!(self_hosted_requires(b"// no dependency declaration\n").is_empty());
    }

    #[test]
    fn embedded_self_hosted_inventory_is_nonempty_and_unique() {
        for (index, module) in SELF_HOSTED_MODULES.iter().enumerate() {
            assert!(!module.name.is_empty());
            assert!(!module.source.is_empty());
            assert!(
                SELF_HOSTED_MODULES[index + 1..]
                    .iter()
                    .all(|other| other.name != module.name),
                "duplicate self-hosted module name: {}",
                module.name
            );
        }
    }
}
