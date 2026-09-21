//! Scoped ordinary-Id recovery requests over the production admission engine.
use super::*;
use crate::execution::{RecoveryAuthority, ResumableEntryCapability};
mod graph;
pub(super) use graph::machine_recover_graph;

pub(super) struct Request {
    authority: RecoveryAuthority,
    consumed: bool,
}

struct RootBatch<'a>(&'a Runtime, Vec<Oop>);
impl Drop for RootBatch<'_> {
    fn drop(&mut self) {
        while let Some(value) = self.1.pop() {
            self.0.pop_stackless_scoped_root(value);
        }
    }
}

struct RequestGuard<'a>(&'a Runtime);
impl Drop for RequestGuard<'_> {
    fn drop(&mut self) {
        let request = self
            .0
            .recovery_requests
            .borrow_mut()
            .pop()
            .expect("scoped request");
        if !request.consumed {
            unsafe {
                (*self.0.state.get())
                    .execution
                    .release_acquired_entry(request.authority.entry)
                    .expect("request owns its acquisition");
            }
        }
    }
}

impl Runtime {
    /// Takes ownership of one acquired entry, including on rejection/unwind.
    pub(super) fn with_recovery_request<T>(
        &self,
        capability: MachineCapability,
        entry: ResumableEntryCapability,
        result_slot: usize,
        body: impl FnOnce() -> Result<T, ExecutionError>,
    ) -> Result<T, ExecutionError> {
        let authority = unsafe {
            (*self.state.get()).execution.recovery_authority(
                current_thread_token(),
                capability,
                entry,
                result_slot,
            )
        };
        let authority = match authority {
            Ok(value) => value,
            Err(error) => {
                unsafe {
                    (*self.state.get())
                        .execution
                        .release_acquired_entry(entry)?;
                }
                return Err(error);
            }
        };
        self.recovery_requests.borrow_mut().push(Request {
            authority,
            consumed: false,
        });
        let _guard = RequestGuard(self);
        body()
    }

    pub(super) fn recover_slots(
        &self,
        capability: MachineCapability,
        pc: usize,
        values: &[Oop],
    ) -> Result<(), ExecutionError> {
        let mut requests = self.recovery_requests.borrow_mut();
        let request = requests
            .last_mut()
            .filter(|r| !r.consumed && r.authority.machine == capability)
            .ok_or(ExecutionError::InvalidAction)?;
        if values.len() != request.authority.descriptor.slot_count() {
            return Err(crate::execution::FrameError::InvalidSlot.into());
        }
        let slots: Vec<_> = (0..values.len()).map(|index| (index, index)).collect();
        let nil = self.canonical().nil;
        unsafe {
            (*self.state.get()).execution.recover_acquired_call(
                current_thread_token(),
                &request.authority,
                pc,
                &slots,
                values,
                nil,
            )?;
        }
        request.consumed = true;
        Ok(())
    }

    fn recover_id_recipe(
        &self,
        machine: Oop,
        pc: usize,
        recipe: Oop,
    ) -> Result<(), ExecutionError> {
        let capability = self.stackless_machine_capability(machine)?;
        let (count, depth) = {
            let requests = self.recovery_requests.borrow();
            let request = requests
                .last()
                .filter(|r| !r.consumed && r.authority.machine == capability)
                .ok_or(ExecutionError::InvalidAction)?;
            if pc >= request.authority.descriptor.program_counter_count() {
                return Err(crate::execution::FrameError::InvalidProgramCounter.into());
            }
            (request.authority.descriptor.slot_count(), requests.len())
        };
        let _recipe_root = ScopedOopRootGuard::new(self, recipe);
        let size = self
            .intern(b"size")
            .map_err(|_| ExecutionError::InvalidAction)?;
        let _size_root = ScopedOopRootGuard::new(self, size);
        let at = self
            .intern(b"at:")
            .map_err(|_| ExecutionError::InvalidAction)?;
        let _at_root = ScopedOopRootGuard::new(self, at);
        let size = self.send_slice(recipe, size, &[]);
        if crate::execution::nonnegative_index(self, size) != Some(count) {
            return Err(crate::execution::FrameError::InvalidSlot.into());
        }
        let mut values = Vec::with_capacity(count);
        let mut roots = RootBatch(self, Vec::with_capacity(count));
        for index in 0..count {
            let boxed = self.make_integer(index as i64);
            let value = self.send_slice(recipe, at, &[boxed]);
            self.push_stackless_scoped_root(value);
            roots.1.push(value);
            values.push(value);
        }
        if self.recovery_requests.borrow().len() != depth {
            return Err(ExecutionError::InvalidAction);
        }
        self.recover_slots(capability, pc, &values)
    }
}

pub(super) unsafe extern "C" fn machine_recover(
    machine: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    with_active(|runtime| {
        if count != 2 || args.is_null() {
            return runtime.canonical().nil;
        }
        let Some(pc) = crate::execution::nonnegative_index(runtime, unsafe { args.read() }) else {
            return runtime.canonical().nil;
        };
        match runtime.recover_id_recipe(machine, pc, unsafe { args.add(1).read() }) {
            Ok(()) => machine,
            Err(_) => runtime.canonical().nil,
        }
    })
}
