use super::*;
use crate::dispatch::build_bootstrap;
use crate::execution::{FrameDescriptorV2Spec, probe_spec};
use crate::registration::{BirnamResumableRegistrationV2, CARRIER_VERSION_V2, PREPARE_V1};
use crate::runtime::{ScopedOopRootGuard, current_thread_token};
use birnam_artifact::code_binding_inventory;
use birnam_codegen::{Block, Expr, Statement, portable};
use core::cell::{Cell, RefCell};
use std::borrow::Cow;

#[test]
fn disabled_closure_mode_skips_preparation_without_changing_ordinary_staging() {
    struct Mode(bool);
    impl Drop for Mode {
        fn drop(&mut self) {
            closure_elision::ENABLED.set(self.0);
        }
    }
    let _mode = Mode(closure_elision::ENABLED.replace(false));
    let bytes = portable::encode_block(
        &Block {
            params: vec![b"input"],
            body: vec![
                Statement::Let {
                    name: b"closure",
                    value: Expr::Block(Box::new(Block {
                        params: vec![],
                        body: vec![Statement::Expr(Expr::Global(b"input"))],
                    })),
                },
                Statement::Let {
                    name: b"result",
                    value: Expr::Send {
                        receiver: Box::new(Expr::Global(b"closure")),
                        selector: Cow::Borrowed(b"value"),
                        args: vec![],
                        source: b"",
                    },
                },
                Statement::Expr(Expr::Global(b"closure")),
            ],
        },
        &[],
    )
    .unwrap();
    let programs = portable_inventory::encode_closure(&[&bytes]).unwrap();
    let inventory = portable_inventory::inspect(&programs).unwrap();
    assert_eq!(inventory.program_count, 2);
    let bindings = code_binding_inventory::encode(&[]).unwrap();
    let selected = selection::encode(
        &programs,
        &bindings,
        &[],
        &vec![selection::Disposition::Resumable; inventory.program_count as usize],
    )
    .unwrap();
    let view = selection::inspect(&selected, &programs, &bindings).unwrap();
    let descriptors: Vec<_> = (0..view.row_count as usize)
        .map(|index| {
            let row = view.row(index).unwrap();
            let logical = resumable::lower_portable_block(
                inventory.program(row.subject_index as usize).unwrap(),
            )
            .unwrap();
            let layout = resumable::color_slots(&logical).unwrap();
            resumable_descriptor::compile_portable_descriptor_v2(&logical, &layout, row, 1).unwrap()
        })
        .collect();
    let authority = selection::authority(&programs, &bindings, &selected).unwrap();
    let specs: Vec<_> = descriptors
        .iter()
        .enumerate()
        .map(|(index, descriptor)| manifest::EntrySpec {
            selection_row_index: index as u32,
            driver_argument_count: 1,
            lowering_version: resumable_descriptor::LOWERING_VERSION_V1,
            descriptor,
        })
        .collect();
    let manifest = manifest::materialize(authority, &view, &specs).unwrap();
    let mut registry = Registry::default();
    registry.stage(&programs, &bindings, &selected, &manifest, authority);
    assert_eq!(registry.staged_closures.capacity(), 0);
    let ordinary: Vec<_> = registry
        .staged
        .iter()
        .map(|p| p.as_ref().map(|p| p.descriptor.clone()))
        .collect();
    assert!(ordinary.iter().any(Option::is_some));
    for enabled in [true, false, true] {
        closure_elision::ENABLED.set(enabled);
        registry.stage(&programs, &bindings, &selected, &manifest, authority);
        assert_eq!(
            registry
                .staged
                .iter()
                .map(|p| p.as_ref().map(|p| p.descriptor.clone()))
                .collect::<Vec<_>>(),
            ordinary
        );
        if enabled {
            assert_eq!(registry.staged_closures.len(), 2);
            assert_eq!(registry.staged_closures.iter().flatten().count(), 1);
            for (index, prepared) in registry.staged_closures.iter().enumerate() {
                if let Some(prepared) = prepared {
                    assert_eq!(prepared.descriptor, descriptors[index]);
                }
            }
        } else {
            assert!(registry.staged_closures.is_empty());
        }
    }
}

