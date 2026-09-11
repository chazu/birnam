//! Design 74G: generated entries, real driver, test-local executable ownership.

use super::*;
use crate::execution::RegistrationGenerationLease;
use birnam_artifact::materialization_selection::{Disposition, Row, SubjectKind};
use birnam_codegen::{resumable, resumable_c, resumable_descriptor};
use core::cell::RefCell;
use core::ffi::{c_char, c_int, c_long, c_void};
use sha2::{Digest, Sha256};
use std::ffi::{CStr, CString};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

#[path = "generated_deopt_program.rs"]
mod input;
#[path = "read_reuse_experiment.rs"]
mod read_reuse;

// Darwin constants. This native experiment is intentionally macOS-only.
const RTLD_NOW: c_int = 2;
const RTLD_LOCAL: c_int = 4;
const RTLD_NOLOAD: c_int = 0x10;
unsafe extern "C" {
    fn dlclose(handle: *mut c_void) -> c_int;
}

fn mapped(path: &CStr) -> bool {
    // SAFETY: NOLOAD never loads a missing library. Balance its reference.
    unsafe {
        let handle = crate::loader::open(path.as_ptr(), RTLD_NOW | RTLD_NOLOAD);
        if handle.is_null() {
            false
        } else {
            assert_eq!(dlclose(handle), 0);
            true
        }
    }
}

#[derive(Default)]
struct Unloads {
    count: usize,
}

struct NativeUnit {
    handle: *mut c_void,
    path: CString,
    unloads: Rc<RefCell<Unloads>>,
}

impl NativeUnit {
    fn load(source: &Path, destination: &Path, unloads: Rc<RefCell<Unloads>>) -> Rc<Self> {
        std::fs::copy(source, destination).unwrap();
        let path = CString::new(destination.to_str().unwrap()).unwrap();
        assert!(!mapped(&path));
        // SAFETY: this is our freshly compiled, unique fixture library.
        let handle = unsafe { crate::loader::open(path.as_ptr(), RTLD_NOW | RTLD_LOCAL) };
        assert!(!handle.is_null(), "cannot load {}", destination.display());
        let unit = Rc::new(Self {
            handle,
            path,
            unloads,
        });
        // SAFETY: fixture_init is emitted with this exact local adapter type.
        unsafe {
            let initialize: unsafe extern "C" fn(*const FixtureApi) =
                core::mem::transmute(unit.symbol(c"fixture_init"));
            initialize(&API);
        }
        unit
    }

    fn symbol(&self, name: &CStr) -> *mut c_void {
        // SAFETY: self owns the live loader handle for the entire lookup.
        let address = unsafe { crate::loader::symbol(self.handle, name.as_ptr()) };
        assert!(!address.is_null());
        address
    }

    fn entry(&self, index: usize) -> Method {
        // SAFETY: the fixture getter returns the unchanged Method type. The
        // caller must retain this unit separately through every invocation.
        unsafe {
            let getter: unsafe extern "C" fn(usize) -> Option<Method> =
                core::mem::transmute(self.symbol(c"fixture_entry"));
            getter(index).expect("compiled fixture entry")
        }
    }
}

impl Drop for NativeUnit {
    fn drop(&mut self) {
        // SAFETY: Rc ownership and generation retirement exclude all entries.
        assert_eq!(unsafe { dlclose(self.handle) }, 0);
        assert!(
            !mapped(&self.path),
            "library remained mapped after final lease"
        );
        self.unloads.borrow_mut().count += 1;
    }
}

#[repr(C)]
struct FixtureApi {
    send: unsafe extern "C" fn(Oop, Oop, *mut Oop, c_long) -> Oop,
    intern: unsafe extern "C" fn(*const c_char) -> Oop,
    integer: unsafe extern "C" fn(c_long) -> Oop,
    value: unsafe extern "C" fn(Oop) -> c_long,
    nil: unsafe extern "C" fn() -> Oop,
    global: unsafe extern "C" fn(*const c_char) -> Oop,
}

static API: FixtureApi = FixtureApi {
    send: shim_send,
    intern: shim_intern,
    integer: shim_integer,
    value: shim_value,
    nil: shim_nil,
    global: shim_global,
};

