//! Synchronous PortableProgram emission with the current Id calling convention.
//! Traversal follows canonical PortableProgram postorder; all owner
//! operations, including captures and nested Block construction, are sends.
use super::*;
use birnam_artifact::portable_program::{self, NodeKind, StatementKind};

pub(super) fn function(
    bytes: &[u8],
    source: &[u8],
    filename: &[u8],
    functions: &mut Vec<Function>,
    policy: Policy<'_>,
) -> Result<usize, Error> {
    let program = portable_program::inspect(bytes).map_err(|_| Error::Portable)?;
    let index = functions.len();
    functions.push(Function {
        portable: Some(bytes.to_vec()),
        synchronous: true,
        body: String::new(),
        descriptor: vec![],
        call_plans: None,
        selection: vec![],
        named: false,
        discarded: None,
        predicate: None,
        inline_return: None,
        word_helpers: policy.optimizations.helpers(),
        batch_cleanup: policy.optimizations.roots(),
    });
    let entry = if policy.optimizations.inputs() { "check_entry" } else { "keep_entry" };
    let mut out = format!(
        "static R sync_{index}(void*context,O self,const A*args,size_t n){{(void)context;Call c={{0}};if(n!={}){{object(&c,obj(self));return finish(&c,obj(named(&c,\"nil\")));}}{entry}(&c,self,args,n);\n",
        program.parameter_count
    );
    for i in 0..program.parameter_count {
        writeln!(out, "O b{i}=object(&c,args[{i}]);").unwrap();
    }
    if program.capture_count != 0 {
        writeln!(out, "O captures[{}];load_captures(&c,self,captures,{});if(c.failure.status)return finish(&c,(A){{0}});",program.capture_count,program.capture_count).unwrap();
        for i in 0..program.capture_count {
            let binding = program.parameter_count + i;
            writeln!(out, "O b{binding}=captures[{i}];").unwrap();
        }
    }
    let mut next = 0;
    let word_plans = if policy.optimizations.words() {super::word_expression::plans(&program)} else {Default::default()};
    let skipped: std::collections::BTreeSet<_> = word_plans.values().flat_map(|p|p.nodes.iter().copied()).collect();
    let mut result = None;
    for i in 0..program.statement_count as usize {
        let statement = program.statement(i).ok_or(Error::Portable)?;
        while next <= statement.root_node {
            let node = program.node(next as usize).ok_or(Error::Portable)?;
            if let Some(plan) = word_plans.get(&(next as usize)) {
                let data=plan.bytes.iter().map(|b|b.to_string()).collect::<Vec<_>>().join(",");
                writeln!(out,"static const unsigned char wp{next}[]={{{data}}};A wa{next}[{}];wa{next}[0]=bytes(wp{next},sizeof(wp{next}));",plan.bindings.len()+1).unwrap();
                for (j,binding) in plan.bindings.iter().enumerate(){writeln!(out,"wa{next}[{}]=obj(b{binding});",j+1).unwrap();}
                writeln!(out,"R wr{next}=raw(&c,named(&c,\"Word\"),\"tryPrimitiveExpression:\",wa{next},{});if(wr{next}.status==6){{c.failure=(R){{0}};wr{next}=ok((A){{0}});}}if(c.failure.status)return finish(&c,(A){{0}});O v{next}=0;if(wr{next}.value.kind==0){{",plan.bindings.len()+1).unwrap();
                fallback(&mut out,&program,&plan.nodes,&plan.aliases)?;
                writeln!(out,"}}else{{v{next}=keep(&c,object(&c,wr{next}.value));}}if(c.failure.status)return finish(&c,(A){{0}});").unwrap();
                next+=1;continue;
            }
            if skipped.contains(&(next as usize)) {
                writeln!(out,"O v{next}=0;").unwrap();next+=1;continue;
            }
            let value = match node.semantic_kind().ok_or(Error::Portable)? {
                NodeKind::Integer => format!("integer(&c,0x{:016x}ULL)", node.payload),
                NodeKind::FloatBits => format!("float_bits(&c,0x{:016x}ULL)", node.payload),
                NodeKind::String => format!("string(&c,{},{})", quoted(node.data), node.data.len()),
                NodeKind::Symbol => format!("literal_symbol(&c,{})", quoted(node.data)),
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
                    if i + 1 == program.statement_count as usize && next == statement.root_node {
                        writeln!(out, "return return_call(&c,v{receiver},{},{arg},{});}}", quoted(node.data), args.len()).unwrap();
                        functions[index].body = out;
                        return Ok(index);
                    }
                    format!(
                        "call(&c,v{receiver},{},{arg},{})",
                        quoted(node.data),
                        args.len()
                    )
                }
                NodeKind::BlockLiteral => {
                    let child = portable_function(node.data, source, filename, functions, policy)?;
                    let captures = (0..node.operand_count as usize).map(|i| {
                        program.node_operand(next as usize, i).map(|binding| format!("obj(b{binding})"))
                            .ok_or(Error::Portable)
                    }).collect::<Result<Vec<_>, _>>()?;
                    let suffix = if captures.is_empty() { String::new() } else { format!(",{}", captures.join(",")) };
                    writeln!(out, "A a{next}[]={{obj(methods[{child}]){suffix}}};").unwrap();
                    format!("call(&c,named(&c,\"Block\"),\"fromMethod:captureOperands:\",a{next},{})",captures.len()+1)

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

fn fallback(out: &mut String, program: &portable_program::Program<'_>, nodes: &std::collections::BTreeSet<usize>, aliases: &std::collections::BTreeMap<usize,usize>) -> Result<(),Error> {
    for &i in nodes {
        if let Some(source)=aliases.get(&i) {writeln!(out,"v{i}=v{source};").unwrap();continue;}
        let n=program.node(i).ok_or(Error::Portable)?;
        let receiver=program.node_operand(i,0).ok_or(Error::Portable)?;
        let args=(1..n.operand_count as usize).map(|j|program.node_operand(i,j).map(|v|format!("obj(v{v})")).ok_or(Error::Portable)).collect::<Result<Vec<_>,_>>()?;
        let arg=if args.is_empty(){"0".to_owned()}else{writeln!(out,"A wf{i}[]={{{}}};",args.join(",")).unwrap();format!("wf{i}")};
        writeln!(out,"v{i}=call(&c,v{receiver},{},{arg},{});if(c.failure.status)return finish(&c,(A){{0}});",quoted(n.data),args.len()).unwrap();
    }
    Ok(())
}
