//! Compiler-owned optimization state, constructed by the later native entry.
use super::*;

pub(super) struct Compiler {
    pub(super) native_tier: core::cell::RefCell<native_tier::Registry>,
    pub(super) numeric: core::cell::RefCell<numeric::Registry>,
}
impl Compiler {
    fn new() -> Self {
        Self { native_tier: Default::default(), numeric: Default::default() }
    }
}

impl Runtime {
    pub(super) fn compiler(&self) -> &Compiler {
        self.compiler.get().expect("compiler extension was not initialized")
    }
    pub(crate) fn compiler_available(&self) -> bool { self.compiler.get().is_some() }
    pub(crate) fn initialize_compiler(&self) -> Option<()> {
        self.assert_owner();
        if self.integer.get().is_none() || self.float.get().is_none()
            || self.word.get().is_none() || self.string.get().is_none() { return None; }
        if !self.compiler_available() {
            unsafe { (*self.state.get()).admission = Some(CompilerAdmission::new()) };
            self.compiler.set(Compiler::new()).ok().expect("one compiler initialization");
        }
        Some(())
    }
    pub(crate) fn compiler_uses_id(&self, id: &birnam_abi::id_protocol::Protocol) -> bool {
        let own = unsafe { (*self.state.get()).id_protocol };
        own.is_some_and(|own| own.context == id.context && own.receiver == id.receiver
            && own.message.map(|f| f as usize) == id.message.map(|f| f as usize))
    }

}