unsafe extern "C" fn shim_send(receiver: Oop, selector: Oop, args: *mut Oop, count: c_long) -> Oop {
    assert!(count >= 0);
    // SAFETY: generated C passes an array of exactly count Oops.
    let args = if count == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(args, count as usize) }
    };
    with_active(|runtime| runtime.send_slice(receiver, selector, args))
}

unsafe extern "C" fn shim_intern(name: *const c_char) -> Oop {
    with_active(|runtime| {
        let retire = PROBE.with_borrow_mut(|probe| {
            let retire =
                probe.case == Case::RetireCandidate && probe.selected && !probe.retired_on_stack;
            probe.retired_on_stack |= retire;
            retire
        });
        if retire {
            // This callback is inside the candidate C activation, before its
            // first Machine send. Admission drops its owner; Selected retains
            // the independent lease until that activation returns.
            let candidate = runtime
                .generated_deopt_trial
                .borrow_mut()
                .as_mut()
                .unwrap()
                .candidate
                .take()
                .unwrap();
            let path = candidate.unit.path.clone();
            let unloads = candidate.unit.unloads.clone();
            drop(candidate);
            assert!(mapped(&path));
            assert_eq!(unloads.borrow().count, 0);
        }
        // SAFETY: emitted string literals are NUL terminated and live.
        runtime
            .intern(unsafe { CStr::from_ptr(name) }.to_bytes())
            .unwrap()
    })
}

unsafe extern "C" fn shim_integer(value: c_long) -> Oop {
    with_active(|runtime| runtime.make_integer(value))
}
unsafe extern "C" fn shim_value(value: Oop) -> c_long {
    PROBE.with_borrow_mut(|probe| probe.pc_decodes += 1);
    with_active(|runtime| runtime.integer_value(value))
}
unsafe extern "C" fn shim_nil() -> Oop {
    with_active(|runtime| runtime.canonical().nil)
}
unsafe extern "C" fn shim_global(name: *const c_char) -> Oop {
    // SAFETY: emitted string literal lifetime.
    with_active(|runtime| {
        runtime
            .import(unsafe { CStr::from_ptr(name) }.to_bytes())
            .unwrap()
    })
}

struct Candidate {
    descriptor: Arc<FrameDescriptor>,
    entries: Vec<Option<Method>>,
    unit: Rc<NativeUnit>,
}

impl Candidate {
    fn admit(
        descriptor: Arc<FrameDescriptor>,
        expected_bytes: &[u8],
        candidate_bytes: &[u8],
        mappings: &[(usize, Method)],
        unit: Rc<NativeUnit>,
    ) -> Result<Self, &'static str> {
        if expected_bytes != candidate_bytes {
            return Err("descriptor mismatch");
        }
        let mut entries = vec![None; descriptor.program_counter_count()];
        for &(pc, method) in mappings {
            let entry = entries.get_mut(pc).ok_or("invalid PC")?;
            if entry.replace(method).is_some() {
                return Err("duplicate PC");
            }
        }
        Ok(Self {
            descriptor,
            entries,
            unit,
        })
    }
}

pub(super) struct Trial {
    descriptor: Arc<FrameDescriptor>,
    baseline: Option<Rc<NativeUnit>>,
    generation: RegistrationGenerationLease,
    retiring: bool,
    candidate: Option<Candidate>,
    epoch: u64,
    disabled: bool,
    optimized: usize,
    fallback: usize,
    deoptimizations: usize,
    observe: fn(&Runtime, Oop, [usize; 3], usize, bool),
}

impl Trial {
    fn collect_baseline(&mut self) -> bool {
        if self.retiring && self.generation.holder_count() == 1 {
            self.baseline.take();
            true
        } else {
            false
        }
    }
}

struct Scope<'a>(&'a Runtime);
impl Drop for Scope<'_> {
    fn drop(&mut self) {
        // On test failure, intentionally leak owners still reachable through
        // a registration rather than unmap executable code. Successful runs
        // must prove retirement and actual unload before reaching this guard.
        let trial = self.0.generated_deopt_trial.borrow_mut().take();
        if let Some(trial) = trial {
            if trial.generation.holder_count() != 1 {
                std::mem::forget(trial);
            }
        }
    }
}

