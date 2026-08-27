//! Direct synchronous C lowering from admitted PortableProgram semantics.
//!
//! This backend never reconstructs source or resolves lexical names. Binding
//! indices name C locals, float payloads remain bit exact, and nested capture
//! operands are copied in their canonical order.

use birnam_artifact::portable_program::{self, NodeKind, StatementKind};

use crate::TranslationUnit;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidProgram,
    QuantityOverflow,
}

#[derive(Debug, PartialEq, Eq)]
pub struct CompiledSynchronousBlockV1 {
    pub c_source: Vec<u8>,
    pub entry_symbol: Vec<u8>,
    pub selector: Vec<u8>,
    pub language_arity: u32,
    pub capture_count: u32,
    pub function_count: u32,
}

struct Compiler {
    functions: Vec<u8>,
    next_function: u32,
}

/// Compile one canonical Block program and all nested synchronous Block
/// programs into a self-contained JIT translation unit. The namespace remains
/// an input to the frozen compiler contract even though this first bounded
/// implementation gives nested literals fresh collectable vtables.
pub fn compile_synchronous_block_v1(
    bytes: &[u8],
    _cache_namespace: u32,
) -> Result<CompiledSynchronousBlockV1, Error> {
    let program = portable_program::inspect(bytes).map_err(|_| Error::InvalidProgram)?;
    let mut compiler = Compiler {
        functions: Vec::new(),
        next_function: 1,
    };
    compiler.compile_program(bytes, 0)?;

    let mut unit = TranslationUnit::new(false);
    unit.push_function(&compiler.functions);
    unit.push_body(b"return birnam_nil();\n");
    Ok(CompiledSynchronousBlockV1 {
        c_source: unit.finish(),
        entry_symbol: b"__birnam_portable_block_0".to_vec(),
        selector: block_selector(program.parameter_count)?,
        language_arity: program.parameter_count,
        capture_count: program.capture_count,
        function_count: compiler.next_function,
    })
}

impl Compiler {
    fn compile_program(&mut self, bytes: &[u8], function_index: u32) -> Result<(), Error> {
        let program = portable_program::inspect(bytes).map_err(|_| Error::InvalidProgram)?;
        let mut body = Vec::new();
        body.extend_from_slice(b"(void)n;\n");
        for index in 0..program.parameter_count {
            body.extend_from_slice(b"void* __b");
            decimal(&mut body, index);
            body.extend_from_slice(b"=args[");
            decimal(&mut body, index);
            body.extend_from_slice(b"];\n");
        }
        for capture in 0..program.capture_count {
            let binding = program
                .parameter_count
                .checked_add(capture)
                .ok_or(Error::QuantityOverflow)?;
            body.extend_from_slice(b"void* __b");
            decimal(&mut body, binding);
            body.extend_from_slice(b"=((void**)self)[");
            decimal(
                &mut body,
                capture.checked_add(1).ok_or(Error::QuantityOverflow)?,
            );
            body.extend_from_slice(b"];\n");
        }

        let mut next_node = 0u32;
        let mut result_node = None;
        for statement_index in 0..program.statement_count as usize {
            let statement = program
                .statement(statement_index)
                .ok_or(Error::InvalidProgram)?;
            while next_node <= statement.root_node {
                self.emit_node(&program, next_node, &mut body)?;
                next_node = next_node.checked_add(1).ok_or(Error::QuantityOverflow)?;
            }
            if statement.kind == StatementKind::Let {
                let binding = statement.binding_index.ok_or(Error::InvalidProgram)?;
                body.extend_from_slice(b"void* __b");
                decimal(&mut body, binding);
                body.extend_from_slice(b"=__n");
                decimal(&mut body, statement.root_node);
                body.extend_from_slice(b";\n");
            }
            result_node = Some(statement.root_node);
        }
        if next_node != program.node_count {
            return Err(Error::InvalidProgram);
        }
        body.extend_from_slice(b"return ");
        if let Some(result) = result_node {
            body.extend_from_slice(b"__n");
            decimal(&mut body, result);
        } else {
            body.extend_from_slice(b"birnam_nil()");
        }
        body.extend_from_slice(b";\n");

        self.functions
            .extend_from_slice(b"void* __birnam_portable_block_");
        decimal(&mut self.functions, function_index);
        self.functions
            .extend_from_slice(b"(void* self,void** args,long n){(void)self;(void)args;\n");
        self.functions.extend_from_slice(&body);
        self.functions.extend_from_slice(b"}\n");
        Ok(())
    }

