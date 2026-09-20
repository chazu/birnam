//! 74H. The executable owners and driver selector are the 74G implementation.
use super::*;
use birnam_codegen::{Block, Expr, Statement, portable};
use std::borrow::Cow;

#[repr(C)]
struct ReadCache {
    value: Oop,
    dictionary_epoch: u64,
    gc_epoch: u64,
    state: u64, // 0 empty, 1 remembered, 2 disabled until this entry returns.
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Baseline,
    Control,
    Reuse,
}
impl Mode {
    fn name(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Control => "control",
            Self::Reuse => "reuse",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scenario {
    Ordinary,
    BeforeEntry,
    Invalidate,
    ReadOverride,
    WriteOverride,
    LookupOverride,
    Gc,
    GcWrite,
    Saturated,
}
impl Scenario {
    fn name(self) -> &'static str {
        match self {
            Self::Ordinary => "ordinary",
            Self::BeforeEntry => "before-entry",
            Self::Invalidate => "invalidate-between-reads",
            Self::ReadOverride => "read-override",
            Self::WriteOverride => "write-override",
            Self::LookupOverride => "lookup-override",
            Self::Gc => "gc-between-reads",
            Self::GcWrite => "gc-slot-write",
            Self::Saturated => "saturated-gc-epoch",
        }
    }
    fn inject(self) -> bool {
        !matches!(self, Self::Ordinary | Self::BeforeEntry | Self::Saturated)
    }
}

struct Observation {
    scenario: Scenario,
    machine: Oop,
    source_slot: usize,
    pc: usize,
    replacement: Oop,
    event_armed: bool,
    event_done: bool,
    gc_write_pending: bool,
    reads: usize,
    reuses: usize,
    misses: usize,
    override_calls: usize,
    effects: Vec<&'static str>,
    arguments: Vec<i64>,
    visits: Vec<([usize; 3], usize, bool)>,
}
impl Observation {
    fn new(scenario: Scenario, source_slot: usize, replacement: Oop) -> Self {
        Self {
            scenario,
            machine: core::ptr::null_mut(),
            source_slot,
            pc: 0,
            replacement,
            event_armed: false,
            event_done: false,
            gc_write_pending: false,
            reads: 0,
            reuses: 0,
            misses: 0,
            override_calls: 0,
            effects: Vec::new(),
            arguments: Vec::new(),
            visits: Vec::new(),
        }
    }
}
thread_local! {
    static OBS: RefCell<Observation> = RefCell::new(Observation::new(Scenario::Ordinary, 0, core::ptr::null_mut()));
}

fn observe<const TRACE: bool, const EVENT: bool>(
    _: &Runtime,
    machine: Oop,
    frame: [usize; 3],
    pc: usize,
    optimized: bool,
) {
    if TRACE || EVENT {
        OBS.with_borrow_mut(|o| {
            o.machine = machine;
            o.pc = pc;
            if TRACE {
                o.visits.push((frame, pc, optimized));
            }
        });
    }
}

fn note_read<const TRACE: bool, const EVENT: bool>(
    runtime: &Runtime,
    receiver: Oop,
    selector: Oop,
    args: &[Oop],
) {
    if TRACE || EVENT {
        if runtime.symbol_bytes(selector) == Some(b"oopAt:") && args.len() == 1 {
            let slot = runtime.integer_value(args[0]);
            OBS.with_borrow_mut(|o| {
                if TRACE {
                    o.reads += 1;
                }
                if EVENT
                    && receiver == o.machine
                    && o.pc == 1
                    && slot == o.source_slot as i64
                    && !o.event_done
                {
                    o.event_armed = true;
                }
            });
        }
    }
}

unsafe extern "C" fn send<const TRACE: bool, const EVENT: bool>(
    receiver: Oop,
    selector: Oop,
    args: *mut Oop,
    count: c_long,
) -> Oop {
    assert!(count >= 0);
    // SAFETY: emitted calls provide exactly count live Oops.
    let args = if count == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(args, count as usize) }
    };
    with_active(|runtime| {
        let answer = runtime.send_slice(receiver, selector, args);
        note_read::<TRACE, EVENT>(runtime, receiver, selector, args);
        answer
    })
}