pub(super) struct Selected {
    pub(super) method: Method,
    _unit: Option<Rc<NativeUnit>>,
}

pub(super) fn select(
    runtime: &Runtime,
    capability: MachineCapability,
    owner: u64,
    machine: Oop,
    method: Method,
) -> Result<Selected, ExecutionError> {
    let mut state = runtime.generated_deopt_trial.borrow_mut();
    let Some(trial) = state.as_mut() else {
        return Ok(Selected {
            method,
            _unit: None,
        });
    };
    // SAFETY: owner-thread observation only; no allocation or callback.
    let runtime_state = unsafe { &*runtime.state.get() };
    let arena = runtime_state.execution.machine(owner, capability)?.arena();
    let frame = arena.current_handle().unwrap();
    let descriptor = arena.descriptor(frame)?;
    if !core::ptr::eq(descriptor, Arc::as_ptr(&trial.descriptor)) {
        return Ok(Selected {
            method,
            _unit: None,
        });
    }
    let pc = arena.program_counter(frame)?;
    let epoch = runtime_state.vtables.mutation_epoch();
    let candidate = trial.candidate.as_ref().filter(|candidate| {
        !trial.disabled
            && epoch != u64::MAX
            && epoch == trial.epoch
            && core::ptr::eq(descriptor, Arc::as_ptr(&candidate.descriptor))
            && runtime.guarded_slot_access_eligible(machine)
    });
    let selected = candidate.and_then(|candidate| {
        candidate
            .entries
            .get(pc)
            .copied()
            .flatten()
            .map(|method| Selected {
                method,
                _unit: Some(candidate.unit.clone()),
            })
    });
    let optimized = selected.is_some();
    let result = if let Some(selected) = selected {
        trial.optimized += 1;
        selected
    } else {
        if !trial.disabled && trial.optimized != 0 {
            trial.deoptimizations += 1;
        }
        trial.disabled = true;
        trial.fallback += 1;
        Selected {
            method,
            _unit: Some(
                trial
                    .baseline
                    .as_ref()
                    .expect("baseline generation retained")
                    .clone(),
            ),
        }
    };
    (trial.observe)(runtime, machine, frame.words().unwrap(), pc, optimized);
    Ok(result)
}

fn observe_entry(runtime: &Runtime, machine: Oop, frame: [usize; 3], pc: usize, optimized: bool) {
    PROBE.with_borrow_mut(|probe| {
        probe.machine = machine;
        probe.selected = optimized;
        probe.visits.push((frame, pc, optimized));
        if pc == 2 {
            assert_eq!(
                runtime
                    .stackless_oop_at(machine, probe.carried_slot)
                    .unwrap(),
                probe.carried
            );
            assert_eq!(
                runtime.stackless_oop_at(machine, probe.alias_slot).unwrap(),
                probe.carried
            );
        }
    });
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Case {
    Canonical,
    Invalidate,
    ReplaceActive,
    CustomLookup,
    PreciseGc,
    DescriptorMismatch,
    DescriptorCollision,
    InvalidPc,
    DuplicatePc,
    MissingPc,
    RetireCandidate,
    RetireBaseline,
}
impl Case {
    fn name(self) -> &'static str {
        match self {
            Self::Canonical => "canonical",
            Self::Invalidate => "invalidate",
            Self::ReplaceActive => "replace-active",
            Self::CustomLookup => "custom-lookup",
            Self::PreciseGc => "precise-gc",
            Self::DescriptorMismatch => "descriptor-mismatch",
            Self::DescriptorCollision => "descriptor-collision",
            Self::InvalidPc => "invalid-pc",
            Self::DuplicatePc => "duplicate-pc",
            Self::MissingPc => "missing-pc",
            Self::RetireCandidate => "retire-candidate-on-stack",
            Self::RetireBaseline => "retire-baseline-active",
        }
    }
}

