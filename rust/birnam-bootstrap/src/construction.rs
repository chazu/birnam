//! Production dependency construction owned by the loaded Bootstrap module.
//! Prefix requests execute this same sequence and stop before successors load.
use super::{BootstrapRef, invalid};
use birnam_abi::{
    IdApi,
    id_protocol::{Operand, Outcome, Protocol},
};
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
fn path_bytes(path: &std::path::Path) -> Vec<u8> {
    path.as_os_str().as_bytes().to_vec()
}

pub(super) struct Construction {
    source_path: PathBuf,
    primitives: birnam_abi::id_primitives::PrimitivesV1,
    lifetime: Option<*const birnam_abi::lifetime::OwnedV2>,
    base_values: Option<*const birnam_abi::base_values::ValuesV1>,
    namespace: Option<*const birnam_abi::namespace_owner::OwnedV1>,
    model: Option<*const birnam_abi::object_model::OwnedV1>,
    string: Option<*mut birnam_abi::string::StringApiV1>,
    error: Option<birnam_abi::Oop>,
    integer: Option<*mut birnam_abi::integer::IntegerApiV1>,
    float: Option<*mut birnam_abi::float::FloatApiV1>,
    word: Option<*mut birnam_abi::word::WordApiV1>,
    graph: Option<*const birnam_abi::trace_graph::GraphV2>,
    collection: Option<*const birnam_abi::collection_state::OwnedV2>,
    text_ready: bool,
    control_ready: bool,
    collection_base: Option<birnam_abi::Oop>,
    string_builder: Option<birnam_abi::Oop>,
    value_policy: Option<*const birnam_abi::collection_value::ApiV1>,
    vector: Option<*const birnam_abi::vector::ApiV6>,
    range: Option<*const birnam_abi::range::ApiV6>,
    map: Option<*const birnam_abi::map::ApiV6>,
    string_collection: Option<*const birnam_abi::string_collection::ApiV1>,
    store: Option<birnam_abi::Oop>,

