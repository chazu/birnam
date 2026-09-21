//! Allocating recovery preparation, followed by the existing atomic publication.
use super::*;
use birnam_codegen::recovery_graph::{self as graph, Graph, Node, Plan, Value};

fn invalid() -> ExecutionError {
    ExecutionError::InvalidAction
}

impl RootBatch<'_> {
    fn retain(&mut self, value: Oop) {
        self.0.push_stackless_scoped_root(value);
        self.1.push(value);
    }
}

fn resolve(value: Value, inputs: &[Oop], nodes: &[Oop]) -> Oop {
    match value {
        Value::Input(index) => inputs[index as usize],
        Value::Node(index) => nodes[index as usize],
    }
}

impl Runtime {
    fn validate_graph_storage(&self, plan: &Plan, inputs: &[Oop]) -> Result<(), ExecutionError> {
        if inputs.len() != plan.input_count() {
            return Err(invalid());
        }
        if plan.nodes().iter().any(|node| matches!(node, Node::Integer(_)))
            && !self.integer_context_current() { return Err(invalid()); }
        let state = unsafe { &*self.state.get() };
        if inputs
            .iter()
            .any(|&oop| !oop.is_null() && !state.heap.contains(oop))
        {
            return Err(invalid());
        }
        for node in plan.nodes() {
            if let Node::Aggregate {
                vtable_input,
                fields,
            } = node
            {
                let vtable = inputs[*vtable_input as usize];
                if !state.vtables.is_registered(vtable)
                    || !state.vtables.has_layout(vtable)
                    || state.vtables.trace_hook(vtable).is_some()
                    // Empty aggregates must not manufacture an undersized
                    // instance of a raw numeric or runtime control carrier.
                    || state.vtables.slot_kind(vtable, 0) != crate::abi::SLOT_OOP
                    || fields
                        .iter()
                        .enumerate()
                        .any(|(i, _)| state.vtables.slot_kind(vtable, i) != crate::abi::SLOT_OOP)
                {
                    return Err(invalid());
                }
            }
        }
        Ok(())
    }

    /// The callback runs while every input and constructed identity is rooted.
    /// It must install its own ownership before returning any of these Oops.
    fn with_materialized_graph<T>(
        &self,
        plan: &Plan,
        inputs: &[Oop],
        admit: impl Fn() -> Result<(), ExecutionError>,
        publish: impl FnOnce(&[Oop]) -> Result<T, ExecutionError>,
    ) -> Result<T, ExecutionError> {
        self.assert_owner();
        admit()?;
        self.validate_graph_storage(plan, inputs)?;
        let mut roots = RootBatch(self, Vec::with_capacity(inputs.len() + plan.nodes().len()));
        for &value in inputs {
            roots.retain(value);
        }
        let mut nodes = Vec::with_capacity(plan.nodes().len());
        for node in plan.nodes() {
            self.require_allocation_allowed();
            #[cfg(test)]
            if COLLECT_EACH_NODE.get() {
                self.gc_collect_from_native_range(0, 0);
            }
            self.maybe_collect_before_allocation();
            admit()?;
            // Collection can invoke foreign callbacks. Recheck layouts after
            // that boundary, before allocating or writing the next object.
            self.validate_graph_storage(plan, inputs)?;
            let value = {
                let state = unsafe { &mut *self.state.get() };
                match node {
                    Node::Integer(value) => {
                        let storage = self.integer_storage();
                        let object = state.heap.allocate(storage.payload_size, Permanence::Collectable);
                        assert!(state.heap.set_vtable(object, self.integer_behavior()));
                        unsafe { (storage.initialize)(object.cast(), *value) };
                        object
                    },
                    Node::FloatBits(bits) => {
                        let storage = self.float_storage();
                        let object = state.heap.allocate(storage.payload_size, Permanence::Collectable);
                        assert!(state.heap.set_vtable(object, self.float_behavior()));
                        unsafe { (storage.initialize_bits)(object.cast(), *bits) };
                        object
                    },
                    Node::Aggregate {
                        vtable_input,
                        fields,
                    } => {
                        let value = state.heap.allocate(
                            fields.len() * core::mem::size_of::<Oop>(),
                            Permanence::Collectable,
                        );
                        assert!(state.heap.set_vtable(value, inputs[*vtable_input as usize]));
                        for (index, &field) in fields.iter().enumerate() {
                            assert!(state.heap.write_payload_word(
                                value,
                                index,
                                resolve(field, inputs, &nodes).addr(),
                            ));
                        }
                        value
                    }
                }
            };
            // No collecting allocation or foreign callback separates the new
            // object from this root; previously built nodes stay rooted too.
            roots.retain(value);
            nodes.push(value);
        }
        let slots: Vec<_> = plan
            .slots()
            .iter()
            .map(|&v| resolve(v, inputs, &nodes))
            .collect();
        admit()?;
        publish(&slots)
    }