struct Probe {
    case: Case,
    machine: Oop,
    carried: Oop, // Observation only, deliberately not a root.
    carried_slot: usize,
    alias_slot: usize,
    selected: bool,
    retired_on_stack: bool,
    pc_decodes: usize,
    lookup_calls: usize,
    effects: Vec<&'static str>,
    visits: Vec<([usize; 3], usize, bool)>,
}
impl Probe {
    fn new(case: Case, slots: [usize; 2]) -> Self {
        Self {
            case,
            machine: core::ptr::null_mut(),
            carried: core::ptr::null_mut(),
            carried_slot: slots[0],
            alias_slot: slots[1],
            selected: false,
            retired_on_stack: false,
            pc_decodes: 0,
            lookup_calls: 0,
            effects: Vec::new(),
            visits: Vec::new(),
        }
    }
}
thread_local! { static PROBE: RefCell<Probe> = RefCell::new(Probe::new(Case::Canonical, [0,1])); }

unsafe extern "C" fn double(receiver: Oop, _: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 0);
    PROBE.with_borrow_mut(|probe| probe.effects.push("double"));
    with_active(|runtime| runtime.make_integer(runtime.integer_value(receiver) * 2))
}
unsafe extern "C" fn replacement(_: Oop, _: *const Oop, _: usize) -> Oop {
    with_active(|runtime| runtime.make_integer(999))
}
unsafe extern "C" fn lookup(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    PROBE.with_borrow_mut(|probe| probe.lookup_calls += 1);
    // SAFETY: preserve the native lookup Method buffer contract.
    unsafe { crate::dispatch::vtable_lookup_method(receiver, args, count) }
}
fn invalidate(runtime: &Runtime) {
    let name = runtime.intern(b"generatedEpochProbe").unwrap();
    let _ = runtime.add_method(runtime.globals().object_vt, name, replacement);
    runtime
        .remove_method(runtime.globals().object_vt, name)
        .unwrap();
}
unsafe extern "C" fn effect(_: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 2);
    // SAFETY: generated call site passes two Oops.
    let (carried, alias) = unsafe { (args.read(), args.add(1).read()) };
    assert_eq!(carried, alias);
    with_active(|runtime| {
        assert_eq!(runtime.integer_value(carried), 14);
        let case = PROBE.with_borrow_mut(|probe| {
            probe.carried = carried;
            probe.effects.push("effect");
            probe.case
        });
        match case {
            Case::Invalidate | Case::PreciseGc => invalidate(runtime),
            Case::CustomLookup => {
                let _ = runtime.add_method(
                    runtime.globals().vtable_vt,
                    runtime.globals().lookup,
                    lookup,
                );
            }
            Case::ReplaceActive | Case::RetireBaseline => {
                let run = runtime.intern(b"generatedRun:").unwrap();
                if case == Case::ReplaceActive {
                    let _ = runtime.add_method(runtime.globals().object_vt, run, replacement);
                } else {
                    runtime
                        .remove_method(runtime.globals().object_vt, run)
                        .unwrap();
                }
                let mut trial = runtime.generated_deopt_trial.borrow_mut();
                let trial = trial.as_mut().unwrap();
                trial.retiring = true;
                assert!(trial.generation.holder_count() > 1);
                assert!(!trial.collect_baseline());
                assert!(mapped(&trial.baseline.as_ref().unwrap().path));
                assert_eq!(
                    runtime
                        .stackless_registration_diagnostics()
                        .active_frame_pins,
                    1
                );
            }
            _ => {}
        }
        if case == Case::PreciseGc {
            runtime.gc_set_precise(true);
            let garbage = runtime.make_integer(123456);
            let _ = runtime.gc_collect_from_native_range(0, 0);
            assert_eq!(runtime.checked_vtable_of(garbage), core::ptr::null_mut());
            assert_eq!(runtime.integer_value(carried), 14);
            PROBE.with_borrow(|probe| {
                assert_eq!(
                    runtime
                        .stackless_oop_at(probe.machine, probe.carried_slot)
                        .unwrap(),
                    carried
                );
                assert_eq!(
                    runtime
                        .stackless_oop_at(probe.machine, probe.alias_slot)
                        .unwrap(),
                    carried
                );
            });
        }
        runtime.canonical().nil
    })
}
unsafe extern "C" fn finish(_: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 2);
    // SAFETY: generated tail-call site passes two Oops.
    let (left, right) = unsafe { (args.read(), args.add(1).read()) };
    assert_eq!(left, right);
    PROBE.with_borrow_mut(|probe| probe.effects.push("finish"));
    with_active(|runtime| {
        runtime.make_integer(runtime.integer_value(left) + runtime.integer_value(right))
    })
}

