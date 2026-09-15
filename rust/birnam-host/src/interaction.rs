//! CLI, REPL, image entry and worker execution on the existing host runtime.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerError {
    Failed(c_int),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerStartupError {
    Image(ImageError),
    EmbeddedNul,
    MissingDispatchGeneration,
    InvalidDispatchGeneration,
    MissingCodegen,
    InvalidGenerationDigest,
    WrongArity,
    MissingGlobals,
    ReceiverNotFound,
    SelectorNotFound,
    Driver(WorkerError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliError {
    Module(RuntimeModuleError),
    Policy(ProjectPolicyError),
    Bridge(ProjectBridgeError),
    MissingCli,
    MissingVector,
    NonUtf8Argument(usize),
    ArgumentVectorRejected,
    MalformedStatus,
    Failed(i64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplError {
    Module(RuntimeModuleError),
    MissingRepl,
    MissingDispatchGeneration,
    MissingEvolutionIntent,
    InitializationRejected,
    GenerationRejected,
    Phase(PhaseHookError),
    ReadyRejected,
    MalformedStep,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SocketReplError {
    Module(RuntimeModuleError),
    MissingRepl,
    ServeRejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplStep {
    Done,
    Evaluated(birnam_abi::Oop),
    Other,
}

pub struct ReplSession {
    repl: birnam_abi::Oop,
    phases: PhaseHooks,
    done: bool,
    finished: bool,
}

impl HostRuntime {
    /// Dispatch one Birnam-owned CLI subcommand through the compiled `Cli`
    /// module. Rust supplies opaque argv and the installed resource roots;
    /// subcommand parsing, diagnostics, and module policy remain language
    /// owned. Project-aware LSP setup installs the scoped Rust `Project`
    /// bridge before dispatch and remains on the same host observation path.
    ///
    /// # Safety
    ///
    /// The caller must have called [`HostRuntime::prepare_execution`] with a
    /// live stack anchor and remain on the floor owner thread for all sends.
    pub unsafe fn run_cli(&self, args: &[OsString]) -> Result<(), CliError> {
        let is_lsp = args.iter().any(|argument| argument == "--lsp");
        if is_lsp {
            unsafe { self.load_project_policy() }.map_err(CliError::Policy)?;
            unsafe { self.install_project_bridge() }.map_err(CliError::Bridge)?;
        }
        unsafe { self.load_runtime_module("cli") }.map_err(CliError::Module)?;
        let api = self.stack.api();
        let Some(cli) = (unsafe { global_named(api, "Cli") }) else {
            return Err(CliError::MissingCli);
        };
        let values = unsafe { native_os_string_vector(api, args) }?;
        let rust_workspace = path_bytes(&self.resources.rust_workspace_dir);
        let guides = path_bytes(&self.resources.guide_dir);
        let selector = unsafe { (api.intern)(c"run:rustWorkspace:guides:".as_ptr()) };
        let result = unsafe {
            (api.send)(
                cli,
                selector,
                [
                    values,
                    (api.mkString)(rust_workspace.as_ptr(), rust_workspace.len()),
                    (api.mkString)(guides.as_ptr(), guides.len()),
                ]
                .as_ptr(),
                3,
            )
        };
        if unsafe { (api.isSmallInt)(result) } == 0 {
            return Err(CliError::MalformedStatus);
        }
        let status = unsafe { (api.smallIntValue)(result) };
        if status == 0 {
            Ok(())
        } else {
            Err(CliError::Failed(status))
        }
    }

    /// Initialize a terminal REPL session and enter the runtime's running
    /// phase. Input/output is still owned by the self-hosted Repl module; Rust
    /// only drives its frozen lifecycle selectors.
    ///
    /// # Safety
    ///
    /// The caller must have called [`HostRuntime::prepare_execution`] with a
    /// live stack anchor and remain on the floor owner thread.
    pub unsafe fn start_repl(&self) -> Result<ReplSession, ReplError> {
        unsafe { self.start_repl_mode(false) }
    }

    /// Run the conventional global `main` block after image recovery. Absence
    /// answers `false` so the caller can open a REPL; presence enters the
    /// running phase and sends `value` through ordinary dispatch.
    ///
    /// # Safety
    ///
    /// The caller must have prepared a live GC anchor and remain on the floor
    /// owner thread for generation publication, phase transition, and dispatch.
    pub unsafe fn run_image_main(&self) -> Result<bool, ImageEntryError> {
        let api = self.stack.api();
        let Some(main) = (unsafe { global_named(api, "main") }) else {
            return Ok(false);
        };
        let Some(generations) = (unsafe { global_named(api, "DispatchGeneration") }) else {
            return Err(ImageEntryError::MissingDispatchGeneration);
        };
        let initialize_selector = unsafe { (api.intern)(c"initializeCurrent".as_ptr()) };
        let generation =
            unsafe { (api.send)(generations, initialize_selector, std::ptr::null(), 0) };
        if send_is_error(api, generation) {
            return Err(ImageEntryError::GenerationRejected);
        }
        let phases =
            resolve_phase_hooks(&self.stack, &self.resources).map_err(ImageEntryError::Phase)?;
        if !phases.enter_running() {
            return Err(ImageEntryError::Phase(PhaseHookError::InvalidPhase));
        }
        let value_selector = unsafe { (api.intern)(c"value".as_ptr()) };
        let result = unsafe { (api.send)(main, value_selector, std::ptr::null(), 0) };
        if result.is_null() || send_is_error(api, result) {
            return Err(ImageEntryError::EvaluationRejected);
        }
        Ok(true)
    }

    /// Start a REPL after image recovery. The image's Module manager may be a
    /// restored application object, so ordinary `Module load:` is deliberately
    /// not called here; the host-capability path is the process-local recovery
    /// boundary and is used only when the restored Repl surface is absent.
    ///
    /// # Safety
    ///
    /// Same owner-thread and GC-anchor requirements as [`HostRuntime::start_repl`].
    pub unsafe fn start_repl_after_image(&self) -> Result<ReplSession, ReplError> {
        unsafe { self.start_repl_mode(true) }
    }

    unsafe fn start_repl_mode(&self, image_booted: bool) -> Result<ReplSession, ReplError> {
        // A normal boot can introduce Repl into G0. An image may already have
        // sealed its application generation, so recover the same process-local
        // host capability used by the image path when ordinary loading is
        // rejected.
        let normal_load_rejected =
            !image_booted && unsafe { self.load_runtime_module("repl") }.is_err();
        let init_selector = unsafe { (self.stack.api().intern)(c"consoleHostInit".as_ptr()) };
        // Image recovery may leave the restored Globals/Module objects in a
        // partially rebuilt state where even reading `Repl` through the normal
        // namespace path can recurse. Always refresh this process-local tool
        // after an image; normal boots retain the selector-surface check.
        let stale_repl = if image_booted {
            true
        } else {
            unsafe { global_named(self.stack.api(), "Repl") }
                .map(|repl| unsafe {
                    !receiver_has_selector(self.stack.api(), repl, init_selector)
                })
                .unwrap_or(true)
        };
        if normal_load_rejected || stale_repl {
            unsafe { self.load_host_runtime_module("repl") }.map_err(ReplError::Module)?;
        }
        let api = self.stack.api();
        let Some(repl) = (unsafe { global_named(api, "Repl") }) else {
            return Err(ReplError::MissingRepl);
        };
        let initialized = unsafe { (api.send)(repl, init_selector, std::ptr::null(), 0) };
        if send_is_error(api, initialized) {
            return Err(ReplError::InitializationRejected);
        }
        let Some(generations) = (unsafe { global_named(api, "DispatchGeneration") }) else {
            return Err(ReplError::MissingDispatchGeneration);
        };
        let initialize_selector = unsafe { (api.intern)(c"initializeCurrent".as_ptr()) };
        let generation =
            unsafe { (api.send)(generations, initialize_selector, std::ptr::null(), 0) };
        if send_is_error(api, generation) {
            return Err(ReplError::GenerationRejected);
        }
        let phases = resolve_phase_hooks(&self.stack, &self.resources).map_err(ReplError::Phase)?;
        if !phases.enter_running() {
            return Err(ReplError::Phase(PhaseHookError::InvalidPhase));
        }
        let ready_selector = unsafe { (api.intern)(c"consoleHostReady".as_ptr()) };
        let ready = unsafe { (api.send)(repl, ready_selector, std::ptr::null(), 0) };
        if send_is_error(api, ready) {
            return Err(ReplError::ReadyRejected);
        }
        Ok(ReplSession {
            repl,
            phases,
            done: false,
            finished: false,
        })
    }

    /// Drive one Repl `hostStep`; the module owns input parsing and output
    /// policy. An evaluated value is returned as an Oop so callers can handle
    /// EvolutionIntent/quiescence exactly at the host boundary.
    ///
    /// # Safety
    ///
    /// `session` must belong to this runtime, and the caller must keep the GC
    /// anchor from `prepare_execution` live.
    pub unsafe fn repl_step(&self, session: &mut ReplSession) -> Result<ReplStep, ReplError> {
        if session.done || session.finished {
            return Ok(ReplStep::Done);
        }
        let api = self.stack.api();
        let step_selector = unsafe { (api.intern)(c"hostStep".as_ptr()) };
        let step = unsafe { (api.send)(session.repl, step_selector, std::ptr::null(), 0) };
        if step == api.nilObj {
            return Err(ReplError::MalformedStep);
        }
        let at_selector = unsafe { (api.intern)(c"at:".as_ptr()) };
        let tag = unsafe { (api.send)(step, at_selector, [(api.mkSmallInt)(0)].as_ptr(), 1) };
        if tag == api.nilObj {
            return Err(ReplError::MalformedStep);
        }
        let tag_name = unsafe { (api.symbolName)(tag) };
        if tag_name.is_null() {
            return Err(ReplError::MalformedStep);
        }
        if unsafe { CStr::from_ptr(tag_name) }.to_bytes() == b"done" {
            session.done = true;
            return Ok(ReplStep::Done);
        }
        if unsafe { CStr::from_ptr(tag_name) }.to_bytes() == b"evaluated" {
            let value = unsafe { (api.send)(step, at_selector, [(api.mkSmallInt)(1)].as_ptr(), 1) };
            return Ok(ReplStep::Evaluated(value));
        }
        Ok(ReplStep::Other)
    }

    /// Finish a REPL session and release its running-phase ownership.
    ///
    /// # Safety
    ///
    /// `session` must belong to this runtime and the caller must remain on the
    /// floor owner thread.
    pub unsafe fn finish_repl(&self, session: &mut ReplSession) {
        if session.finished {
            return;
        }
        let api = self.stack.api();
        let selector = unsafe { (api.intern)(c"consoleDone".as_ptr()) };
        unsafe { (api.send)(session.repl, selector, std::ptr::null(), 0) };
        session.finished = true;
    }

    /// Quiesce the runtime around an EvolutionIntent fulfillment.
    ///
    /// # Safety
    ///
    /// The caller must be on the owner thread and invoke `end_quiescence` only
    /// after a successful `begin_quiescence`.
    pub unsafe fn begin_quiescence(&self, session: &ReplSession) -> Result<(), ReplError> {
        if session.phases.begin_quiescence() {
            Ok(())
        } else {
            Err(ReplError::Phase(PhaseHookError::InvalidPhase))
        }
    }

    /// # Safety
    ///
    /// See [`HostRuntime::begin_quiescence`].
    pub unsafe fn end_quiescence(&self, session: &ReplSession) -> Result<(), ReplError> {
        if session.phases.end_quiescence() {
            Ok(())
        } else {
            Err(ReplError::Phase(PhaseHookError::InvalidPhase))
        }
    }

    /// Run the complete terminal REPL loop until `hostStep` reports `done`.
    /// Evolution intents are fulfilled only inside the Codegen quiescence
    /// phase, and result presentation remains delegated to Repl.
    ///
    /// # Safety
    ///
    /// `session` must belong to this runtime; the caller must have established
    /// a live GC anchor and remain on the floor owner thread.
    pub unsafe fn run_repl(&self, session: &mut ReplSession) -> Result<(), ReplError> {
        let api = self.stack.api();
        let Some(intents) = (unsafe { global_named(api, "EvolutionIntent") }) else {
            return Err(ReplError::MissingEvolutionIntent);
        };
        let intent_selector = unsafe { (api.intern)(c"isIntent:".as_ptr()) };
        let fulfill_selector = unsafe { (api.intern)(c"fulfill".as_ptr()) };
        let print_selector = unsafe { (api.intern)(c"printHostResult:".as_ptr()) };
        loop {
            match unsafe { self.repl_step(session) }? {
                ReplStep::Done => break,
                ReplStep::Other => {}
                ReplStep::Evaluated(mut value) => {
                    let is_intent =
                        unsafe { (api.send)(intents, intent_selector, [value].as_ptr(), 1) }
                            == api.trueObj;
                    if is_intent {
                        unsafe { self.begin_quiescence(session) }?;
                        value = unsafe { (api.send)(value, fulfill_selector, std::ptr::null(), 0) };
                        unsafe { self.end_quiescence(session) }?;
                    }
                    unsafe { (api.send)(session.repl, print_selector, [value].as_ptr(), 1) };
                }
            }
            unsafe { self.drain_isolates() };
        }
        unsafe { self.finish_repl(session) };
        Ok(())
    }

    /// Serve the socket REPL through the self-hosted `Repl serveOn:bind:`
    /// protocol. The socket module is loaded before Repl, matching the
    /// host's runtime-tool dependency order.
    ///
    /// # Safety
    ///
    /// The runtime must remain on the selected floor's owner thread and the
    /// bind string must remain valid for the synchronous send.
    pub unsafe fn serve_socket(&self, port: u16, bind: &str) -> Result<(), SocketReplError> {
        unsafe { self.load_runtime_module("socket") }.map_err(SocketReplError::Module)?;
        unsafe { self.load_runtime_module("repl") }.map_err(SocketReplError::Module)?;
        let api = self.stack.api();
        let Some(repl) = (unsafe { global_named(api, "Repl") }) else {
            return Err(SocketReplError::MissingRepl);
        };
        let bind = CString::new(bind).map_err(|_| SocketReplError::ServeRejected)?;
        let bind = unsafe { (api.mkString)(bind.as_bytes().as_ptr(), bind.as_bytes().len()) };
        let selector = unsafe { (api.intern)(c"serveOn:bind:".as_ptr()) };
        let result = unsafe {
            (api.send)(
                repl,
                selector,
                [(api.mkSmallInt)(i64::from(port)), bind].as_ptr(),
                2,
            )
        };
        if send_is_error(api, result) {
            return Err(SocketReplError::ServeRejected);
        }
        Ok(())
    }

    /// Run one isolate worker over an inherited channel descriptor. The driver
    /// borrows the receiver/selector and never takes ownership of the fd.
    ///
    /// # Safety
    ///
    /// `fd` must be a live inherited channel owned by the caller; `receiver` and
    /// `selector` must be live Oops from this runtime; the caller must remain on
    /// the floor owner thread for the complete worker loop.
    pub unsafe fn run_worker(
        &self,
        fd: c_int,
        receiver: birnam_abi::Oop,
        selector: birnam_abi::Oop,
    ) -> Result<(), WorkerError> {
        let api = self.stack.api() as *const IdApi as *mut IdApi;
        let status = unsafe { self.isolate.driver(api, fd, receiver, selector) };
        if status == 0 {
            Ok(())
        } else {
            Err(WorkerError::Failed(status))
        }
    }

    /// Recover one worker image, validate its durable generation, resolve the
    /// named receiver/selector through Globals/import and the live vtable
    /// chain, then invoke the retained isolate driver. The inherited channel
    /// descriptor remains borrowed; the caller owns close/error framing.
    ///
    /// # Safety
    ///
    /// `fd` must be a live inherited isolate channel, `image` must be readable
    /// by the selected runtime, and the caller must keep the GC anchor live on
    /// the floor owner thread for the complete operation.
    pub unsafe fn run_worker_image(
        &mut self,
        image: &Path,
        receiver_name: &str,
        selector_name: &str,
        fd: c_int,
    ) -> Result<(), WorkerStartupError> {
        unsafe { load_image(self, image, false) }.map_err(WorkerStartupError::Image)?;
        let api = self.stack.api();
        let Some(generations) = (unsafe { global_named(api, "DispatchGeneration") }) else {
            return Err(WorkerStartupError::MissingDispatchGeneration);
        };
        let current_selector = unsafe { (api.intern)(c"current".as_ptr()) };
        let current = unsafe { (api.send)(generations, current_selector, std::ptr::null(), 0) };
        if current == api.nilObj || send_is_error(api, current) {
            return Err(WorkerStartupError::InvalidDispatchGeneration);
        }
        let codegen =
            unsafe { global_named(api, "Codegen") }.ok_or(WorkerStartupError::MissingCodegen)?;
        let digest_selector = unsafe { (api.intern)(c"currentGenerationDigest".as_ptr()) };
        let digest = unsafe { (api.send)(codegen, digest_selector, std::ptr::null(), 0) };
        if digest == api.nilObj || unsafe { (api.vtableOf)(digest) } != api.string_vt {
            return Err(WorkerStartupError::InvalidGenerationDigest);
        }
        let mut digest_len = 0usize;
        let digest_bytes = unsafe { (api.stringBytes)(digest, &mut digest_len) };
        if digest_bytes.is_null() || digest_len != 64 {
            return Err(WorkerStartupError::InvalidGenerationDigest);
        }
        if !selector_has_arity_one(selector_name) {
            return Err(WorkerStartupError::WrongArity);
        }
        let receiver_name =
            CString::new(receiver_name).map_err(|_| WorkerStartupError::EmbeddedNul)?;
        let selector_name =
            CString::new(selector_name).map_err(|_| WorkerStartupError::EmbeddedNul)?;
        let globals =
            unsafe { global_named(api, "Globals") }.ok_or(WorkerStartupError::MissingGlobals)?;
        let receiver_symbol = unsafe { (api.intern)(receiver_name.as_ptr()) };
        let at_selector = unsafe { (api.intern)(c"at:".as_ptr()) };
        let mut receiver =
            unsafe { (api.send)(globals, at_selector, [receiver_symbol].as_ptr(), 1) };
        if receiver == api.nilObj {
            receiver = unsafe { (api.import)(receiver_name.as_ptr()) };
        }
        if receiver == api.nilObj {
            return Err(WorkerStartupError::ReceiverNotFound);
        }
        let selector = unsafe { (api.intern)(selector_name.as_ptr()) };
        if receiver == api.nilObj || !unsafe { receiver_has_selector(api, receiver, selector) } {
            return Err(WorkerStartupError::SelectorNotFound);
        }
        unsafe { self.run_worker(fd, receiver, selector) }.map_err(WorkerStartupError::Driver)
    }

    /// # Safety
    ///
    /// The host must remain on the floor owner thread.
    pub unsafe fn drain_isolates(&self) {
        unsafe { self.isolate.drain() };
    }

    /// # Safety
    ///
    /// The host must remain on the floor owner thread and must not use this
    /// runtime's isolate hooks after shutdown.
    pub unsafe fn shutdown_isolates(&self) {
        unsafe { self.isolate.shutdown() };
    }
}
