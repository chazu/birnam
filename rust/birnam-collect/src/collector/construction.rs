//! Activation and ordinary construction above the non-collecting Id primitives.
use super::*;
use birnam_abi::{bootstrap, root_registry};

const DISABLED: u32 = 0;
const ADOPTING: u32 = 1;
const ACTIVE: u32 = 2;
const FAILED: u32 = 3;

pub(super) struct Construction {
    state: Cell<u32>,
    roots: Cell<Option<root_registry::OwnedV1>>,
    id: Protocol,
    add: Oop,
    object_behavior: Oop,
    allocate_selector: Oop,
    delegated_selector: Oop,
    clone_selector: Oop,
    new_selector: Oop,
    allocate: NativeEntry,
    delegated: NativeEntry,
    intern: NativeEntry,
    size: NativeEntry,
    behavior: NativeEntry,
}
impl Construction {
    pub(super) fn new(
        id: Protocol,
        intern: &impl Fn(&[u8]) -> Result<Oop, Outcome>,
        select: &impl Fn(&[u8]) -> Result<NativeEntry, Outcome>,
    ) -> Result<Self, Outcome> {
        let prototype = select(b"objectPrototype")?;
        let behavior = select(b"behaviorOf:")?;
        let invoke = |entry: NativeEntry, args: &[Operand]| {
            loaded(unsafe {
                entry.method.unwrap()(entry.context, id.receiver, args.as_ptr(), args.len())
            })
            .map_err(|_| invalid())
        };
        let object = invoke(prototype, &[])?;
        let object_behavior = invoke(behavior, &[object])?
            .as_object()
            .ok_or_else(invalid)?;
        Ok(Self {
            state: Cell::new(DISABLED),
            roots: Cell::new(None),
            id,
            add: intern(b"addMethod")?,
            object_behavior,
            allocate_selector: intern(b"allocate")?,
            delegated_selector: intern(b"delegated")?,
            clone_selector: intern(b"clone")?,
            new_selector: intern(b"new")?,
            allocate: select(b"allocate")?,
            delegated: select(b"delegated")?,
            intern: select(b"intern:")?,
            size: select(b"allocationSizeOf:")?,
            behavior,
        })
    }
}

pub(super) unsafe extern "C" fn active(context: *const c_void) -> i32 {
    if !crate::aligned(context.cast::<Collector>()) {
        return 0;
    }
    let collector = unsafe { &*context.cast::<Collector>() };
    collector.check();
    i32::from(collector.construction.state.get() == ACTIVE)
}

pub(super) unsafe extern "C" fn activate(
    context: *mut c_void,
    bootstrap: *const bootstrap::OwnedV2,
    roots: *const root_registry::OwnedV1,
) -> Status {
    if !crate::aligned(context.cast::<Collector>())
        || !unsafe { bootstrap::valid(bootstrap) && root_registry::valid_owned(roots) }
    {
        return INVALID;
    }
    let collector = unsafe { &*context.cast::<Collector>() };
    collector.check();
    if collector.construction.state.get() != DISABLED
        || unsafe { (*collector.state.phase_factory).read.unwrap()(collector.state.phase_context) }
            != coordination::IDLE
    {
        return BUSY;
    }
    let bootstrap = unsafe { *bootstrap };
    // Idempotent only for the already adopted same registry; a different owner
    // is rejected before activation or any constructor replacement.
    if unsafe { (bootstrap.adopt_roots)(bootstrap.context, roots) } != native::LOADED {
        return INVALID;
    }
    collector.construction.roots.set(Some(unsafe { *roots }));
    collector.construction.state.set(ADOPTING);
    let answer =
        unsafe { (bootstrap.pause)(bootstrap.context, NativeEntry::new(install, context)) };
    if answer.status != native::LOADED {
        collector.construction.state.set(FAILED);
        return if answer.status == native::INITIALIZER_FAILED {
            answer.detail as Status
        } else {
            INVALID
        };
    }
    OK
}

