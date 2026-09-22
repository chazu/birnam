/// Retained roots released when their guard leaves scope.
/// The owner adapter still needs migration to Id messages.
pub(crate) struct Retained {
    owner: birnam_abi::root_registry::OwnedV1,
    tokens: Vec<usize>,
}
impl Retained {
    pub(crate) fn new(owner: birnam_abi::root_registry::OwnedV1, objects: &[crate::abi::Oop]) -> Self {
        let mut retained = Self { owner, tokens: Vec::new() };
        for object in objects { retained.retain(*object); }
        retained
    }
    pub(crate) fn retain(&mut self, object: crate::abi::Oop) {
        if !object.is_null() {
            let token = unsafe { self.owner.retain.unwrap()(self.owner.context, object) };
            assert_ne!(token, 0);
            self.tokens.push(token);
        }
    }
}
impl Drop for Retained {
    fn drop(&mut self) {
        for token in &self.tokens {
            assert_eq!(unsafe { self.owner.release.unwrap()(self.owner.context, *token) }, 1);
        }
    }
}