    fn emit_node(
        &mut self,
        program: &portable_program::Program<'_>,
        index: u32,
        output: &mut Vec<u8>,
    ) -> Result<(), Error> {
        let node = program.node(index as usize).ok_or(Error::InvalidProgram)?;
        let kind = node.semantic_kind().ok_or(Error::InvalidProgram)?;
        output.extend_from_slice(b"void* __n");
        decimal(output, index);
        output.extend_from_slice(b"=");
        match kind {
            NodeKind::Integer => {
                output.extend_from_slice(b"birnam_mkSmallInt(");
                output.extend_from_slice((node.payload as i64).to_string().as_bytes());
                output.extend_from_slice(b"L)");
            }
            NodeKind::FloatBits => {
                output.extend_from_slice(b"({union{unsigned long long u;double d;}v={0x");
                output.extend_from_slice(format!("{:016x}", node.payload).as_bytes());
                output.extend_from_slice(b"ULL};birnam_mkFloat(v.d);})");
            }
            NodeKind::String => emit_bytes_constructor(output, b"birnam_mkString", node.data),
            NodeKind::Symbol => emit_named_call(output, b"birnam_intern", node.data),
            NodeKind::Nil => output.extend_from_slice(b"birnam_nil()"),
            NodeKind::True => output.extend_from_slice(b"birnam_true()"),
            NodeKind::False => output.extend_from_slice(b"birnam_false()"),
            NodeKind::LexicalRead => {
                output.extend_from_slice(b"__b");
                decimal(output, node.auxiliary_0);
            }
            NodeKind::GlobalRead => emit_named_call(output, b"birnam_global", node.data),
            NodeKind::Send => {
                output.extend_from_slice(b"birnam_send(__n");
                decimal(
                    output,
                    program
                        .node_operand(index as usize, 0)
                        .ok_or(Error::InvalidProgram)?,
                );
                output.extend_from_slice(b",birnam_intern(\"");
                push_c_string_body(output, node.data);
                output.extend_from_slice(b"\"),");
                if node.operand_count == 1 {
                    output.extend_from_slice(b"(void**)0");
                } else {
                    output.extend_from_slice(b"(void*[]){");
                    for operand in 1..node.operand_count as usize {
                        if operand != 1 {
                            output.push(b',');
                        }
                        output.extend_from_slice(b"__n");
                        decimal(
                            output,
                            program
                                .node_operand(index as usize, operand)
                                .ok_or(Error::InvalidProgram)?,
                        );
                    }
                    output.push(b'}');
                }
                output.push(b',');
                decimal(output, node.operand_count - 1);
                output.extend_from_slice(b"L)");
            }
            NodeKind::BlockLiteral => self.emit_block_literal(program, index, node.data, output)?,
        }
        output.extend_from_slice(b";\n");
        Ok(())
    }

    fn emit_block_literal(
        &mut self,
        parent: &portable_program::Program<'_>,
        node_index: u32,
        nested_bytes: &[u8],
        output: &mut Vec<u8>,
    ) -> Result<(), Error> {
        let nested = portable_program::inspect(nested_bytes).map_err(|_| Error::InvalidProgram)?;
        let function_index = self.next_function;
        self.next_function = self
            .next_function
            .checked_add(1)
            .ok_or(Error::QuantityOverflow)?;
        self.compile_program(nested_bytes, function_index)?;
        output.extend_from_slice(
            b"({void* vt=birnam_proto(birnam_blockVt());if(vt){birnam_method(vt,birnam_intern(\"",
        );
        push_block_selector(output, nested.parameter_count)?;
        output.extend_from_slice(b"\"),(void*)&__birnam_portable_block_");
        decimal(output, function_index);
        output.extend_from_slice(b");birnam_layoutAllOops(vt);}static const unsigned char pp[]={");
        for (index, byte) in nested_bytes.iter().enumerate() {
            if index != 0 {
                output.push(b',');
            }
            output.extend_from_slice(byte.to_string().as_bytes());
        }
        output.extend_from_slice(
            b"};void* co=birnam_portable_code_admit_v1(pp,sizeof(pp));void* captures[",
        );
        decimal(output, nested.capture_count.max(1));
        output.extend_from_slice(b"];");
        for capture in 0..nested.capture_count as usize {
            output.extend_from_slice(b"captures[");
            decimal(output, capture as u32);
            output.extend_from_slice(b"]=__b");
            decimal(
                output,
                parent
                    .node_operand(node_index as usize, capture)
                    .ok_or(Error::InvalidProgram)?,
            );
            output.push(b';');
        }
        output.extend_from_slice(
            b"(vt&&co&&!birnam_is_error(co))?birnam_portable_materialized_block_new_v2(vt,co,",
        );
        if nested.capture_count == 0 {
            output.extend_from_slice(b"(void**)0");
        } else {
            output.extend_from_slice(b"captures");
        }
        output.push(b',');
        decimal(output, nested.capture_count);
        output.extend_from_slice(
            b"UL,(void*)(unsigned long)__birnam_code_generation_identity):birnam_nil();})",
        );
        Ok(())
    }
}

