//! Design 74O feasibility probes. Experimental mechanisms are test-only.

use super::*;
use crate::execution::{FrameHandle, InlineRecovery, RegistrationGenerationLease};
use birnam_artifact::{
    code_binding_inventory, materialization_selection as selection, portable_inventory,
};
use birnam_codegen::{
    Block, Expr, Statement, portable, resumable, resumable_c, resumable_descriptor,
};
use birnam_native_compiler::Unit;
use std::borrow::Cow;
use std::cell::RefCell;
use std::ffi::{CStr, CString, c_char, c_void};
use std::rc::Rc;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Case {
    Plain,
    Gc,
    ReplaceActive,
    MachineOverride,
    LookupOverride,
    NativeObserver,
    UnsafeNativeObserver,
    UnreifiedFrame,
    BadMap,
    NestedEntry,
    NativeBarrier,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Observations {
    effects: Vec<&'static str>,
    frames: Vec<(u64, usize, usize)>,
    overrides: usize,
    lookups: usize,
}

pub(super) struct Program {
    _unit: Unit,
    pub(super) step: Method,
    inline: unsafe extern "C" fn(Oop, *const Oop, *mut Oop),
    descriptor: Vec<u8>,
    inventory: Vec<u8>,
    bindings: Vec<u8>,
    selected: Vec<u8>,
    pub(super) layout: resumable::PhysicalLayout,
    live: Vec<resumable::ValueId>,
    value_count: usize,
    aliases: [usize; 2],
}

struct Trial {
    caller: Rc<Program>,
    callee: Rc<Program>,
    caller_descriptor: Arc<FrameDescriptor>,
    callee_descriptor: Arc<FrameDescriptor>,
    original_frame: Option<FrameHandle>,
    case: Case,
    optimized: bool,
    recovered: usize,
    rejected: usize,
    before_depth: usize,
    after_depth: usize,
    observations: Observations,
    trusted_leaf: Option<MethodAddress>,
}

thread_local! {
    static TRIAL: RefCell<Option<Trial>> = const { RefCell::new(None) };
    static MACHINE: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
    static NESTED: Cell<bool> = const { Cell::new(false) };
    static GC_VALUE: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
    static FINALIZED: Cell<usize> = const { Cell::new(0) };
}

unsafe extern "C" fn allocation_observed(object: Oop) {
    FINALIZED.set(FINALIZED.get() + 1);
    with_active(|runtime| runtime.release_string(object));
}

#[test]
fn architectural_fit_object_materialization_has_identity_and_effect_limits() {
    let bootstrap = crate::dispatch::build_bootstrap();
    let runtime = &bootstrap.runtime;
    runtime.enter(|| {
        let identical = runtime.intern(b"==").unwrap();
        let original = runtime.make_integer(74);
        let _original_root = ScopedOopRootGuard::new(runtime, original);
        let escaped_alias = original;
        let replacement = runtime.make_integer(74);
        let _replacement_root = ScopedOopRootGuard::new(runtime, replacement);
        assert_eq!(runtime.integer_value(original), runtime.integer_value(replacement));
        assert_eq!(runtime.send_slice(original, identical, &[escaped_alias]), runtime.canonical().true_object);
        assert_eq!(runtime.send_slice(replacement, identical, &[escaped_alias]), runtime.canonical().false_object);

        // A not-yet-observed virtual identity can instead materialize once and
        // supply that same Oop to every alias. Two fresh allocations cannot.
        let materialized_aliases = [replacement, replacement];
        assert_eq!(runtime.send_slice(materialized_aliases[0], identical, &[materialized_aliases[1]]), runtime.canonical().true_object);
        let separately_materialized = runtime.make_integer(74);
        assert_eq!(runtime.send_slice(replacement, identical, &[separately_materialized]), runtime.canonical().false_object);

        // Explicit collection and a legal native finalizer make an otherwise
        // unused allocation observable. Late materialization cannot repair the
        // observation at the earlier collection checkpoint.
        FINALIZED.set(0);
        runtime.make_string_with_finalizer(b"effectful allocation", allocation_observed);
        runtime.gc_collect_from_native_range(0, 0);
        let baseline_checkpoint = FINALIZED.get();
        assert_eq!(baseline_checkpoint, 1);
        FINALIZED.set(0);
        runtime.gc_collect_from_native_range(0, 0);
        let eliminated_checkpoint = FINALIZED.get();
        assert_eq!(eliminated_checkpoint, 0);
        runtime.make_string_with_finalizer(b"effectful allocation", allocation_observed);
        runtime.gc_collect_from_native_range(0, 0);
        assert_eq!(FINALIZED.get(), baseline_checkpoint);
        assert_ne!(eliminated_checkpoint, baseline_checkpoint);

        // Conservative eligibility: retaining the allocation before the
        // observation reproduces the checkpoint, including finalizer effects.
        FINALIZED.set(0);
        runtime.make_string_with_finalizer(b"effectful allocation", allocation_observed);
        runtime.gc_collect_from_native_range(0, 0);
        assert_eq!(FINALIZED.get(), baseline_checkpoint);
        println!("ARCHITECTURAL_FIT_OBJECT {{\"escaped_replacement_identical\":false,\"single_materialization_aliases\":true,\"separate_materializations_alias\":false,\"baseline_finalizers_at_checkpoint\":1,\"late_finalizers_at_checkpoint\":0,\"retained_finalizers_at_checkpoint\":1}}");
    });
}

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

fn source(caller: bool) -> Vec<u8> {
    let body = if caller {
        vec![
            Statement::Let {
                name: b"answer",
                value: send(
                    Expr::Global(b"probe"),
                    b"fitCallee:input:",
                    vec![Expr::Global(b"probe"), Expr::Global(b"input")],
                ),
            },
            Statement::Expr(send(
                Expr::Global(b"probe"),
                b"fitAfter:",
                vec![Expr::Global(b"answer")],
            )),
        ]
    } else {
        vec![
            Statement::Let {
                name: b"carried",
                value: send(Expr::Global(b"input"), b"fitIdentity", vec![]),
            },
            Statement::Let {
                name: b"alias",
                value: Expr::Global(b"carried"),
            },
            Statement::Expr(send(
                Expr::Global(b"probe"),
                b"fitObserve:alias:",
                vec![Expr::Global(b"carried"), Expr::Global(b"alias")],
            )),
            Statement::Expr(send(
                Expr::Global(b"probe"),
                b"fitFinish:alias:",
                vec![Expr::Global(b"carried"), Expr::Global(b"alias")],
            )),
        ]
    };
    portable::encode_block(
        &Block {
            params: vec![b"probe", b"input"],
            body,
        },
        &[],
    )
    .unwrap()
}

impl Program {
    pub(super) fn compile(caller: bool) -> Rc<Self> {
        let bytes = source(caller);
        let inventory = portable_inventory::encode_closure(&[&bytes]).unwrap();
        let bindings = code_binding_inventory::encode(&[]).unwrap();
        let selected = selection::encode(
            &inventory,
            &bindings,
            &[],
            &[selection::Disposition::Resumable],
        )
        .unwrap();
        let view = selection::inspect(&selected, &inventory, &bindings).unwrap();
        let logical = resumable::lower_portable_block(&bytes).unwrap();
        let layout = resumable::color_slots(&logical).unwrap();
        let descriptor = resumable_descriptor::compile_portable_descriptor_v2(
            &logical,
            &layout,
            view.row(0).unwrap(),
            1,
        )
        .unwrap();
        let live = resumable::reference_liveness(&logical)[1].live_in.clone();
        let mut aliases = [0; 2];
        if !caller {
            for (index, name) in [b"carried".as_slice(), b"alias"].iter().enumerate() {
                let op = logical.blocks.iter().flat_map(|b| &b.operations).find(|op| matches!(op.kind, resumable::OperationKind::Bind { name: actual, .. } if actual == *name)).unwrap();
                aliases[index] = layout.value_slots[op.result.0 as usize] as usize;
            }
        }
        let mut c = include_bytes!("native_tier/shims.c").to_vec();
        c.extend_from_slice(b"extern long birnam_smallIntValue(void*);\n");
        c.extend(resumable_c::emit_step_method(0, &logical, &layout, 1).unwrap());
        c.extend_from_slice(b"void* get_step(void){return (void*)__birnam_resumable_step_0;}\nvoid inline_prefix(void* receiver,void** args,void** values){\n");
        if !caller {
            // This bounded emitter admits only entry values, aliases, and one
            // explicitly guarded identity leaf. It emits real native locals;
            // the callee has no physical Frame while this function executes.
            for op in &logical.blocks[0].operations {
                let rhs = match op.kind {
                    resumable::OperationKind::EntryReceiver => "receiver".to_string(),
                    resumable::OperationKind::EntryArgument { index, .. } => {
                        format!("args[{index}]")
                    }
                    resumable::OperationKind::Bind { source, .. } => {
                        format!("values[{}]", source.0)
                    }
                    _ => panic!("bounded prefix cannot inline {:?}", op.kind),
                };
                c.extend(format!("values[{}]={rhs};\n", op.result.0).bytes());
            }
            let resumable::Terminator::Call {
                receiver,
                selector,
                arguments,
                result,
                successor,
                ..
            } = &logical.blocks[0].terminator
            else {
                panic!("expected identity call")
            };
            assert_eq!(*selector, b"fitIdentity");
            assert!(arguments.is_empty());
            assert_eq!(successor.0, 1);
            c.extend(format!("values[{}]=values[{}];\n", result.0, receiver.0).bytes());
        }
        c.extend_from_slice(b"}\n");
        if let Ok(directory) = std::env::var("BIRNAM_74O_DUMP") {
            let directory = std::path::Path::new(&directory);
            std::fs::create_dir_all(directory).unwrap();
            let name = if caller { "caller" } else { "callee" };
            std::fs::write(directory.join(format!("{name}.portable")), &bytes).unwrap();
            std::fs::write(directory.join(format!("{name}.descriptor")), &descriptor).unwrap();
            std::fs::write(directory.join(format!("{name}.c")), &c).unwrap();
            let mut facts = format!("{} {}\n", layout.slot_count, logical.value_count);
            for value in &live {
                facts.push_str(&format!(
                    "{} {}\n",
                    layout.value_slots[value.0 as usize], value.0
                ));
            }
            std::fs::write(directory.join(format!("{name}.facts")), facts).unwrap();
        }
        let unit = unsafe {
            Unit::compile(
                &CString::new(c).unwrap(),
                &[
                    (c"birnam_send", shim_send as *const c_void),
                    (c"birnam_intern", shim_intern as *const c_void),
                    (c"birnam_smallIntValue", shim_value as *const c_void),
                    (c"birnam_mkSmallInt", shim_integer as *const c_void),
                    (c"birnam_nil", shim_nil as *const c_void),
                ],
            )
        }
        .unwrap();
        let getter: unsafe extern "C" fn() -> Method =
            unsafe { core::mem::transmute(unit.symbol(c"get_step").unwrap().as_ptr()) };
        let inline = unsafe {
            core::mem::transmute::<*mut c_void, unsafe extern "C" fn(Oop, *const Oop, *mut Oop)>(
                unit.symbol(c"inline_prefix").unwrap().as_ptr(),
            )
        };
        Rc::new(Self {
            step: unsafe { getter() },
            inline,
            _unit: unit,
            descriptor,
            inventory,
            bindings,
            selected,
            layout,
            live,
            value_count: logical.value_count as usize,
            aliases,
        })
    }

    pub(super) fn descriptor(&self, generation: u64, step: Method) -> Arc<FrameDescriptor> {
        let view = selection::inspect(&self.selected, &self.inventory, &self.bindings).unwrap();
        FrameDescriptor::admit_portable_v2(&self.descriptor, view.row(0).unwrap(), generation, step)
            .unwrap()
    }
}

unsafe extern "C" fn shim_send(receiver: Oop, selector: Oop, args: *mut Oop, count: i64) -> Oop {
    with_active(|r| unsafe { r.send_abi(receiver, selector, args, count as usize) })
}
unsafe extern "C" fn shim_intern(name: *const c_char) -> Oop {
    with_active(|r| {
        r.intern(unsafe { CStr::from_ptr(name) }.to_bytes())
            .unwrap()
    })
}
unsafe extern "C" fn shim_value(value: Oop) -> i64 {
    with_active(|r| r.integer_value(value))
}
unsafe extern "C" fn shim_integer(value: i64) -> Oop {
    with_active(|r| r.make_integer(value))
}
unsafe extern "C" fn shim_nil() -> Oop {
    with_active(|r| r.canonical().nil)
}
unsafe extern "C" fn caller_thunk(_: Oop, _: *const Oop, _: usize) -> Oop {
    panic!("registered carrier must be classified")
}
unsafe extern "C" fn callee_thunk(_: Oop, _: *const Oop, _: usize) -> Oop {
    panic!("registered carrier must be classified")
}
unsafe extern "C" fn identity(receiver: Oop, _: *const Oop, _: usize) -> Oop {
    receiver
}

fn snapshot(runtime: &Runtime, effect: &'static str) {
    let capability = runtime.stackless_machine_capability(MACHINE.get()).unwrap();
    let (identity, pc, depth) = unsafe {
        let arena = (*runtime.state.get())
            .execution
            .machine(current_thread_token(), capability)
            .unwrap()
            .arena();
        let frame = arena.current_handle().unwrap();
        (
            arena.descriptor(frame).unwrap().identity().generation,
            arena.program_counter(frame).unwrap(),
            arena.diagnostics().live_frames,
        )
    };
    let view = runtime.send_slice(
        MACHINE.get(),
        runtime.intern(b"currentFrameView").unwrap(),
        &[],
    );
    let _view_root = ScopedOopRootGuard::new(runtime, view);
    let generation = runtime.send_slice(view, runtime.intern(b"generationIdentity").unwrap(), &[]);
    assert_eq!(runtime.word_value(generation), identity);
    let current_pc = runtime.send_slice(view, runtime.intern(b"programCounter").unwrap(), &[]);
    assert_eq!(runtime.integer_value(current_pc), pc as i64);
    let caller_view = runtime.import(b"FitOriginalCallerView").unwrap();
    let caller_generation = runtime.send_slice(
        caller_view,
        runtime.intern(b"generationIdentity").unwrap(),
        &[],
    );
    assert_eq!(runtime.word_value(caller_generation), 74001);
    if effect == "observe" && !NESTED.get() {
        runtime.export(b"FitRecoveredCalleeView", view).unwrap();
    }
    TRIAL.with_borrow_mut(|t| {
        let t = t.as_mut().unwrap();
        t.observations.effects.push(effect);
        t.observations.frames.push((identity, pc, depth));
    });
}

unsafe extern "C" fn observing_identity(receiver: Oop, _: *const Oop, _: usize) -> Oop {
    with_active(|r| snapshot(r, "identity-observation"));
    receiver
}
unsafe extern "C" fn read_override(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    TRIAL.with_borrow_mut(|t| t.as_mut().unwrap().observations.overrides += 1);
    unsafe { crate::execution::machine_oop_at(receiver, args, count) }
}
unsafe extern "C" fn lookup_override(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    TRIAL.with_borrow_mut(|t| t.as_mut().unwrap().observations.lookups += 1);
    unsafe { crate::dispatch::vtable_lookup_method(receiver, args, count) }
}
unsafe extern "C" fn replacement(_: Oop, _: *const Oop, _: usize) -> Oop {
    with_active(|r| r.make_integer(99))
}

unsafe extern "C" fn gc_mutate_recovered(mark: crate::abi::MarkFn) {
    let value = GC_VALUE.get();
    if value.is_null() {
        return;
    }
    unsafe { mark(value) };
    with_active(|runtime| {
        let slots = TRIAL.with_borrow(|t| t.as_ref().unwrap().callee.aliases);
        for slot in slots {
            runtime
                .stackless_oop_at_put(MACHINE.get(), slot, value)
                .unwrap();
        }
    });
}

unsafe extern "C" fn observe(_: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 2);
    assert_eq!(unsafe { args.read() }, unsafe { args.add(1).read() });
    with_active(|r| {
        snapshot(r, "observe");
        let case = TRIAL.with_borrow(|t| t.as_ref().unwrap().case);
        match case {
            Case::Gc => {
                let value = r.make_integer(22);
                let _root = ScopedOopRootGuard::new(r, value);
                let garbage = r.make_integer(909);
                GC_VALUE.set(value);
                r.gc_register_root_provider(gc_mutate_recovered);
                r.gc_collect_from_native_range(0, 0);
                GC_VALUE.set(core::ptr::null_mut());
                assert!(r.checked_vtable_of(garbage).is_null());
            }
            Case::NestedEntry | Case::NativeBarrier if !NESTED.get() => {
                NESTED.set(true);
                let receiver = r.canonical().object_prototype;
                let input = r.make_integer(3);
                let nested = r.send_slice(
                    receiver,
                    r.intern(b"fitCallee:input:").unwrap(),
                    &[receiver, input],
                );
                assert_eq!(r.integer_value(nested), 6);
                NESTED.set(false);
            }
            Case::NativeBarrier => {
                // The entry barrier belongs to the nested stackless drive,
                // not merely to the synchronous native callback around it.
                assert!(NESTED.get());
                assert_eq!(
                    r.stackless_capture_current(MACHINE.get(), 0, 2),
                    Err(ExecutionError::Machine(MachineError::ContinuationBarrier))
                );
                assert_eq!(
                    r.stackless_suspend_current(MACHINE.get(), 0, 2),
                    Err(ExecutionError::Machine(MachineError::ContinuationBarrier))
                );
            }
            Case::ReplaceActive => {
                r.add_method(
                    r.globals().object_vt,
                    r.intern(b"fitCallee:input:").unwrap(),
                    replacement,
                );
                assert!(r.stackless_registration_diagnostics().active_frame_pins >= 2);
                assert!(
                    !unsafe { (*r.state.get()).execution.unregister_code_generation(74002) }
                        .unwrap()
                );
            }
            Case::MachineOverride => {
                let objects = unsafe { (*r.state.get()).execution.objects().unwrap() };
                r.add_method(objects.machine_vtable, objects.oop_at, read_override);
            }
            Case::LookupOverride => {
                r.add_method(r.globals().vtable_vt, r.globals().lookup, lookup_override);
            }
            _ => {}
        }
        r.canonical().nil
    })
}
unsafe extern "C" fn finish(_: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 2);
    assert_eq!(unsafe { args.read() }, unsafe { args.add(1).read() });
    with_active(|r| {
        snapshot(r, "finish");
        r.make_integer(2 * r.integer_value(unsafe { args.read() }))
    })
}
unsafe extern "C" fn after(_: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 1);
    with_active(|r| snapshot(r, "after"));
    unsafe { args.read() }
}

