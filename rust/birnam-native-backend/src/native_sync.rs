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
    for i in 0..program.capture_count {
        let binding = program.parameter_count + i;
        // Ask the Block owner, rather than inspecting its payload or dispatching
        // an overridable inspection method on the particular Block instance.
        writeln!(out, "A capture_{i}[2]={{word({i}),obj(self)}};O b{binding}=call(&c,named(&c,\"Block\"),\"captureWord:of:\",capture_{i},2);").unwrap();
    }
    let mut next = 0;
    let word_plans = if policy.optimizations.words() {super::word_expression::plans(&program)} else {Default::default()};
    let abi_plans = if policy.optimizations.abi() {super::abi_arguments::plans(&program)} else {Default::default()};
    let skipped: std::collections::BTreeSet<_> = word_plans.values().flat_map(|p|p.nodes.iter().copied()).chain(abi_plans.values().flat_map(|p|p.nodes.iter().copied())).collect();
    let mut result = None;
    for i in 0..program.statement_count as usize {
        let statement = program.statement(i).ok_or(Error::Portable)?;
        while next <= statement.root_node {
            let node = program.node(next as usize).ok_or(Error::Portable)?;
            if let Some(plan) = abi_plans.get(&(next as usize)) {
                let args=plan.arguments.iter().map(|i|format!("obj(v{i})")).collect::<Vec<_>>().join(",");
                writeln!(out,"A na{next}[]={{{args}}};R nr{next}=raw(&c,v{},\"tryNativeArguments:\",na{next},{});if(nr{next}.status==6){{c.failure=(R){{0}};nr{next}=ok((A){{0}});}}if(c.failure.status)return finish(&c,(A){{0}});O v{next}=0;if(nr{next}.value.kind==0){{",plan.receiver,plan.arguments.len()).unwrap();
                fallback(&mut out,&program,&plan.nodes,&std::collections::BTreeMap::new())?;
                writeln!(out,"}}else{{v{next}=keep(&c,object(&c,nr{next}.value));}}if(c.failure.status)return finish(&c,(A){{0}});").unwrap();
                next+=1;continue;
            }
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
                    let child = portable_function(node.data, source, filename, functions, policy)?;
                    let captures = (0..node.operand_count as usize).map(|i| {
                        program.node_operand(next as usize, i).map(|binding| format!("obj(b{binding})"))
                            .ok_or(Error::Portable)
                    }).collect::<Result<Vec<_>, _>>()?;
                    let suffix = if captures.is_empty() { String::new() } else { format!(",{}", captures.join(",")) };
                    writeln!(out, "A a{next}[]={{obj(methods[{child}]),obj(codes[{child}]){suffix}}};").unwrap();
                    format!("call(&c,named(&c,\"Block\"),\"fromMethod:codeObject:captureOperands:\",a{next},{})",captures.len()+2)

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