    error_value: Option<*const birnam_abi::error_value::ApiV3>,
    printing: Option<*const birnam_abi::printing::ApiV1>,
    block_loop: Option<*const birnam_abi::block_loop::ApiV3>,
    id_protocol: Protocol,
    roots: Option<*const birnam_abi::root_registry::OwnedV1>,
    bootstrap: BootstrapRef,
    runtime: Option<*mut IdApi>,
    support_result: Option<Box<birnam_abi::runtime_support::FactoriesV1>>,
    values_result: Option<Box<birnam_abi::runtime_values::ApisV1>>,
}
impl Construction {
    pub(super) fn new(
        source_path: PathBuf,
        id_protocol: Protocol,
        primitives: birnam_abi::id_primitives::PrimitivesV1,
        bootstrap: BootstrapRef,
    ) -> Self {
        Self {
            source_path,
            id_protocol,
            primitives,
            bootstrap,
            roots: None,
            lifetime: None,
            base_values: None,
            namespace: None,
            model: None,
            string: None,
            error: None,
            integer: None,
            float: None,
            word: None,
            graph: None,
            collection: None,
            text_ready: false,
            control_ready: false,
            collection_base: None,
            string_builder: None,
            value_policy: None,
            vector: None,
            range: None,
            map: None,
            string_collection: None,
            store: None,

            error_value: None,
            printing: None,
            block_loop: None,
            runtime: None,
            support_result: None,
            values_result: None,
        }
    }
    pub(super) fn has_path(&self, path: &std::path::Path) -> bool {
        self.source_path == path
    }
    unsafe fn load_native(&self, path: &[u8], args: &[Operand]) -> Outcome {
        unsafe { self.bootstrap.initialize(path, args) }
    }
    fn initialize_roots(
        &mut self,
    ) -> Result<Option<*const birnam_abi::root_registry::OwnedV1>, Outcome> {
        if self.roots.is_some() {
            return Ok(self.roots);
        }
        use birnam_abi::id_protocol::{self as native, Operand};
        let path = self
            .source_path
            .with_file_name("libbirnam_root_registry.dylib");
        let answer = unsafe {
            self.load_native(
                &path_bytes(&path),
                &[Operand::object(self.id_protocol.receiver)],
            )
        };
        if answer.status != native::LOADED {
            return Err(Outcome::failed(answer.status, answer.detail));
        }
        let pointer =
            answer.value.as_word().unwrap_or(0) as *const birnam_abi::root_registry::OwnedV1;
        if !unsafe { birnam_abi::root_registry::valid_owned(pointer) } {
            return Err(invalid());
        }
        let status = unsafe { self.bootstrap.adopt_roots(pointer) };
        if status != native::LOADED {
            return Err(Outcome::failed(status, 0));
        }
        self.roots = Some(pointer);
        Ok(self.roots)
    }
    /// Construct complete payload, trace-hook and finalizer records through Id.
    /// This is the same step used by ordinary production startup.
    fn initialize_lifetime(
        &mut self,
    ) -> Result<Option<*const birnam_abi::lifetime::OwnedV2>, Outcome> {
        if self.lifetime.is_some() {
            return Ok(self.lifetime);
        }
        self.initialize_roots()?;
        use birnam_abi::id_protocol::{self as native, Operand};
        let load = |file: &str, arguments: &[Operand]| -> Result<Operand, Outcome> {
            let path = self.source_path.with_file_name(file);
            let bytes = path_bytes(&path);
            let answer = unsafe { self.load_native(&bytes, arguments) };
            if answer.status != native::LOADED {
                return Err(Outcome::failed(answer.status, answer.detail));
            }
            Ok(answer.value)
        };
        let layout = load("libbirnam_payload_layout.dylib", &[])?;
        let hooks = load("libbirnam_trace_hooks.dylib", &[])?;
        let finalizers = load("libbirnam_finalizers.dylib", &[])?;
        let answer = load("libbirnam_lifetime.dylib", &[layout, hooks, finalizers])?;
        let pointer = answer.as_word().unwrap_or(0) as *const birnam_abi::lifetime::OwnedV2;
        if !unsafe { birnam_abi::lifetime::valid(pointer) } {
            return Err(Outcome::failed(native::INVALID_REQUEST, 0));
        }
        self.lifetime = Some(pointer);
        Ok(self.lifetime)
    }
    /// Construct base identities and predicates before String, numbers or Block.
    fn initialize_base_values(
        &mut self,
    ) -> Result<Option<*const birnam_abi::base_values::ValuesV1>, Outcome> {
        if self.base_values.is_some() {
            return Ok(self.base_values);
        }
        self.initialize_lifetime()?;
        use birnam_abi::id_protocol::{self as native, Operand};
        let boolean = self.source_path.with_file_name("libbirnam_boolean.dylib");
        let nil = self.source_path.with_file_name("libbirnam_nil.dylib");
        let answer = unsafe {
            self.load_native(
                &path_bytes(&boolean),
                &[
                    Operand::word(self.roots.unwrap() as usize),
                    Operand::word(self.lifetime.unwrap() as usize),
                ],
            )
        };
        if answer.status != native::LOADED {
            return Err(Outcome::failed(answer.status, answer.detail));
        }
        let values =
            answer.value.as_word().unwrap_or(0) as *const birnam_abi::base_values::ValuesV1;
        if !unsafe { birnam_abi::base_values::valid(values) } {
            return Err(Outcome::failed(native::INVALID_REQUEST, 0));
        }
        let answer =
            unsafe { self.load_native(&path_bytes(&nil), &[Operand::word(values as usize)]) };
        if answer.status != native::LOADED {
            return Err(Outcome::failed(answer.status, answer.detail));
        }
        self.base_values = Some(values);
        Ok(self.base_values)
    }
    /// Native publication/import and Word-valued enumeration, before String.
    fn initialize_namespace(
        &mut self,
    ) -> Result<Option<*const birnam_abi::namespace_owner::OwnedV1>, Outcome> {
        if self.namespace.is_some() {
            return Ok(self.namespace);
        }
        self.initialize_base_values()?;
        use birnam_abi::id_protocol::{self as native, Operand};
        let path = self.source_path.with_file_name("libbirnam_namespace.dylib");
        let args = [
            Operand::word(self.roots.unwrap() as usize),
            Operand::word(self.lifetime.unwrap() as usize),
            Operand::word(self.base_values.unwrap() as usize),
        ];
        let answer = unsafe { self.load_native(&path_bytes(&path), &args) };
        if answer.status != native::LOADED {
            return Err(Outcome::failed(answer.status, answer.detail));
        }
        let namespace =
            answer.value.as_word().unwrap_or(0) as *const birnam_abi::namespace_owner::OwnedV1;
        if !unsafe { birnam_abi::namespace_owner::valid(namespace) } {
            return Err(Outcome::failed(native::INVALID_REQUEST, 0));
        }
        self.namespace = Some(namespace);
        Ok(self.namespace)
    }
    /// Construct private behaviors and enumeration before Runtime/GC.
    fn initialize_object_model(
        &mut self,
    ) -> Result<Option<*const birnam_abi::object_model::OwnedV1>, Outcome> {
        if self.model.is_some() {
            return Ok(self.model);
        }
        self.initialize_namespace()?;
        use birnam_abi::id_protocol::{self as native, Operand};
        let path = self
            .source_path
            .with_file_name("libbirnam_object_model.dylib");
        let args = [
            Operand::word(self.roots.unwrap() as usize),
            Operand::word(self.namespace.unwrap() as usize),
            Operand::word(self.base_values.unwrap() as usize),
        ];
        let answer = unsafe { self.load_native(&path_bytes(&path), &args) };
        if answer.status != native::LOADED {
            return Err(Outcome::failed(answer.status, answer.detail));
        }
        let model = answer.value.as_word().unwrap_or(0) as *const birnam_abi::object_model::OwnedV1;
        if !unsafe { birnam_abi::object_model::valid(model) } {
            return Err(Outcome::failed(native::INVALID_REQUEST, 0));
        }
        self.model = Some(model);
        Ok(self.model)
    }
    /// Construct usable byte storage before Integer, Error, Float or Runtime.
    fn initialize_string(
        &mut self,
    ) -> Result<Option<*mut birnam_abi::string::StringApiV1>, Outcome> {
        if self.string.is_some() {
            return Ok(self.string);
        }
        self.initialize_object_model()?;
        use birnam_abi::id_protocol::{self as native, Operand};
        let path = self.source_path.with_file_name("libbirnam_string.dylib");
        let args = [
            Operand::word(self.roots.unwrap() as usize),
            Operand::word(self.lifetime.unwrap() as usize),
            Operand::word(self.namespace.unwrap() as usize),
        ];
        let answer = unsafe { self.load_native(&path_bytes(&path), &args) };
        if answer.status != native::LOADED {
            return Err(Outcome::failed(answer.status, answer.detail));
        }
        let string = answer.value.as_word().unwrap_or(0) as *mut birnam_abi::string::StringApiV1;
        if !unsafe { birnam_abi::string::valid(string) } {
            return Err(Outcome::failed(native::INVALID_REQUEST, 0));
        }
        self.string = Some(string);
        Ok(self.string)
    }
    /// Construct Error values and propagation before numbers or Runtime.
    fn initialize_error(&mut self) -> Result<Option<birnam_abi::Oop>, Outcome> {
        if self.error.is_some() {
            return Ok(self.error);
        }
        self.initialize_string()?;
        use birnam_abi::id_protocol::{self as native, Operand};
        let path = self.source_path.with_file_name("libbirnam_id_error.dylib");
        let args = [
            Operand::word(self.roots.unwrap() as usize),
            Operand::word(self.lifetime.unwrap() as usize),
            Operand::word(self.base_values.unwrap() as usize),
            Operand::word(self.namespace.unwrap() as usize),
            Operand::word(self.string.unwrap() as usize),
        ];
        let answer = unsafe { self.load_native(&path_bytes(&path), &args) };
        if answer.status != native::LOADED {
            return Err(Outcome::failed(answer.status, answer.detail));
        }
        let error = answer
            .value
            .as_object()
            .filter(|value| !value.is_null())
            .ok_or(Outcome::failed(native::INVALID_REQUEST, 0))?;
        // Error has published its own roots and Namespace names. Its factory
        // no longer needs Bootstrap's temporary initialization-result root.
        assert_eq!(unsafe { self.bootstrap.release(answer.value) }, 1);
        self.error = Some(error);
        Ok(self.error)
    }
    /// Construct checked Integer arithmetic and errors before Float or Runtime.
    fn initialize_integer(
        &mut self,
    ) -> Result<Option<*mut birnam_abi::integer::IntegerApiV1>, Outcome> {
        if self.integer.is_some() {
            return Ok(self.integer);
        }
        self.initialize_error()?;
        use birnam_abi::id_protocol::{self as native, Operand};
        let path = self.source_path.with_file_name("libbirnam_integer.dylib");
        let args = [
            Operand::word(self.roots.unwrap() as usize),
            Operand::word(self.lifetime.unwrap() as usize),
            Operand::word(self.base_values.unwrap() as usize),
            Operand::word(self.namespace.unwrap() as usize),
            Operand::word(self.string.unwrap() as usize),
            Operand::object(self.error.unwrap()),
        ];
        let answer = unsafe { self.load_native(&path_bytes(&path), &args) };
        if answer.status != native::LOADED {
            return Err(Outcome::failed(answer.status, answer.detail));
        }
        let integer = answer.value.as_word().unwrap_or(0) as *mut birnam_abi::integer::IntegerApiV1;
        if !unsafe { birnam_abi::integer::valid(integer) } {
            return Err(Outcome::failed(native::INVALID_REQUEST, 0));
        }
        self.integer = Some(integer);
        Ok(self.integer)
    }
    /// Construct Float and extend Integer only after the Integer base exists.
    fn initialize_float(&mut self) -> Result<Option<*mut birnam_abi::float::FloatApiV1>, Outcome> {
        if self.float.is_some() {
            return Ok(self.float);
        }
        self.initialize_integer()?;
        use birnam_abi::id_protocol::{self as native, Operand};
        let path = self.source_path.with_file_name("libbirnam_float.dylib");
        let args = [
            Operand::word(self.roots.unwrap() as usize),
            Operand::word(self.lifetime.unwrap() as usize),
            Operand::word(self.base_values.unwrap() as usize),
            Operand::word(self.namespace.unwrap() as usize),
            Operand::word(self.string.unwrap() as usize),
            Operand::object(self.error.unwrap()),
            Operand::word(self.integer.unwrap() as usize),
        ];
        let answer = unsafe { self.load_native(&path_bytes(&path), &args) };
        if answer.status != native::LOADED {
            return Err(Outcome::failed(answer.status, answer.detail));
        }
        let float = answer.value.as_word().unwrap_or(0) as *mut birnam_abi::float::FloatApiV1;
        if !unsafe { birnam_abi::float::valid(float) } {
            return Err(Outcome::failed(native::INVALID_REQUEST, 0));
        }
        self.float = Some(float);
        Ok(self.float)
    }
    /// Construct Word after Integer without a Float or Runtime dependency.
    fn initialize_word(&mut self) -> Result<Option<*mut birnam_abi::word::WordApiV1>, Outcome> {
        if self.word.is_some() {
            return Ok(self.word);
        }
        self.initialize_integer()?;
        use birnam_abi::id_protocol::{self as native, Operand};
        let path = self.source_path.with_file_name("libbirnam_word.dylib");
        let args = [
            Operand::word(self.roots.unwrap() as usize),
            Operand::word(self.lifetime.unwrap() as usize),
            Operand::word(self.base_values.unwrap() as usize),
            Operand::word(self.namespace.unwrap() as usize),
            Operand::word(self.string.unwrap() as usize),
            Operand::object(self.error.unwrap()),
            Operand::word(self.integer.unwrap() as usize),
        ];
        let answer = unsafe { self.load_native(&path_bytes(&path), &args) };
        if answer.status != native::LOADED {
            return Err(Outcome::failed(answer.status, answer.detail));
        }
        let word = answer.value.as_word().unwrap_or(0) as *mut birnam_abi::word::WordApiV1;
        if !unsafe { birnam_abi::word::valid(word) } {
            return Err(Outcome::failed(native::INVALID_REQUEST, 0));
        }
        self.word = Some(word);
        Ok(self.word)
    }
    /// Extend existing String storage after both numeric families are complete.
    fn initialize_text(&mut self) -> Result<(), Outcome> {
        if self.text_ready {
            return Ok(());
        }
        self.initialize_float()?;
        use birnam_abi::id_protocol::{self as native, Operand};
        let id = self.id_protocol;
        let name = Operand::bytes(b"installText:");
        let selector = unsafe { id.message.unwrap()(id.context, id.receiver, id.intern, &name, 1) };
        if selector.status != native::LOADED {
            return Err(Outcome::failed(selector.status, selector.detail));
        }
        let selector = selector
            .value
            .as_object()
            .ok_or(Outcome::failed(native::INVALID_REQUEST, 0))?;
        let args = [
            Operand::word(self.base_values.unwrap() as usize),
            Operand::word(self.integer.unwrap() as usize),
            Operand::word(self.float.unwrap() as usize),
        ];
        let result = unsafe {
            id.message.unwrap()(
                id.context,
                (*self.string.unwrap()).prototype,
                selector,
                args.as_ptr(),
                args.len(),
            )
        };
        if result.status != native::LOADED {
            return Err(Outcome::failed(result.status, result.detail));
        }
        if result.value.kind() != native::UNIT {
            return Err(Outcome::failed(native::INVALID_REQUEST, 0));
        }
        let name = Operand::bytes(b"installPrinting:");
        let selector = unsafe { id.message.unwrap()(id.context, id.receiver, id.intern, &name, 1) };
        if selector.status != native::LOADED {
            return Err(Outcome::failed(selector.status, selector.detail));
        }
        let selector = selector
            .value
            .as_object()
            .ok_or(Outcome::failed(native::INVALID_REQUEST, 0))?;
        let values = unsafe { *self.base_values.unwrap() };
        let args = [
            Operand::word(self.roots.unwrap() as usize),
            Operand::word(self.string.unwrap() as usize),
        ];
        for receiver in [values.nil, values.true_object] {
            let result = unsafe {
                id.message.unwrap()(id.context, receiver, selector, args.as_ptr(), args.len())
            };
            if result.status != native::LOADED {
                return Err(Outcome::failed(result.status, result.detail));
            }
            if result.value.kind() != native::UNIT {
                return Err(Outcome::failed(native::INVALID_REQUEST, 0));
            }
        }
        self.text_ready = true;
        Ok(())
    }
    /// Construct live graph access from completed lower owners, before Runtime.
    fn initialize_trace(
        &mut self,
    ) -> Result<Option<*const birnam_abi::trace_graph::GraphV2>, Outcome> {
        if self.graph.is_some() {
            return Ok(self.graph);
        }
        self.initialize_text()?;
        self.initialize_word()?;
        use birnam_abi::id_protocol::{self as native, Operand};
        let mark_path = self
            .source_path
            .with_file_name("libbirnam_native_mark.dylib");
        let mark = unsafe { self.load_native(&path_bytes(&mark_path), &[]) };
        if mark.status != native::LOADED {
            return Err(Outcome::failed(mark.status, mark.detail));
        }
        let path = self.source_path.with_file_name("libbirnam_trace.dylib");
        let args = [
            Operand::word(self.roots.unwrap() as usize),
            Operand::word(self.lifetime.unwrap() as usize),
            Operand::word(self.model.unwrap() as usize),
            mark.value,
        ];
        let answer = unsafe { self.load_native(&path_bytes(&path), &args) };
        if answer.status != native::LOADED {
            return Err(Outcome::failed(answer.status, answer.detail));
        }
        let graph = answer.value.as_word().unwrap_or(0) as *const birnam_abi::trace_graph::GraphV2;
        if !unsafe { birnam_abi::trace_graph::valid(graph) } {
            return Err(Outcome::failed(native::INVALID_REQUEST, 0));
        }
        self.graph = Some(graph);
        Ok(self.graph)
    }
    /// Construct the collection records and controls before aggregate Runtime.
    fn initialize_collection(
        &mut self,
    ) -> Result<Option<*const birnam_abi::collection_state::OwnedV2>, Outcome> {
        if self.collection.is_some() {
            return Ok(self.collection);
        }
        self.initialize_trace()?;
        use birnam_abi::id_protocol as native;
        let mut dependencies = Vec::new();
        for file in [
            "libbirnam_native_stack.dylib",
            "libbirnam_gc_settings.dylib",
            "libbirnam_allocation_budget.dylib",
        ] {
            let path = self.source_path.with_file_name(file);
            let answer = unsafe { self.load_native(&path_bytes(&path), &[]) };
            if answer.status != native::LOADED {
                return Err(Outcome::failed(answer.status, answer.detail));
            }
            dependencies.push(answer.value);
        }
        dependencies.extend([
            native::Operand::word(self.graph.unwrap() as usize),
            native::Operand::word(self.lifetime.unwrap() as usize),
            native::Operand::word(self.model.unwrap() as usize),
        ]);
        let path = self.source_path.with_file_name("libbirnam_collect.dylib");
        let answer = unsafe { self.load_native(&path_bytes(&path), &dependencies) };
        if answer.status != native::LOADED {
            return Err(Outcome::failed(answer.status, answer.detail));
        }
        let owner =
            answer.value.as_word().unwrap_or(0) as *const birnam_abi::collection_state::OwnedV2;
        if !unsafe { birnam_abi::collection_state::valid(owner) } {
            return Err(Outcome::failed(native::INVALID_REQUEST, 0));
        }
        self.collection = Some(owner);
        Ok(self.collection)
    }
    /// Activate the existing heap and install ordinary collecting constructors.
    fn activate_collection(&mut self) -> Result<(), Outcome> {
        let Some(owner) = self.initialize_collection()? else {
            return Ok(());
        };
        let collector = unsafe { *(*owner).collector };
        if unsafe { (collector.active)(collector.context) } != 0 {
            return Ok(());
        }
        let bootstrap = self.bootstrap;
        let status = unsafe {
            (collector.activate)(
                collector.context,
                bootstrap.descriptor(),
                self.roots.unwrap(),
            )
        };
        if status != birnam_abi::storage::OK {
            return Err(Outcome::failed(
                birnam_abi::id_protocol::INITIALIZER_FAILED,
                status as usize,
            ));
        }
        Ok(())
    }
    /// Load complete factory operations through Id, with Runtime still absent.
    fn initialize_runtime_support(
        &self,
    ) -> Result<birnam_abi::runtime_support::FactoriesV1, Outcome> {
        use birnam_abi::id_protocol::{self as native, Operand};
        unsafe fn factory<T: Copy>(
            initial: &Construction,
            library: &str,
            name: &[u8],
            valid: unsafe fn(*const T) -> bool,
        ) -> Result<T, Outcome> {
            let path = initial.source_path.with_file_name(library);
            let answer =
                unsafe { initial.load_native(&path_bytes(&path), &[Operand::bytes(name)]) };
            if answer.status != native::LOADED {
                return Err(Outcome::failed(answer.status, answer.detail));
            }
            let pointer = answer.value.as_word().unwrap_or(0) as *const T;
            if !unsafe { valid(pointer) } {
                return Err(Outcome::failed(native::INVALID_REQUEST, 0));
            }
            Ok(unsafe { *pointer })
        }
        Ok(birnam_abi::runtime_support::FactoriesV1 {
            version: 1,
            byte_size: size_of::<birnam_abi::runtime_support::FactoriesV1>(),
            observation: unsafe {
                factory(
                    self,
                    "libbirnam_method_observation.dylib",
                    b"methodObservation",
                    birnam_abi::method_observation::valid_factory,
                )?
            },
            storage: unsafe {
                factory(
                    self,
                    "libbirnam_storage.dylib",
                    b"observation",
                    birnam_abi::storage_observation::valid,
                )?
            },
            identity: unsafe {
                factory(
                    self,
                    "libbirnam_storage.dylib",
                    b"deferredIdentity",
                    birnam_abi::deferred_identity::valid,
                )?
            },
            index: unsafe {
                factory(
                    self,
                    "libbirnam_storage.dylib",
                    b"deferredIndex",
                    birnam_abi::deferred_index::valid,
                )?
            },
            cache: unsafe {
                factory(
                    self,
                    "libbirnam_dispatch_cache.dylib",
                    b"dispatchCache",
                    birnam_abi::dispatch_cache::valid,
                )?
            },
            verdict: unsafe {
                factory(
                    self,
                    "libbirnam_dispatch_cache.dylib",
                    b"guardVerdict",
                    birnam_abi::guard_verdict::valid,
                )?
            },
        })
    }
    fn initialize_runtime_values(&mut self) -> Result<birnam_abi::runtime_values::ApisV1, Outcome> {
        self.initialize_text()?;
        self.initialize_word()?;
        Ok(birnam_abi::runtime_values::ApisV1 {
            version: 1,
            byte_size: size_of::<birnam_abi::runtime_values::ApisV1>(),
            integer: self.integer.expect("completed Integer"),
            float: self.float.expect("completed Float"),
            word: self.word.expect("completed Word"),
            string: self.string.expect("completed String"),
        })
    }
    fn initialize_runtime(&mut self) -> Result<*mut IdApi, Outcome> {
        if let Some(runtime) = self.runtime {
            return Ok(runtime);
        }
        self.initialize_float()?;
        self.initialize_word()?;
        self.initialize_text()?;
        self.initialize_trace()?;
        self.activate_collection()?;
        let support = self.initialize_runtime_support()?;
        let operations = self.initialize_runtime_values()?;
        let arguments = [
            Operand::word(self.roots.unwrap().addr()),
            Operand::word(self.lifetime.unwrap().addr()),
            Operand::word(self.base_values.unwrap().addr()),
            Operand::word(self.namespace.unwrap().addr()),
            Operand::word(self.model.unwrap().addr()),
            Operand::word(self.graph.unwrap().addr()),
            Operand::word(self.collection.unwrap().addr()),
            Operand::word(
                (&self.primitives as *const birnam_abi::id_primitives::PrimitivesV1).addr(),
            ),
            Operand::word((&support as *const birnam_abi::runtime_support::FactoriesV1).addr()),
            Operand::word((&operations as *const birnam_abi::runtime_values::ApisV1).addr()),
        ];
        let path = self.source_path.with_file_name("libbirnam_runtime.dylib");
        let answer = unsafe { self.load_native(&path_bytes(&path), &arguments) }.into_result()?;
        let pointer = answer
            .as_word()
            .filter(|pointer| *pointer != 0)
            .ok_or_else(invalid)? as *mut IdApi;
        self.runtime = Some(pointer);
        Ok(pointer)
    }
    fn load(&self, library: &str, args: &[Operand]) -> Result<Operand, Outcome> {
        let path = self.source_path.with_file_name(library);
        unsafe { self.load_native(&path_bytes(&path), args) }.into_result()
    }
    fn runtime_service(&mut self, name: &[u8]) -> Result<Operand, Outcome> {
        let runtime = self.initialize_runtime()?;
        self.load("libbirnam_runtime.dylib", &[Operand::bytes(name), Operand::word(runtime.addr())])
    }
    fn initialize_error_value(&mut self) -> Result<*const birnam_abi::error_value::ApiV3, Outcome> {
        if let Some(api) = self.error_value { return Ok(api); }
        self.initialize_error()?;
        let value = self.load("libbirnam_error_value.dylib", &[Operand::word(self.string.unwrap().addr())])?;
        let api = value.as_word().ok_or_else(invalid)? as *const birnam_abi::error_value::ApiV3;
        if unsafe { birnam_abi::error_value::attach(api) } != 1 { return Err(invalid()); }
        self.error_value = Some(api);
        Ok(api)
    }
    fn initialize_printing(&mut self) -> Result<*const birnam_abi::printing::ApiV1, Outcome> {
        if let Some(api) = self.printing { return Ok(api); }
        let runtime = self.initialize_runtime()?;
        let value = self.load("libbirnam_printing.dylib", &[Operand::word(runtime.addr()), Operand::word(self.string.unwrap().addr())])?;
        let api = value.as_word().ok_or_else(invalid)? as *const birnam_abi::printing::ApiV1;
        if unsafe { birnam_abi::printing::attach(api) } != 1 { return Err(invalid()); }
        self.printing = Some(api);
        Ok(api)
    }
    fn initialize_block_loop(&mut self) -> Result<*const birnam_abi::block_loop::ApiV3, Outcome> {
        if let Some(api) = self.block_loop { return Ok(api); }
        let runtime = self.initialize_runtime()?;
        let error = self.initialize_error_value()?;
        let presence = self.runtime_service(b"namespacePresence")?;
        let imports = self.runtime_service(b"namespaceImport")?;
        let value = self.load("libbirnam_block_loop.dylib", &[Operand::word(runtime.addr()), presence, imports, Operand::word(error.addr())])?;
        let api = value.as_word().ok_or_else(invalid)? as *const birnam_abi::block_loop::ApiV3;
        if unsafe { birnam_abi::block_loop::attach(api) } != 1 { return Err(invalid()); }
        self.block_loop = Some(api);
        Ok(api)
    }
    fn initialize_control(&mut self) -> Result<(), Outcome> {
        if self.control_ready { return Ok(()); }
        let runtime = self.initialize_runtime()?;
        let error = self.initialize_error_value()?;
        let presence = self.runtime_service(b"namespacePresence")?;
        let imports = self.runtime_service(b"namespaceImport")?;
        let value = self.load("libbirnam_integer.dylib", &[Operand::bytes(b"control"), Operand::word(runtime.addr()), presence, imports, Operand::word(error.addr())])?;
        if value.kind() != birnam_abi::id_protocol::UNIT { return Err(invalid()); }
        self.initialize_printing()?;
        self.initialize_block_loop()?;
        let value = self.load("libbirnam_boolean.dylib", &[Operand::bytes(b"control"), Operand::word(runtime.addr())])?;
        if value.kind() != birnam_abi::id_protocol::UNIT { return Err(invalid()); }
        self.control_ready = true;
        Ok(())
    }
    fn family_arguments(&mut self) -> Result<Vec<Operand>, Outcome> {
        self.initialize_control()?;
        Ok(vec![Operand::word(self.runtime.unwrap().addr()),
            self.runtime_service(b"namespacePresence")?, self.runtime_service(b"namespaceImport")?,
            self.runtime_service(b"namespaceExport")?, Operand::word(self.roots.unwrap().addr()),
            Operand::word(unsafe { (*self.lifetime.unwrap()).layouts }.addr())])
    }
    fn published_object(&self, value: Operand) -> Result<birnam_abi::Oop, Outcome> {
        let object = value.as_object().filter(|p| !p.is_null()).ok_or_else(invalid)?;
        // The completed module has registered its roots before returning.
        assert_eq!(unsafe { self.bootstrap.release(value) }, 1);
        Ok(object)
    }
    fn initialize_collection_base(&mut self) -> Result<birnam_abi::Oop, Outcome> {
        if let Some(value) = self.collection_base { return Ok(value); }
        let args = self.family_arguments()?;
        let result = self.load("libbirnam_id_collection.dylib", &args)?;
        let object = self.published_object(result)?;
        self.collection_base = Some(object);
        Ok(object)
    }
    fn initialize_string_builder(&mut self) -> Result<birnam_abi::Oop, Outcome> {
        if let Some(value) = self.string_builder { return Ok(value); }
        let mut args = self.family_arguments()?;
        args.extend([Operand::word(unsafe { (*self.lifetime.unwrap()).finalizers }.addr()),
            Operand::word(self.integer.unwrap().addr()), Operand::word(self.string.unwrap().addr())]);
        let result = self.load("libbirnam_string_builder.dylib", &args)?;
        let object = self.published_object(result)?;
        self.string_builder = Some(object);
        Ok(object)
    }
    fn initialize_value_policy(&mut self) -> Result<*const birnam_abi::collection_value::ApiV1, Outcome> {
        if let Some(value) = self.value_policy { return Ok(value); }
        self.initialize_float()?;
        let args = [Operand::word(self.integer.unwrap().addr()), Operand::word(self.float.unwrap().addr()), Operand::word(self.string.unwrap().addr())];
        let result = self.load("libbirnam_collection_value.dylib", &args)?;
        let api = result.as_word().ok_or_else(invalid)? as *const birnam_abi::collection_value::ApiV1;
        if unsafe { birnam_abi::collection_value::attach(api) } != 1 { return Err(invalid()); }
        self.value_policy = Some(api);
        Ok(api)
    }
    fn initialize_vector(&mut self) -> Result<*const birnam_abi::vector::ApiV6, Outcome> {
        if let Some(value) = self.vector { return Ok(value); }
        let parent = self.initialize_collection_base()?;
        let policy = self.initialize_value_policy()?;
        let mut args = self.family_arguments()?;
        args.extend([Operand::word(self.integer.unwrap().addr()), Operand::word(self.string.unwrap().addr()), Operand::word(policy.addr()), Operand::object(parent)]);
        let result = self.load("libbirnam_vector.dylib", &args)?;
        let api = result.as_word().ok_or_else(invalid)? as *const birnam_abi::vector::ApiV6;
        if unsafe { birnam_abi::vector::attach(api) } != 1 { return Err(invalid()); }
        self.vector = Some(api);
        Ok(api)
    }
    fn initialize_range(&mut self) -> Result<*const birnam_abi::range::ApiV6, Outcome> {
        if let Some(value) = self.range { return Ok(value); }
        let vector = self.initialize_vector()?;
        let comma = self.load("libbirnam_vector.dylib", &[Operand::bytes(b"comma")])?;
        let mut args = self.family_arguments()?;
        args.extend([Operand::word(self.integer.unwrap().addr()), Operand::word(self.string.unwrap().addr()), Operand::word(vector.addr()), Operand::object(self.collection_base.unwrap()), comma]);
        let result = self.load("libbirnam_range.dylib", &args)?;
        let api = result.as_word().ok_or_else(invalid)? as *const birnam_abi::range::ApiV6;
        if unsafe { birnam_abi::range::attach(api) } != 1 { return Err(invalid()); }
        self.range = Some(api);
        Ok(api)
    }
    fn initialize_map(&mut self) -> Result<*const birnam_abi::map::ApiV6, Outcome> {
        if let Some(value) = self.map { return Ok(value); }
        let vector = self.initialize_vector()?;
        let mut args = self.family_arguments()?;
        args.extend([Operand::word(self.integer.unwrap().addr()), Operand::word(self.string.unwrap().addr()), Operand::word(vector.addr()), Operand::word(self.value_policy.unwrap().addr()), Operand::object(self.collection_base.unwrap())]);
        let result = self.load("libbirnam_map.dylib", &args)?;
        let api = result.as_word().ok_or_else(invalid)? as *const birnam_abi::map::ApiV6;
        if unsafe { birnam_abi::map::attach(api) } != 1 { return Err(invalid()); }
        self.map = Some(api);
        Ok(api)
    }
    fn initialize_string_collection(&mut self) -> Result<*const birnam_abi::string_collection::ApiV1, Outcome> {
        if let Some(value) = self.string_collection { return Ok(value); }
        let vector = self.initialize_vector()?;
        let args = [Operand::word(self.runtime.unwrap().addr()), Operand::word(self.string.unwrap().addr()), Operand::word(vector.addr()), Operand::object(self.collection_base.unwrap())];
        let result = self.load("libbirnam_string_collection.dylib", &args)?;
        let api = result.as_word().ok_or_else(invalid)? as *const birnam_abi::string_collection::ApiV1;
        if unsafe { birnam_abi::string_collection::attach(api) } != 1 { return Err(invalid()); }
        self.string_collection = Some(api);
        Ok(api)
    }
    fn initialize_collections(&mut self) -> Result<(), Outcome> {
        self.initialize_range()?;
        self.initialize_map()?;
        self.initialize_string_collection()?;
        self.initialize_string_builder()?;
        Ok(())
    }
    fn initialize_store(&mut self) -> Result<birnam_abi::Oop, Outcome> {
        if let Some(value) = self.store { return Ok(value); }
        self.initialize_collections()?;
        let mut args = self.family_arguments()?;
        args.extend([self.runtime_service(b"vtables")?, self.runtime_service(b"methodObservation")?,
            Operand::word(unsafe { (*self.lifetime.unwrap()).hooks }.addr()), Operand::word(self.integer.unwrap().addr())]);
        let result = self.load("libbirnam_store.dylib", &args)?;
        let object = self.published_object(result)?;
        self.store = Some(object);
        Ok(object)
    }
    pub(super) fn advance(&mut self, stage: &[u8]) -> Result<Operand, Outcome> {
        macro_rules! owner {
            ($method:ident) => {
                self.$method()?
                    .map(|p| Operand::word(p.addr()))
                    .ok_or_else(invalid)
            };
        }
        match stage {
            b"existingRoots" => Ok(self
                .roots
                .map(|p| Operand::word(p.addr()))
                .unwrap_or_else(Operand::unit)),
            b"roots" => owner!(initialize_roots),
            b"lifetime" => owner!(initialize_lifetime),
            b"values" => owner!(initialize_base_values),
            b"namespace" => owner!(initialize_namespace),
            b"objectModel" => owner!(initialize_object_model),
            b"string" => owner!(initialize_string),
            b"error" => self
                .initialize_error()?
                .map(Operand::object)
                .ok_or_else(invalid),
            b"integer" => owner!(initialize_integer),
            b"float" => owner!(initialize_float),
            b"word" => owner!(initialize_word),
            b"text" => {
                self.initialize_text()?;
                Ok(Operand::unit())
            }
            b"trace" => owner!(initialize_trace),
            b"collection" => owner!(initialize_collection),
            b"activate" => {
                self.activate_collection()?;
                Ok(Operand::unit())
            }
            b"runtimeSupport" => {
                if self.support_result.is_none() {
                    self.support_result = Some(Box::new(self.initialize_runtime_support()?));
                }
                Ok(Operand::word(
                    (&**self.support_result.as_ref().unwrap()
                        as *const birnam_abi::runtime_support::FactoriesV1)
                        .addr(),
                ))
            }
            b"runtimeValues" => {
                if self.values_result.is_none() {
                    self.values_result = Some(Box::new(self.initialize_runtime_values()?));
                }
                Ok(Operand::word(
                    (&**self.values_result.as_ref().unwrap()
                        as *const birnam_abi::runtime_values::ApisV1)
                        .addr(),
                ))
            }
            b"runtime" => Ok(Operand::word(self.initialize_runtime()?.addr())),
            b"errorValue" => Ok(Operand::word(self.initialize_error_value()?.addr())),
            b"printing" => Ok(Operand::word(self.initialize_printing()?.addr())),
            b"blockLoop" => Ok(Operand::word(self.initialize_block_loop()?.addr())),
            b"control" => { self.initialize_control()?; Ok(Operand::unit()) },
            b"collectionBase" => Ok(Operand::object(self.initialize_collection_base()?)),
            b"stringBuilder" => Ok(Operand::object(self.initialize_string_builder()?)),
            b"valuePolicy" => Ok(Operand::word(self.initialize_value_policy()?.addr())),
            b"vector" => Ok(Operand::word(self.initialize_vector()?.addr())),
            b"range" => Ok(Operand::word(self.initialize_range()?.addr())),
            b"map" => Ok(Operand::word(self.initialize_map()?.addr())),
            b"stringCollection" => Ok(Operand::word(self.initialize_string_collection()?.addr())),
            b"collections" => { self.initialize_collections()?; Ok(Operand::unit()) },
            b"store" => Ok(Operand::object(self.initialize_store()?)),
            b"storeCompensation" => {
                self.initialize_store()?;
                self.load("libbirnam_store.dylib", &[Operand::bytes(b"compensation"), Operand::word(self.runtime.unwrap().addr())])
            },
            _ => Err(invalid()),
        }
    }
}