#[test]
fn real_one_pc_leaf_preparation_uses_the_registered_driver_arity() {
    let bytes = portable::encode_block(
        &Block {
            params: vec![b"input"],
            body: vec![Statement::Expr(Expr::Global(b"input"))],
        },
        &[],
    )
    .unwrap();
    let programs = portable_inventory::encode_closure(&[&bytes]).unwrap();
    let bindings = code_binding_inventory::encode(&[]).unwrap();
    let selected = selection::encode(
        &programs,
        &bindings,
        &[],
        &[selection::Disposition::Resumable],
    )
    .unwrap();
    let selected = selection::inspect(&selected, &programs, &bindings).unwrap();
    let row = selected.row(0).unwrap();
    let logical = resumable::lower_portable_block(&bytes).unwrap();
    let layout = resumable::color_slots(&logical).unwrap();
    let baseline = birnam_codegen::resumable_block::compile_portable_materialization_block_v1(
        &bytes,
        row,
        0,
        0,
        &[],
    )
    .unwrap();
    assert_eq!(baseline.driver_argument_count, 0);
    let prepared = Prepared::new(&bytes, row, baseline.driver_argument_count).unwrap();
    assert!(prepared.leaf.is_some());
    assert_eq!(prepared.descriptor, baseline.descriptor);
    assert_ne!(
        prepared.descriptor,
        resumable_descriptor::compile_portable_descriptor_v2(&logical, &layout, row, 1).unwrap()
    );
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Case {
    Canonical,
    Mutation,
    CustomLookup,
    PrivateReceiver,
    Collection,
    Eviction,
    Saturated,
    ReplaceActive,
    Nested,
    FailedCompile,
    DescriptorMismatch,
    Abort,
    NewSymbol,
    Export,
    ReplaceTarget,
    RegisterRead,
    RegisterWrite,
    RegisterLookup,
}

#[derive(Default)]
struct Observations {
    effects: Vec<&'static str>,
    frames: Vec<([usize; 3], usize)>,
    lookups: usize,
    overrides: usize,
    evicted: Option<std::rc::Weak<Candidate>>,
}

thread_local! {
    static CASE: Cell<Case> = const { Cell::new(Case::Canonical) };
    static OBS: RefCell<Observations> = RefCell::new(Observations::default());
    static DECODES: Cell<usize> = const { Cell::new(0) };
    static MACHINE: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
    static GC_VALUE: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
    static CARRIED: Cell<usize> = const { Cell::new(0) };
    static ALIAS: Cell<usize> = const { Cell::new(0) };
    static NESTED: Cell<bool> = const { Cell::new(false) };
    static OUTER_FRAME: Cell<Option<[usize; 3]>> = const { Cell::new(None) };
}

pub(super) fn on_intern(runtime: &Runtime) {
    if CASE.get() != Case::Eviction || HITS.get() == 0 {
        return;
    }
    let already = OBS.with_borrow(|obs| obs.evicted.is_some());
    if already {
        return;
    }
    let mut registry = runtime.native_tier.borrow_mut();
    let candidate = registry
        .records
        .values()
        .find_map(|record| record.candidate.as_ref())
        .unwrap();
    let weak = Rc::downgrade(candidate);
    registry.records.clear();
    assert!(
        weak.upgrade().is_some(),
        "selected native activation must retain its code"
    );
    OBS.with_borrow_mut(|obs| obs.evicted = Some(weak));
}

pub(super) fn on_send(runtime: &Runtime, receiver: Oop) {
    if runtime.stackless_machine_capability(receiver).is_ok() {
        MACHINE.set(receiver);
    }
}

fn program_bytes() -> Vec<u8> {
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
            params: vec![b"probe", b"input"],
            body: vec![
                Statement::Let {
                    name: b"carried",
                    value: send(Expr::Global(b"input"), b"tierDouble", vec![]),
                },
                Statement::Let {
                    name: b"alias",
                    value: Expr::Global(b"carried"),
                },
                Statement::Expr(send(
                    Expr::Global(b"probe"),
                    b"tierEffect:alias:",
                    vec![Expr::Global(b"carried"), Expr::Global(b"alias")],
                )),
                Statement::Expr(send(
                    Expr::Global(b"probe"),
                    b"tierFinish:alias:",
                    vec![Expr::Global(b"carried"), Expr::Global(b"alias")],
                )),
            ],
        },
        &[],
    )
    .unwrap()
}

