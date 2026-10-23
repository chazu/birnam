//! Replaceable implementations for the initial object graph. These functions
//! interpret their own formats; allocation and generic dispatch do not.
use super::native_id::{Action, OPERATIONS, Scope, State};
use crate::abi::{self, Oop, id_protocol::Method};
#[repr(C)]
#[derive(Clone, Copy)]
struct NativeMethod {
    entry_index: usize,
    context: Oop,
    owner: Oop,
}
#[derive(Clone, Copy)]
pub(super) struct MethodData {
    pub entry: Method,
    pub context: Oop,
    pub owner: Oop,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct FaultData {
    pub kind: Oop,
    pub detail: Oop,
}
impl State {
    fn native_application(&self,m:Oop)->bool{
        if self.uses_default_lookup(m)!=Ok(true) { return false; }
        let Ok(behavior)=self.behavior(m) else { return false; };
        self.tables.borrow().lookup(behavior,self.selector(b"apply"))==Some(self.native_apply.get())
    }
    pub(super) fn method_is_builtin(&self,m:Oop,action:Action)->bool{
        if !self.native_application(m) { return false; }
        let Ok(fields)=self.native_method_fields(m) else { return false; };
        fields.context==self.root && fields.owner==self.root
            && OPERATIONS.get(fields.entry_index).is_some_and(|(_,found,_)|*found==action)
    }
    pub(super) fn same_native_method(&self,a:Oop,b:Oop)->bool{
        self.native_method_index(a,&[b]).is_some()
    }
    pub(super) fn native_method_index(&self,selected:Oop,candidates:&[Oop])->Option<usize>{
        if !self.native_application(selected) { return None; }
        let selected=self.method_data(selected).ok()?;
        candidates.iter().position(|&candidate|{
            if !self.native_application(candidate) { return false; }
            let Ok(candidate)=self.method_data(candidate) else { return false; };
            selected.entry as usize==candidate.entry as usize
                && selected.context==candidate.context && selected.owner==candidate.owner
        })
    }
    pub(super) fn representation_queries_current(&self, receiver: Oop) -> bool {
        if receiver != self.root || self.uses_default_lookup(receiver) != Ok(true) { return false; }
        let Ok(behavior)=self.behavior(receiver) else { return false; };
        [(b"tryRepresentationIdentityOf:behavior:bytes:".as_slice(),Action::Representation),
            (b"behaviorOf:",Action::Behavior),(b"allocationSizeOf:",Action::Extent),(b"identityOf:",Action::Identity)]
            .into_iter().all(|(name,action)|self.tables.borrow().lookup(behavior,self.selector(name))
                .is_some_and(|m|self.method_is_builtin(m,action)))
    }
    pub(super) fn method_data(&self, o: Oop) -> Result<MethodData, Oop> {
        let data = self.native_method_fields(o)?;
        let entry = self.native_entry(data.owner, data.entry_index)?;
        Ok(MethodData {
            entry,
            context: data.context,
            owner: data.owner,
        })
    }
    fn native_entry(&self, owner: Oop, index: usize) -> Result<Method, Oop> {
        if owner == self.root {
            OPERATIONS.get(index).map(|(_, _, entry)| *entry)
        } else {
            self.modules.entry(owner, index)
        }.ok_or(self.invalid())
    }
    pub(super) fn clone_method(&self, method: Oop, context: Oop) -> Result<Oop, Oop> {
        let data = self.native_method_fields(method)?;
        self.new_method_index(data.entry_index, context, data.owner)
    }
    fn native_method_fields(&self, o: Oop) -> Result<NativeMethod, Oop> {
        let (p, n) = self.payload(o)?;
        if n != size_of::<NativeMethod>() {
            return Err(self.invalid());
        }
        // The owner interprets a bounded declaration index, never unvalidated
        // payload bytes as a Rust function pointer.
        let data = unsafe { p.cast::<NativeMethod>().read() };
        // The bootstrap context is permanently rooted. Other contexts must
        // still be valid Oops before native code sees them. Invocation checks
        // the code owner when resolving its declared entry; tracing validates
        // every emitted owner through the collector's allocation inventory.
        if data.context != self.root { self.behavior(data.context)?; }
        Ok(data)
    }
    pub(super) fn fault_data(&self, o: Oop) -> Result<FaultData, Oop> {
        let (p, n) = self.payload(o)?;
        if n != size_of::<FaultData>() {
            return Err(self.invalid());
        }
        let data = unsafe { p.cast::<FaultData>().read() };
        self.behavior(data.kind)?;
        self.behavior(data.detail)?;
        Ok(data)
    }
    pub(super) fn make_name(&self, b: &[u8]) -> Result<Oop, Oop> {
        if b.contains(&0) {
            return Err(self.invalid());
        }
        let o = self.allocate(self.name, b.len())?;
        unsafe { core::ptr::copy_nonoverlapping(b.as_ptr(), o.cast::<u8>(), b.len()) };
        Ok(o)
    }
    pub(super) fn name_from_values(&self, values: &[Oop]) -> Result<Oop, Oop> {
        if values
            .iter()
            .any(|&v| abi::as_size(v).is_none_or(|b| b == 0 || b > 255))
        {
            return Err(self.invalid());
        }
        let name = self.allocate(self.name, values.len())?;
        unsafe {
            for (i, &v) in values.iter().enumerate() {
                name.cast::<u8>()
                    .add(i)
                    .write(abi::as_size(v).unwrap() as u8)
            }
        }
        Ok(name)
    }
    pub(super) fn reference_at(&self, o: Oop, index: usize) -> Result<Oop, Oop> {
        let (p, n) = self.sequence_storage(o)?;
        if index >= n {
            return Err(self.invalid());
        }
        let value = unsafe { p.add(index).read() };
        self.behavior(value)?;
        Ok(value)
    }
    pub(super) fn reference_put(&self, o: Oop, index: usize, value: Oop) -> Result<Oop, Oop> {
        let (p, n) = self.sequence_storage(o)?;
        if index >= n {
            return Err(self.invalid());
        }
        // A single-field write needs the checked extent and that field, not a
        // copied/validated inventory of every other element.
        self.behavior(unsafe { p.add(index).read() })?;
        self.behavior(value)?;
        unsafe { p.add(index).write(value) };
        Ok(value)
    }
    pub(super) fn name_data(&self, o: Oop) -> Result<(*const u8, usize), Oop> {
        self.payload(o).map(|(p, n)| (p.cast_const(), n))
    }
    pub(super) fn sequence_storage(&self, o: Oop) -> Result<(*mut Oop, usize), Oop> {
        let (p, n) = self.payload(o)?;
        if n < size_of::<usize>() || n % size_of::<Oop>() != 0 {
            return Err(self.invalid());
        }
        let len = unsafe { p.cast::<usize>().read() };
        if len != (n - size_of::<usize>()) / size_of::<Oop>() {
            return Err(self.invalid());
        }
        Ok((unsafe { p.add(size_of::<usize>()) }.cast(), len))
    }
    pub(super) fn sequence_values(&self, o: Oop) -> Result<Vec<Oop>, Oop> {
        self.sequence_snapshot(o, &[])
    }
    fn sequence_snapshot(&self, o: Oop, prefix: &[Oop]) -> Result<Vec<Oop>, Oop> {
        self.sequence_snapshot_with(o, prefix, &[])
    }
    fn sequence_snapshot_with(&self, o: Oop, prefix: &[Oop], suffix: &[Oop]) -> Result<Vec<Oop>, Oop> {
        let (p, n) = self.sequence_storage(o)?;
        let mut values = Vec::with_capacity(prefix.len() + n + suffix.len());
        values.extend_from_slice(prefix);
        values.extend_from_slice(unsafe { core::slice::from_raw_parts(p, n) });
        values.extend_from_slice(suffix);
        for &v in &values {
            self.behavior(v)?;
        }
        Ok(values)
    }
    pub(super) unsafe fn apply_arguments(
        &self, operation: Action, sequence: Oop, args: &[Oop],
    ) -> Result<Oop, Oop> {
        // This is the argument container's implementation. Other containers
        // implement these messages without exposing their payload to Id.
        match (operation, args) {
            (Action::ApplyArguments, &[callable, receiver]) => {
                let forwarded = self.sequence_snapshot(sequence, &[receiver])?;
                unsafe { self.send(callable, b"apply", &forwarded) }
            }
            (Action::ApplyArgumentsWith, [callable, receiver, suffix @ ..]) => {
                let forwarded = self.sequence_snapshot_with(sequence, &[*receiver], suffix)?;
                unsafe { self.send(*callable, b"apply", &forwarded) }
            }
            (Action::AppendArguments, &[target]) => {
                let values = self.sequence_values(sequence)?;
                // The destination owns validation and interpretation of byte values.
                unsafe { self.send(target, b"appendByteValues", &values) }
            }
            (Action::LoadArguments, &[path]) => {
                let values = self.sequence_values(sequence)?;
                unsafe { self.send(path, b"loadNative", &values) }
            },
            _ => Err(self.invalid()),
        }
    }
    pub(super) fn new_sequence(&self, values: &[Oop]) -> Result<Oop, Oop> {
        let n = values
            .len()
            .checked_add(1)
            .and_then(|n| n.checked_mul(size_of::<Oop>()))
            .ok_or(self.invalid())?;
        let o = self.allocate(self.sequence, n)?;
        unsafe {
            o.cast::<usize>().write(values.len());
            core::ptr::copy_nonoverlapping(values.as_ptr(), o.cast::<Oop>().add(1), values.len())
        };
        Ok(o)
    }
    pub(super) fn new_method_index(&self, entry_index: usize, context: Oop, owner: Oop) -> Result<Oop, Oop> {
        let context=abi::object_reference(context);
        let owner=abi::object_reference(owner);
        self.native_entry(owner, entry_index)?;
        self.behavior(context)?;
        let o = self.allocate(self.method, size_of::<NativeMethod>())?;
        unsafe {
            o.cast::<NativeMethod>().write(NativeMethod {
                entry_index,
                context,
                owner,
            })
        };
        Ok(o)
    }
    pub(super) fn make_fault(&self, kind: Oop, detail: Oop) -> Oop {
        let o = self
            .allocate(self.fault, size_of::<FaultData>())
            .expect("failure object allocation");
        unsafe { o.cast::<FaultData>().write(FaultData { kind:abi::object_reference(kind), detail:abi::object_reference(detail) }) };
        o
    }
    pub(super) unsafe fn apply_native(
        &self,
        m: Oop,
        r: Oop,
        args: &[Oop],
        scope: &Scope<'_>,
    ) -> Result<Oop, Oop> {
        let method = self.method_data(m)?;
        scope.entered(m, method.context, method.owner);
        let result =
            unsafe { (method.entry)(self.root, method.context, r, args.as_ptr(), args.len()) };
        self.behavior(result)?;
        if abi::is_fault(result) {
            Err(abi::object_reference(result))
        } else {
            Ok(result)
        }
    }
    pub(super) fn install_reference_owners(&self) {
        // Ordinary owner objects, not per-allocation tags. New native or
        // language-defined owners register with Collect without changing Id.
        for (behavior, implementation) in [
            (self.meta, b"tableReferences".as_slice()),
            (self.symbol, b"noReferences"),
            (self.name, b"noReferences"),
            (self.root_behavior, b"noReferences"),
            (self.method, b"methodReferences"),
            (self.fault, b"faultReferences"),
            (self.sequence, b"sequenceReferences"),
            (self.module, b"noReferences"),
            (self.modules.registry_behavior, b"moduleReferences"),
        ] {
            let vt = self.tables.borrow_mut().create(
                &mut self.heap.borrow_mut(),
                self.object,
                self.meta,
            );
            let owner = self.allocate(vt, 0).unwrap();
            let method = self
                .tables
                .borrow()
                .lookup(self.object, self.selector(implementation))
                .unwrap();
            self.tables
                .borrow_mut()
                .add_method(vt, self.selector(b"referencesOf:do:"), method);
            self.reference_owners.borrow_mut().push((behavior, owner));
            self.permanent.borrow_mut().push(owner);
        }
    }
    pub(super) fn owned_references(&self, operation: Action, o: Oop) -> Result<Vec<Oop>, Oop> {
        match operation {
            Action::NoReferences => Ok(Vec::new()),
            Action::MethodReferences => {
                let m = self.native_method_fields(o)?;
                Ok(vec![m.context, m.owner])
            }
            Action::FaultReferences => {
                let f = self.fault_data(o)?;
                Ok(vec![f.kind, f.detail])
            }
            Action::SequenceReferences => self.sequence_values(o),
            // The binding-table implementation owns this state. This path
            // remains available as an ordinary owner method, not a type tag.
            Action::TableReferences => {
                if !self.tables.borrow().is_registered(o) {
                    return Err(self.invalid());
                }
                let tables = self.tables.borrow();
                let row = tables.record(o);
                let mut out = vec![row.parent];
                for ((key, value), weak) in row.keys.iter().zip(&row.values).zip(&row.weak_keys) {
                    if !weak {
                        out.extend([*key, *value]);
                    }
                }
                Ok(out)
            }
            Action::ModuleReferences => self.modules.references(o).ok_or(self.invalid()),
            _ => Err(self.invalid()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn object_valued_fact_queries_decline_after_live_replacement() {
        let s = super::super::initial_objects::construct();
        let id = s.protocol;
        let size = abi::size(8).unwrap();
        let value = s.allocate(s.object, 8).unwrap();
        let query = s.selector(b"tryRepresentationIdentityOf:behavior:bytes:");
        let identity = s.selector(b"identityOf:");
        let matches = s.selector(b"nativeBindingsMatch:");
        let root_type = s.behavior(s.root).unwrap();
        let original = s.tables.borrow().lookup(root_type, identity).unwrap();
        let witness = s.clone_method(original, s.root).unwrap();
        assert!(abi::as_size(unsafe { id.send(s.root, query, &[value, s.object, size]) }.unwrap()).unwrap() > 0);
        assert_eq!(unsafe { id.send(s.root, matches, &[s.root, identity, witness]) }.unwrap(), abi::size(1).unwrap());
        let replacement = s.tables.borrow().lookup(s.object, s.selector(b"yourself")).unwrap();
        s.tables.borrow_mut().add_method(root_type, identity, replacement);
        assert!(unsafe { id.send(s.root, query, &[value, s.object, size]) }.unwrap().is_null());
        assert_eq!(unsafe { id.send(s.root, matches, &[s.root, identity, witness]) }.unwrap(), abi::size(0).unwrap());
        s.tables.borrow_mut().add_method(root_type, identity, original);
        let private = s.tables.borrow_mut().create(&mut s.heap.borrow_mut(), s.method, s.native_meta);
        s.tables.borrow_mut().add_method(private, s.selector(b"apply"), replacement);
        s.heap.borrow_mut().set_vtable(witness, private);
        s.tables.borrow_mut().note_mutation();
        assert_eq!(unsafe { id.send(s.root, matches, &[s.root, identity, witness]) }.unwrap(), abi::size(0).unwrap());
    }
    #[test]
    fn failure_can_be_caught_and_stored_as_data() {
        let s = super::super::initial_objects::construct();
        let id = s.protocol;
        let arguments = [s.selector(b"invalidRequest"), s.root];
        let error = unsafe { id.send(s.root, s.selector(b"fault:detail:"), &arguments) }.unwrap_err();
        assert!(!abi::is_fault(error), "caught errors are ordinary object references");
        let sequence = unsafe { id.send(s.root, s.selector(b"sequence"), &[error]) }.unwrap();
        assert_eq!(unsafe { id.send(sequence, s.selector(b"at:"), &[abi::size(0).unwrap()]) }.unwrap(), error);
        assert_eq!(unsafe { id.send(s.root, s.selector(b"allocationAt:"), &[abi::size(error.addr()).unwrap()]) }.unwrap(), error);
    }
    #[test]
    fn payload_meaning_can_be_supplied_after_allocation() {
        let s = super::super::initial_objects::construct();
        let name = s.allocate(s.object, 3).unwrap();
        unsafe { core::ptr::copy_nonoverlapping(b"abc".as_ptr(), name.cast::<u8>(), 3) };
        assert!(s.heap.borrow_mut().set_vtable(name, s.name));
        let symbol = unsafe { s.send(name, b"intern", &[]) }.unwrap();
        assert_eq!(s.symbols.borrow().bytes_of(symbol), Some(b"abc".as_slice()));
    }
    #[test]
    fn malformed_native_method_bytes_are_not_called() {
        let s = super::super::initial_objects::construct();
        let object = s.allocate(s.method, size_of::<NativeMethod>()).unwrap();
        // A native Method format is checked by its implementation, not by Id's
        // allocator. A declaration index with no code owner is rejected.
        assert!(s.method_data(object).is_err());
        unsafe {
            object.cast::<NativeMethod>().write(NativeMethod {
                entry_index: usize::MAX,
                context: s.root,
                owner: s.root,
            })
        };
        assert!(s.method_data(object).is_err());
        for (context, owner) in [
            (core::ptr::without_provenance_mut(0x1000), s.root),
            (s.root, core::ptr::without_provenance_mut(0x1000)),
        ] {
            unsafe { object.cast::<NativeMethod>().write(NativeMethod { entry_index: 0, context, owner }) };
            assert!(s.method_data(object).is_err());
        }
    }
    #[test]
    fn reference_storage_rejects_invalid_lengths_and_non_object_words() {
        let s = super::super::initial_objects::construct();
        let malformed = s.allocate(s.sequence, 2 * size_of::<Oop>()).unwrap();
        unsafe { malformed.cast::<usize>().write(usize::MAX) };
        assert!(s.sequence_values(malformed).is_err());
        unsafe {
            malformed.cast::<usize>().write(1);
            malformed.cast::<usize>().add(1).write(0x1000)
        };
        assert!(s.sequence_values(malformed).is_err());
    }
    #[test]
    fn detached_payloads_keep_their_reference_and_contents() {
        let s = super::super::initial_objects::construct();
        let name = s.make_name(b"still here").unwrap();
        let fault = s.make_fault(s.selector(b"invalidRequest"), name);
        let storage = s.heap.borrow_mut().detach(fault.addr()).unwrap();
        assert_eq!(storage.object(), fault);
        s.detached
            .borrow_mut()
            .insert(storage.identity() as usize, storage);
        assert_eq!(s.fault_data(fault).unwrap().detail, name);
        s.detached.borrow_mut().clear();
        assert!(s.fault_data(fault).is_err());
    }
}
