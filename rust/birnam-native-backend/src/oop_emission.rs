//! Synchronous emission from the existing canonical portable program. No parser,
//! alternate lowering or runtime representation is defined here.
#[path = "oop_resumable.rs"]
mod resumable;
use birnam_artifact::portable_program::{self, NodeKind, StatementKind};
use std::fmt::Write;
use sha2::{Digest,Sha256};
#[derive(Debug)]
pub enum Error {
    InvalidProgram,
    DepthLimit,
    InvalidName,
}
struct Emitter {
    source_revision: [u8; 32],
    names: Vec<Vec<u8>>,
    functions: Vec<String>,
    programs: Vec<Vec<u8>>,
    descriptors: Vec<Option<ResumableMetadata>>,
    persistent: bool,
    dispositions: Option<Vec<bool>>,
}
struct ResumableMetadata {
    descriptor: Vec<u8>,
    selection: Vec<u8>,
    call_plans: Vec<u8>,
}
impl Emitter {
    fn name(&mut self, n: &[u8]) -> Result<usize, Error> {
        if n.contains(&0) {
            return Err(Error::InvalidName);
        }
        if let Some(i) = self.names.iter().position(|v| v == n) {
            return Ok(i);
        }
        self.names.push(n.to_vec());
        Ok(self.names.len() - 1)
    }
    fn selected_function(
        &mut self,
        bytes: &[u8],
        source: &[u8],
        filename: &[u8],
        depth: usize,
        synchronous: bool,
    ) -> Result<usize, Error> {
        let synchronous = match &self.dispositions {
            Some(values) => *values
                .get(self.functions.len())
                .ok_or(Error::InvalidProgram)?,
            None => synchronous,
        };
        if synchronous {
            self.function(bytes, source, filename, depth)
        } else {
            self.resumable_function(bytes, source, filename, depth)
        }
    }
    fn function(
        &mut self,
        bytes: &[u8],
        source: &[u8],
        filename: &[u8],
        depth: usize,
    ) -> Result<usize, Error> {
        if depth > 256 {
            return Err(Error::DepthLimit);
        }
        let p = portable_program::inspect(bytes).map_err(|_| Error::InvalidProgram)?;
        let index = self.functions.len();
        self.functions.push(String::new());
        self.programs.push(bytes.to_vec());
        self.descriptors.push(None);
        let nil = self.name(b"nil")?;
        let mut out = format!(
            "static O function_{index}(O root,O context,O self,const O*args,size_t n){{(void)root;(void)context;Call c={{0}};if(n!={}){{O result=named(&c,{nil});return finish(&c,result);}}\n",
            p.parameter_count
        );
        for i in 0..p.parameter_count {
            writeln!(out, "O b{i}=args[{i}];").unwrap();
        }
        if p.capture_count != 0 {
            let capture = self.name(b"captureWord:of:")?;
            let block = self.name(b"Block")?;
            writeln!(out, "O capture_owner=named(&c,{block});").unwrap();
            for i in 0..p.capture_count {
                writeln!(out,"O capture_args{i}[]={{size_oop({i}),self}};O b{}=call(&c,capture_owner,{capture},capture_args{i},2);if(c.error)return finish(&c,0);",p.parameter_count+i).unwrap();
            }
        }
        let mut next = 0;
        let mut answer = None;
        for i in 0..p.statement_count as usize {
            let statement = p.statement(i).ok_or(Error::InvalidProgram)?;
            while next <= statement.root_node {
                let node = p.node(next as usize).ok_or(Error::InvalidProgram)?;
                let value = match node.semantic_kind().ok_or(Error::InvalidProgram)? {
                    NodeKind::Integer => format!("integer(&c,UINT64_C({}))", node.payload),
                    NodeKind::FloatBits => format!("float_bits(&c,UINT64_C({}))", node.payload),
                    NodeKind::String => {
                        let data = if node.data.is_empty() {
                            "0".to_owned()
                        } else {
                            node.data
                                .iter()
                                .map(u8::to_string)
                                .collect::<Vec<_>>()
                                .join(",")
                        };
                        writeln!(out, "static const unsigned char text{next}[]={{{data}}};")
                            .unwrap();
                        format!("string(&c,text{next},{})", node.data.len())
                    }
                    NodeKind::Symbol => format!("symbols[{}]", self.name(node.data)?),
                    NodeKind::Nil => format!("named(&c,{nil})"),
                    NodeKind::True => format!("named(&c,{})", self.name(b"true")?),
                    NodeKind::False => format!("named(&c,{})", self.name(b"false")?),
                    NodeKind::LexicalRead => format!("b{}", node.auxiliary_0),
                    NodeKind::GlobalRead => format!("global(&c,{})", self.name(node.data)?),
                    NodeKind::Send => {
                        let receiver = p
                            .node_operand(next as usize, 0)
                            .ok_or(Error::InvalidProgram)?;
                        let args = (1..node.operand_count as usize)
                            .map(|i| {
                                p.node_operand(next as usize, i)
                                    .map(|v| format!("v{v}"))
                                    .ok_or(Error::InvalidProgram)
                            })
                            .collect::<Result<Vec<_>, _>>()?;
                        let arguments = if args.is_empty() {
                            "0".to_owned()
                        } else {
                            writeln!(out, "O args{next}[]={{{}}};", args.join(",")).unwrap();
                            format!("args{next}")
                        };
                        format!(
                            "call(&c,v{receiver},{},{arguments},{})",
                            self.name(node.data)?,
                            args.len()
                        )
                    }
                    NodeKind::BlockLiteral => {
                        let child =
                            self.selected_function(node.data, source, filename, depth + 1, true)?;
                        let mut captures = vec![format!("methods[{child}]")];
                        for i in 0..node.operand_count as usize {
                            captures.push(format!(
                                "b{}",
                                p.node_operand(next as usize, i)
                                    .ok_or(Error::InvalidProgram)?
                            ));
                        }
                        writeln!(
                            out,
                            "O captures{next}[]={{{}}};O factory{next}=named(&c,{});",
                            captures.join(","),
                            self.name(b"Block")?
                        )
                        .unwrap();
                        format!(
                            "call(&c,factory{next},{},captures{next},{})",
                            self.name(b"fromMethod:captureOperands:")?,
                            captures.len()
                        )
                    }
                };
                writeln!(out, "O v{next}={value};if(c.error)return finish(&c,0);").unwrap();
                next += 1;
            }
            if statement.kind == StatementKind::Let {
                let binding = statement.binding_index.ok_or(Error::InvalidProgram)?;
                writeln!(out, "O b{binding}=v{};", statement.root_node).unwrap();
                if depth == 0 && self.persistent {
                    let name = p
                        .binding_name(binding as usize)
                        .ok_or(Error::InvalidProgram)?;
                    let name = self.name(name)?;
                    let globals = self.name(b"Globals")?;
                    let put = self.name(b"at:put:")?;
                    writeln!(out,"O publish{binding}[]={{symbols[{name}],b{binding}}};O globals{binding}=named(&c,{globals});call(&c,globals{binding},{put},publish{binding},2);if(c.error)return finish(&c,0);").unwrap();
                }
            }
            answer = Some(statement.root_node);
        }
        let result = answer
            .map(|i| format!("v{i}"))
            .unwrap_or_else(|| format!("named(&c,{nil})"));
        writeln!(out, "O result={result};return finish(&c,result);}}").unwrap();
        self.functions[index] = out;
        Ok(index)
    }
}
pub fn compile(bytes: &[u8]) -> Result<Vec<u8>, Error> {
    compile_mode(bytes, &[], b"<compiler>", true, false, None)
}
pub fn compile_resumable(bytes: &[u8], source: &[u8]) -> Result<Vec<u8>, Error> {
    compile_mode(bytes, source, b"<compiler>", true, true, None)
}
pub fn compile_from(
    bytes: &[u8],
    source: &[u8],
    filename: &[u8],
    persistent: bool,
    synchronous: bool,
) -> Result<Vec<u8>, Error> {
    if persistent && synchronous {
        return Err(Error::InvalidProgram);
    }
    compile_mode(bytes, source, filename, persistent, !synchronous, None)
}
pub fn compile_factory(
    bytes: &[u8],
    source: &[u8],
    dispositions: &[bool],
    persistent: bool,
) -> Result<Vec<u8>, Error> {
    if dispositions.is_empty() || dispositions.len() > 65536 || (persistent && dispositions[0]) {
        return Err(Error::InvalidProgram);
    }
    compile_mode(
        bytes,
        source,
        b"<compiler>",
        persistent,
        true,
        Some(dispositions.to_vec()),
    )
}
fn compile_mode(
    bytes: &[u8],
    source: &[u8],
    filename: &[u8],
    persistent: bool,
    resumable: bool,
    dispositions: Option<Vec<bool>>,
) -> Result<Vec<u8>, Error> {
    let mut e = Emitter {
        source_revision: Sha256::digest(source).into(),
        names: Vec::new(),
        functions: Vec::new(),
        programs: Vec::new(),
        descriptors: Vec::new(),
        persistent,
        dispositions,
    };
    let mut definitions = String::new();
    for (key, name) in [
        ("RETAIN", b"retain:".as_slice()),
        ("AT", b"at:"),
        ("CLEANUP", b"releaseTokensProtecting"),
        ("WORD", b"Word"),
        ("INTEGER", b"Integer"),
        ("FLOAT", b"Float"),
        ("FROM_PARTS", b"fromUnsignedLow:high:"),
        ("FROM_WORD", b"fromWord:"),
        ("FROM_BITS", b"fromBits:"),
        ("STRING", b"String"),
        ("BYTES", b"Bytes"),
        ("FROM_BYTE_VALUES", b"fromByteValues"),
        ("APPEND_BYTE_VALUES", b"appendByteValues"),
        ("SEQUENCE", b"sequence"),
        ("FROM_BYTES", b"fromBytes:"),
        ("GLOBAL", b"resolveGlobal:"),
        ("ANSWER", b"answerTo:with:"),
        ("VECTOR", b"Vector"),
        ("NEW", b"new"),
        ("ADD", b"add:"),
        ("NIL", b"nil"),
        ("IS_ERROR", b"isError"),
        ("ERROR", b"Error"),
        ("TRUE", b"true"),
    ] {
        writeln!(definitions, "#define S_{key} {}", e.name(name)?).unwrap();
    }
    let roots = e.name(b"Roots")?;
    let declared = e.name(b"declaredMethodAt:")?;
    let native_declared = e.name(b"newMethodAt:")?;
    e.selected_function(bytes, source, filename, 0, !resumable)?;
    if e.dispositions
        .as_ref()
        .is_some_and(|v| v.len() != e.functions.len())
    {
        return Err(Error::InvalidProgram);
    }
    let execution = e.name(b"Execution")?;
    let register = e.name(b"fromStep:descriptor:selection:owner:callPlans:codeObject:")?;
    let code_object = e.name(b"CodeObject")?;
    let from_canonical = e.name(b"fromCanonicalBytes:")?;
    let native_register = e.name(b"fromNativeCodeFactory:codeObject:")?;
    let bind_context = e.name(b"declaredMethodAt:context:")?;
    let with_context = e.name(b"withContext:")?;
    let parameters = e.name(b"parameterCount")?;
    let captures = e.name(b"captureCount")?;
    let environment = e.name(b"environment")?;
    let source_query = e.name(b"codeObject")?;
    let canonical = e.name(b"canonicalBytesOf:")?;
    let sha = e.name(b"sha256")?;
    let matches_bytes = e.name(b"matchesByteValues")?;
    e.name(b"false")?;
    let nil_name = e.name(b"nil")?;
    let method_names = (0..e.functions.len())
        .map(|i| e.name(format!("compiledFunction{i}").as_bytes()))
        .collect::<Result<Vec<_>, _>>()?;
    let native_bodies: Vec<_> = e.descriptors.iter().enumerate().filter_map(|(i,d)| d.is_none().then_some(i)).collect();
    let factory_names = native_bodies.iter().map(|i| e.name(format!("compiledFactory{i}").as_bytes())).collect::<Result<Vec<_>,_>>()?;
    let bound_names = native_bodies.iter().map(|i| e.name(format!("compiledMethod{i}").as_bytes())).collect::<Result<Vec<_>,_>>()?;
    let dnu_names = native_bodies.iter().map(|i| e.name(format!("compiledDnu{i}").as_bytes())).collect::<Result<Vec<_>,_>>()?;
    writeln!(
        definitions,
        "#define SYMBOL_COUNT {}\n#define METHOD_COUNT {}",
        e.names.len(),
        e.functions.len() + 3 * native_bodies.len()
    )
    .unwrap();
    let mut out = definitions + include_str!("oop_prelude.c");
    for i in 0..e.functions.len() {
        writeln!(out, "static O function_{i}(O,O,O,const O*,size_t);").unwrap();
    }
    for (i, f) in e.functions.iter().enumerate() {
        if e.descriptors[i].is_some() {
            out.push_str(&f.replace(&format!("function_{i}("), &format!("step_body_{i}(")));
            writeln!(out,"static O function_{i}(O root,O context,O machine,const O*args,size_t n){{if(n!=2)return id.invalid;O result=step_body_{i}(root,context,machine,args+1,n-1);if(failed(result))return result;O answer[]={{args[0],result}};return control(id.root,symbols[S_ANSWER],answer,2);}}").unwrap();
        } else {
            out.push_str(f);
        }
    }
    out.push_str("static O code_provider;static int native_provider;\n");
    for (ordinal, &i) in native_bodies.iter().enumerate() {
        let p = portable_program::inspect(&e.programs[i]).map_err(|_| Error::InvalidProgram)?;
        let argc = p.parameter_count as usize;
        let capc = p.capture_count;
        let bound_index = e.functions.len() + ordinal * 3 + 1;
        let dnu_index = bound_index + 1;
        let digest = Sha256::digest(&e.programs[i]).iter().map(u8::to_string).collect::<Vec<_>>().join(",");
        for (label, test) in [("bound", format!("n+1!={argc}")), ("dnu", format!("n+1<{argc}"))] {
            writeln!(out,"static O {label}_{i}(O root,O context,O receiver,const O*args,size_t n){{Call c={{0}};if({argc}==0||{test})return finish(&c,named(&c,{nil_name}));O block=call(&c,context,{environment},0,0);if(c.error)return finish(&c,0);O values[{}];values[0]=receiver;for(size_t k=1;k<{argc};k++)values[k]=args[k-1];O result=function_{i}(root,context,block,values,{argc});if(failed(result))c.error=result;return finish(&c,result);}}",argc.max(1)).unwrap();
        }
        writeln!(out,"static O factory_{i}(O root,O unused,O receiver,const O*args,size_t n){{(void)root;(void)unused;(void)receiver;if(n!=2)return id.invalid;Call c={{0}};O context=args[0];O source=call(&c,context,{source_query},0,0);O bytes=call(&c,named(&c,{code_object}),{canonical},&source,1);O digest=call(&c,bytes,{sha},0,0);static const unsigned char expected[]={{{digest}}};O expected_values[32];for(size_t k=0;k<32;k++)expected_values[k]=size_oop(expected[k]);if(call(&c,digest,{matches_bytes},expected_values,32)!=size_oop(1)){{if(!c.error)c.error=id.invalid;return finish(&c,0);}}if(call(&c,context,{parameters},0,0)!=size_oop({argc})||call(&c,context,{captures},0,0)!=size_oop({capc})){{if(!c.error)c.error=id.invalid;return finish(&c,0);}}size_t index;if(args[1]==size_oop(0))index={i};else if(args[1]==size_oop(1)&&{argc}!=0)index={bound_index};else if(args[1]==size_oop(2)&&{argc}!=0)index={dnu_index};else{{c.error=id.invalid;return finish(&c,0);}}O at=size_oop(index),result;if(native_provider){{O m=call(&c,code_provider,{native_declared},&at,1);result=call(&c,m,{with_context},&context,1);}}else{{O a[]={{at,context}};result=call(&c,code_provider,{bind_context},a,2);}}return finish(&c,result);}}").unwrap();
    }
    for (i, descriptor) in e.descriptors.iter().enumerate() {
        if let Some(metadata) = descriptor {
            for (label, bytes) in [
                ("descriptor", &metadata.descriptor),
                ("selection", &metadata.selection),
                ("plans", &metadata.call_plans),
            ] {
                writeln!(
                    out,
                    "static const unsigned char {label}_{i}[]={{{}}};",
                    bytes
                        .iter()
                        .map(u8::to_string)
                        .collect::<Vec<_>>()
                        .join(",")
                )
                .unwrap();
            }
        }
    }
    for (i, bytes) in e.programs.iter().enumerate() {
        writeln!(
            out,
            "static const unsigned char program_{i}[]={{{}}};",
            bytes
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join(",")
        )
        .unwrap();
    }
    out.push_str("static const Literal literals[]={\n");
    for name in &e.names {
        let mut quoted = b"\"".to_vec();
        crate::c_string::push_c_string_body(&mut quoted, name);
        quoted.push(b'"');
        writeln!(
            out,
            "{{(const unsigned char*){},{}}},",
            String::from_utf8(quoted).unwrap(),
            name.len()
        )
        .unwrap();
    }
    out.push_str("};\nstatic const MethodSpec declarations[]={\n");
    for (i, name) in method_names.iter().enumerate() {
        writeln!(
            out,
            "{{{name},function_{i},{}}},",
            usize::from(e.descriptors[i].is_some())
        )
        .unwrap();
    }
    for (ordinal, &i) in native_bodies.iter().enumerate() {
        writeln!(out,"{{{},factory_{i},0}},{{{},bound_{i},0}},{{{},dnu_{i},0}},",factory_names[ordinal],bound_names[ordinal],dnu_names[ordinal]).unwrap();
    }
    out.push_str("};\n");
    // TCC supplies its Unit owner. Standalone callers supply their own retention
    // owner; the native image still supplies declared Methods through Id messages.
    writeln!(out,"static O initialize(const CP*p,O unit,const O*names,size_t count,const O*args,size_t n){{if(!p||p->common.version!=3||p->common.byte_size!=sizeof(CP))return 0;if(n<1||n>2||count!=SYMBOL_COUNT)return p->common.invalid;id=p->common;control=p->control;ns=args[0];owner=n==2?args[1]:unit;code_provider=unit;native_provider=n==2;for(size_t i=0;i<count;i++)symbols[i]=names[i];O key=symbols[{roots}];roots=id.message(id.root,ns,symbols[S_AT],&key,1);if(failed(roots)||!roots)return id.invalid;Call c={{0}};").unwrap();
    for i in 0..e.functions.len() {
        writeln!(
            out,
            "{{O at=size_oop({i});O method=call(&c,unit,n==2?{native_declared}:{declared},&at,1);"
        )
        .unwrap();
        writeln!(out,"O encoded=blob(&c,program_{i},sizeof(program_{i}));O code=call(&c,named(&c,{code_object}),{from_canonical},&encoded,1);if(c.error)return finish(&c,0);if(code==named(&c,{nil_name})||is_error(&c,code))return finish(&c,code);").unwrap();
        if e.descriptors[i].is_some() {
            writeln!(out,"O descriptor=blob(&c,descriptor_{i},sizeof(descriptor_{i}));O selection=blob(&c,selection_{i},sizeof(selection_{i}));O plans=blob(&c,plans_{i},sizeof(plans_{i}));O a[]={{method,descriptor,selection,owner,plans,code}};O execution=named(&c,{execution});method=call(&c,execution,{register},a,6);if(c.error)return finish(&c,0);").unwrap();
        } else {
            let ordinal = native_bodies.iter().position(|&index| index == i).unwrap();
            let factory_index = e.functions.len() + 3 * ordinal;
            writeln!(out,"O at_factory=size_oop({factory_index});O factory=call(&c,unit,n==2?{native_declared}:{declared},&at_factory,1);O a[]={{factory,code}};method=call(&c,named(&c,{execution}),{native_register},a,2);if(c.error)return finish(&c,0);").unwrap();
        }
        writeln!(
            out,
            "methods[{i}]=method;raw(&c,owner,S_RETAIN,&method,1);if(c.error)return finish(&c,0);}}"
        )
        .unwrap();
    }
    out.push_str("return finish(&c,methods[0]);}\nconst Module birnam_native_module_v3={3,sizeof(Module),literals,SYMBOL_COUNT,declarations,METHOD_COUNT,initialize};\n");
    Ok(out.into_bytes())
}
