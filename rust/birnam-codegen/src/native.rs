//! Production emission uses the retained portable lowering, slot coloring and
//! descriptor validator, then spells all inter-owner operations as Id messages.
use crate::{
    resumable::{self as ir, Literal, OperationKind as Op, Terminator as Term},
    resumable_descriptor,
};
use birnam_artifact::stackless_selection::{
    self as selection, Disposition, ImplementationKind, RootKind, SelectionSpec,
};
use sha2::{Digest, Sha256};
use std::fmt::Write;
#[derive(Debug)]
pub enum Error {
    Portable,
    Lower,
    Layout,
    Descriptor,
    Selection,
    Unsupported,
}
struct Function {
    body: String,
    descriptor: Vec<u8>,
    selection: Vec<u8>,
    named: bool,
}
fn entry_prefix(name: &str) -> String {
    format!(
        "static R {name}(void*context,O machine,const A*args,size_t n){{(void)context;if(n!=1)return bad();Call c={{0}};keep(&c,machine);if(args[0].kind!=1)return finish(&c,(A){{0}});keep(&c,args[0].data.object);A pc=send(&c,named(&c,\"Integer\"),\"wordOf:\",args,1);if(pc.kind!=2)return finish(&c,(A){{0}});\n"
    )
}
fn quoted(b: &[u8]) -> String {
    let mut v = vec![b'"'];
    crate::c_string::push_c_string_body(&mut v, b);
    v.push(b'"');
    String::from_utf8(v).unwrap()
}
fn literal(l: &Literal<'_>) -> Result<String, Error> {
    Ok(match l {
        Literal::SmallInt(v) => format!("integer(&c,(size_t){}ULL)", *v as u64),
        Literal::SmallIntSource(v) => format!(
            "integer(&c,(size_t){}ULL)",
            std::str::from_utf8(v)
                .map_err(|_| Error::Unsupported)?
                .parse::<i64>()
                .map_err(|_| Error::Unsupported)? as u64
        ),
        Literal::Float(v) => format!(
            "float_bits(&c,{}ULL)",
            std::str::from_utf8(v)
                .map_err(|_| Error::Unsupported)?
                .parse::<f64>()
                .map_err(|_| Error::Unsupported)?
                .to_bits()
        ),
        Literal::FloatBits(v) => format!("float_bits(&c,{v}ULL)"),
        Literal::String(v) => format!("string(&c,{},{}UL)", quoted(v), v.len()),
        Literal::StringSource(v) => {
            let b = crate::literal::decode_body(&v[1..v.len() - 1]);
            format!("string(&c,{},{}UL)", quoted(&b), b.len())
        }
        Literal::Symbol(v) => format!("keep(&c,intern(&c,{}))", quoted(v)),
        Literal::Nil => "named(&c,\"nil\")".into(),
        Literal::True => "named(&c,\"true\")".into(),
        Literal::False => "named(&c,\"false\")".into(),
    })
}
fn function(
    program: &ir::LogicalProgram<'_>,
    source: &[u8],
    functions: &mut Vec<Function>,
    named: bool,
) -> Result<usize, Error> {
    let index = functions.len();
    functions.push(Function {
        body: String::new(),
        descriptor: vec![],
        selection: vec![],
        named,
    });
    let layout = ir::color_slots(program).map_err(|_| Error::Layout)?;
    let revision = Sha256::digest(source).into();
    let path = [index as u32];
    let authority = SelectionSpec {
        source_revision: revision,
        root_kind: if named {
            RootKind::MethodOperation
        } else {
            RootKind::NonMethodProgram
        },
        root_index: 0,
        path: if named { &[] } else { &path },
        implementation_kind: if named {
            ImplementationKind::NamedAdapter
        } else {
            ImplementationKind::BlockBody
        },
        disposition: Disposition::ResumableRequired,
    };
    let descriptor = resumable_descriptor::compile_descriptor_v2(
        program,
        &layout,
        authority,
        b"<compiler>",
        source,
        1,
    )
    .map_err(|_| Error::Descriptor)?;
    let selected = selection::encode_artifact_selection(&[revision], &[authority])
        .map_err(|_| Error::Selection)?;
    let slot = |v: ir::ValueId| layout.value_slots[v.0 as usize];
    let mut out = entry_prefix(&format!("step_{index}"));
    out.push_str("switch(pc.data.word){\n");
    for block in &program.blocks {
        writeln!(out, "case {}:{{", block.pc.0).unwrap();
        for operation in &block.operations {
            let expr = match &operation.kind {
                Op::EntryReceiver
                | Op::EntryImplementation
                | Op::EntryArgument { .. }
                | Op::EntryCapture { .. } => continue,
                Op::Literal(v) => literal(v)?,
                Op::GlobalLookup(n) => format!("named(&c,{})", quoted(n)),
                Op::Bind { source, .. } => format!("get(&c,machine,{})", slot(*source)),
                Op::PortableBlockLiteral {
                    program, captures, ..
                } => {
                    let inner = ir::lower_portable_block(program).map_err(|_| Error::Lower)?;
                    let child = function(&inner, source, functions, false)?;
                    let v = operation.result.0;
                    writeln!(
                        out,
                        "O captures_{v}=call(&c,named(&c,\"Vector\"),\"new\",0,0);"
                    )
                    .unwrap();
                    for value in captures {
                        writeln!(out,"{{A a=obj(get(&c,machine,{}));captures_{v}=call(&c,captures_{v},\"add:\",&a,1);}}",slot(*value)).unwrap();
                    }
                    writeln!(
                        out,
                        "A block_args_{v}[2]={{obj(methods[{child}]),obj(captures_{v})}};"
                    )
                    .unwrap();
                    format!(
                        "call(&c,named(&c,\"Block\"),\"fromMethod:captures:\",block_args_{v},2)"
                    )
                }
                Op::BlockLiteral { .. } => return Err(Error::Unsupported),
            };
            writeln!(
                out,
                "O value_{}={expr};put(&c,machine,{},value_{});",
                operation.result.0,
                slot(operation.result),
                operation.result.0
            )
            .unwrap();
            if index == 0 && !named {
                if let Op::Bind { name, .. } = &operation.kind {
                    writeln!(out,"{{A a[2]={{obj(intern(&c,{})),obj(value_{})}};send(&c,ns,\"at:put:\",a,2);}}",quoted(name),operation.result.0).unwrap();
                }
            }
        }
        let (name, index) = match &block.terminator {
            Term::Call {
                call_site,
                receiver,
                selector,
                arguments,
                ..
            }
            | Term::TailCall {
                call_site,
                receiver,
                selector,
                arguments,
                ..
            } => {
                let start = layout.call_window_start.ok_or(Error::Layout)?;
                writeln!(out,"put(&c,machine,{start},get(&c,machine,{}));put(&c,machine,{},keep(&c,intern(&c,{})));",slot(*receiver),start+1,quoted(selector)).unwrap();
                for (i, arg) in arguments.iter().enumerate() {
                    writeln!(
                        out,
                        "put(&c,machine,{},get(&c,machine,{}));",
                        start + 2 + i as u32,
                        slot(*arg)
                    )
                    .unwrap();
                }
                (
                    if matches!(block.terminator, Term::TailCall { .. }) {
                        "tailCallSite:"
                    } else {
                        "callSite:"
                    },
                    call_site.0,
                )
            }
            Term::Return(v) => ("returnAt:", slot(*v)),
            Term::Continue(pc) => ("continueAt:", pc.0),
        };
        writeln!(
            out,
            "O result=action(&c,machine,\"{name}\",{index});return finish(&c,obj(result));}}"
        )
        .unwrap();
    }
    out.push_str("default:c.failure=bad();return finish(&c,(A){0});}}\n");
    functions[index] = Function {
        body: out,
        descriptor,
        selection: selected,
        named,
    };
    Ok(index)
}
fn array(out: &mut String, n: &str, bytes: &[u8]) {
    write!(out, "static const unsigned char {n}[]={{").unwrap();
    for v in bytes {
        write!(out, "{v},").unwrap();
    }
    out.push_str("};\n");
}
fn module(functions: Vec<Function>) -> Vec<u8> {
    let mut out = include_str!("native_prelude.c").to_string();
    writeln!(out, "static O methods[{}];", functions.len()).unwrap();
    for (i, f) in functions.iter().enumerate() {
        out.push_str(&f.body);
        array(&mut out, &format!("descriptor_{i}"), &f.descriptor);
        array(&mut out, &format!("selection_{i}"), &f.selection);
    }
    out.push_str("static R initialize(const P*p,const A*a,size_t n){if(!p||p->version!=1||p->byte_size!=sizeof(P)||n<2||n>3||a[0].kind!=1||a[1].kind!=1)return bad();id=*p;ns=a[0].data.object;owner=a[1].data.object;Call c={0};A key=obj(intern(&c,\"Roots\"));R root=raw(&c,ns,\"at:\",&key,1);if(root.status||root.value.kind!=1)return bad();roots=root.value.data.object;keep(&c,owner);keep(&c,ns);A root_arg=obj(roots);call(&c,owner,\"retain:\",&root_arg,1);\n");
    for (i, f) in functions.iter().enumerate() {
        writeln!(out,"{{A args[5]={{entry(step_{i}),bytes(descriptor_{i},sizeof(descriptor_{i})),bytes(selection_{i},sizeof(selection_{i})),obj(owner),n==3?a[2]:(A){{0}}}};").unwrap();
        let (name, count) = if f.named {
            ("fromStep:descriptor:selection:owner:implementation:", 5)
        } else {
            ("fromStep:descriptor:selection:owner:", 4)
        };
        writeln!(out,"methods[{i}]=call(&c,named(&c,\"Method\"),\"{name}\",args,{count});A m=obj(methods[{i}]);call(&c,owner,\"retain:\",&m,1);}}").unwrap();
    }
    out.push_str("return finish(&c,obj(methods[0]));}\nconst Module birnam_native_module_v1={1,sizeof(Module),initialize};\n");
    out.into_bytes()
}
pub fn compile_portable(bytes: &[u8], source: &[u8]) -> Result<Vec<u8>, Error> {
    let logical = ir::lower_portable_block(bytes).map_err(|_| Error::Lower)?;
    let mut functions = Vec::new();
    function(&logical, source, &mut functions, false)?;
    Ok(module(functions))
}
pub fn compile_program(program: crate::Program<'_>, source: &[u8]) -> Result<Vec<u8>, Error> {
    let block = crate::Block {
        params: Vec::new(),
        body: program.body,
    };
    let bytes = crate::portable::encode_block(&block, &[]).map_err(|_| Error::Portable)?;
    compile_portable(&bytes, source)
}
pub fn compile_named(arity: u32) -> Result<Vec<u8>, Error> {
    let selector = b"value:".repeat(arity as usize + 1);
    let logical = ir::lower_named_adapter(arity, &selector).map_err(|_| Error::Lower)?;
    let mut functions = Vec::new();
    function(&logical, &selector, &mut functions, true)?;
    Ok(module(functions))
}