unsafe extern "C" fn shim_value(value: Oop) -> i64 {
    DECODES.set(DECODES.get() + 1);
    with_active(|runtime| runtime.integer_value(value))
}

fn observe(runtime: &Runtime, effect: &'static str) {
    let machine = MACHINE.get();
    let capability = runtime.stackless_machine_capability(machine).unwrap();
    let arena = unsafe {
        (*runtime.state.get())
            .execution
            .machine(current_thread_token(), capability)
            .unwrap()
            .arena()
    };
    let frame = arena.current_handle().unwrap();
    let visit = (
        frame.words().unwrap(),
        arena.program_counter(frame).unwrap(),
    );
    if NESTED.get() {
        assert_ne!(Some(visit.0), OUTER_FRAME.get());
        return;
    }
    OUTER_FRAME.set(Some(visit.0));
    OBS.with_borrow_mut(|obs| {
        obs.effects.push(effect);
        obs.frames.push(visit);
    });
}

unsafe extern "C" fn double(receiver: Oop, _: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 0);
    with_active(|runtime| {
        // The current Machine is captured by the baseline/candidate send shim
        // before the driver's ordinary Call invokes this native method.
        observe(runtime, "double");
        runtime.make_integer(runtime.integer_value(receiver) * 2)
    })
}

unsafe extern "C" fn observing_send(
    receiver: Oop,
    selector: Oop,
    args: *mut Oop,
    count: i64,
) -> Oop {
    with_active(|runtime| {
        if runtime.stackless_machine_capability(receiver).is_ok() {
            MACHINE.set(receiver);
        }
    });
    unsafe { shim_send(receiver, selector, args, count) }
}

unsafe extern "C" fn read_override(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    OBS.with_borrow_mut(|obs| obs.overrides += 1);
    unsafe { crate::execution::machine_oop_at(receiver, args, count) }
}

unsafe extern "C" fn lookup_override(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    OBS.with_borrow_mut(|obs| obs.lookups += 1);
    unsafe { crate::dispatch::vtable_lookup_method(receiver, args, count) }
}

unsafe extern "C" fn gc_write(mark: crate::abi::MarkFn) {
    if GC_VALUE.get().is_null() {
        return;
    }
    unsafe { mark(GC_VALUE.get()) };
    with_active(|runtime| {
        runtime
            .stackless_oop_at_put(MACHINE.get(), CARRIED.get(), GC_VALUE.get())
            .unwrap();
        runtime
            .stackless_oop_at_put(MACHINE.get(), ALIAS.get(), GC_VALUE.get())
            .unwrap();
    });
}