struct Fixture {
    directory: PathBuf,
    descriptor_bytes: Vec<u8>,
    digest: [u8; 32],
    slots: [usize; 2],
    entry_receiver_slot: usize,
    entry_argument_start: usize,
    pc_count: usize,
    sequence: usize,
}
impl Fixture {
    fn row(&self) -> Row<'_> {
        Row {
            kind: SubjectKind::BlockBody,
            disposition: Disposition::Resumable,
            subject_index: 0,
            language_arity: 1,
            capture_count: 0,
            semantic_digest: self.digest,
            selector: b"generatedRun:",
        }
    }

    fn compile() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "birnam-74g-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        let bytes = input::portable_program();
        let program = resumable::lower_portable_block(&bytes).unwrap();
        let layout = resumable::color_slots(&program).unwrap();
        let slots = [b"carried".as_slice(), b"alias".as_slice()].map(|name| {
            let value = program.blocks.iter().flat_map(|block| &block.operations).find(|operation|
                matches!(&operation.kind, resumable::OperationKind::Bind { name: binding_name, .. } if *binding_name == name)
            ).unwrap().result;
            layout.value_slots[value.0 as usize] as usize
        });
        let mut fixture = Self {
            directory,
            descriptor_bytes: Vec::new(),
            digest: Sha256::digest(&bytes).into(),
            slots,
            entry_receiver_slot: layout.entry_receiver_slot as usize,
            entry_argument_start: layout.entry_argument_start as usize,
            pc_count: program.blocks.len(),
            sequence: 0,
        };
        fixture.descriptor_bytes = resumable_descriptor::compile_portable_descriptor_v2(
            &program,
            &layout,
            fixture.row(),
            1,
        )
        .unwrap();
        // Independently lower the same canonical input for the candidate.
        let candidate_program = resumable::lower_portable_block(&bytes).unwrap();
        let candidate_layout = resumable::color_slots(&candidate_program).unwrap();
        assert_eq!(
            fixture.descriptor_bytes,
            resumable_descriptor::compile_portable_descriptor_v2(
                &candidate_program,
                &candidate_layout,
                fixture.row(),
                1
            )
            .unwrap()
        );
        let baseline = resumable_c::emit_step_method(0, &program, &layout, 1).unwrap();
        // Captured with unmodified 43935092 emitter before this experiment.
        assert_eq!(baseline, include_bytes!("generated_deopt_baseline.c"));
        let mut baseline_source = include_bytes!("generated_deopt_adapter.c").to_vec();
        baseline_source.extend(resumable_c::emit_entry_thunk(0, b"generatedRun:"));
        baseline_source.extend(&baseline);
        baseline_source.extend(b"Method fixture_entry(unsigned long n){switch(n){case 0:return __birnam_resumable_thunk_0;case 1:return __birnam_resumable_step_0;default:return 0;}}\n");
        let mut candidate_source = include_bytes!("generated_deopt_adapter.c").to_vec();
        let baseline_text = String::from_utf8(baseline).unwrap();
        for pc in 0..fixture.pc_count {
            let code = resumable_c::emit_pc_specialized_step(
                pc as u32,
                &candidate_program,
                &candidate_layout,
                pc as u32,
            )
            .unwrap();
            let text = std::str::from_utf8(&code).unwrap();
            assert!(!text.contains("switch(pc)") && !text.contains("birnam_smallIntValue"));
            // Body equality includes staging, allocation order and publication.
            let marker = format!("case {pc}L:{{\n");
            let body = baseline_text
                .split_once(&marker)
                .unwrap()
                .1
                .split_once("}\n")
                .unwrap()
                .0;
            assert!(text.contains(body));
            candidate_source.extend(code);
        }
        assert_eq!(
            resumable_c::emit_pc_specialized_step(0, &program, &layout, fixture.pc_count as u32),
            Err(resumable_c::Error::InvalidProgram)
        );
        candidate_source.extend(b"Method fixture_entry(unsigned long n){switch(n){");
        for pc in 0..fixture.pc_count {
            candidate_source
                .extend(format!("case {pc}:return __birnam_resumable_step_{pc};").bytes());
        }
        candidate_source.extend(b"default:return 0;}}\n");
        for (name, source) in [
            ("baseline", baseline_source),
            ("candidate", candidate_source),
        ] {
            let c_path = fixture.directory.join(format!("{name}.c"));
            let library = fixture.directory.join(format!("{name}.dylib"));
            std::fs::write(&c_path, &source).unwrap();
            let started = Instant::now();
            let output = Command::new("cc")
                .args([
                    "-dynamiclib",
                    "-O2",
                    "-Wall",
                    "-Wextra",
                    "-Werror",
                    "-Wno-unused-function",
                ])
                .arg(&c_path)
                .arg("-o")
                .arg(&library)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            println!(
                "GENERATED_DEOPT_COST {{\"unit\":\"{name}\",\"source_bytes\":{},\"native_bytes\":{},\"compile_ns\":{}}}",
                source.len(),
                std::fs::metadata(&library).unwrap().len(),
                started.elapsed().as_nanos()
            );
        }
        println!(
            "GENERATED_DEOPT_SHAPE {{\"pcs\":{},\"slots\":{},\"carried_slot\":{},\"alias_slot\":{},\"root_words\":{:?},\"descriptor_bytes\":{},\"program_bytes\":{}}}",
            fixture.pc_count,
            layout.slot_count,
            slots[0],
            slots[1],
            layout.root_bitmap_words,
            fixture.descriptor_bytes.len(),
            bytes.len()
        );
        fixture
    }

    fn load(&mut self, name: &str, unloads: Rc<RefCell<Unloads>>) -> Rc<NativeUnit> {
        self.sequence += 1;
        NativeUnit::load(
            &self.directory.join(format!("{name}.dylib")),
            &self
                .directory
                .join(format!("{name}-{}.dylib", self.sequence)),
            unloads,
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.directory).unwrap();
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Outcome {
    answer: i64,
    effects: Vec<&'static str>,
    next: Option<i64>,
}

fn run(
    fixture: &mut Fixture,
    case: Case,
    optimized: bool,
    repetitions: usize,
) -> (Outcome, usize, usize, usize, u128) {
    let bootstrap = crate::dispatch::build_bootstrap();
    let runtime = &bootstrap.runtime;
    runtime.publish_canonical_exports();
    runtime.install_execution_objects();
    let unloads = Rc::new(RefCell::new(Unloads::default()));
    let baseline = fixture.load("baseline", unloads.clone());
    let candidate_unit = fixture.load("candidate", unloads.clone());
    let candidate_path = candidate_unit.path.clone();
    let thunk = baseline.entry(0);
    let descriptor = FrameDescriptor::admit_portable_v2(
        &fixture.descriptor_bytes,
        fixture.row(),
        7407,
        baseline.entry(1),
    )
    .unwrap();
    let generation = RegistrationGenerationLease::new(7407);
    runtime
        .register_stackless_resumable(
            SemanticRegistrationV1::new(
                thunk,
                descriptor.clone(),
                1,
                fixture.entry_receiver_slot,
                fixture.entry_argument_start,
            )
            .with_generation_lease(generation.clone()),
        )
        .unwrap();
    let receiver = runtime.canonical().object_prototype;
    let run = runtime.intern(b"generatedRun:").unwrap();
    runtime.export(b"Probe", receiver).unwrap();
    for (name, method) in [
        (run, thunk),
        (runtime.intern(b"probeDouble").unwrap(), double as Method),
        (runtime.intern(b"probeEffect:alias:").unwrap(), effect),
        (runtime.intern(b"probeFinish:alias:").unwrap(), finish),
    ] {
        let _ = runtime.add_method(runtime.globals().object_vt, name, method);
    }
    for name in [
        b"oopAt:".as_slice(),
        b"oopAt:put:",
        b"callSite:",
        b"tailCallSite:",
        b"generatedEpochProbe",
    ] {
        runtime.intern(name).unwrap();
    }
    let argument = runtime.make_integer(7);
    let _argument_root = ScopedOopRootGuard::new(runtime, argument);
    let candidate_descriptor = if case == Case::DescriptorCollision {
        let collision = FrameDescriptor::admit_portable_v2(
            &fixture.descriptor_bytes,
            fixture.row(),
            7407,
            baseline.entry(1),
        )
        .unwrap();
        assert_eq!(collision.identity(), descriptor.identity());
        collision
    } else {
        descriptor.clone()
    };
    let mut mappings: Vec<_> = (0..fixture.pc_count)
        .map(|pc| (pc, candidate_unit.entry(pc)))
        .collect();
    if case == Case::InvalidPc {
        mappings.push((fixture.pc_count, candidate_unit.entry(0)));
    }
    if case == Case::DuplicatePc {
        mappings.push(mappings[0]);
    }
    if case == Case::MissingPc {
        mappings.pop();
    }
    let mut candidate_bytes = fixture.descriptor_bytes.clone();
    if case == Case::DescriptorMismatch {
        candidate_bytes[0] ^= 1;
    }
    let candidate = Candidate::admit(
        candidate_descriptor,
        &fixture.descriptor_bytes,
        &candidate_bytes,
        &mappings,
        candidate_unit,
    );
    assert_eq!(
        candidate.is_err(),
        matches!(
            case,
            Case::DescriptorMismatch | Case::InvalidPc | Case::DuplicatePc
        )
    );
    let candidate = match candidate {
        Ok(candidate) if optimized => Some(candidate),
        Ok(candidate) => {
            drop(candidate);
            None
        }
        Err(_) => None,
    };
    // SAFETY: all setup and interning precedes the entry dependency snapshot.
    let epoch = unsafe { (*runtime.state.get()).vtables.mutation_epoch() };
    *runtime.generated_deopt_trial.borrow_mut() = Some(Trial {
        descriptor,
        baseline: Some(baseline),
        generation,
        retiring: false,
        candidate,
        epoch,
        disabled: !optimized,
        optimized: 0,
        fallback: 0,
        deoptimizations: 0,
        observe: observe_entry,
    });
    let _scope = Scope(runtime);
    if repetitions > 1 {
        assert_eq!(case, Case::Canonical);
        for _ in 0..16 {
            PROBE.with_borrow_mut(|probe| *probe = Probe::new(case, fixture.slots));
            let answer = runtime.enter(|| runtime.send_slice(receiver, run, &[argument]));
            assert_eq!(runtime.integer_value(answer), 28);
        }
        let mut trial = runtime.generated_deopt_trial.borrow_mut();
        let trial = trial.as_mut().unwrap();
        trial.optimized = 0;
        trial.fallback = 0;
        trial.deoptimizations = 0;
    }
    let started = Instant::now();
    let mut outcome = None;
    for _ in 0..repetitions {
        PROBE.with_borrow_mut(|probe| *probe = Probe::new(case, fixture.slots));
        let answer = runtime.enter(|| runtime.send_slice(receiver, run, &[argument]));
        let answer = runtime.integer_value(answer);
        assert_eq!(answer, 28);
        let effects = PROBE.with_borrow(|probe| {
            assert_eq!(probe.effects, ["double", "effect", "finish"]);
            assert_eq!(probe.visits.len(), fixture.pc_count);
            for (pc, visit) in probe.visits.iter().enumerate() {
                assert_eq!(
                    visit.0, probe.visits[0].0,
                    "recovery changed the Frame handle"
                );
                assert_eq!(visit.1, pc, "effect was replayed or continuation skipped");
            }
            assert_eq!(
                probe.pc_decodes,
                probe.visits.iter().filter(|visit| !visit.2).count()
            );
            if case == Case::CustomLookup {
                assert!(probe.lookup_calls > 0);
            }
            if case == Case::RetireCandidate && optimized {
                assert!(probe.retired_on_stack);
                assert!(!mapped(&candidate_path));
                assert_eq!(unloads.borrow().count, 1);
            }
            probe.effects.clone()
        });
        let next =
            if case == Case::ReplaceActive {
                Some(runtime.integer_value(
                    runtime.enter(|| runtime.send_slice(receiver, run, &[argument])),
                ))
            } else {
                None
            };
        if let Some(next) = next {
            assert_eq!(next, 999);
        }
        outcome = Some(Outcome {
            answer,
            effects,
            next,
        });
    }
    let elapsed = started.elapsed().as_nanos();
    // Removing/replacing the published binding retires the semantic generation.
    if case != Case::RetireBaseline {
        runtime
            .remove_method(runtime.globals().object_vt, run)
            .unwrap();
    }
    let (entries, fallback, deopts) = {
        let mut trial = runtime.generated_deopt_trial.borrow_mut();
        let trial = trial.as_mut().unwrap();
        trial.retiring = true;
        assert_eq!(trial.generation.holder_count(), 1);
        assert!(trial.collect_baseline());
        trial.candidate.take();
        (trial.optimized, trial.fallback, trial.deoptimizations)
    };
    assert_eq!(unloads.borrow().count, 2);
    let diagnostics = runtime.stackless_registration_diagnostics();
    assert_eq!(diagnostics.active_frame_pins, 0);
    assert_eq!(diagnostics.acquired_entries, 0);
    assert_eq!(diagnostics.continuation_frame_pins, 0);
    assert_eq!(diagnostics.explicit_generation_leases, 0);
    assert_eq!(diagnostics.registration_generation_leases, 0);
    assert_eq!(diagnostics.live_entries, 0);
    assert_eq!(runtime.stackless_runtime_diagnostics().live_frames, 0);
    (outcome.unwrap(), entries, fallback, deopts, elapsed)
}

#[test]
fn generated_same_frame_deopt_experiment() {
    let mut fixture = Fixture::compile();
    for case in [
        Case::Canonical,
        Case::Invalidate,
        Case::ReplaceActive,
        Case::CustomLookup,
        Case::PreciseGc,
        Case::DescriptorMismatch,
        Case::DescriptorCollision,
        Case::InvalidPc,
        Case::DuplicatePc,
        Case::MissingPc,
        Case::RetireCandidate,
        Case::RetireBaseline,
    ] {
        let baseline = run(&mut fixture, case, false, 1);
        let candidate = run(&mut fixture, case, true, 1);
        assert_eq!(baseline.0, candidate.0, "{case:?}");
        let expected = match case {
            Case::Canonical => (3, 0, 0),
            Case::DescriptorMismatch
            | Case::DescriptorCollision
            | Case::InvalidPc
            | Case::DuplicatePc => (0, 3, 0),
            Case::RetireCandidate => (1, 2, 1),
            _ => (2, 1, 1),
        };
        assert_eq!(
            (candidate.1, candidate.2, candidate.3),
            expected,
            "{case:?}"
        );
        println!(
            "GENERATED_DEOPT_EXPERIMENT {{\"case\":\"{}\",\"answer\":28,\"effects\":[\"double\",\"effect\",\"finish\"],\"optimized_entries\":{},\"baseline_entries\":{},\"deoptimizations\":{},\"unloads\":4}}",
            case.name(),
            candidate.1,
            candidate.2,
            candidate.3
        );
    }
    for cycle in 0..16 {
        run(
            &mut fixture,
            if cycle % 2 == 0 {
                Case::Canonical
            } else {
                Case::Invalidate
            },
            true,
            1,
        );
    }
    println!(
        "GENERATED_DEOPT_EXPERIMENT {{\"case\":\"repeated-load-retire\",\"cycles\":16,\"unloads\":32,\"leaked_frames\":0,\"leaked_native_leases\":0}}"
    );
    for optimized in [false, true] {
        let result = run(&mut fixture, Case::Canonical, optimized, 128);
        assert_eq!(result.1, if optimized { 384 } else { 0 });
        println!(
            "GENERATED_DEOPT_COST {{\"unit\":\"warm-execution\",\"optimized\":{optimized},\"warmup_iterations\":16,\"iterations\":128,\"elapsed_ns\":{},\"optimized_entries\":{}}}",
            result.4, result.1
        );
    }
}