unsafe extern "C" fn caller_step(machine: Oop, args: *const Oop, count: usize) -> Oop {
    MACHINE.set(machine);
    with_active(|runtime| {
        let (caller, enabled, case) = TRIAL.with_borrow(|t| {
            let t = t.as_ref().unwrap();
            (t.caller.clone(), t.optimized, t.case)
        });
        let pc = runtime.integer_value(unsafe { args.read() });
        let capability = runtime.stackless_machine_capability(machine).unwrap();
        let frame = unsafe {
            (*runtime.state.get())
                .execution
                .machine(current_thread_token(), capability)
                .unwrap()
                .arena()
                .current_handle()
                .unwrap()
        };
        TRIAL.with_borrow_mut(|t| {
            let t = t.as_mut().unwrap();
            if let Some(original) = t.original_frame {
                assert_eq!(frame, original);
            } else {
                t.original_frame = Some(frame);
            }
        });
        if pc == 0 {
            let view =
                runtime.send_slice(machine, runtime.intern(b"currentFrameView").unwrap(), &[]);
            runtime.export(b"FitOriginalCallerView", view).unwrap();
        }
        let action = unsafe { (caller.step)(machine, args, count) };
        if !enabled || pc != 0 {
            return action;
        }
        let _read = runtime.enter_managed_turn();
        if !runtime.guarded_slot_access_eligible(machine) {
            return action;
        }
        let (operands, result_slot) = unsafe {
            let state = &*runtime.state.get();
            let DecodedAction::Call {
                receiver_slot,
                selector_slot,
                argument_start,
                argument_count,
                result_slot,
            } = state
                .execution
                .decode_action(&state.heap, current_thread_token(), capability, action)
                .unwrap()
            else {
                panic!("caller must publish Call")
            };
            (
                state
                    .execution
                    .copy_send_operands(
                        current_thread_token(),
                        capability,
                        receiver_slot,
                        selector_slot,
                        argument_start,
                        argument_count,
                    )
                    .unwrap(),
                result_slot,
            )
        };
        let leaf = operands.arguments[1];
        let leaf_vtable = unsafe { Vtables::checked_vtable_of(&(*runtime.state.get()).heap, leaf) };
        if !runtime.guarded_default_lookup_eligible(leaf_vtable) {
            return action;
        }
        let identity_selector = runtime.intern(b"fitIdentity").unwrap();
        let actual = runtime.lookup_method(leaf_vtable, identity_selector);
        let trusted = TRIAL.with_borrow(|t| t.as_ref().unwrap().trusted_leaf);
        if !matches!(case, Case::UnsafeNativeObserver | Case::UnreifiedFrame)
            && (trusted.is_none() || actual != trusted)
        {
            return action;
        }
        if unsafe {
            (*runtime.state.get())
                .execution
                .has_resumable_thunk(actual.unwrap())
        } {
            return action;
        }
        let StacklessTarget::Registered { entry } = runtime
            .resolve_stackless_target(operands.receiver, operands.selector)
            .unwrap()
        else {
            return action;
        };
        let target = runtime
            .materialize_registered_target(entry, operands.receiver, &operands.arguments)
            .unwrap();
        let (callee, caller_descriptor, expected) = TRIAL.with_borrow(|t| {
            let t = t.as_ref().unwrap();
            (
                t.callee.clone(),
                t.caller_descriptor.clone(),
                t.callee_descriptor.clone(),
            )
        });
        assert!(Arc::ptr_eq(&target.descriptor, &expected));
        if case == Case::UnreifiedFrame {
            // Deliberately wrong placement: the effect occurs, but reflection
            // sees the physical caller because the logical callee is absent.
            unsafe {
                observing_identity(leaf, core::ptr::null(), 0);
            }
        }
        let mut spills = vec![runtime.canonical().nil; callee.value_count + 1];
        unsafe {
            (callee.inline)(
                operands.receiver,
                operands.arguments.as_ptr(),
                spills.as_mut_ptr(),
            )
        };
        let nil_spill = callee.value_count;
        let mut slots: Vec<_> = (0..callee.layout.slot_count as usize)
            .map(|slot| (slot, nil_spill))
            .collect();
        for value in &callee.live {
            slots[callee.layout.value_slots[value.0 as usize] as usize].1 = value.0 as usize;
        }
        if let Ok(path) = std::env::var("BIRNAM_74O_RECIPE") {
            let text = std::fs::read_to_string(path).unwrap();
            let produced: Vec<(usize, usize)> = text
                .lines()
                .map(|line| {
                    let fields: Vec<usize> = line
                        .split_whitespace()
                        .map(|field| field.parse().unwrap())
                        .collect();
                    assert_eq!(fields.len(), 2);
                    (fields[0], fields[1])
                })
                .collect();
            // The ordinary Id producer supplies the actual recipe consumed by
            // recovery. Independent compiler facts check its semantic mapping.
            assert_eq!(
                produced, slots,
                "Id-authored recipe differs from compiler liveness/layout"
            );
            slots = produced;
        }
        let before = unsafe {
            (*runtime.state.get())
                .execution
                .machine(current_thread_token(), capability)
                .unwrap()
                .arena()
                .diagnostics()
                .live_frames
        };
        assert_eq!(before, 1, "no physical callee Frame during native prefix");
        let make_plan = |slots| InlineRecovery {
            caller: frame,
            caller_descriptor: &caller_descriptor,
            caller_pc: 1,
            callee_descriptor: expected.clone(),
            callee_pc: 1,
            result_slot,
            slots,
            spills: &spills,
            entry: Some(entry),
        };
        if case == Case::BadMap {
            let arena = unsafe {
                (*runtime.state.get())
                    .execution
                    .machine_mut(current_thread_token(), capability)
                    .unwrap()
                    .arena_mut()
            };
            let prior = arena.recovery_experiment_snapshot();
            let duplicate_descriptor = caller.descriptor(74001, caller_step);
            for invalid in 0..7 {
                let mut bad = slots.clone();
                match invalid {
                    0 => bad.push(bad[0]),
                    1 => bad.clear(),
                    2 => bad[0].1 = spills.len(),
                    3 => bad[0].0 = callee.layout.slot_count as usize,
                    _ => (),
                }
                let mut plan = InlineRecovery {
                    slots: &bad,
                    ..make_plan(&slots)
                };
                match invalid {
                    4 => plan.callee_pc = usize::MAX,
                    5 => plan.result_slot = usize::MAX,
                    // Even a descriptor with the same public identity must
                    // not substitute for the live original allocation.
                    6 => plan.caller_descriptor = &duplicate_descriptor,
                    _ => (),
                }
                assert!(
                    arena
                        .recover_inline_child(plan, runtime.canonical().nil)
                        .is_err(),
                    "invalid recipe {invalid}"
                );
                assert_eq!(
                    arena.recovery_experiment_snapshot(),
                    prior,
                    "invalid recipe {invalid} changed live state"
                );
                TRIAL.with_borrow_mut(|t| t.as_mut().unwrap().rejected += 1);
            }
        }
        unsafe {
            let state = &mut *runtime.state.get();
            state
                .execution
                .transfer_acquired_entry_to_frame(entry)
                .unwrap();
            let result = state
                .execution
                .machine_mut(current_thread_token(), capability)
                .unwrap()
                .arena_mut()
                .recover_inline_child(make_plan(&slots), runtime.canonical().nil);
            if result.is_err() {
                state.execution.release_entry_frame_pin(entry).unwrap();
            }
            result.unwrap();
        }
        TRIAL.with_borrow_mut(|t| {
            let t = t.as_mut().unwrap();
            t.recovered += 1;
            t.before_depth = before;
            t.after_depth = 2;
        });
        // The baseline callee begins at its next real compiler PC. Its current
        // CallAction is returned to the unmodified driver in the same epoch.
        let token = runtime.make_integer(1);
        unsafe { (callee.step)(machine, &token, 1) }
    })
}

