//! Production emission uses the retained portable lowering, slot coloring and
//! descriptor validator, then spells all inter-owner operations as Id messages.
use birnam_codegen::{
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
#[path = "native_sync.rs"]
mod synchronous;
#[path = "word_expression.rs"]
mod word_expression;
#[path = "abi_arguments.rs"]
mod abi_arguments;

struct Function {
    portable: Option<Vec<u8>>,
    synchronous: bool,
    body: String,
    descriptor: Vec<u8>,
    selection: Vec<u8>,
    named: bool,
    discarded: Option<DiscardedClone>,
    predicate: Option<NumericPredicate>,
    inline_return: Option<InlineReturn>,
    word_helpers: bool,
    batch_cleanup: bool,
}
struct InlineReturn { operand: u32, argc: u32, slot: u32 }
fn inline_return(p: &ir::LogicalProgram<'_>, layout: &ir::PhysicalLayout) -> Option<InlineReturn> {
    if p.capture_count != 0 { return None; }
    let plan = birnam_codegen::small_callee::compile(p, layout).ok()?;
    if plan.has_literal_allocations() { return None; }
    let source = u32::try_from(plan.return_entry_slot()?).ok()?;
    let operand = if source == layout.entry_receiver_slot { 0 }
        else { source.checked_sub(layout.entry_argument_start)?.checked_add(1)? };
    if operand > layout.entry_argument_count { return None; }
    Some(InlineReturn { operand, argc: layout.entry_argument_count + 1, slot: plan.return_slot() as u32 })
}
struct DiscardedClone {
    source: u32,
    answer: u32,
    selector: Vec<u8>,
    return_slot: u32,
}
fn discarded_clone(
    p: &ir::LogicalProgram<'_>,
    layout: &ir::PhysicalLayout,
) -> Option<DiscardedClone> {
    if p.capture_count != 0 || p.blocks.len() != 2 || p.entry.0 != 0 {
        return None;
    }
    let first = &p.blocks[0];
    let last = &p.blocks[1];
    if first.pc.0 != 0
        || last.pc.0 != 1
        || !last.operations.is_empty()
        || first
            .operations
            .iter()
            .any(|op| !matches!(op.kind, Op::EntryReceiver | Op::EntryArgument { .. }))
    {
        return None;
    }
    let Term::Call {
        receiver,
        selector,
        arguments,
        successor,
        ..
    } = &first.terminator
    else {
        return None;
    };
    if !arguments.is_empty() || successor.0 != 1 {
        return None;
    }
    let Term::Return(answer) = last.terminator else {
        return None;
    };
    let argument = |value| {
        first.operations.iter().find_map(|op| {
            if op.result != value {
                return None;
            }
            if let Op::EntryArgument { index, .. } = op.kind {
                Some(index)
            } else {
                None
            }
        })
    };
    Some(DiscardedClone {
        source: argument(*receiver)?,
        answer: argument(answer)?,
        selector: selector.to_vec(),
        return_slot: layout.value_slots[answer.0 as usize],
    })
}
struct NumericPredicate {
    inputs: [u32; 3],
    selectors: [Vec<u8>; 2],
    return_slot: u32,
}
fn numeric_predicate(
    p: &ir::LogicalProgram<'_>,
    layout: &ir::PhysicalLayout,
) -> Option<NumericPredicate> {
    if p.capture_count != 0 || p.entry.0 != 0 || p.blocks.len() != 3 {
        return None;
    }
    let [first, second, last] = p.blocks.as_slice() else {
        return None;
    };
    if first.pc.0 != 0
        || second.pc.0 != 1
        || last.pc.0 != 2
        || !second.operations.is_empty()
        || first
            .operations
            .iter()
            .any(|op| !matches!(op.kind, Op::EntryReceiver | Op::EntryArgument { .. }))
    {
        return None;
    }
    let Term::Call {
        receiver: left,
        selector: arithmetic,
        arguments: right,
        result: intermediate,
        successor,
        ..
    } = &first.terminator
    else {
        return None;
    };
    if right.len() != 1 || successor.0 != 1 {
        return None;
    }
    let Term::Call {
        receiver,
        selector: comparison,
        arguments: limit,
        result,
        successor,
        ..
    } = &second.terminator
    else {
        return None;
    };
    if receiver != intermediate || limit.len() != 1 || successor.0 != 2 {
        return None;
    }
    let Term::Return(answer) = last.terminator else {
        return None;
    };
    match last.operations.as_slice() {
        [] if answer == *result => (),
        [op] if op.result == answer
            && matches!(op.kind, Op::Bind {source, ..} if source == *result) =>
        {
            ()
        }
        _ => return None,
    }
    let argument = |value| {
        first.operations.iter().find_map(|op| {
            if op.result != value {
                return None;
            }
            if let Op::EntryArgument { index, .. } = op.kind {
                Some(index)
            } else {
                None
            }
        })
    };
    Some(NumericPredicate {
        inputs: [argument(*left)?, argument(right[0])?, argument(limit[0])?],
        selectors: [arithmetic.to_vec(), comparison.to_vec()],
        return_slot: layout.value_slots[answer.0 as usize],
    })
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
/// Synchronous emission switches, supplied explicitly to the compiler.
/// 1 = Word expressions/locals, 2 = ABI literals, 4 = authored helper plans, 8 = owner-batched root cleanup, 16 = invocation-covered input roots (no duplicate registration).
#[derive(Clone, Copy)]
pub struct Optimizations(u8);
impl Default for Optimizations { fn default()->Self {Self(31)} }
impl Optimizations {
    pub fn from_bits(bits:usize)->Option<Self> {(bits<=31).then_some(Self(bits as u8))}
    fn words(self)->bool {self.0&1!=0}
    fn abi(self)->bool {self.0&2!=0}
    fn helpers(self)->bool {self.0&4!=0}
    fn roots(self)->bool {self.0&8!=0}
    fn inputs(self)->bool {self.0&16!=0}
}
#[derive(Clone, Copy, Default)]
struct Policy<'a> {
    synchronous: bool,
    dispositions: Option<&'a [bool]>,
    persist_root: bool,
    optimizations: Optimizations,
}
fn portable_function(
    bytes: &[u8],
    source: &[u8],
    filename: &[u8],
    functions: &mut Vec<Function>,
    policy: Policy<'_>,
) -> Result<usize, Error> {
    let sync = match policy.dispositions {
        Some(rows) => *rows.get(functions.len()).ok_or(Error::Selection)?,
        None => policy.synchronous,
    };
    if sync {
        return synchronous::function(bytes, source, filename, functions, policy);
    }
    let logical = ir::lower_portable_block(bytes).map_err(|_| Error::Lower)?;
    let index = function(&logical, source, functions, false, filename, policy)?;
    functions[index].portable = Some(bytes.to_vec());
    Ok(index)
}
fn function(
    program: &ir::LogicalProgram<'_>,
    source: &[u8],
    functions: &mut Vec<Function>,
    named: bool,
    filename: &[u8],
    policy: Policy<'_>,
) -> Result<usize, Error> {
    let index = functions.len();
    functions.push(Function {
        portable: None,
        synchronous: false,
        body: String::new(),
        descriptor: vec![],
        selection: vec![],
        named,
        discarded: None,
        predicate: None,
        inline_return: None,
        word_helpers: policy.optimizations.helpers(),
        batch_cleanup: policy.optimizations.roots(),
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
        program, &layout, authority, filename, source, 1,
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
                Op::GlobalLookup(n) => format!("global(&c,{})", quoted(n)),
                Op::Bind { source, .. } => format!("get(&c,machine,{})", slot(*source)),
                Op::PortableBlockLiteral {
                    program, captures, ..
                } => {
                    let child = portable_function(program, source, filename, functions, policy)?;
                    let v = operation.result.0;
                    writeln!(out, "A block_args_{v}[{}];block_args_{v}[0]=obj(methods[{child}]);block_args_{v}[1]=obj(codes[{child}]);",captures.len()+2).unwrap();
                    for (i,value) in captures.iter().enumerate() {
                        writeln!(out,"block_args_{v}[{}]=obj(get(&c,machine,{}));",i+2,slot(*value)).unwrap();
                    }
                    format!("call(&c,named(&c,\"Block\"),\"fromMethod:codeObject:captureOperands:\",block_args_{v},{})",captures.len()+2)

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
            if index == 0 && !named && policy.persist_root {
                if let Op::Bind { name, .. } = &operation.kind {
                    writeln!(out,"{{O published=global_put(&c,{},value_{});if(c.failure.status)return finish(&c,(A){{0}});if(published!=value_{}){{put(&c,machine,{},published);return finish(&c,obj(action(&c,machine,\"returnAt:\",{})));}}}}",quoted(name),operation.result.0,operation.result.0,slot(operation.result),slot(operation.result)).unwrap();
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
        portable: None,
        synchronous: false,
        body: out,
        descriptor,
        selection: selected,
        named,
        inline_return: if named { None } else { inline_return(program, &layout) },
        word_helpers: policy.optimizations.helpers(),
        batch_cleanup: policy.optimizations.roots(),
        predicate: if named {
            None
        } else {
            numeric_predicate(program, &layout)
        },
        discarded: if named {
            None
        } else {
            discarded_clone(program, &layout)
        },
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
    let batch_cleanup = functions.iter().all(|f| f.batch_cleanup) as u8;
    if functions.iter().any(|f| f.inline_return.is_some()) {
        out.push_str(include_str!("native_inline_return.c"));
    }
    writeln!(
        out,
        "static O methods[{}],codes[{}];",
        functions.len(),
        functions.len()
    )
    .unwrap();
    for (i, f) in functions.iter().enumerate() {
        if f.synchronous && f.named {
            writeln!(out, "static O implementation_{i};").unwrap();
        }
        out.push_str(&f.body);
        if let Some(plan) = &f.inline_return {
            writeln!(out, "static R inline_return_{i}(void*context,O machine,const A*a,size_t n){{(void)context;(void)machine;return try_inline_return(a,n,{},{});}}", plan.argc, plan.operand).unwrap();
        }
        if let Some(bytes) = &f.portable {
            array(&mut out, &format!("program_{i}"), bytes);
        }
        array(&mut out, &format!("descriptor_{i}"), &f.descriptor);
        array(&mut out, &format!("selection_{i}"), &f.selection);
    }
    out.push_str("static R initialize(const P*p,const A*a,size_t n){if(!p||p->version!=1||p->byte_size!=sizeof(P)||n<2||n>3||a[0].kind!=1||a[1].kind!=1)return bad();id=*p;ns=a[0].data.object;owner=a[1].data.object;Call c={0};A key=obj(intern(&c,\"Roots\"));R root=raw(&c,ns,\"at:\",&key,1);if(root.status||root.value.kind!=1)return bad();roots=root.value.data.object;cache_selectors=1;batch_cleanup=ROOT_BATCH_POLICY;keep(&c,owner);keep(&c,ns);A root_arg=obj(roots);call(&c,owner,\"retain:\",&root_arg,1);\n");
    for (i, f) in functions.iter().enumerate() {
        if f.portable.is_some() {
            writeln!(out, "{{A b=bytes(program_{i},sizeof(program_{i}));O encoded=call(&c,named(&c,\"Bytes\"),\"fromBytes:\",&b,1);A a=obj(encoded);codes[{i}]=call(&c,named(&c,\"CodeObject\"),\"fromCanonicalBytes:\",&a,1);if(c.failure.status)return finish(&c,(A){{0}});if(codes[{i}]==named(&c,\"nil\")||is_error(&c,codes[{i}]))return finish(&c,obj(codes[{i}]));A co=obj(codes[{i}]);call(&c,owner,\"retain:\",&co,1);}} ").unwrap();
        }
    }
    for (i, f) in functions.iter().enumerate() {
        if f.synchronous {
            if f.named {
                writeln!(out,"if(n!=3||a[2].kind!=1)return bad();implementation_{i}=keep(&c,a[2].data.object);{{A a[3]={{entry(sync_{i}),obj(owner),obj(implementation_{i})}};methods[{i}]=call(&c,named(&c,\"Method\"),\"fromNative:owner:implementation:\",a,3);A m=obj(methods[{i}]);call(&c,owner,\"retain:\",&m,1);A impl=obj(implementation_{i});call(&c,owner,\"retain:\",&impl,1);}}").unwrap();
            } else {
                writeln!(out, "{{A a[2]={{entry(sync_{i}),obj(owner)}};methods[{i}]=call(&c,named(&c,\"Method\"),\"fromNative:owner:\",a,2);A m=obj(methods[{i}]);call(&c,owner,\"retain:\",&m,1);}} ").unwrap();
            }
            if f.named {
                writeln!(out,"{{methods[{i}]=call(&c,methods[{i}],\"withWordHelperAdapter\",0,0);}}").unwrap();
            } else if let Some(plan) = f.portable.as_deref().filter(|_|f.word_helpers).and_then(|b|birnam_artifact::portable_program::inspect(b).ok()).and_then(|p|word_expression::helper(&p).map(|plan|(plan,p.parameter_count))) {
                let (plan,parameters)=plan;
                array(&mut out,&format!("helper_plan_{i}"),&plan.bytes);
                let bindings=plan.bindings.iter().flat_map(|b|b.to_le_bytes()).collect::<Vec<_>>();
                array(&mut out,&format!("helper_bindings_{i}"),&bindings);
                writeln!(out,"{{A a[3]={{bytes(helper_plan_{i},sizeof(helper_plan_{i})),bytes(helper_bindings_{i},sizeof(helper_bindings_{i})),word({parameters})}};methods[{i}]=call(&c,methods[{i}],\"withWordExpression:bindings:parameters:\",a,3);}}").unwrap();
            }
            continue;
        }
        writeln!(out,"{{A args[5]={{entry(step_{i}),bytes(descriptor_{i},sizeof(descriptor_{i})),bytes(selection_{i},sizeof(selection_{i})),obj(owner),n==3?a[2]:(A){{0}}}};").unwrap();
        let (name, count) = if f.named {
            ("fromStep:descriptor:selection:owner:implementation:", 5)
        } else {
            ("fromStep:descriptor:selection:owner:", 4)
        };
        writeln!(out,"methods[{i}]=call(&c,named(&c,\"Method\"),\"{name}\",args,{count});A m=obj(methods[{i}]);call(&c,owner,\"retain:\",&m,1);}}").unwrap();
        if let Some(plan) = &f.inline_return {
            writeln!(out, "{{A a[3]={{entry(inline_return_{i}),obj(integer(&c,{})),obj(owner)}};methods[{i}]=call(&c,methods[{i}],\"withInline:at:owner:\",a,3);A m=obj(methods[{i}]);call(&c,owner,\"retain:\",&m,1);}}", plan.slot).unwrap();
        }
        if let Some(plan) = &f.predicate {
            writeln!(out, "{{A a[7]={{obj(intern(&c,{})),obj(intern(&c,{})),word({}),word({}),word({}),word({}),obj(ns)}};methods[{i}]=call(&c,methods[{i}],\"withNumericPredicate:then:left:right:limit:return:in:\",a,7);A m=obj(methods[{i}]);call(&c,owner,\"retain:\",&m,1);}}", quoted(&plan.selectors[0]), quoted(&plan.selectors[1]), plan.inputs[0], plan.inputs[1], plan.inputs[2], plan.return_slot).unwrap();
        }
        if let Some(plan) = &f.discarded {
            writeln!(out, "{{A a[5]={{word({}),word({}),obj(intern(&c,{})),word({}),obj(ns)}};methods[{i}]=call(&c,methods[{i}],\"withDiscardedClone:answer:selector:return:in:\",a,5);A m=obj(methods[{i}]);call(&c,owner,\"retain:\",&m,1);}}", plan.source, plan.answer, quoted(&plan.selector), plan.return_slot).unwrap();
        }
    }
    out.push_str("return finish(&c,obj(methods[0]));}\nconst Module birnam_native_module_v1={1,sizeof(Module),initialize};\n");
    out.replace("ROOT_BATCH_POLICY", &batch_cleanup.to_string()).into_bytes()
}
/// Compile an inert candidate factory with an exact preorder disposition for
/// its factory and each nested Block. Its lexical lets never publish Globals.
pub fn compile_factory(
    bytes: &[u8],
    source: &[u8],
    dispositions: &[bool],
) -> Result<Vec<u8>, Error> {
    compile_factory_with_optimizations(bytes,source,dispositions,Optimizations::default())
}
pub fn compile_factory_with_optimizations(bytes:&[u8],source:&[u8],dispositions:&[bool],optimizations:Optimizations)->Result<Vec<u8>,Error> {
    let mut functions = Vec::new();
    portable_function(
        bytes,
        source,
        b"<candidate>",
        &mut functions,
        Policy {
            dispositions: Some(dispositions),
            optimizations,
            ..Policy::default()
        },
    )?;
    if functions.len() != dispositions.len() {
        return Err(Error::Selection);
    }
    Ok(module(functions))
}
pub fn compile_portable_from(bytes:&[u8],source:&[u8],filename:&[u8],persistent:bool,synchronous:bool)->Result<Vec<u8>,Error>{
    compile_portable_with_optimizations(bytes,source,filename,persistent,synchronous,Optimizations::default())
}
pub fn compile_portable_with_optimizations(bytes:&[u8],source:&[u8],filename:&[u8],persistent:bool,synchronous:bool,optimizations:Optimizations)->Result<Vec<u8>,Error> {
    if persistent && synchronous{return Err(Error::Unsupported);}
    let mut functions=vec![];
    portable_function(bytes,source,filename,&mut functions,Policy{persist_root:persistent,synchronous,optimizations,..Policy::default()})?;
    Ok(module(functions))
}
fn named_functions(arity: u32) -> Result<Vec<Function>, Error> {
    let selector = b"value:".repeat(arity as usize + 1);
    let logical = ir::lower_named_adapter(arity, &selector).map_err(|_| Error::Lower)?;
    let mut functions = Vec::new();
    function(
        &logical,
        &selector,
        &mut functions,
        true,
        b"<compiler>",
        Policy::default(),
    )?;
    Ok(functions)
}
pub fn compile_named(arity: u32) -> Result<Vec<u8>, Error> {
    Ok(module(named_functions(arity)?))
}
pub fn compile_named_synchronous(arity: u32) -> Result<Vec<u8>, Error> {
    if arity > 16 {
        return Err(Error::Unsupported);
    }
    let mut body = format!(
        "static R sync_0(void*context,O self,const A*args,size_t n){{(void)context;Call c={{0}};object(&c,obj(self));if(n!={arity})return finish(&c,obj(named(&c,\"nil\")));check_entry(&c,self,args,n);A a[{}];a[0]=obj(self);",
        arity + 1
    );
    for i in 0..arity {
        write!(body, "a[{}]=obj(object(&c,args[{i}]));", i + 1).unwrap();
    }
    write!(
        body,
        "O result=call(&c,implementation_0,{},a,{});return finish(&c,obj(result));}}",
        quoted(&b"value:".repeat(arity as usize + 1)),
        arity + 1
    )
    .unwrap();
    Ok(module(vec![Function {
        portable: None,
        synchronous: true,
        body,
        descriptor: vec![],
        selection: vec![],
        named: true,
        discarded: None,
        predicate: None,
        inline_return: None,
        word_helpers: true,
        batch_cleanup: true,
    }]))
}
/// Exact descriptor emitted by the current named-adapter backend. Generation
/// identity must use this, rather than descriptors from the retired C emitter.
pub fn named_descriptor(selector: &[u8]) -> Result<Vec<u8>, Error> {
    let arity = birnam_artifact::selector::arity(selector);
    let arity = u32::try_from(arity).map_err(|_| Error::Unsupported)?;
    Ok(named_functions(arity)?.remove(0).descriptor)
}

#[cfg(test)]
mod predicate_tests {
    use super::*;
    use crate::{Block, Expr, Statement};
    use std::borrow::Cow;
    fn body() -> Vec<u8> {
        let send = |receiver, selector: &'static [u8], argument| Expr::Send {
            receiver: Box::new(receiver),
            selector: Cow::Borrowed(selector),
            args: vec![argument],
            source: b"",
        };
        crate::portable::encode_block(
            &Block {
                params: vec![b"a", b"b", b"c"],
                body: vec![
                    Statement::Let {
                        name: b"answer",
                        value: send(
                            send(Expr::Global(b"a"), b"sum:", Expr::Global(b"b")),
                            b"below:",
                            Expr::Global(b"c"),
                        ),
                    },
                    Statement::Expr(Expr::Global(b"answer")),
                ],
            },
            &[],
        )
        .unwrap()
    }
    #[test]
    fn predicate_producer_preserves_arguments_and_actual_selector_names() {
        let bytes = body();
        let p = ir::lower_portable_block(&bytes).unwrap();
        let layout = ir::color_slots(&p).unwrap();
        let plan = numeric_predicate(&p, &layout).unwrap();
        assert_eq!(plan.inputs, [0, 1, 2]);
        assert_eq!(plan.selectors, [b"sum:".to_vec(), b"below:".to_vec()]);
        assert!(plan.return_slot < layout.slot_count);
    }
    #[test]
    fn predicate_producer_refuses_captures_extra_operations_and_wrong_dataflow() {
        let bytes = body();
        let p = ir::lower_portable_block(&bytes).unwrap();
        let layout = ir::color_slots(&p).unwrap();
        let mut invalid = p.clone();
        invalid.capture_count = 1;
        assert!(numeric_predicate(&invalid, &layout).is_none());
        let mut invalid = p.clone();
        invalid.blocks[1]
            .operations
            .push(p.blocks[0].operations[0].clone());
        assert!(numeric_predicate(&invalid, &layout).is_none());
        let mut invalid = p.clone();
        let original_left = match p.blocks[0].terminator {
            Term::Call { receiver, .. } => receiver,
            _ => unreachable!(),
        };
        if let Term::Call { receiver, .. } = &mut invalid.blocks[1].terminator {
            *receiver = original_left;
        }
        assert!(numeric_predicate(&invalid, &layout).is_none());
    }
}