    fn recover_graph(&self, machine: Oop, pc: usize, recipe: Oop) -> Result<(), ExecutionError> {
        let capability = self.stackless_machine_capability(machine)?;
        let count = {
            let requests = self.recovery_requests.borrow();
            let request = requests
                .last()
                .filter(|r| !r.consumed && r.authority.machine == capability)
                .ok_or_else(invalid)?;
            if pc >= request.authority.descriptor.program_counter_count() {
                return Err(invalid());
            }
            request.authority.descriptor.slot_count()
        };
        // Snapshot through ordinary size/at: sends. Keep every returned carrier
        // rooted, including carriers whose methods allocate or run the GC.
        let mut reader = Reader::new(self, recipe)?;
        let parts = reader.vector(recipe, 3)?;
        let [input_object, node_object, slot_object] = parts.as_slice() else {
            return Err(invalid());
        };
        let inputs = reader.vector(*input_object, graph::MAX_INPUTS)?;
        let encoded_nodes = reader.vector(*node_object, graph::MAX_NODES)?;
        let encoded_slots = reader.vector(*slot_object, graph::MAX_SLOTS)?;
        if encoded_slots.len() != count {
            return Err(invalid());
        }
        let slots = encoded_slots
            .iter()
            .map(|&v| reader.reference(v))
            .collect::<Result<_, _>>()?;
        let mut nodes = Vec::with_capacity(encoded_nodes.len());
        let mut remaining_fields = graph::MAX_FIELDS;
        for node in encoded_nodes {
            let fields = reader.vector(node, 3)?;
            let Some(&kind) = fields.first() else {
                return Err(invalid());
            };
            nodes.push(match (reader.integer(kind)?, fields.as_slice()) {
                (0, [_, value]) => Node::Integer(reader.integer(*value)?),
                (1, [_, value]) if self.is_float(*value) && reader.word_sized(*value) => {
                    Node::FloatBits(self.float_value(*value).to_bits())
                }
                (2, [_, vtable, values]) => {
                    let vtable_input =
                        u32::try_from(reader.integer(*vtable)?).map_err(|_| invalid())?;
                    let fields = reader.vector(*values, remaining_fields)?;
                    remaining_fields -= fields.len();
                    Node::Aggregate {
                        vtable_input,
                        fields: fields
                            .iter()
                            .map(|&v| reader.reference(v))
                            .collect::<Result<_, _>>()?,
                    }
                }
                _ => return Err(invalid()),
            });
        }
        let plan = Graph {
            input_count: inputs.len(),
            nodes,
            slots,
        }
        .validate()
        .map_err(|_| invalid())?;
        // A recipe callback may have consumed the request. Reject before any
        // construction, and let ordinary recovery revalidate the complete
        // authority once more after potentially collecting materialization.
        let admit = || {
            let requests = self.recovery_requests.borrow();
            let request = requests
                .last()
                .filter(|r| !r.consumed && r.authority.machine == capability)
                .ok_or_else(invalid)?;
            unsafe {
                (*self.state.get())
                    .execution
                    .validate_recovery_authority(current_thread_token(), &request.authority)
            }
        };
        self.with_materialized_graph(&plan, &inputs, admit, |slots| {
            self.recover_slots(capability, pc, slots)
        })
    }
}

/// Graph encoding: [inputs, nodes, slots]. Node encodings: [0, integer],
/// [1, float], [2, vtable-input-index, fields]. References >= 0 select inputs;
/// negative references select node (-reference - 1). All sequences are ordinary
/// zero-based size/at: objects; no particular collection implementation is used.
struct Reader<'a> {
    runtime: &'a Runtime,
    roots: RootBatch<'a>,
    size: Oop,
    at: Oop,
}

impl<'a> Reader<'a> {
    fn new(runtime: &'a Runtime, recipe: Oop) -> Result<Self, ExecutionError> {
        let mut roots = RootBatch(runtime, Vec::new());
        roots.retain(recipe);
        let size = runtime.intern(b"size").map_err(|_| invalid())?;
        roots.retain(size);
        let at = runtime.intern(b"at:").map_err(|_| invalid())?;
        roots.retain(at);
        Ok(Self {
            runtime,
            roots,
            size,
            at,
        })
    }

    fn word_sized(&self, oop: Oop) -> bool {
        unsafe { (*self.runtime.state.get()).heap.payload_size(oop) }.is_some_and(|n| n >= 8)
    }

    fn integer(&self, oop: Oop) -> Result<i64, ExecutionError> {
        if !self.runtime.is_integer(oop) || !unsafe { (*self.runtime.state.get()).heap.payload_size(oop) }.is_some_and(|size| size >= self.runtime.integer_storage().payload_size) {
            return Err(invalid());
        }
        Ok(self.runtime.integer_value(oop))
    }

    fn reference(&self, oop: Oop) -> Result<Value, ExecutionError> {
        let value = self.integer(oop)?;
        if value >= 0 {
            Ok(Value::Input(u32::try_from(value).map_err(|_| invalid())?))
        } else {
            Ok(Value::Node(
                u32::try_from(-(value + 1)).map_err(|_| invalid())?,
            ))
        }
    }

    fn vector(&mut self, object: Oop, limit: usize) -> Result<Vec<Oop>, ExecutionError> {
        let size = self.runtime.send_slice(object, self.size, &[]);
        let size = usize::try_from(self.integer(size)?).map_err(|_| invalid())?;
        if size > limit {
            return Err(invalid());
        }
        let mut values = Vec::with_capacity(size);
        for index in 0..size {
            let index = self.runtime.make_integer(index as i64);
            let value = self.runtime.send_slice(object, self.at, &[index]);
            self.roots.retain(value);
            values.push(value);
        }
        Ok(values)
    }
}

pub(in crate::runtime) unsafe extern "C" fn machine_recover_graph(
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
        match runtime.recover_graph(machine, pc, unsafe { args.add(1).read() }) {
            Ok(()) => machine,
            Err(_) => runtime.canonical().nil,
        }
    })
}

#[cfg(test)]
thread_local! {
    static COLLECT_EACH_NODE: core::cell::Cell<bool> = const { core::cell::Cell::new(false) };
}

#[cfg(test)]
mod tests;