unsafe extern "C" fn reuse<const TRACE: bool, const EVENT: bool>(
    machine: Oop,
    selector: Oop,
    operand: Oop,
    cache: *mut ReadCache,
) -> Oop {
    // SAFETY: this entry receives one C-stack local cache with identical repr(C).
    let cache = unsafe { &mut *cache };
    with_active(|runtime| {
        let mut remember = false;
        if cache.state != 2 {
            // No allocation/callback here. Capture after operand preparation.
            let dictionary = unsafe { (*runtime.state.get()).vtables.mutation_epoch() };
            let gc = runtime.read_reuse_gc_epoch.get();
            let eligible = dictionary != u64::MAX
                && gc != u64::MAX
                && runtime.guarded_slot_access_eligible(machine);
            if eligible
                && cache.state == 1
                && cache.dictionary_epoch == dictionary
                && cache.gc_epoch == gc
            {
                if TRACE {
                    OBS.with_borrow_mut(|o| o.reuses += 1);
                }
                return cache.value;
            }
            if eligible && cache.state == 0 {
                cache.dictionary_epoch = dictionary;
                cache.gc_epoch = gc;
                remember = true;
            } else {
                cache.state = 2;
            }
        }
        if TRACE {
            OBS.with_borrow_mut(|o| o.misses += 1);
        }
        let value = runtime.send_slice(machine, selector, &[operand]);
        note_read::<TRACE, EVENT>(runtime, machine, selector, &[operand]);
        if remember {
            cache.value = value;
            cache.state = 1;
        }
        value
    })
}

unsafe extern "C" fn integer<const EVENT: bool>(value: c_long) -> Oop {
    with_active(|runtime| {
        if EVENT {
            let event = OBS.with_borrow_mut(|o| {
                if !o.event_armed || o.event_done {
                    return None;
                }
                o.event_done = true;
                Some(o.scenario)
            });
            if let Some(event) = event {
                match event {
                    Scenario::Invalidate => invalidate(runtime),
                    Scenario::ReadOverride | Scenario::WriteOverride => {
                        let objects =
                            unsafe { (*runtime.state.get()).execution.objects() }.unwrap();
                        let (selector, method) = if event == Scenario::ReadOverride {
                            (objects.oop_at, overridden_read as Method)
                        } else {
                            (objects.oop_at_put, overridden_write as Method)
                        };
                        let _ = runtime.add_method(objects.machine_vtable, selector, method);
                    }
                    Scenario::LookupOverride => {
                        let _ = runtime.add_method(
                            runtime.globals().vtable_vt,
                            runtime.globals().lookup,
                            overridden_lookup,
                        );
                    }
                    Scenario::Gc | Scenario::GcWrite => {
                        OBS.with_borrow_mut(|o| o.gc_write_pending = event == Scenario::GcWrite);
                        let before = unsafe { (*runtime.state.get()).vtables.mutation_epoch() };
                        let gc_before = runtime.read_reuse_gc_epoch.get();
                        // Enter the real collector before making this index box.
                        // Root the C activation's prepared operands normally;
                        // the separate effect collection below uses no native roots.
                        let _ = runtime.gc_collect_active();
                        assert!(runtime.read_reuse_gc_epoch.get() > gc_before);
                        assert_eq!(
                            unsafe { (*runtime.state.get()).vtables.mutation_epoch() },
                            before,
                            "the slot-write case must not be rescued by dictionary invalidation"
                        );
                        assert!(!OBS.with_borrow(|o| o.gc_write_pending));
                    }
                    _ => unreachable!(),
                }
            }
        }
        runtime.make_integer(value)
    })
}

unsafe extern "C" fn gc_provider(mark: crate::abi::MarkFn) {
    with_active(|runtime| {
        let write = OBS.with_borrow_mut(|o| {
            if !o.gc_write_pending {
                return None;
            }
            o.gc_write_pending = false;
            Some((o.machine, o.source_slot, o.replacement))
        });
        if let Some((machine, slot, value)) = write {
            assert_eq!(
                runtime.stackless_oop_at_put(machine, slot, value).unwrap(),
                value
            );
            // SAFETY: the provider was given this invocation's live mark callback.
            unsafe { mark(value) };
        }
    });
}

unsafe extern "C" fn plain_intern(name: *const c_char) -> Oop {
    // SAFETY: generated C supplies a live NUL-terminated literal.
    with_active(|runtime| {
        runtime
            .intern(unsafe { CStr::from_ptr(name) }.to_bytes())
            .unwrap()
    })
}
unsafe extern "C" fn plain_value(value: Oop) -> c_long {
    with_active(|runtime| runtime.integer_value(value))
}

