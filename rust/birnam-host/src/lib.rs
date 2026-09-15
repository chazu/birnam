//! Rust host coordination and stable public entry points.
//! Command parsing, project data, filesystem work, live project policy and
//! startup mechanics have private owners. HostRuntime and its address-fenced
//! bridge lifetime remain coordinated here.
mod startup;
#[cfg(test)]
use startup::selector_has_arity_one;
pub use startup::{
    ArtifactActivationError,
    ArtifactEntry,
    ArtifactPlan,
    ArtifactPreflightError,
    BAKE_MAGIC,
    BAKE_TRAILER_LEN,
    BakeError,
    BootstrapOrderError,
    BootstrapRuntimeError,
    BootstrapSourceError,
    CliError,
    CompileModuleError,
    FloorError,
    ImageEntryError,
    ImageError,
    ImageLoadOutcome,
    IsolateHookError,
    IsolateHooks,
    LoadedFloor,
    LoadedSelfHostedModule,
    LoadedStack,
    MAX_SCRIPT_BYTES,
    MODULE_MANAGER,
    ModuleAotPolicy,
    ModuleError,
    Overrides,
    PROJECT_POLICY_MODULE,
    PhaseHookError,
    PhaseHooks,
    ReplError,
    ReplSession,
    ReplStep,
    Resources,
    RuntimeModuleError,
    RuntimeStartError,
    SCRIPT_MODULES,
    SELF_HOSTED_MODULES,
    ScriptError,
    SelfHostedLoadOutcome,
    SelfHostedModule,
    SocketReplError,
    StartupPolicy,
    WorkerError,
    WorkerStartupError,
    activate_artifact,
    activate_preflighted_artifact,
    bake_executable,
    baked_image,
    bootstrap_self_hosted,
    compile_module,
    eval_script,
    host_target_identity,
    load_floor,
    load_image,
    load_image_bytes,
    load_selected_floor,
    load_self_hosted,
    preflight_artifact,
    read_baked_image,
    record_and_eval_source,
    resolve,
    resolve_from_exe_dir,
    self_hosted_boot_order,
    self_hosted_requires,
    start,
    start_from_environment,
};
use startup::{bridge_string, error_diagnostic, global_named, mark_module_loaded, native_os_string_vector, path_bytes, read_artifact_bytes, send_is_error};

mod project_runtime;
pub use project_runtime::{decode_project_source_metadata, decode_project_manifest, decode_project_resolution, decode_project_artifact_plan};

mod project_files;
pub use project_files::{
    collect_project_source_files,
    nearest_project_root,
    project_git_dependency_cache_path,
    publish_project_build,
    resolve_project_dependencies_cached,
    resolve_project_dependency_paths,
};
use project_files::{acquire_project_git_dependency, create_check_temp_directory, project_publication_paths};

