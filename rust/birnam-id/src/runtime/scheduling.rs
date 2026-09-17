//! Bounded owner drives. Scheduling and cancellation decisions remain in Birnam.
use super::*;

impl Runtime {
    pub(crate) fn drive_stackless_budget(
        &self,
        machine: Oop,
        descriptor: std::sync::Arc<FrameDescriptor>,
        initial_oops: &[(usize, Oop)],
        budget: usize,
    ) -> Result<Oop, ExecutionError> {
        self.drive_stackless_with_resolver_budget(
            machine,
            descriptor,
            initial_oops,
            None,
            Some(budget),
            |runtime, receiver, selector, _| runtime.resolve_stackless_target(receiver, selector),
        )
    }

    pub(crate) fn continue_stackless_budget(
        &self,
        machine: Oop,
        budget: usize,
    ) -> Result<Oop, ExecutionError> {
        self.assert_owner();
        let capability = self.stackless_machine_capability(machine)?;
        unsafe {
            (*self.state.get())
                .execution
                .machine_mut(current_thread_token(), capability)?
                .resume_runnable(current_thread_token())?;
        }
        self.run_resumed_stackless_budget(machine, capability, budget)
    }

    pub(crate) fn resume_stackless_budget(
        &self,
        machine: Oop,
        value: Oop,
        budget: usize,
    ) -> Result<Oop, ExecutionError> {
        self.assert_owner();
        let capability = self.stackless_machine_capability(machine)?;
        unsafe {
            (*self.state.get()).execution.resume_suspended_machine(
                current_thread_token(),
                capability,
                value,
                self.canonical().nil,
            )?;
        }
        self.run_resumed_stackless_budget(machine, capability, budget)
    }

    fn run_resumed_stackless_budget(
        &self,
        machine: Oop,
        capability: MachineCapability,
        budget: usize,
    ) -> Result<Oop, ExecutionError> {
        self.push_stackless_scoped_root(machine);
        let _root_guard = StacklessDriveRootGuard {
            runtime: self,
            machine,
        };
        let owner = current_thread_token();
        let result = self.run_stackless_loop_budget(
            machine,
            capability,
            owner,
            self.canonical().nil,
            Some(budget),
            &mut |runtime, receiver, selector, _| {
                runtime.resolve_stackless_target(receiver, selector)
            },
        );
        self.fail_stackless_drive_on_error(owner, capability, result)
    }

    pub(crate) fn exit_stackless_runnable_boundary(
        &self,
        machine: Oop,
        value: Oop,
    ) -> Result<(), ExecutionError> {
        self.assert_owner();
        let capability = self.stackless_machine_capability(machine)?;
        unsafe {
            (*self.state.get()).execution.exit_runnable_boundary(
                current_thread_token(),
                capability,
                value,
                self.canonical().nil,
            )
        }
    }
}