unsafe extern "C" fn overridden_read(receiver: Oop, args: *const Oop, n: usize) -> Oop {
    OBS.with_borrow_mut(|o| o.override_calls += 1);
    // SAFETY: forwarding the original Method buffer to the canonical primitive.
    unsafe { crate::execution::machine_oop_at(receiver, args, n) }
}
unsafe extern "C" fn overridden_write(receiver: Oop, args: *const Oop, n: usize) -> Oop {
    OBS.with_borrow_mut(|o| o.override_calls += 1);
    // SAFETY: forwarding the original Method buffer to the canonical primitive.
    unsafe { crate::execution::machine_oop_at_put(receiver, args, n) }
}
unsafe extern "C" fn overridden_lookup(receiver: Oop, args: *const Oop, n: usize) -> Oop {
    OBS.with_borrow_mut(|o| o.override_calls += 1);
    // SAFETY: forwarding the original Method buffer to native lookup.
    unsafe { crate::dispatch::vtable_lookup_method(receiver, args, n) }
}

unsafe extern "C" fn doubled<const TRACE: bool>(receiver: Oop, _: *const Oop, n: usize) -> Oop {
    assert_eq!(n, 0);
    if TRACE {
        OBS.with_borrow_mut(|o| o.effects.push("double"));
    }
    with_active(|runtime| runtime.make_integer(runtime.integer_value(receiver) * 2))
}
unsafe extern "C" fn sum<const TRACE: bool>(_: Oop, args: *const Oop, n: usize) -> Oop {
    // SAFETY: the descriptor and driver supply n staged arguments.
    let args = if n == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(args, n) }
    };
    with_active(|runtime| {
        if TRACE {
            OBS.with_borrow_mut(|o| {
                o.effects.push("sum");
                o.arguments = args.iter().map(|&arg| runtime.integer_value(arg)).collect();
                if o.scenario != Scenario::GcWrite && !args.is_empty() {
                    assert!(args.iter().all(|arg| *arg == args[0]));
                }
            });
            // No C activation remains here. Staged arguments and caller live
            // values must survive precise collection with no native range.
            if OBS.with_borrow(|o| matches!(o.scenario, Scenario::Gc | Scenario::GcWrite)) {
                let garbage = runtime.make_integer(123456);
                runtime.gc_set_precise(true);
                let _ = runtime.gc_collect_from_native_range(0, 0);
                assert_eq!(runtime.checked_vtable_of(garbage), core::ptr::null_mut());
            }
        }
        runtime.make_integer(args.iter().map(|&arg| runtime.integer_value(arg)).sum())
    })
}
unsafe extern "C" fn finished<const TRACE: bool>(_: Oop, args: *const Oop, n: usize) -> Oop {
    assert_eq!(n, 1);
    if TRACE {
        OBS.with_borrow_mut(|o| o.effects.push("finish"));
    }
    // SAFETY: the one argument remains rooted by the driver's send extent.
    unsafe { args.read() }
}

fn sum_selector(repeats: usize) -> Vec<u8> {
    let mut name = b"readReuseSum".to_vec();
    if repeats != 0 {
        name.push(b':');
    }
    for _ in 1..repeats {
        name.extend(b"with:");
    }
    name
}

fn input(repeats: usize) -> Vec<u8> {
    fn send(
        receiver: Expr<'static>,
        selector: &'static [u8],
        args: Vec<Expr<'static>>,
    ) -> Expr<'static> {
        Expr::Send {
            receiver: Box::new(receiver),
            selector: Cow::Borrowed(selector),
            args,
            source: b"",
        }
    }
    portable::encode_block(
        &Block {
            params: vec![b"input"],
            body: vec![
                Statement::Let {
                    name: b"carried",
                    value: send(Expr::Global(b"input"), b"readReuseDouble", vec![]),
                },
                Statement::Let {
                    name: b"observed",
                    value: Expr::Send {
                        receiver: Box::new(Expr::Global(b"Probe")),
                        selector: Cow::Owned(sum_selector(repeats)),
                        args: (0..repeats).map(|_| Expr::Global(b"carried")).collect(),
                        source: b"",
                    },
                },
                Statement::Expr(send(
                    Expr::Global(b"Probe"),
                    b"readReuseFinish:",
                    vec![Expr::Global(b"observed")],
                )),
            ],
        },
        &[],
    )
    .unwrap()
}