mod project_data;
pub use project_data::{
    ProjectArtifactCompileError,
    ProjectArtifactPlan,
    ProjectArtifactPlanEntry,
    ProjectArtifactPlanError,
    ProjectArtifactPlanRequest,
    ProjectArtifactPlanValidationError,
    ProjectArtifactStageError,
    ProjectArtifactStageRequest,
    ProjectBridgeError,
    ProjectBuildError,
    ProjectBuildPublicationError,
    ProjectBuildRequest,
    ProjectBuildResult,
    ProjectCheckError,
    ProjectCheckResult,
    ProjectCompileDiagnostic,
    ProjectCompiledArtifactValidationError,
    ProjectCompositionError,
    ProjectCompositionPlanError,
    ProjectDependencyClosureError,
    ProjectDependencyClosureNode,
    ProjectDependencyObservation,
    ProjectDependencyResolutionError,
    ProjectDependencySourceObservation,
    ProjectDiscoveryError,
    ProjectFetchError,
    ProjectFetchResult,
    ProjectInstallError,
    ProjectManifestDecodeError,
    ProjectManifestDependencyObservation,
    ProjectManifestError,
    ProjectManifestObservation,
    ProjectMetadataError,
    ProjectModulePlan,
    ProjectObservation,
    ProjectObservationBuildError,
    ProjectObservationError,
    ProjectPolicyError,
    ProjectPreparationError,
    ProjectPreviousPublication,
    ProjectPreviousPublishedArtifact,
    ProjectResolutionObservation,
    ProjectRunError,
    ProjectRunObservation,
    ProjectSnapshot,
    ProjectSnapshotError,
    ProjectSourceIndexError,
    ProjectSourceIndexOutcome,
    ProjectSourceMetadata,
    ProjectSourceObservation,
    ProjectSourceWalkError,
    ProjectStagedArtifact,
    ProjectUriError,
    ProjectWrappedArtifactObservation,
    ProjectWrappedBuildError,
    ProjectWrappedObservation,
    expand_project_wrapped_argument,
    normalize_git_revision,
    parse_project_compile_diagnostic,
    project_build_resolution_digest,
    project_composition_paths,
    project_dependency_closure_order,
    project_path_from_uri,
    project_resolution_digest,
    validate_project_artifact_plan,
    validate_project_compiled_artifact,
    validate_project_observations,
};
use project_data::{PROJECT_APPLICATION_FILENAME, PROJECT_CONFIGURATION_FILENAME, PROJECT_MODULE_ROOT_FILENAME, PROJECT_REPL_GLOBAL, PROJECT_SOCKET_BIND_GLOBAL, PROJECT_SOCKET_PORT_GLOBAL, previous_artifact, project_application_source, project_configuration_source, project_module_root_source, safe_artifact_relative_path, valid_git_remote};

mod command;
pub use command::{
    EarlyAction,
    CommandKind,
    CommandError,
    OutputFormat,
    CheckOptions,
    FetchOptions,
    BuildOptions,
    InstallOptions,
    TestOptions,
    ProjectTestWorkerOptions,
    ProjectTestWorkerParseError,
    ReplMode,
    RunOptions,
    RunParseError,
    BakeOptions,
    RebuildOptions,
    CompileModuleOptions,
    ToolingParseError,
    WorkerOptions,
    WorkerParseError,
    ParseOutcome,
    ProjectParseError,
    parse_check_args,
    parse_fetch_args,
    parse_build_args,
    parse_install_args,
    parse_test_args,
    parse_project_test_worker_args,
    parse_run_args,
    parse_bake_args,
    parse_rebuild_args,
    parse_compile_module_args,
    parse_worker_args,
    classify_command,
    classify_early_args,
};

use birnam_artifact::digest::sha256_hex;
use std::collections::HashSet;
use std::env;
use std::ffi::{CStr, CString, OsStr, OsString, c_char, c_int, c_void};
use std::fmt;
use std::io;
use std::io::{Read, Seek, SeekFrom};
use std::marker::PhantomData;
use std::path::{Component, Path, PathBuf};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicPtr, AtomicU64, Ordering};

use birnam_abi::{IdApi, IdInitFn};

#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;

pub struct HostRuntime {
    pub resources: Resources,
    pub stack: LoadedStack,
    pub isolate: IsolateHooks,
    pub policy: StartupPolicy,
    pub codegen: birnam_abi::Oop,
    pub self_hosted: Vec<LoadedSelfHostedModule>,
}

static PROJECT_BRIDGE_RUNTIME: AtomicPtr<HostRuntime> = AtomicPtr::new(std::ptr::null_mut());

impl Drop for HostRuntime {
    fn drop(&mut self) {
        let _ = PROJECT_BRIDGE_RUNTIME.compare_exchange(
            self as *mut HostRuntime,
            std::ptr::null_mut(),
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }
}

pub mod bootstrap_receipt;
pub mod publication_receipt;
mod receipt_validation;

#[cfg(test)]
mod host_bridge_contract;