fn run(case: Case, optimized: bool) -> (i64, Observations) {
    NESTED.set(false);
    GC_VALUE.set(core::ptr::null_mut());
    let caller = Program::compile(true);
    let callee = Program::compile(false);
    let caller_descriptor = caller.descriptor(74001, caller_step);
    let callee_descriptor = callee.descriptor(74002, callee.step);
    let bootstrap = crate::dispatch::build_bootstrap();
    let runtime = &bootstrap.runtime;
    runtime.publish_canonical_exports();
    runtime.install_execution_objects();
    runtime.gc_set_precise(true);
    let caller_lease = RegistrationGenerationLease::new(74001);
    let callee_lease = RegistrationGenerationLease::new(74002);
    for (thunk, descriptor, program, lease) in [
        (
            caller_thunk as Method,
            caller_descriptor.clone(),
            &caller,
            &caller_lease,
        ),
        (
            callee_thunk as Method,
            callee_descriptor.clone(),
            &callee,
            &callee_lease,
        ),
    ] {
        runtime
            .register_stackless_resumable(
                SemanticRegistrationV1::new(
                    thunk,
                    descriptor,
                    2,
                    program.layout.entry_receiver_slot as usize,
                    program.layout.entry_argument_start as usize,
                )
                .with_generation_lease(lease.clone()),
            )
            .unwrap();
    }
    // Capture the exact installed pointer once: Rust may duplicate function
    // bodies across codegen units, so re-taking a function address is not an
    // identity contract in optimized builds.
    let observable_leaf = matches!(
        case,
        Case::NativeObserver | Case::UnsafeNativeObserver | Case::UnreifiedFrame
    );
    let leaf_method: Method = std::hint::black_box(if observable_leaf {
        observing_identity
    } else {
        identity
    });
    let trusted_leaf = (!observable_leaf).then(|| MethodAddress::from_method(leaf_method));
    for (name, method) in [
        (b"fitRun:input:".as_slice(), caller_thunk as Method),
        (b"fitCallee:input:", callee_thunk),
        (b"fitIdentity", leaf_method),
        (b"fitObserve:alias:", observe),
        (b"fitFinish:alias:", finish),
        (b"fitAfter:", after),
    ] {
        runtime.add_method(
            runtime.globals().object_vt,
            runtime.intern(name).unwrap(),
            method,
        );
    }
    TRIAL.set(Some(Trial {
        caller,
        callee,
        caller_descriptor,
        callee_descriptor,
        original_frame: None,
        case,
        optimized,
        recovered: 0,
        rejected: 0,
        before_depth: 0,
        after_depth: 0,
        observations: Observations::default(),
        trusted_leaf,
    }));
    let receiver = runtime.canonical().object_prototype;
    let input = runtime.make_integer(14);
    let _input_root = ScopedOopRootGuard::new(runtime, input);
    let answer = runtime.enter(|| {
        runtime.send_slice(
            receiver,
            runtime.intern(b"fitRun:input:").unwrap(),
            &[receiver, input],
        )
    });
    let answer_value = runtime.integer_value(answer);
    assert_eq!(answer_value, if case == Case::Gc { 44 } else { 28 });
    for name in [
        b"FitOriginalCallerView".as_slice(),
        b"FitRecoveredCalleeView",
    ] {
        let view = runtime.import(name).unwrap();
        assert_eq!(
            runtime.stackless_frame_view_identity_word(view, 0),
            Err(FrameViewError::StaleFrameView)
        );
    }
    if case == Case::ReplaceActive {
        let newer = runtime.enter(|| {
            runtime.send_slice(
                receiver,
                runtime.intern(b"fitCallee:input:").unwrap(),
                &[receiver, input],
            )
        });
        assert_eq!(runtime.integer_value(newer), 99);
    }
    for name in [b"fitRun:input:".as_slice(), b"fitCallee:input:"] {
        runtime
            .remove_method(runtime.globals().object_vt, runtime.intern(name).unwrap())
            .unwrap();
    }
    // This fixture uses private semantic registration, not a committed portable
    // unit. Final unregistration uses that matching execution-level boundary.
    for generation in [74001, 74002] {
        assert!(
            unsafe {
                (*runtime.state.get())
                    .execution
                    .unregister_code_generation(generation)
            }
            .unwrap()
        );
    }
    assert_eq!(
        runtime
            .stackless_registration_diagnostics()
            .registered_entries,
        0
    );
    assert_eq!(caller_lease.holder_count(), 1);
    assert_eq!(callee_lease.holder_count(), 1);
    assert_eq!(
        runtime
            .stackless_registration_diagnostics()
            .active_frame_pins,
        0
    );
    let trial = TRIAL.take().unwrap();
    assert_eq!(
        trial.recovered,
        usize::from(optimized && case != Case::NativeObserver)
    );
    let observed_frames: Vec<_> = trial
        .observations
        .frames
        .iter()
        .map(|(generation, pc, depth)| [*generation, *pc as u64, *depth as u64])
        .collect();
    println!(
        "ARCHITECTURAL_FIT {{\"case\":\"{case:?}\",\"optimized\":{optimized},\"answer\":{answer_value},\"reconstructions\":{},\"before_depth\":{},\"after_depth\":{},\"rejected_maps\":{},\"effects\":{:?},\"frames\":{:?},\"machine_override_calls\":{},\"lookup_override_calls\":{}}}",
        trial.recovered,
        trial.before_depth,
        trial.after_depth,
        trial.rejected,
        trial.observations.effects,
        observed_frames,
        trial.observations.overrides,
        trial.observations.lookups,
    );
    (answer_value, trial.observations)
}