fn compile(repeats: usize, prepared: Option<&Path>) -> Fixture {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let directory = std::env::temp_dir().join(format!(
        "birnam-74h-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&directory).unwrap();
    let bytes = input(repeats);
    let program = resumable::lower_portable_block(&bytes).unwrap();
    let layout = resumable::color_slots(&program).unwrap();
    let source = program.blocks.iter().flat_map(|b| &b.operations).find(|o|
        matches!(&o.kind, resumable::OperationKind::Bind { name, .. } if *name == b"carried")
    ).unwrap().result;
    let mut fixture = Fixture {
        directory,
        descriptor_bytes: Vec::new(),
        digest: Sha256::digest(&bytes).into(),
        slots: [layout.value_slots[source.0 as usize] as usize, 0],
        entry_receiver_slot: layout.entry_receiver_slot as usize,
        entry_argument_start: layout.entry_argument_start as usize,
        pc_count: program.blocks.len(),
        sequence: 0,
    };
    fixture.descriptor_bytes =
        resumable_descriptor::compile_portable_descriptor_v2(&program, &layout, fixture.row(), 1)
            .unwrap();
    assert_eq!(fixture.pc_count, 3);
    for mode in [Mode::Baseline, Mode::Control, Mode::Reuse] {
        let mut c = include_bytes!("generated_deopt_adapter.c").to_vec();
        if mode == Mode::Reuse && repeats >= 2 {
            c.extend(resumable_c::READ_REUSE_PRELUDE);
        }
        c.extend(resumable_c::emit_entry_thunk(99, b"generatedRun:"));
        for pc in 0..if mode == Mode::Baseline {
            1
        } else {
            fixture.pc_count
        } {
            let body = match mode {
                Mode::Baseline => resumable_c::emit_step_method(pc as u32, &program, &layout, 1),
                Mode::Control => {
                    resumable_c::emit_pc_specialized_step(pc as u32, &program, &layout, pc as u32)
                }
                Mode::Reuse => resumable_c::emit_pc_step_with_read_reuse(
                    pc as u32, &program, &layout, pc as u32,
                ),
            }
            .unwrap();
            if mode == Mode::Reuse && (repeats == 0 || pc != 1) {
                assert_eq!(
                    body,
                    resumable_c::emit_pc_specialized_step(pc as u32, &program, &layout, pc as u32)
                        .unwrap(),
                    "no-repeat body changed"
                );
            }
            if mode == Mode::Reuse && pc == 1 && repeats >= 2 {
                assert_eq!(
                    String::from_utf8_lossy(&body)
                        .matches("__birnam_read_reuse(machine")
                        .count(),
                    repeats
                );
            }
            c.extend(body);
        }
        c.extend(b"Method fixture_entry(unsigned long n){switch(n){case 99:return __birnam_resumable_thunk_99;");
        for pc in 0..if mode == Mode::Baseline {
            1
        } else {
            fixture.pc_count
        } {
            c.extend(format!("case {pc}:return __birnam_resumable_step_{pc};").bytes());
        }
        c.extend(b"default:return 0;}}\n");
        let c_path = fixture.directory.join(format!("{}.c", mode.name()));
        let library = fixture.directory.join(format!("{}.dylib", mode.name()));
        std::fs::write(&c_path, &c).unwrap();
        if let Some(prepared) = prepared {
            assert_eq!(
                c,
                std::fs::read(prepared.join(format!("{repeats}-{}.c", mode.name()))).unwrap()
            );
            std::fs::copy(
                prepared.join(format!("{repeats}-{}.dylib", mode.name())),
                &library,
            )
            .unwrap();
        } else {
            let started = Instant::now();
            let out = Command::new("cc")
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
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            println!(
                "READ_REUSE_COST {{\"repeats\":{repeats},\"mode\":\"{}\",\"source_bytes\":{},\"native_bytes\":{},\"compile_ns\":{}}}",
                mode.name(),
                c.len(),
                std::fs::metadata(&library).unwrap().len(),
                started.elapsed().as_nanos()
            );
        }
    }
    fixture
}

#[derive(Debug, PartialEq, Eq)]
struct Answer {
    value: i64,
    arguments: Vec<i64>,
    effects: Vec<&'static str>,
    override_calls: usize,
}
struct Run {
    answer: Answer,
    reads: usize,
    reuses: usize,
    misses: usize,
    optimized: usize,
    fallback: usize,
    elapsed: u128,
}

fn run<const TRACE: bool, const EVENT: bool>(
    fixture: &mut Fixture,
    repeats: usize,
    mode: Mode,
    scenario: Scenario,
    iterations: usize,
) -> Run {
    // The original adapter's observation state is independent of this fixture.
    PROBE.with_borrow_mut(|p| *p = Probe::new(Case::Canonical, [0, 1]));
    let bootstrap = crate::dispatch::build_bootstrap();
    let runtime = &bootstrap.runtime;
    runtime.publish_canonical_exports();
    runtime.install_execution_objects();
    let stack_anchor = 0usize;
    runtime.gc_set_stack_bottom(core::ptr::from_ref(&stack_anchor).addr() + size_of::<usize>());
    if matches!(scenario, Scenario::Gc | Scenario::GcWrite) {
        runtime.gc_register_root_provider(gc_provider);
    }
    let unloads = Rc::new(RefCell::new(Unloads::default()));
    let baseline = fixture.load("baseline", unloads.clone());
    let candidate_unit = fixture.load(
        if mode == Mode::Reuse {
            "reuse"
        } else {
            "control"
        },
        unloads.clone(),
    );
    let api = FixtureApi {
        enter: crate::registration::birnam_enter_resumable_v1,
        send: send::<TRACE, EVENT>,
        intern: plain_intern,
        integer: integer::<EVENT>,
        value: plain_value,
        nil: shim_nil,
        global: shim_global,
    };
    for unit in [&baseline, &candidate_unit] {
        // SAFETY: initialize the known local adapter, owned through every call.
        unsafe {
            let init: unsafe extern "C" fn(*const FixtureApi) =
                core::mem::transmute(unit.symbol(c"fixture_init"));
            init(&api);
        }
    }
    if mode == Mode::Reuse && repeats >= 2 {
        // SAFETY: the compiler's development-only prelude declares this exact
        // function-pointer signature; the unit remains owned by Trial/Selected.
        unsafe {
            type Reuse = unsafe extern "C" fn(Oop, Oop, Oop, *mut ReadCache) -> Oop;
            let init: unsafe extern "C" fn(Reuse) =
                core::mem::transmute(candidate_unit.symbol(c"__birnam_read_cache_init"));
            init(reuse::<TRACE, EVENT>);
        }
    }
    let thunk = baseline.entry(99);
    let descriptor = FrameDescriptor::admit_portable_v2(
        &fixture.descriptor_bytes,
        fixture.row(),
        7408,
        baseline.entry(0),
    )
    .unwrap();
    let generation = RegistrationGenerationLease::new(7408);
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
    runtime.export(b"Probe", receiver).unwrap();
    let selector = runtime.intern(b"generatedRun:").unwrap();
    for (name, method) in [
        (selector, thunk),
        (
            runtime.intern(b"readReuseDouble").unwrap(),
            doubled::<TRACE> as Method,
        ),
        (
            runtime.intern(&sum_selector(repeats)).unwrap(),
            sum::<TRACE>,
        ),
        (
            runtime.intern(b"readReuseFinish:").unwrap(),
            finished::<TRACE>,
        ),
    ] {
        let _ = runtime.add_method(runtime.globals().object_vt, name, method);
    }
    let argument = runtime.make_integer(7);
    let _argument = ScopedOopRootGuard::new(runtime, argument);
    let epoch = unsafe { (*runtime.state.get()).vtables.mutation_epoch() };
    let candidate = Candidate::admit(
        descriptor.clone(),
        &fixture.descriptor_bytes,
        &fixture.descriptor_bytes,
        &(0..fixture.pc_count)
            .map(|pc| (pc, candidate_unit.entry(pc)))
            .collect::<Vec<_>>(),
        candidate_unit,
    )
    .unwrap();
    let candidate = if mode == Mode::Baseline {
        drop(candidate);
        None
    } else {
        Some(candidate)
    };
    *runtime.generated_deopt_trial.borrow_mut() = Some(Trial {
        descriptor,
        baseline: Some(baseline),
        generation,
        retiring: false,
        candidate,
        epoch,
        disabled: mode == Mode::Baseline,
        optimized: 0,
        fallback: 0,
        deoptimizations: 0,
        observe: observe::<TRACE, EVENT>,
    });
    let _scope = Scope(runtime);
    if scenario == Scenario::BeforeEntry {
        invalidate(runtime);
    }
    if scenario == Scenario::Saturated {
        runtime.read_reuse_gc_epoch.set(u64::MAX - 1);
        for _ in 0..2 {
            runtime.enter(|| runtime.gc_collect_from_native_range(0, 0));
            assert_eq!(runtime.read_reuse_gc_epoch.get(), u64::MAX);
        }
    }
    let invoke = || {
        OBS.with_borrow_mut(|o| *o = Observation::new(scenario, fixture.slots[0], argument));
        // Re-arm experimental candidate selection per call for the frequent
        // invalidation workload. The baseline binding/Frame remain unchanged.
        if scenario == Scenario::Invalidate {
            let mut trial = runtime.generated_deopt_trial.borrow_mut();
            let trial = trial.as_mut().unwrap();
            trial.epoch = unsafe { (*runtime.state.get()).vtables.mutation_epoch() };
            trial.disabled = mode == Mode::Baseline;
        }
        runtime.enter(|| runtime.send_slice(receiver, selector, &[argument]))
    };
    if !TRACE {
        for _ in 0..64 {
            let answer = invoke();
            assert_eq!(runtime.integer_value(answer), 14 * repeats as i64);
        }
    }
    {
        let mut trial = runtime.generated_deopt_trial.borrow_mut();
        let trial = trial.as_mut().unwrap();
        trial.optimized = 0;
        trial.fallback = 0;
        trial.deoptimizations = 0;
    }
    let mut total = 0i64;
    let started = Instant::now();
    for _ in 0..iterations {
        let answer = invoke();
        total += runtime.integer_value(answer);
    }
    let elapsed = started.elapsed().as_nanos();
    let expected = if scenario == Scenario::GcWrite && repeats != 0 {
        14 + (repeats as i64 - 1) * 7
    } else {
        14 * repeats as i64
    };
    assert_eq!(total, expected * iterations as i64);
    let (answer, reads, reuses, misses) = OBS.with_borrow(|o| {
        if TRACE {
            assert_eq!(o.effects, ["double", "sum", "finish"]);
            assert_eq!(o.visits.len(), 3);
            for (pc, visit) in o.visits.iter().enumerate() {
                assert_eq!(visit.0, o.visits[0].0);
                assert_eq!(visit.1, pc);
            }
            let expected_args: Vec<_> = (0..repeats)
                .map(|i| {
                    if scenario == Scenario::GcWrite && i != 0 {
                        7
                    } else {
                        14
                    }
                })
                .collect();
            assert_eq!(o.arguments, expected_args);
            if EVENT && repeats != 0 {
                assert!(o.event_done);
            }
        }
        (
            Answer {
                value: expected,
                arguments: o.arguments.clone(),
                effects: o.effects.clone(),
                override_calls: o.override_calls,
            },
            o.reads,
            o.reuses,
            o.misses,
        )
    });
    runtime
        .remove_method(runtime.globals().object_vt, selector)
        .unwrap();
    let (optimized, fallback) = {
        let mut trial = runtime.generated_deopt_trial.borrow_mut();
        let trial = trial.as_mut().unwrap();
        assert_eq!(trial.generation.holder_count(), 1);
        trial.retiring = true;
        assert!(trial.collect_baseline());
        trial.candidate.take();
        (trial.optimized, trial.fallback)
    };
    assert_eq!(unloads.borrow().count, 2);
    let d = runtime.stackless_registration_diagnostics();
    assert_eq!(
        (
            d.live_entries,
            d.active_frame_pins,
            d.acquired_entries,
            d.continuation_frame_pins,
            d.registration_generation_leases
        ),
        (0, 0, 0, 0, 0)
    );
    assert_eq!(runtime.stackless_runtime_diagnostics().live_frames, 0);
    Run {
        answer,
        reads,
        reuses,
        misses,
        optimized,
        fallback,
        elapsed,
    }
}

#[test]
fn frame_read_reuse_experiment() {
    let prepared = std::env::var_os("BIRNAM_74H_PREPARE").map(PathBuf::from);
    if let Some(path) = &prepared {
        std::fs::create_dir_all(path).unwrap();
    }
    for repeats in [0, 2, 4, 16] {
        let mut fixture = compile(repeats, None);
        if let Some(path) = &prepared {
            for mode in [Mode::Baseline, Mode::Control, Mode::Reuse] {
                for suffix in ["c", "dylib"] {
                    std::fs::copy(
                        fixture.directory.join(format!("{}.{suffix}", mode.name())),
                        path.join(format!("{repeats}-{}.{suffix}", mode.name())),
                    )
                    .unwrap();
                }
            }
            continue;
        }
        let scenarios: &[Scenario] = if repeats == 16 {
            &[
                Scenario::Ordinary,
                Scenario::BeforeEntry,
                Scenario::Invalidate,
                Scenario::ReadOverride,
                Scenario::WriteOverride,
                Scenario::LookupOverride,
                Scenario::Gc,
                Scenario::GcWrite,
                Scenario::Saturated,
            ]
        } else {
            &[Scenario::Ordinary]
        };
        for &scenario in scenarios {
            let mut reference = None;
            for mode in [Mode::Baseline, Mode::Control, Mode::Reuse] {
                let result = if scenario.inject() {
                    run::<true, true>(&mut fixture, repeats, mode, scenario, 1)
                } else {
                    run::<true, false>(&mut fixture, repeats, mode, scenario, 1)
                };
                if let Some(answer) = &reference {
                    assert_eq!(&result.answer, answer, "{scenario:?} {mode:?}");
                } else {
                    reference = Some(result.answer);
                }
                let expected_reuses = if mode == Mode::Reuse && scenario == Scenario::Ordinary {
                    repeats.saturating_sub(1)
                } else {
                    0
                };
                assert_eq!(result.reuses, expected_reuses, "{scenario:?} {mode:?}");
                println!(
                    "READ_REUSE_CASE {{\"repeats\":{repeats},\"scenario\":\"{}\",\"mode\":\"{}\",\"answer\":{},\"reads\":{},\"reuses\":{},\"misses\":{},\"optimized_entries\":{},\"fallback_entries\":{},\"unloads\":2}}",
                    scenario.name(),
                    mode.name(),
                    reference.as_ref().unwrap().value,
                    result.reads,
                    result.reuses,
                    result.misses,
                    result.optimized,
                    result.fallback
                );
            }
        }
        for scenario in if repeats == 16 {
            &[Scenario::Ordinary, Scenario::Invalidate][..]
        } else {
            &[Scenario::Ordinary][..]
        } {
            let result = if scenario.inject() {
                run::<true, true>(&mut fixture, repeats, Mode::Reuse, *scenario, 64)
            } else {
                run::<true, false>(&mut fixture, repeats, Mode::Reuse, *scenario, 64)
            };
            assert_eq!(
                result.optimized,
                if *scenario == Scenario::Invalidate {
                    128
                } else {
                    192
                }
            );
            println!(
                "READ_REUSE_REPEAT {{\"repeats\":{repeats},\"scenario\":\"{}\",\"calls\":64,\"unloads\":2,\"live_frames\":0,\"live_registrations\":0}}",
                scenario.name()
            );
        }
    }
}

#[test]
#[ignore = "74H controlled fresh-process benchmark; use the measurement runner"]
fn frame_read_reuse_benchmark() {
    let prepared =
        PathBuf::from(std::env::var_os("BIRNAM_74H_ARTIFACTS").expect("prepared native artifacts"));
    let mode = match std::env::var("BIRNAM_74H_MODE").unwrap().as_str() {
        "control" => Mode::Control,
        "reuse" => Mode::Reuse,
        _ => panic!("unknown benchmark mode"),
    };
    for repeats in [0, 2, 4, 16] {
        let mut fixture = compile(repeats, Some(&prepared));
        for scenario in [
            Scenario::Ordinary,
            Scenario::Invalidate,
            Scenario::BeforeEntry,
        ] {
            let result = if scenario.inject() {
                run::<false, true>(&mut fixture, repeats, mode, scenario, 4096)
            } else {
                run::<false, false>(&mut fixture, repeats, mode, scenario, 4096)
            };
            println!(
                "READ_REUSE_TIME {{\"repeats\":{repeats},\"scenario\":\"{}\",\"mode\":\"{}\",\"iterations\":4096,\"warmup\":64,\"elapsed_ns\":{},\"optimized_entries\":{},\"fallback_entries\":{}}}",
                scenario.name(),
                mode.name(),
                result.elapsed,
                result.optimized,
                result.fallback
            );
        }
    }
}