unsafe extern "C" fn install(context: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    let collector = unsafe { &*context.cast::<Collector>() };
    let construction = &collector.construction;
    // Validation has no marking, hooks, finalizers or application callbacks.
    let status = unsafe { (collector.graph.validate)(collector.graph.context) };
    if status != OK {
        return Outcome::failed(native::INITIALIZER_FAILED, status as usize);
    }
    let status = collector.reconcile();
    if status != OK {
        return Outcome::failed(native::INITIALIZER_FAILED, status as usize);
    }
    let stack = unsafe { *collector.state.stack_factory };
    let bottom = unsafe { stack.thread_bottom.unwrap()() };
    if bottom == 0 {
        return invalid();
    }
    unsafe { stack.set.unwrap()(collector.state.stack, bottom) };
    let id = construction.id;
    for (receiver, selector, method) in [
        (
            id.receiver,
            construction.allocate_selector,
            allocate as native::Method,
        ),
        (
            id.receiver,
            construction.delegated_selector,
            delegated as native::Method,
        ),
        (id.receiver, id.intern, intern as native::Method),
        (
            construction.object_behavior,
            construction.clone_selector,
            clone_object as native::Method,
        ),
        (
            construction.object_behavior,
            construction.new_selector,
            clone_object as native::Method,
        ),
    ] {
        let args = [
            Operand::object(selector),
            Operand::entry(NativeEntry::new(method, context)),
        ];
        let answer = unsafe {
            id.message.unwrap()(
                id.context,
                receiver,
                construction.add,
                args.as_ptr(),
                args.len(),
            )
        };
        if answer.status != native::LOADED {
            return answer;
        }
    }
    // All installed entries remain non-collecting until this final publication.
    construction.state.set(ACTIVE);
    Outcome::loaded(Operand::unit())
}