#[test]
fn reconstructs_compiler_colored_callee_and_preserves_original_caller() {
    for case in [
        Case::Plain,
        Case::Gc,
        Case::ReplaceActive,
        Case::MachineOverride,
        Case::LookupOverride,
        Case::NativeObserver,
        Case::BadMap,
        Case::NestedEntry,
        Case::NativeBarrier,
    ] {
        assert_eq!(run(case, false), run(case, true), "{case:?}");
    }
}

#[test]
fn arbitrary_native_identity_is_not_a_pure_leaf_contract() {
    let baseline = run(Case::UnsafeNativeObserver, false);
    let unsound = run(Case::UnsafeNativeObserver, true);
    assert_eq!(baseline.0, unsound.0);
    assert_ne!(
        baseline.1, unsound.1,
        "equal numeric answers conceal a lost native effect and Frame observation"
    );
    assert_eq!(baseline.1.effects[0], "identity-observation");
    assert_eq!(unsound.1.effects[0], "observe");
}

#[test]
fn architectural_fit_observation_before_reification_exposes_wrong_frame() {
    let baseline = run(Case::UnreifiedFrame, false);
    let unreified = run(Case::UnreifiedFrame, true);
    assert_eq!(baseline.0, unreified.0);
    assert_eq!(baseline.1.effects, unreified.1.effects);
    assert_ne!(baseline.1.frames, unreified.1.frames);
    assert_eq!(baseline.1.frames[0].0, 74002);
    assert_eq!(unreified.1.frames[0].0, 74001);
}

