//! Synchronous PortableProgram emission with the current Id calling convention.
//! The canonical postorder traversal is the same as portable_c; all owner
//! operations, including captures and nested Block construction, are sends.
use super::*;
use birnam_artifact::portable_program::{self, NodeKind, StatementKind};

pub(super) fn function(bytes: &[u8], functions: &mut Vec<Function>) -> Result<usize, Error> {
    let program = portable_program::inspect(bytes).map_err(|_| Error::Portable)?;
    let index = functions.len();
    functions.push(Function {
        portable: Some(bytes.to_vec()),
        synchronous: true,
        body: String::new(),
        descriptor: vec![],
        selection: vec![],
        named: false,
        discarded: None,
        predicate: None,
    });
    let mut out = format!(
        "static R sync_{index}(void*context,O self,const A*args,size_t n){{(void)context;Call c={{0}};keep(&c,self);if(n!={})return finish(&c,obj(named(&c,\"nil\")));\n",
        program.parameter_count
    );
    for i in 0..program.parameter_count {
        writeln!(out, "O b{i}=keep(&c,object(&c,args[{i}]));").unwrap();
    }
    for i in 0..program.capture_count {
        let binding = program.parameter_count + i;
        // Ask the Block owner, rather than inspecting its payload or dispatching
        // an overridable inspection method on the particular Block instance.
        writeln!(out, "A capture_{i}[2]={{obj(integer(&c,{i})),obj(self)}};O b{binding}=call(&c,named(&c,\"Block\"),\"captureAt:of:\",capture_{i},2);").unwrap();
    }
    let mut next = 0;
    let mut result = None;
    for i in 0..program.statement_count as usize {
        let statement = program.statement(i).ok_or(Error::Portable)?;
        while next <= statement.root_node {
            let node = program.node(next as usize).ok_or(Error::Portable)?;
            let value = match node.semantic_kind().ok_or(Error::Portable)? {
                NodeKind::Integer => format!("integer(&c,0x{:016x}ULL)", node.payload),
                NodeKind::FloatBits => format!("float_bits(&c,0x{:016x}ULL)", node.payload),
                NodeKind::String => format!("string(&c,{},{})", quoted(node.data), node.data.len()),
                NodeKind::Symbol => format!("keep(&c,intern(&c,{}))", quoted(node.data)),
                NodeKind::Nil => "named(&c,\"nil\")".into(),
                NodeKind::True => "named(&c,\"true\")".into(),
                NodeKind::False => "named(&c,\"false\")".into(),
                NodeKind::LexicalRead => format!("b{}", node.auxiliary_0),
                NodeKind::GlobalRead => format!("global(&c,{})", quoted(node.data)),
                NodeKind::Send => {
                    let receiver = program
                        .node_operand(next as usize, 0)
                        .ok_or(Error::Portable)?;
                    let args = (1..node.operand_count as usize)
                        .map(|j| {
                            program
                                .node_operand(next as usize, j)
                                .map(|v| format!("obj(v{v})"))
                                .ok_or(Error::Portable)
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    let arg = if args.is_empty() {
                        "0".into()
                    } else {
                        writeln!(out, "A a{next}[]={{{}}};", args.join(",")).unwrap();
                        format!("a{next}")
                    };
                    format!(
                        "call(&c,v{receiver},{},{arg},{})",
                        quoted(node.data),
                        args.len()
                    )
                }
                NodeKind::BlockLiteral => {
                    let child = function(node.data, functions)?;
                    writeln!(
                        out,
                        "O captures{next}=call(&c,named(&c,\"Vector\"),\"new\",0,0);"
                    )
                    .unwrap();
                    for i in 0..node.operand_count as usize {
                        let binding = program
                            .node_operand(next as usize, i)
                            .ok_or(Error::Portable)?;
                        writeln!(out, "{{A a=obj(b{binding});captures{next}=call(&c,captures{next},\"add:\",&a,1);}}").unwrap();
                    }
                    writeln!(out,"A a{next}[3]={{obj(methods[{child}]),obj(codes[{child}]),obj(captures{next})}};").unwrap();
                    format!(
                        "call(&c,named(&c,\"Block\"),\"fromMethod:codeObject:captures:\",a{next},3)"
                    )
                }
            };
            writeln!(
                out,
                "O v{next}={value};if(c.failure.status)return finish(&c,(A){{0}});"
            )
            .unwrap();
            next += 1;
        }
        if statement.kind == StatementKind::Let {
            writeln!(
                out,
                "O b{}=v{};",
                statement.binding_index.ok_or(Error::Portable)?,
                statement.root_node
            )
            .unwrap();
        }
        result = Some(statement.root_node);
    }
    let result = result
        .map(|v| format!("v{v}"))
        .unwrap_or_else(|| "named(&c,\"nil\")".into());
    writeln!(out, "return finish(&c,obj({result}));}}").unwrap();
    functions[index].body = out;
    Ok(index)
}