unsafe extern "C" fn effect(_: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 2);
    assert_eq!(unsafe { args.read() }, unsafe { args.add(1).read() });
    with_active(|runtime| {
        observe(runtime, "effect");
        let objects = unsafe { (*runtime.state.get()).execution.objects().unwrap() };
        match CASE.get() {
            Case::Mutation => {
                runtime.add_method(objects.machine_vtable, objects.oop_at, read_override);
            }
            Case::CustomLookup => {
                runtime.add_method(
                    runtime.globals().vtable_vt,
                    runtime.globals().lookup,
                    lookup_override,
                );
            }
            Case::PrivateReceiver => {
                runtime.ensure_private_vtable(MACHINE.get());
            }
            Case::Saturated => unsafe {
                (*runtime.state.get())
                    .vtables
                    .exhaust_mutation_epoch_for_test();
            },
            Case::ReplaceActive => {
                runtime.add_method(
                    runtime.globals().object_vt,
                    runtime.intern(b"value:value:").unwrap(),
                    replacement,
                );
                assert_eq!(
                    runtime.request_stackless_code_generation_retirement(101),
                    crate::runtime::CodeGenerationRetirement::Deferred
                );
            }
            Case::NewSymbol | Case::Export | Case::ReplaceTarget => {
                let before = unsafe { (*runtime.state.get()).vtables.mutation_epoch() };
                match CASE.get() {
                    Case::NewSymbol => {
                        runtime.intern(b"unrelatedNativeTierSymbol74N").unwrap();
                    }
                    Case::Export => {
                        runtime
                            .export(b"unrelatedNativeTierExport74N", runtime.canonical().nil)
                            .unwrap();
                    }
                    Case::ReplaceTarget => {
                        runtime.add_method(
                            runtime.globals().object_vt,
                            runtime.intern(b"tierFinish:alias:").unwrap(),
                            changed_finish,
                        );
                    }
                    _ => unreachable!(),
                }
                assert!(unsafe { (*runtime.state.get()).vtables.mutation_epoch() } > before);
            }
            Case::Nested if !NESTED.get() => {
                NESTED.set(true);
                let receiver = runtime
                    .allocate_instance(runtime.globals().object_vt, core::mem::size_of::<Oop>());
                let _root = ScopedOopRootGuard::new(runtime, receiver);
                let input = runtime.make_integer(3);
                let answer = runtime.send_slice(
                    receiver,
                    runtime.intern(b"value:value:").unwrap(),
                    &[receiver, input],
                );
                assert_eq!(runtime.integer_value(answer), 12);
                NESTED.set(false);
            }
            Case::Collection => {
                let value = runtime.make_integer(22);
                let _root = ScopedOopRootGuard::new(runtime, value);
                GC_VALUE.set(value);
                runtime.gc_register_root_provider(gc_write);
                let before = unsafe { (*runtime.state.get()).vtables.mutation_epoch() };
                runtime.gc_collect_from_native_range(0, 0);
                GC_VALUE.set(core::ptr::null_mut());
                assert_eq!(before, unsafe {
                    (*runtime.state.get()).vtables.mutation_epoch()
                });
            }
            _ => {}
        }
        runtime.canonical().nil
    })
}

unsafe extern "C" fn replacement(_: Oop, _: *const Oop, _: usize) -> Oop {
    with_active(|runtime| runtime.make_integer(99))
}

unsafe extern "C" fn finish(_: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 2);
    let (left, right) = unsafe { (args.read(), args.add(1).read()) };
    with_active(|runtime| {
        observe(runtime, "finish");
        assert_eq!(left, right);
        runtime.make_integer(runtime.integer_value(left) + runtime.integer_value(right))
    })
}

unsafe extern "C" fn changed_finish(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    // Preserve the effect/alias observation and change the current send's answer.
    unsafe { finish(receiver, args, count) };
    with_active(|runtime| runtime.make_integer(99))
}

fn check_registration_dependency(runtime: &Runtime, case: Case) {
    use crate::execution::{DescriptorIdentity, SemanticRegistrationV1};
    let method = match case {
        Case::RegisterRead => crate::execution::machine_oop_at as Method,
        Case::RegisterWrite => crate::execution::machine_oop_at_put as Method,
        Case::RegisterLookup => crate::dispatch::vtable_lookup_method as Method,
        _ => return,
    };
    let descriptor = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
        identity: DescriptorIdentity {
            implementation: 749,
            generation: 749,
            source_map: 749,
        },
        program_counter_count: 1,
        driver_argument_count: 0,
        call_sites: Vec::new(),
        step_method: replacement,
        edges: vec![],
        ..probe_spec(3, 1)
    })
    .unwrap();
    let before = unsafe { (*runtime.state.get()).vtables.mutation_epoch() };
    let registration = runtime
        .register_stackless_resumable(SemanticRegistrationV1::new(method, descriptor, 1, 0, 1))
        .unwrap();
    assert_eq!(
        unsafe { (*runtime.state.get()).vtables.mutation_epoch() },
        before
    );
    let key = runtime.native_tier.borrow().records.keys().next().copied();
    if let Some(key) = key {
        assert!(
            runtime.native_tier.borrow().records[&key]
                .candidate
                .is_some()
        );
        assert!(select(runtime, MACHINE.get(), key, 0).is_none());
        assert!(runtime.native_tier.borrow().records[&key].disabled);
    }
    runtime
        .unregister_unpublished_stackless_resumable(registration)
        .unwrap();
    assert_eq!(
        unsafe { (*runtime.state.get()).vtables.mutation_epoch() },
        before
    );
    // Restoring the dependency does not resurrect a withdrawn candidate. The
    // next ordinary invocation below must run all three baseline PCs correctly.
}

