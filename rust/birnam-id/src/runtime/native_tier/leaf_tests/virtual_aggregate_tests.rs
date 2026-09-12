use super::*;
use crate::runtime::native_tier::virtual_aggregate;
use core::cell::RefCell;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

struct ForcedExit(Option<usize>);
impl ForcedExit {
    fn set(value: Option<usize>) -> Self {
        Self(virtual_aggregate::FORCE_EXIT.replace(value))
    }
}
impl Drop for ForcedExit {
    fn drop(&mut self) {
        virtual_aggregate::FORCE_EXIT.set(self.0);
    }
}

struct Mode(bool);
impl Mode {
    fn set(value: bool) -> Self {
        Self(virtual_aggregate::ENABLED.replace(value))
    }
}
impl Drop for Mode {
    fn drop(&mut self) {
        virtual_aggregate::ENABLED.set(self.0);
    }
}

struct Native {
    handle: *mut c_void,
    directory: PathBuf,
}
impl Native {
    fn load(r: &Runtime) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let directory = std::env::temp_dir().join(format!(
            "birnam-fields-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("fields.dylib");
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts/fixtures/field-projection.c");
        assert!(
            std::process::Command::new("cc")
                .args(["-dynamiclib", "-O2"])
                .arg(source)
                .arg("-o")
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        let name = CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        let handle = unsafe { crate::loader::open(name.as_ptr(), 0x1 | 0x4) };
        assert!(!handle.is_null());
        let native = Self { handle, directory };
        let setup: unsafe extern "C" fn(Oop) =
            unsafe { core::mem::transmute(native.symbol(c"field_setup")) };
        unsafe { setup(r.canonical().nil) };
        native
    }
    fn symbol(&self, name: &std::ffi::CStr) -> *mut c_void {
        let pointer = unsafe { crate::loader::symbol(self.handle, name.as_ptr()) };
        assert!(!pointer.is_null());
        pointer
    }
    fn method(&self) -> Method {
        unsafe { core::mem::transmute(self.symbol(c"field_project")) }
    }
    fn enable(&self, enabled: bool) {
        let function: unsafe extern "C" fn(u32) =
            unsafe { core::mem::transmute(self.symbol(c"field_enable")) };
        unsafe { function(u32::from(enabled)) };
    }
}
impl Drop for Native {
    fn drop(&mut self) {
        unsafe extern "C" {
            fn dlclose(handle: *mut c_void) -> i32;
        }
        unsafe { dlclose(self.handle) };
        std::fs::remove_dir_all(&self.directory).unwrap();
    }
}

#[derive(Clone, Copy, Debug)]
enum Case {
    Field,
    Object,
    Observe,
    Rebind,
    Withdraw,
    Twins,
    Capture,
    Suspend,
    MixedLayout,
    RawTail,
    Oversized,
}
fn program(case: Case) -> Vec<u8> {
    let send = |receiver, selector, args| Expr::Send {
        receiver: Box::new(Expr::Global(receiver)),
        selector: Cow::Borrowed(selector),
        args,
        source: b"",
    };
    let mut body = vec![
        Statement::Let {
            name: b"copy",
            value: send(b"prototype", b"duplicate", vec![]),
        },
        Statement::Let {
            name: b"alias",
            value: Expr::Global(b"copy"),
        },
        Statement::Let {
            name: b"field",
            value: send(b"copy", b"project", vec![]),
        },
    ];
    if matches!(case, Case::Twins) {
        body.insert(
            2,
            Statement::Let {
                name: b"second",
                value: send(b"copy", b"duplicate", vec![]),
            },
        );
    }
    if matches!(
        case,
        Case::Observe | Case::Twins | Case::Capture | Case::Suspend
    ) {
        body.push(Statement::Let {
            name: b"observed",
            value: send(
                b"observer",
                if matches!(case, Case::Twins) {
                    b"inspect:alias:second:"
                } else {
                    b"inspect:alias:"
                },
                if matches!(case, Case::Twins) {
                    vec![
                        Expr::Global(b"copy"),
                        Expr::Global(b"alias"),
                        Expr::Global(b"second"),
                    ]
                } else {
                    vec![Expr::Global(b"copy"), Expr::Global(b"alias")]
                },
            ),
        });
    }
    if matches!(case, Case::Capture | Case::Suspend) {
        // The first observer changes the next publication into a control
        // action. Resume supplies that Call's result; the final observer must
        // see the same clone and aliases without replaying the first effect.
        body.push(Statement::Expr(send(
            b"observer",
            b"inspect:alias:",
            vec![Expr::Global(b"copy"), Expr::Global(b"alias")],
        )));
        body.push(Statement::Let {
            name: b"resumed",
            value: send(
                b"observer",
                b"inspect:alias:",
                vec![Expr::Global(b"copy"), Expr::Global(b"alias")],
            ),
        });
    }
    body.push(Statement::Expr(Expr::Global(match case {
        Case::Object => b"alias",
        Case::Capture | Case::Suspend => b"resumed",
        Case::Observe | Case::Twins => b"observed",
        _ => b"field",
    })));
    portable::encode_block(
        &Block {
            params: vec![b"prototype", b"observer"],
            body,
        },
        &[],
    )
    .unwrap()
}

thread_local! {
    static PAUSE: Cell<Option<bool>> = const { Cell::new(None) };
    static RECOVERED_IDENTITY: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
    static PROTOTYPE: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
    static OBSERVED: Cell<usize> = const { Cell::new(0) };
    static ALIASES: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}
unsafe extern "C" fn redefined(_: Oop, _: *const Oop, _: usize) -> Oop {
    OBSERVED.set(OBSERVED.get() + 1);
    with_active(|r| r.canonical().true_object)
}
unsafe extern "C" fn observe(_: Oop, args: *const Oop, count: usize) -> Oop {
    assert!((2..=3).contains(&count));
    let copy = unsafe { *args };
    assert_eq!(copy, unsafe { *args.add(1) });
    assert_ne!(copy, PROTOTYPE.get());
    let second = if count == 3 {
        Some(unsafe { *args.add(2) })
    } else {
        None
    };
    if let Some(second) = second {
        assert_ne!(copy, second);
        assert_ne!(second, PROTOTYPE.get());
    }
    if !RECOVERED_IDENTITY.get().is_null() {
        assert_eq!(copy, RECOVERED_IDENTITY.get());
    }
    OBSERVED.set(OBSERVED.get() + 1);
    with_active(|r| {
        if let Some(capture) = PAUSE.take() {
            RECOVERED_IDENTITY.set(copy);
            let objects = unsafe { (*r.state.get()).execution.objects().unwrap() };
            r.add_method(
                objects.machine_vtable,
                objects.call_site,
                if capture { capture_call } else { suspend_call },
            );
        }
        let cap = unsafe { (&*r.entry_scopes.get()).last().copied() }.unwrap();
        let aliases = || unsafe {
            let arena = (*r.state.get())
                .execution
                .machine(current_thread_token(), cap)
                .unwrap()
                .arena();
            let frame = arena.current_handle().unwrap();
            (0..arena.descriptor(frame).unwrap().slot_count())
                .filter(|slot| arena.read_oop(frame, *slot).unwrap() == copy)
                .collect::<Vec<_>>()
        };
        let before = aliases();
        assert!(before.len() >= 2);
        r.gc_collect_from_native_range(0, 0);
        assert_eq!(aliases(), before);
        ALIASES.replace(before);
        let heap = unsafe { &(*r.state.get()).heap };
        assert!(heap.contains(copy));
        if let Some(second) = second {
            assert!(heap.contains(second));
            for slot in 0..2 {
                assert_eq!(
                    heap.read_payload_word(copy, slot),
                    heap.read_payload_word(second, slot)
                );
            }
        }
        core::ptr::with_exposed_provenance_mut(heap.read_payload_word(copy, 0).unwrap())
    })
}

unsafe extern "C" fn suspend_call(machine: Oop, args: *const Oop, count: usize) -> Oop {
    let action = unsafe { crate::execution::machine_call_site(machine, args, count) };
    with_active(|r| {
        let cap = r.stackless_machine_capability(machine).unwrap();
        let (result, pc) = unsafe {
            let state = &*r.state.get();
            let arena = state
                .execution
                .machine(current_thread_token(), cap)
                .unwrap()
                .arena();
            (
                state.heap.read_payload_word(action, 8).unwrap(),
                arena
                    .program_counter(arena.current_handle().unwrap())
                    .unwrap(),
            )
        };
        r.stackless_suspend_current(machine, result, pc).unwrap()
    })
}

#[test]
fn virtual_fields_recover_identity_and_frame_aliases_before_collecting_observers() {
    compare_recovery(None);
}

#[test]
fn virtual_fields_force_recovery_at_every_admitted_call_boundary() {
    for stop in 1..=3 {
        compare_recovery(Some(stop));
    }
}

fn compare_recovery(forced_exit: Option<usize>) {
    for case in [
        Case::Field,
        Case::Object,
        Case::Observe,
        Case::Rebind,
        Case::Withdraw,
        Case::Twins,
        Case::Capture,
        Case::Suspend,
        Case::MixedLayout,
        Case::RawTail,
        Case::Oversized,
    ] {
        let mut evidence = Vec::new();
        for enabled in [false, true] {
            let _mode = Mode::set(false);
            RECOVERED_IDENTITY.set(core::ptr::null_mut());
            PAUSE.set(None);
            let bootstrap = crate::dispatch::build_bootstrap();
            let r = &bootstrap.runtime;
            r.publish_canonical_exports();
            r.install_execution_objects();
            let native = Native::load(r);
            let block = install(r, &program(case), 741801, None);
            let _block = ScopedOopRootGuard::new(r, block.receiver);
            let vt = r.create_vtable(r.globals().object_vt);
            r.set_layout(vt, core::ptr::null(), 0, crate::abi::SLOT_OOP);
            let project = r.intern(b"project").unwrap();
            r.add_method(vt, project, native.method());
            r.add_method(
                vt,
                r.intern(b"duplicate").unwrap(),
                crate::dispatch::object_clone,
            );
            if matches!(case, Case::MixedLayout | Case::RawTail) {
                let kinds = [crate::abi::SLOT_OOP, crate::abi::SLOT_RAW];
                r.set_layout(
                    vt,
                    kinds.as_ptr(),
                    if matches!(case, Case::RawTail) { 1 } else { 2 },
                    if matches!(case, Case::RawTail) {
                        crate::abi::SLOT_RAW
                    } else {
                        crate::abi::SLOT_OOP
                    },
                );
            }
            let prototype = r.allocate_instance(
                vt,
                if matches!(case, Case::Oversized) {
                    1032
                } else {
                    16
                },
            );
            let _prototype = ScopedOopRootGuard::new(r, prototype);
            let field = r.make_integer(71);
            let _field = ScopedOopRootGuard::new(r, field);
            for slot in 0..2 {
                assert!(unsafe {
                    (*r.state.get())
                        .heap
                        .write_payload_word(prototype, slot, field.addr())
                });
            }
            PROTOTYPE.set(prototype);
            let observer_vt = r.create_vtable(r.globals().object_vt);
            r.add_method(observer_vt, r.intern(b"inspect:alias:").unwrap(), observe);
            r.add_method(
                observer_vt,
                r.intern(b"inspect:alias:second:").unwrap(),
                observe,
            );
            let observer = r.allocate_instance(observer_vt, 0);
            let _observer = ScopedOopRootGuard::new(r, observer);
            let invoke =
                || r.enter(|| r.send_slice(block.receiver, block.selector, &[prototype, observer]));
            for _ in 0..128 {
                invoke();
            }
            virtual_aggregate::ENABLED.set(enabled);
            invoke(); // Prepare a retained declaration before withdrawing it.
            r.enter(|| r.gc_collect_from_native_range(0, 0));
            if matches!(case, Case::Rebind) {
                r.add_method(vt, project, redefined);
            }
            if matches!(case, Case::Withdraw) {
                native.enable(false);
            }
            virtual_aggregate::ENABLED.set(enabled);
            virtual_aggregate::HITS.set(0);
            virtual_aggregate::MATERIALIZED.set(0);
            OBSERVED.set(0);
            ALIASES.with_borrow_mut(Vec::clear);
            let controlled = matches!(case, Case::Capture | Case::Suspend);
            let machine = controlled.then(|| r.create_stackless_machine().unwrap());
            let _machine = machine.map(|machine| ScopedOopRootGuard::new(r, machine));
            let before = (r.gc_live_count(), r.gc_live_bytes());
            let _forced = ForcedExit::set(forced_exit);
            let answer = if let Some(machine) = machine {
                let capture = matches!(case, Case::Capture);
                PAUSE.set(Some(capture));
                let binding = block.descriptor.v2_entry_binding().unwrap();
                let StacklessTarget::Registered { entry } = r
                    .enter(|| r.resolve_stackless_target(block.receiver, block.selector))
                    .unwrap()
                else {
                    panic!("registered entry");
                };
                let paused = r
                    .enter(|| {
                        r.drive_stackless_with_resolver(
                            machine,
                            block.descriptor.clone(),
                            &[
                                (binding.receiver_slot, block.receiver),
                                (binding.argument_start, prototype),
                                (binding.argument_start + 1, observer),
                            ],
                            Some(entry),
                            |runtime, receiver, selector, _| {
                                runtime.resolve_stackless_target(receiver, selector)
                            },
                        )
                    })
                    .unwrap();
                let _paused = ScopedOopRootGuard::new(r, paused);
                assert_eq!(OBSERVED.get(), 1, "no later effect before resume");
                let pins = r.stackless_registration_diagnostics();
                assert_eq!(pins.active_frame_pins, usize::from(!capture));
                assert_eq!(pins.continuation_frame_pins, usize::from(capture));
                let objects = unsafe { (*r.state.get()).execution.objects().unwrap() };
                r.add_method(
                    objects.machine_vtable,
                    objects.call_site,
                    crate::execution::machine_call_site,
                );
                r.enter(|| r.gc_collect_from_native_range(0, 0));
                assert!(unsafe { (*r.state.get()).heap.contains(RECOVERED_IDENTITY.get()) });
                let answer = if capture {
                    r.enter(|| r.restore_stackless_continuation(machine, paused, field))
                        .unwrap()
                } else {
                    assert_eq!(paused, machine);
                    r.enter(|| r.resume_stackless_machine(machine, field))
                        .unwrap()
                };
                assert_eq!(
                    OBSERVED.get(),
                    2,
                    "resume neither replays nor skips effects"
                );
                assert_eq!(
                    r.stackless_registration_diagnostics()
                        .continuation_frame_pins,
                    0
                );
                answer
            } else {
                invoke()
            };
            let _answer = ScopedOopRootGuard::new(r, answer);
            if matches!(case, Case::Object) {
                assert_ne!(answer, prototype);
                assert_eq!(
                    unsafe { (*r.state.get()).heap.read_payload_word(answer, 0) },
                    Some(field.addr())
                );
                assert_eq!(
                    unsafe { (*r.state.get()).heap.read_payload_word(answer, 1) },
                    Some(field.addr())
                );
            } else {
                assert_eq!(
                    answer,
                    if matches!(case, Case::Rebind) {
                        r.canonical().true_object
                    } else {
                        field
                    }
                );
            }
            let optimized = enabled
                && !matches!(
                    case,
                    Case::Rebind
                        | Case::Withdraw
                        | Case::MixedLayout
                        | Case::RawTail
                        | Case::Oversized
                );
            let identities = if matches!(case, Case::Twins) { 2 } else { 1 };
            let identities = identities.min(forced_exit.unwrap_or(usize::MAX));
            assert_eq!(
                virtual_aggregate::HITS.get(),
                if optimized { identities } else { 0 },
                "{case:?}"
            );
            assert_eq!(
                virtual_aggregate::MATERIALIZED.get(),
                if optimized
                    && (forced_exit == Some(1)
                        || matches!(
                            case,
                            Case::Object
                                | Case::Observe
                                | Case::Twins
                                | Case::Capture
                                | Case::Suspend
                        ))
                {
                    identities
                } else {
                    0
                },
                "{case:?}"
            );
            assert_eq!(
                r.gc_live_count() - unsafe { (*r.state.get()).heap.physical_allocation_count() },
                usize::from(optimized && forced_exit != Some(1) && matches!(case, Case::Field))
            );
            assert_eq!(r.stackless_registration_diagnostics().active_frame_pins, 0);
            evidence.push((
                r.gc_live_count() as isize - before.0 as isize,
                r.gc_live_bytes() as isize - before.1 as isize,
                OBSERVED.get(),
                ALIASES.with_borrow(Clone::clone),
            ));
            r.enter(|| r.gc_collect_from_native_range(0, 0));
            assert!(unsafe { (*r.state.get()).heap.contains(answer) });
        }
        assert_eq!(evidence[0], evidence[1], "{case:?}");
    }
}

thread_local! {
    static REPLACEMENT: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
    static COLLECTIONS: Cell<usize> = const { Cell::new(0) };
}
unsafe extern "C" fn mutate_prototype(mark: crate::abi::MarkFn) {
    COLLECTIONS.set(COLLECTIONS.get() + 1);
    with_active(|r| {
        assert!(unsafe {
            (*r.state.get())
                .heap
                .write_payload_word(PROTOTYPE.get(), 0, REPLACEMENT.get().addr())
        });
        unsafe {
            mark(PROTOTYPE.get());
            mark(REPLACEMENT.get());
        }
    });
}

#[test]
fn virtual_field_budget_preserves_collection_before_clone_and_after_recovery() {
    let bytes = program(Case::Observe);
    let logical = resumable::lower_portable_block(&bytes).unwrap();
    let layout = resumable::color_slots(&logical).unwrap();
    let plan = birnam_codegen::virtual_aggregate::Plan::derive(&logical, &layout).unwrap();
    let box_size = crate::heap::Heap::allocation_total_size(8).unwrap();
    let prefix_bytes = (1 + plan.steps[..2]
        .iter()
        .map(|step| step.integer_boxes)
        .sum::<usize>())
        * box_size
        + crate::heap::Heap::allocation_total_size(16).unwrap();
    for gap in [
        prefix_bytes + box_size,
        prefix_bytes,
        prefix_bytes - box_size,
        box_size,
        0,
    ] {
        let mut evidence = Vec::new();
        for enabled in [false, true] {
            let _mode = Mode::set(false);
            let bootstrap = crate::dispatch::build_bootstrap();
            let r = &bootstrap.runtime;
            r.publish_canonical_exports();
            r.install_execution_objects();
            let native = Native::load(r);
            let block = install(r, &bytes, 741802, None);
            let _block = ScopedOopRootGuard::new(r, block.receiver);
            let vt = r.create_vtable(r.globals().object_vt);
            r.set_layout(vt, core::ptr::null(), 0, crate::abi::SLOT_OOP);
            r.add_method(
                vt,
                r.intern(b"duplicate").unwrap(),
                crate::dispatch::object_clone,
            );
            r.add_method(vt, r.intern(b"project").unwrap(), native.method());
            let prototype = r.allocate_instance(vt, 16);
            let _prototype = ScopedOopRootGuard::new(r, prototype);
            PROTOTYPE.set(prototype);
            let field = r.make_integer(71);
            let _field = ScopedOopRootGuard::new(r, field);
            assert!(unsafe {
                (*r.state.get())
                    .heap
                    .write_payload_word(prototype, 0, field.addr())
            });
            let replacement = r.make_integer(93);
            let _replacement = ScopedOopRootGuard::new(r, replacement);
            REPLACEMENT.set(replacement);
            let observer_vt = r.create_vtable(r.globals().object_vt);
            r.add_method(observer_vt, r.intern(b"inspect:alias:").unwrap(), observe);
            let observer = r.allocate_instance(observer_vt, 0);
            let _observer = ScopedOopRootGuard::new(r, observer);
            for _ in 0..128 {
                r.enter(|| r.send_slice(block.receiver, block.selector, &[prototype, observer]));
            }
            let machine = r.create_stackless_machine().unwrap();
            let _machine = ScopedOopRootGuard::new(r, machine);
            let current = unsafe { (*r.state.get()).heap.bytes_since_collection() };
            let target = crate::gc::AUTO_COLLECTION_THRESHOLD - gap;
            r.allocate_instance(
                r.globals().object_vt,
                target - current - crate::heap::Heap::allocation_total_size(0).unwrap(),
            );
            assert_eq!(
                unsafe { (*r.state.get()).heap.bytes_since_collection() },
                target
            );
            COLLECTIONS.set(0);
            OBSERVED.set(0);
            r.gc_register_root_provider(mutate_prototype);
            virtual_aggregate::ENABLED.set(enabled);
            virtual_aggregate::HITS.set(0);
            let anchor = [0usize; 32];
            r.gc_set_stack_bottom(anchor.as_ptr().addr() + core::mem::size_of_val(&anchor));
            let answer = r
                .enter(|| {
                    r.drive_stackless(
                        machine,
                        block.descriptor.clone(),
                        &[(0, block.receiver), (1, prototype), (2, observer)],
                    )
                })
                .unwrap();
            r.gc_set_stack_bottom(0);
            assert_eq!(
                virtual_aggregate::HITS.get(),
                usize::from(enabled && (gap == 0 || gap > prefix_bytes)),
                "gap={gap}"
            );
            assert_eq!(OBSERVED.get(), 1);
            assert!(COLLECTIONS.get() >= 2);
            evidence.push((r.integer_value(answer), COLLECTIONS.get(), unsafe {
                (*r.state.get()).heap.bytes_since_collection()
            }));
        }
        assert_eq!(evidence[0], evidence[1], "allocation gap={gap}");
    }
}

#[test]
#[ignore = "paired live-field timing; host qualification is external"]
fn virtual_field_timing() {
    let batches = std::env::var("BENCH_FIELD_BATCHES")
        .ok()
        .map(|value| value.parse::<usize>().unwrap())
        .unwrap_or(1);
    assert!((1..=500).contains(&batches));
    for (shape, case) in [(0, Case::Field), (1, Case::Object), (2, Case::Withdraw)] {
        let bytes = program(case);
        for payload in [16, 1024] {
            for pair in 0..4 {
                for enabled in if pair % 2 == 0 {
                    [false, true]
                } else {
                    [true, false]
                } {
                    let _mode = Mode::set(enabled);
                    let bootstrap = crate::dispatch::build_bootstrap();
                    let r = &bootstrap.runtime;
                    r.publish_canonical_exports();
                    r.install_execution_objects();
                    let native = Native::load(r);
                    let block = install(r, &bytes, 741803, None);
                    let _block = ScopedOopRootGuard::new(r, block.receiver);
                    let vt = r.create_vtable(r.globals().object_vt);
                    r.set_layout(vt, core::ptr::null(), 0, crate::abi::SLOT_OOP);
                    r.add_method(
                        vt,
                        r.intern(b"duplicate").unwrap(),
                        crate::dispatch::object_clone,
                    );
                    r.add_method(vt, r.intern(b"project").unwrap(), native.method());
                    let prototype = r.allocate_instance(vt, payload);
                    let _prototype = ScopedOopRootGuard::new(r, prototype);
                    let field = r.make_integer(71);
                    let _field = ScopedOopRootGuard::new(r, field);
                    for slot in 0..payload / core::mem::size_of::<Oop>() {
                        assert!(unsafe {
                            (*r.state.get())
                                .heap
                                .write_payload_word(prototype, slot, field.addr())
                        });
                    }
                    let invoke = || {
                        let answer = r.enter(|| {
                            r.send_slice(block.receiver, block.selector, &[prototype, field])
                        });
                        if shape == 1 {
                            assert_ne!(answer, prototype);
                            assert_eq!(
                                unsafe { (*r.state.get()).heap.read_payload_word(answer, 0) },
                                Some(field.addr())
                            );
                        } else {
                            assert_eq!(answer, field);
                        }
                    };
                    let start = std::time::Instant::now();
                    for _ in 0..128 {
                        invoke();
                    }
                    let cold_ns = start.elapsed().as_nanos();
                    if shape == 2 {
                        native.enable(false);
                    }
                    invoke();
                    r.enter(|| r.gc_collect_from_native_range(0, 0));
                    virtual_aggregate::HITS.set(0);
                    virtual_aggregate::MATERIALIZED.set(0);
                    let mut ns = 0;
                    let mut gc_ns = 0;
                    let mut allocation_bytes = 0;
                    let mut physical_saved = 0;
                    let mut journal_peak_bytes = 0;
                    for _ in 0..batches {
                        let before = r.gc_live_bytes();
                        let start = std::time::Instant::now();
                        for _ in 0..128 {
                            invoke();
                        }
                        ns += start.elapsed().as_nanos();
                        allocation_bytes += r.gc_live_bytes() - before;
                        let heap = unsafe { &(*r.state.get()).heap };
                        let missing = heap.allocation_count() - heap.physical_allocation_count();
                        assert_eq!(missing, if enabled && shape == 0 { 128 } else { 0 });
                        physical_saved += missing;
                        journal_peak_bytes =
                            journal_peak_bytes.max(heap.dead_allocation_storage_bytes());
                        assert!(
                            heap.bytes_since_collection() < crate::gc::AUTO_COLLECTION_THRESHOLD
                        );
                        let start = std::time::Instant::now();
                        r.enter(|| r.gc_collect_from_native_range(0, 0));
                        gc_ns += start.elapsed().as_nanos();
                    }
                    let calls = batches * 128;
                    let hits = virtual_aggregate::HITS.get();
                    let materialized = virtual_aggregate::MATERIALIZED.get();
                    assert_eq!(hits, if enabled && shape != 2 { calls } else { 0 });
                    assert_eq!(materialized, if enabled && shape == 1 { calls } else { 0 });
                    assert_eq!(r.stackless_registration_diagnostics().active_frame_pins, 0);
                    let metadata_bytes = r
                        .native_tier
                        .borrow()
                        .records
                        .values()
                        .filter_map(|record| record.candidate.as_ref())
                        .filter_map(|candidate| candidate.virtual_aggregate.as_ref())
                        .map(|compiled| compiled.storage_bytes())
                        .sum::<usize>();
                    let physical_saved_bytes =
                        physical_saved * crate::heap::Heap::allocation_total_size(payload).unwrap();
                    println!(
                        "virtual-field shape={shape} payload={payload} pair={pair} enabled={enabled} calls={calls} ns={ns} gc_ns={gc_ns} allocation_bytes={allocation_bytes} physical_saved={physical_saved} physical_saved_bytes={physical_saved_bytes} journal_peak_bytes={journal_peak_bytes} metadata_bytes={metadata_bytes} hits={hits} materialized={materialized} cold_ns={cold_ns}"
                    );
                }
            }
        }
    }
}
