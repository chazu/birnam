//! Process-local recovery descriptions. Validation establishes shape, not the
//! semantic permission to eliminate an allocation or replace an Id operation.

pub const MAX_INPUTS: usize = 128;
pub const MAX_NODES: usize = 64;
pub const MAX_SLOTS: usize = 128;
pub const MAX_FIELDS: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Value {
    Input(u32),
    Node(u32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Node {
    Integer(i64),
    FloatBits(u64),
    Aggregate {
        vtable_input: u32,
        fields: Vec<Value>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Graph {
    pub input_count: usize,
    pub nodes: Vec<Node>,
    pub slots: Vec<Value>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    Limit,
    InvalidInput,
    ForwardReference,
    InvalidOutput,
}

/// Only a validated graph can reach the materializer. Its immutable accessors
/// cannot introduce a forward edge after validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan(Graph);

impl Graph {
    pub fn validate(self) -> Result<Plan, Refusal> {
        if self.input_count > MAX_INPUTS
            || self.nodes.len() > MAX_NODES
            || self.slots.len() > MAX_SLOTS
        {
            return Err(Refusal::Limit);
        }
        let mut field_count = 0usize;
        for (index, node) in self.nodes.iter().enumerate() {
            if let Node::Aggregate {
                vtable_input,
                fields,
            } = node
            {
                if *vtable_input as usize >= self.input_count {
                    return Err(Refusal::InvalidInput);
                }
                field_count = field_count
                    .checked_add(fields.len())
                    .ok_or(Refusal::Limit)?;
                if field_count > MAX_FIELDS {
                    return Err(Refusal::Limit);
                }
                for &value in fields {
                    match value {
                        Value::Input(input) if input as usize >= self.input_count => {
                            return Err(Refusal::InvalidInput);
                        }
                        Value::Node(node) if node as usize >= index => {
                            return Err(Refusal::ForwardReference);
                        }
                        _ => {}
                    }
                }
            }
        }
        for &value in &self.slots {
            match value {
                Value::Input(input) if input as usize >= self.input_count => {
                    return Err(Refusal::InvalidInput);
                }
                Value::Node(node) if node as usize >= self.nodes.len() => {
                    return Err(Refusal::InvalidOutput);
                }
                _ => {}
            }
        }
        Ok(Plan(self))
    }
}

impl Plan {
    pub fn input_count(&self) -> usize {
        self.0.input_count
    }
    pub fn nodes(&self) -> &[Node] {
        &self.0.nodes
    }
    pub fn slots(&self) -> &[Value] {
        &self.0.slots
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_references_do_not_coalesce_equal_fresh_identities() {
        let graph = Graph {
            input_count: 1,
            nodes: vec![
                Node::Integer(3),
                Node::Integer(3),
                Node::Aggregate {
                    vtable_input: 0,
                    fields: vec![Value::Node(0), Value::Node(0), Value::Node(1)],
                },
            ],
            slots: vec![Value::Node(2), Value::Node(2)],
        };
        let plan = graph.clone().validate().unwrap();
        assert_eq!(plan.nodes(), &graph.nodes);
        assert_eq!(plan.slots(), &graph.slots);
    }

    #[test]
    fn rejects_cycles_missing_inputs_and_missing_outputs() {
        for reference in [Value::Node(0), Value::Node(1), Value::Node(u32::MAX)] {
            let graph = Graph {
                input_count: 1,
                nodes: vec![Node::Aggregate {
                    vtable_input: 0,
                    fields: vec![reference],
                }],
                slots: vec![],
            };
            assert_eq!(graph.validate(), Err(Refusal::ForwardReference));
        }
        for reference in [Value::Input(0), Value::Node(0)] {
            let graph = Graph {
                input_count: 0,
                nodes: vec![],
                slots: vec![reference],
            };
            assert!(graph.validate().is_err());
        }
        let graph = Graph {
            input_count: 0,
            nodes: vec![Node::Aggregate {
                vtable_input: 0,
                fields: vec![],
            }],
            slots: vec![],
        };
        assert_eq!(graph.validate(), Err(Refusal::InvalidInput));
    }

    #[test]
    fn bounds_total_fields_as_well_as_nodes_inputs_and_slots() {
        for graph in [
            Graph {
                input_count: MAX_INPUTS + 1,
                nodes: vec![],
                slots: vec![],
            },
            Graph {
                input_count: 0,
                nodes: vec![Node::Integer(0); MAX_NODES + 1],
                slots: vec![],
            },
            Graph {
                input_count: 1,
                nodes: vec![],
                slots: vec![Value::Input(0); MAX_SLOTS + 1],
            },
            Graph {
                input_count: 1,
                nodes: vec![
                    Node::Aggregate {
                        vtable_input: 0,
                        fields: vec![Value::Input(0); MAX_FIELDS / 2 + 1],
                    };
                    2
                ],
                slots: vec![],
            },
        ] {
            assert_eq!(graph.validate(), Err(Refusal::Limit));
        }
    }
}