fn block_selector(arity: u32) -> Result<Vec<u8>, Error> {
    let mut output = Vec::new();
    push_block_selector(&mut output, arity)?;
    Ok(output)
}

fn push_block_selector(output: &mut Vec<u8>, arity: u32) -> Result<(), Error> {
    if arity == 0 {
        output.extend_from_slice(b"value");
        return Ok(());
    }
    for _ in 0..arity {
        output.extend_from_slice(b"value:");
    }
    Ok(())
}

fn emit_named_call(output: &mut Vec<u8>, function: &[u8], name: &[u8]) {
    output.extend_from_slice(function);
    output.extend_from_slice(b"(\"");
    push_c_string_body(output, name);
    output.extend_from_slice(b"\")");
}

fn emit_bytes_constructor(output: &mut Vec<u8>, function: &[u8], bytes: &[u8]) {
    output.extend_from_slice(function);
    output.extend_from_slice(b"(\"");
    push_c_string_body(output, bytes);
    output.extend_from_slice(b"\",");
    output.extend_from_slice(bytes.len().to_string().as_bytes());
    output.extend_from_slice(b"L)");
}

fn decimal(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(value.to_string().as_bytes());
}

fn push_c_string_body(output: &mut Vec<u8>, bytes: &[u8]) {
    for &byte in bytes {
        match byte {
            b'"' => output.extend_from_slice(br#"\""#),
            b'\\' => output.extend_from_slice(br#"\\"#),
            b'\n' => output.extend_from_slice(br#"\n"#),
            b'\r' => output.extend_from_slice(br#"\r"#),
            b'\t' => output.extend_from_slice(br#"\t"#),
            0x20..=0x7e => output.push(byte),
            other => output.extend_from_slice(&[
                b'\\',
                b'0' + ((other >> 6) & 7),
                b'0' + ((other >> 3) & 7),
                b'0' + (other & 7),
            ]),
        }
    }
}

#[cfg(test)]
mod tests {
    use birnam_artifact::portable_program::{
        LetSpec, NodeSpec, ProgramSpec, StatementSpec, encode,
    };

    use super::*;

    #[test]
    fn emits_explicit_bindings_float_bits_and_nested_capture_operands() {
        let nested = encode(ProgramSpec {
            parameters: &[],
            captures: &[b"outer"],
            lets: &[],
            statements: &[StatementSpec::Expression { root_node: 0 }],
            nodes: &[NodeSpec::LexicalRead(0)],
        })
        .unwrap();
        let program = encode(ProgramSpec {
            parameters: &[b"same"],
            captures: &[],
            lets: &[LetSpec {
                name: b"same",
                defining_statement: 0,
            }],
            statements: &[
                StatementSpec::Let {
                    root_node: 0,
                    let_index: 0,
                },
                StatementSpec::Expression { root_node: 1 },
            ],
            nodes: &[
                NodeSpec::FloatBits(0x7ff8_0000_0000_1234),
                NodeSpec::BlockLiteral {
                    program: &nested,
                    captures: &[0],
                },
            ],
        })
        .unwrap();

        let compiled = compile_synchronous_block_v1(&program, 17).unwrap();
        let c = String::from_utf8(compiled.c_source).unwrap();
        assert!(c.contains("void* __b0=args[0]"));
        assert!(c.contains("void* __b1=__n0"));
        assert!(c.contains("0x7ff8000000001234ULL"));
        assert!(c.contains("captures[0]=__b0"));
        assert!(c.contains("__birnam_portable_block_1"));
        assert_eq!(compiled.function_count, 2);
    }
}