#[test]
fn architectural_fit_v2_continuation_storage_and_publication_are_distinct() {
    let bootstrap = crate::dispatch::build_bootstrap();
    let r = &bootstrap.runtime;
    r.publish_canonical_exports();
    r.install_execution_objects();
    let caller = Program::compile(true);
    let callee = Program::compile(false);
    let caller_descriptor = caller.descriptor(74101, caller.step);
    let callee_descriptor = callee.descriptor(74102, callee.step);
    r.enter(|| {
        let owner = current_thread_token();
        let machine = r.create_stackless_machine().unwrap();
        let _machine_root = ScopedOopRootGuard::new(r, machine);
        let capability = r.stackless_machine_capability(machine).unwrap();
        let nil = r.canonical().nil;
        let value = r.make_integer(74);
        let slots: Vec<_> = (0..callee.layout.slot_count as usize).map(|slot| (slot, 0)).collect();
        let spills = [value];
        let original_caller = unsafe {
            let state = &mut *r.state.get();
            let machine = state.execution.machine_mut(owner, capability).unwrap();
            machine.begin_initial_drive(owner).unwrap();
            let arena = machine.arena_mut();
            let frame = arena.push(caller_descriptor.clone(), 0, nil, &[]).unwrap();
            arena.begin_v2_transition(frame, crate::execution::EdgeTransitionKind::Call, 1, Some(0)).unwrap();
            arena.recover_inline_child(InlineRecovery {
                caller: frame, caller_descriptor: &caller_descriptor, caller_pc: 1,
                callee_descriptor: callee_descriptor.clone(), callee_pc: 1,
                result_slot: caller.layout.edges[0].result_initialization_slot.unwrap() as usize,
                slots: &slots, spills: &spills, entry: None,
            }, nil).unwrap();
            frame
        };
        let before = unsafe { (*r.state.get()).execution.machine(owner, capability).unwrap().arena().recovery_experiment_logical_frames() };
        // The missing self edge remains invalid; a declared Call edge below
        // now supports normal V2 control publication (Design 60P).
        assert_eq!(r.stackless_capture_current(machine, 0, 1), Err(ExecutionError::Frame(crate::execution::FrameError::InvalidProgramCounter)));
        assert_eq!(r.stackless_suspend_current(machine, 0, 1), Err(ExecutionError::Frame(crate::execution::FrameError::InvalidProgramCounter)));
        assert_eq!(unsafe { (*r.state.get()).execution.machine(owner, capability).unwrap().arena().recovery_experiment_logical_frames() }, before);
        let result_slot = callee.layout.edges[1].result_initialization_slot.unwrap() as usize;
        r.stackless_capture_current(machine, result_slot, 2).unwrap();
        let objects = unsafe { (*r.state.get()).execution.objects().unwrap() };
        let continuation = r.allocate_instance(objects.continuation_vtable, 3 * size_of::<usize>());
        let _continuation_root = ScopedOopRootGuard::new(r, continuation);
        let captured = unsafe {
            let state = &mut *r.state.get();
            let delimiter = state.execution.current_delimiter(owner, capability).unwrap();
            state.execution.capture_continuation(&mut state.heap, owner, capability, delimiter, result_slot, continuation).unwrap()
        };
        r.set_finalizer(continuation, crate::execution::continuation_finalizer);
        assert_eq!(unsafe { (*r.state.get()).execution.machine(owner, capability).unwrap().arena().diagnostics().live_frames }, 0);
        r.gc_collect_from_native_range(0, 0);
        assert_eq!(r.integer_value(value), 74, "detached V2 roots retain the reconstructed value");
        unsafe {
            let state = &mut *r.state.get();
            state.execution.restore_continuation(owner, capability, captured, value, nil).unwrap();
            let arena = state.execution.machine(owner, capability).unwrap().arena();
            assert_eq!(arena.recovery_experiment_logical_frames()[0], before[0]);
            let frame = arena.current_handle().unwrap();
            assert_eq!(arena.program_counter(frame).unwrap(), 2);
            assert_eq!(arena.root_state_program_counter(frame).unwrap(), 2);
            let mut expected = vec![value; callee.layout.slot_count as usize];
            let edge = &callee.layout.edges[1];
            for copy in &edge.copies { expected[copy.destination as usize] = value; }
            expected[result_slot] = value;
            for (word, bits) in edge.clear_bitmap_words.iter().enumerate() {
                for bit in 0..64 { if bits & (1u64 << bit) != 0 { expected[word * 64 + bit] = nil; } }
            }
            for (slot, expected) in expected.iter().enumerate() { assert_eq!(arena.read_oop(frame, slot).unwrap(), *expected); }
            assert!(arena.descriptor(original_caller).is_err(), "capture stales old Frame handles by contract");
            assert_eq!(state.execution.restore_continuation(owner, capability, captured, value, nil), Err(ExecutionError::ContinuationConsumed));
            state.execution.machine_mut(owner, capability).unwrap().fail(owner).unwrap();
        }
        r.destroy_stackless_machine(machine).unwrap();
        println!("ARCHITECTURAL_FIT_CONTINUATION {{\"v2_capture_publication\":\"passed\",\"invalid_self_edge_rejected\":true,\"v2_suspend_publication\":\"covered_by_60P\",\"private_storage_roundtrip\":true,\"detached_roots_survive_gc\":true,\"pending_caller_edge_preserved\":true,\"old_handles_stale\":true,\"one_shot_restore\":true}}");
    });
}