struct Root {
    registry: root_registry::OwnedV1,
    token: usize,
}
impl Drop for Root {
    fn drop(&mut self) {
        assert_eq!(
            unsafe { self.registry.release.unwrap()(self.registry.context, self.token) },
            1
        );
    }
}
impl Collector {
    fn account_constructed(&self, object: Oop) -> Result<bool, Status> {
        let records = unsafe { *self.state.records };
        if unsafe { (records.get)(records.context, object.addr()) } != MISSING {
            return Ok(false);
        }
        let size = self
            .invoke(self.construction.size, &[Operand::object(object)])?
            .as_word()
            .ok_or(INVALID)?;
        let total = size
            .checked_add(size_of::<Oop>())
            .and_then(|size| size.checked_add(7))
            .map(|size| size & !7)
            .ok_or(INVALID)?;
        if unsafe { (records.insert)(records.context, object, 0) } != 1
            || unsafe {
                (*self.state.budget_factory).charge.unwrap()(self.state.budget_context, total)
            } != 1
        {
            return Err(INVALID);
        }
        Ok(true)
    }
    fn after_construction(&self, answer: Outcome, collect_result: bool) -> Outcome {
        if answer.status != native::LOADED {
            return answer;
        }
        let Some(object) = answer.value.as_object() else {
            return invalid();
        };
        let new = self
            .account_constructed(object)
            .expect("constructed object accounting failed");
        // Intern creates a complete Symbol before returning; ordinary instance
        // allocation leaves payload initialization to its caller.
        if new && collect_result {
            self.collect_if_due(object);
        }
        answer
    }
    fn collect_if_due(&self, object: Oop) {
        if self.construction.state.get() != ACTIVE {
            return;
        }
        let state = self.state;
        let bottom = unsafe { (*state.stack_factory).get.unwrap()(state.stack) };
        let due = unsafe {
            (*state.settings_factory).automatic_due.unwrap()(
                state.settings,
                i32::from(bottom != 0),
                (*state.budget_factory).read.unwrap()(state.budget_context),
            )
        };
        if due == 0 {
            return;
        }
        let registry = self
            .construction
            .roots
            .get()
            .expect("active construction has roots");
        let token = unsafe { registry.retain.unwrap()(registry.context, object) };
        assert_ne!(token, 0);
        let _root = Root { registry, token };
        unsafe extern "C" fn captured(context: *mut c_void, start: usize, bottom: usize) {
            let collector = unsafe { &*context.cast::<Collector>() };
            let input = InputV1 {
                start,
                bottom,
                context: ptr::null_mut(),
                roots: None,
                retirement: ptr::null(),
            };
            let mut count = 0;
            assert_eq!(
                unsafe { (collector.api.collect)(context, &input, &mut count) },
                OK,
                "automatic collection failed"
            );
        }
        assert_eq!(
            unsafe {
                (*state.stack_factory).capture.unwrap()(
                    bottom,
                    ptr::from_ref(self).cast_mut().cast(),
                    Some(captured),
                )
            },
            1
        );
    }
    fn construction_allowed(&self) -> bool {
        self.check();
        unsafe {
            (*self.state.phase_factory).read.unwrap()(self.state.phase_context)
                == coordination::IDLE
        }
    }
}
unsafe fn construct(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
    select: impl FnOnce(&Construction) -> NativeEntry,
    collect_result: bool,
) -> Outcome {
    let collector = unsafe { &*context.cast::<Collector>() };
    if !collector.construction_allowed() {
        return invalid();
    }
    if !collect_result {
        // Root only exact live inputs. The primitive remains responsible for
        // validating its own argument shape and behavior kind.
        if collector
            .invoke(collector.construction.size, &[Operand::object(receiver)])
            .ok()
            .and_then(|value| value.as_word())
            .is_none()
        {
            return invalid();
        }
        collector.collect_if_due(receiver);
    }
    let entry = select(&collector.construction);
    let result = unsafe { entry.method.unwrap()(entry.context, receiver, args, count) };
    collector.after_construction(result, collect_result)
}
unsafe extern "C" fn allocate(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    unsafe { construct(context, receiver, args, count, |c| c.allocate, false) }
}
unsafe extern "C" fn delegated(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    unsafe { construct(context, receiver, args, count, |c| c.delegated, false) }
}
unsafe extern "C" fn intern(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    unsafe { construct(context, receiver, args, count, |c| c.intern, true) }
}
unsafe extern "C" fn clone_object(
    context: *mut c_void,
    receiver: Oop,
    _: *const Operand,
    count: usize,
) -> Outcome {
    let collector = unsafe { &*context.cast::<Collector>() };
    if count != 0 || !collector.construction_allowed() {
        return invalid();
    }
    let arg = [Operand::object(receiver)];
    let Ok(size) = collector.invoke(collector.construction.size, &arg) else {
        return invalid();
    };
    let Some(size) = size
        .as_word()
        .and_then(|size| size.checked_add(7))
        .map(|size| size & !7)
    else {
        return invalid();
    };
    let Ok(behavior) = collector.invoke(collector.construction.behavior, &arg) else {
        return invalid();
    };
    let Some(behavior) = behavior.as_object() else {
        return invalid();
    };
    collector.collect_if_due(receiver);
    let entry = collector.construction.allocate;
    let answer = unsafe { entry.method.unwrap()(entry.context, behavior, &Operand::word(size), 1) };
    let Some(object) = answer
        .value
        .as_object()
        .filter(|_| answer.status == native::LOADED)
    else {
        return answer;
    };
    // No callback occurs between primitive allocation and completion of the
    // padded payload copy. Finalizer ownership is deliberately not copied.
    if size != 0 {
        unsafe { ptr::copy_nonoverlapping(receiver.cast::<u8>(), object.cast::<u8>(), size) };
    }
    collector.after_construction(answer, false)
}