fn run(case: Case, optimized: bool) -> (i64, Vec<&'static str>, usize, usize) {
    // These probes inject events through the PC-specialized C shims and count
    // that path's entries. Whole-PC regions have independent boundary probes.
    regions::ENABLED.set(false);
    CASE.set(Case::Canonical);
    NESTED.set(false);
    OUTER_FRAME.set(None);
    HITS.set(0);
    DECODES.set(0);
    OBS.set(Observations::default());
    let bytes = program_bytes();
    let programs = portable_inventory::encode_closure(&[&bytes]).unwrap();
    let bindings = code_binding_inventory::encode(&[]).unwrap();
    let selected = selection::encode(
        &programs,
        &bindings,
        &[],
        &[selection::Disposition::Resumable],
    )
    .unwrap();
    let view = selection::inspect(&selected, &programs, &bindings).unwrap();
    let row = view.row(0).unwrap();
    let logical = resumable::lower_portable_block(&bytes).unwrap();
    let layout = resumable::color_slots(&logical).unwrap();
    for (name, slot) in [
        (b"carried".as_slice(), &CARRIED),
        (b"alias".as_slice(), &ALIAS),
    ] {
        let operation = logical.blocks.iter().flat_map(|block| &block.operations).find(|operation| matches!(&operation.kind, resumable::OperationKind::Bind {name: actual, ..} if *actual == name)).unwrap();
        slot.set(layout.value_slots[operation.result.0 as usize] as usize);
    }
    let compiled = birnam_codegen::resumable_block::compile_portable_materialization_block_v1(
        &bytes,
        row,
        0,
        0,
        &[],
    )
    .unwrap();
    let authority = selection::authority(&programs, &bindings, &selected).unwrap();
    let manifest_bytes = manifest::materialize(
        authority,
        &view,
        &[manifest::EntrySpec {
            selection_row_index: 0,
            driver_argument_count: 1,
            lowering_version: resumable_descriptor::LOWERING_VERSION_V1,
            descriptor: &compiled.descriptor,
        }],
    )
    .unwrap();
    let entry = manifest::inspect(&manifest_bytes, authority, &view)
        .unwrap()
        .entry(0)
        .unwrap();
    let mut source = include_bytes!("shims.c").to_vec();
    source.extend_from_slice(b"extern long birnam_smallIntValue(void*);\n");
    source.extend(resumable_c::emit_step_method(0, &logical, &layout, 1).unwrap());
    source.extend(resumable_c::emit_entry_thunk(0, row.selector));
    source.extend_from_slice(b"void* get_entry(long n){return n ? (void*)__birnam_resumable_step_0 : (void*)__birnam_resumable_thunk_0;}\n");
    let source = CString::new(source).unwrap();
    let unit = unsafe {
        Unit::compile(
            &source,
            &[
                (c"birnam_send", observing_send as *const c_void),
                (c"birnam_intern", shim_intern as *const c_void),
                (c"birnam_smallIntValue", shim_value as *const c_void),
                (c"birnam_mkSmallInt", shim_integer as *const c_void),
                (c"birnam_nil", shim_nil as *const c_void),
            ],
        )
    }
    .unwrap();
    let getter: unsafe extern "C" fn(i64) -> *mut c_void =
        unsafe { core::mem::transmute(unit.symbol(c"get_entry").unwrap().as_ptr()) };
    let thunk: Method = unsafe { core::mem::transmute(getter(0)) };
    let step: Method = unsafe { core::mem::transmute(getter(1)) };
    let bootstrap = build_bootstrap();
    let runtime = &bootstrap.runtime;
    runtime.publish_canonical_exports();
    runtime.install_execution_objects();
    runtime.gc_set_precise(true);
    runtime.add_method(
        runtime.globals().object_vt,
        runtime.intern(b"tierDouble").unwrap(),
        double,
    );
    runtime.add_method(
        runtime.globals().object_vt,
        runtime.intern(b"tierEffect:alias:").unwrap(),
        effect,
    );
    runtime.add_method(
        runtime.globals().object_vt,
        runtime.intern(b"tierFinish:alias:").unwrap(),
        finish,
    );
    assert!(runtime.begin_staged_portable_resumable_unit(
        &programs,
        &bindings,
        &selected,
        &manifest_bytes,
        authority,
        101,
        103
    ));
    if matches!(case, Case::FailedCompile | Case::DescriptorMismatch) {
        let mut registry = runtime.native_tier.borrow_mut();
        let prepared = Rc::get_mut(registry.staged[0].as_mut().unwrap()).unwrap();
        if case == Case::FailedCompile {
            prepared.source = CString::new("invalid generated C").unwrap();
        } else {
            prepared.descriptor[0] ^= 1;
        }
    }
    let mut envelope = BirnamResumableRegistrationV2 {
        carrier_version: CARRIER_VERSION_V2,
        struct_size: core::mem::size_of::<BirnamResumableRegistrationV2>() as u32,
        operation: PREPARE_V1,
        flags: 0,
        manifest_entry_index: 0,
        reserved0: 0,
        registration_token: 0,
        descriptor_bytes: compiled.descriptor.as_ptr(),
        descriptor_length: compiled.descriptor.len() as u64,
        entry_thunk: Some(thunk),
        step_method: Some(step),
        implementation_oop: core::ptr::null_mut(),
        code_generation_identity: 101,
        code_generation_lease: 103,
        portable_implementation_identity: entry.portable_implementation_identity,
        language_arity: row.language_arity,
        driver_argument_count: 1,
        descriptor_sha256: entry.descriptor_sha256,
        receiver_capture_offset_words: 1,
        reserved1: 0,
    };
    assert_eq!(
        runtime.enter(|| unsafe { runtime.register_resumable_envelope(&mut envelope) }),
        crate::registration::OK_V1
    );
    let selector = runtime.intern(row.selector).unwrap();
    runtime.add_method(runtime.globals().object_vt, selector, thunk);
    if case == Case::Abort {
        assert!(runtime.cancel_staged_resumable_unit());
        assert!(
            runtime
                .lookup_method(runtime.globals().object_vt, selector)
                .is_none()
        );
        runtime.native_tier.borrow_mut().prune();
        assert!(runtime.native_tier.borrow().records.is_empty());
        assert_eq!(
            runtime
                .stackless_registration_diagnostics()
                .registered_entries,
            0
        );
        println!(
            "NATIVE_TIER_CASE {{\"case\":\"Abort\",\"optimized\":{optimized},\"answer\":0,\"entries\":0,\"baseline_entries\":0}}"
        );
        return (0, Vec::new(), 0, 0);
    }
    assert!(runtime.finish_staged_resumable_unit());
    assert_eq!(
        runtime.native_tier.borrow().records.len(),
        usize::from(case != Case::DescriptorMismatch)
    );
    if !optimized {
        runtime.native_tier.borrow_mut().records.clear();
    }
    let receiver =
        runtime.allocate_instance(runtime.globals().object_vt, core::mem::size_of::<Oop>());
    let _receiver_root = ScopedOopRootGuard::new(runtime, receiver);
    let input = runtime.make_integer(7);
    let _input_root = ScopedOopRootGuard::new(runtime, input);
    for _ in 0..25 {
        let answer = runtime.enter(|| runtime.send_slice(receiver, selector, &[receiver, input]));
        assert_eq!(runtime.integer_value(answer), 28);
    }
    assert_eq!(
        HITS.get() != 0,
        optimized && !matches!(case, Case::FailedCompile | Case::DescriptorMismatch),
        "candidate did not activate independently of answers"
    );
    HITS.set(0);
    DECODES.set(0);
    OBS.set(Observations::default());
    check_registration_dependency(runtime, case);
    CASE.set(case);
    let answer = runtime.enter(|| runtime.send_slice(receiver, selector, &[receiver, input]));
    let value = runtime.integer_value(answer);
    assert_eq!(
        value,
        match case {
            Case::Collection => 44,
            Case::ReplaceTarget => 99,
            _ => 28,
        }
    );
    OBS.with_borrow(|obs| {
        assert_eq!(obs.effects, ["double", "effect", "finish"]);
        // Call edges commit the successor PC before invoking the native
        // target; the terminal tail call observes the final PC.
        assert_eq!(
            obs.frames.iter().map(|visit| visit.1).collect::<Vec<_>>(),
            [1, 2, 2]
        );
        assert!(obs.frames.iter().all(|visit| visit.0 == obs.frames[0].0));
        if let Some(evicted) = &obs.evicted {
            assert!(evicted.upgrade().is_none());
        }
    });
    let counts = (HITS.get(), DECODES.get());
    if optimized {
        assert_eq!(
            counts,
            match case {
                Case::Canonical
                | Case::Collection
                | Case::ReplaceActive
                | Case::NewSymbol
                | Case::Export
                | Case::ReplaceTarget => (3, 0),
                Case::Nested => (6, 0),
                Case::FailedCompile
                | Case::DescriptorMismatch
                | Case::RegisterRead
                | Case::RegisterWrite
                | Case::RegisterLookup => (0, 3),
                Case::Eviction => (1, 2),
                _ => (2, 1),
            }
        );
    } else {
        assert_eq!(counts, (0, if case == Case::Nested { 6 } else { 3 }));
    }
    if case == Case::ReplaceActive {
        let newer = runtime.enter(|| runtime.send_slice(receiver, selector, &[receiver, input]));
        assert_eq!(
            runtime.integer_value(newer),
            99,
            "later send must see the replacement body"
        );
    }
    let observed = OBS.with_borrow(|obs| (value, obs.effects.clone(), obs.lookups, obs.overrides));
    runtime
        .remove_method(runtime.globals().object_vt, selector)
        .unwrap();
    assert!(runtime.retire_stackless_code_generation(101));
    runtime.native_tier.borrow_mut().prune();
    assert!(runtime.native_tier.borrow().records.is_empty());
    let marker = if matches!(
        case,
        Case::NewSymbol
            | Case::Export
            | Case::ReplaceTarget
            | Case::RegisterRead
            | Case::RegisterWrite
            | Case::RegisterLookup
    ) {
        "NATIVE_TIER_REVALIDATION_CASE"
    } else {
        "NATIVE_TIER_CASE"
    };
    println!(
        "{marker} {{\"case\":\"{case:?}\",\"optimized\":{optimized},\"answer\":{value},\"entries\":{},\"baseline_entries\":{},\"lookup_effects\":{},\"read_override_effects\":{}}}",
        counts.0, counts.1, observed.2, observed.3
    );
    observed
}

#[test]
fn revalidates_actual_dependencies_without_caching_ordinary_targets() {
    for case in [
        Case::NewSymbol,
        Case::Export,
        Case::ReplaceTarget,
        Case::RegisterRead,
        Case::RegisterWrite,
        Case::RegisterLookup,
    ] {
        assert_eq!(run(case, false), run(case, true), "{case:?}");
    }
}

#[test]
fn portable_registration_activates_and_recovers_on_the_same_frame() {
    for case in [
        Case::Canonical,
        Case::Mutation,
        Case::CustomLookup,
        Case::PrivateReceiver,
        Case::Collection,
        Case::Eviction,
        Case::Saturated,
        Case::ReplaceActive,
        Case::Nested,
        Case::FailedCompile,
        Case::DescriptorMismatch,
        Case::Abort,
    ] {
        assert_eq!(run(case, false), run(case, true), "{case:?}");
    }
}
