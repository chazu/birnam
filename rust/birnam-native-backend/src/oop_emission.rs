//! Synchronous emission from the existing canonical portable program. No parser,
//! alternate lowering or runtime representation is defined here.
#[path = "oop_resumable.rs"]
mod resumable;
use birnam_artifact::portable_program::{self, NodeKind, StatementKind};
use std::fmt::Write;
#[derive(Debug)]
pub enum Error {
    InvalidProgram,
    DepthLimit,
    InvalidName,
}
struct Emitter {
    names: Vec<Vec<u8>>,
    functions: Vec<String>,
    descriptors: Vec<Option<(Vec<u8>, Vec<u8>)>>,
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
    fn function(&mut self, bytes: &[u8], depth: usize) -> Result<usize, Error> {
        if depth > 256 {
            return Err(Error::DepthLimit);
        }
        let p = portable_program::inspect(bytes).map_err(|_| Error::InvalidProgram)?;
        let index = self.functions.len();
        self.functions.push(String::new());
        self.descriptors.push(None);
        let nil = self.name(b"nil")?;
        let mut out = format!(
            "static O function_{index}(O root,O context,O self,const O*args,size_t n){{(void)root;(void)context;Call c={{0}};if(n!={}){{O result=named(&c,{nil});return finish(&c,result);}}\n",
            p.parameter_count
        );
        for i in 0..p.parameter_count {
            writeln!(out, "O b{i}=args[{i}];").unwrap();
        }
        let capture = self.name(b"captureWordAt:")?;
        for i in 0..p.capture_count {
            writeln!(out,"O ix{i}=size_oop({i});O b{}=call(&c,self,{capture},&ix{i},1);if(c.error)return finish(&c,0);",p.parameter_count+i).unwrap();
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
                        let child = self.function(node.data, depth + 1)?;
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
                if depth == 0 {
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
    compile_mode(bytes, &[], false)
}
pub fn compile_resumable(bytes: &[u8], source: &[u8]) -> Result<Vec<u8>, Error> {
    compile_mode(bytes, source, true)
}
fn compile_mode(bytes: &[u8], source: &[u8], resumable: bool) -> Result<Vec<u8>, Error> {
    let mut e = Emitter {
        names: Vec::new(),
        functions: Vec::new(),
        descriptors: Vec::new(),
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
        ("WORD_OF", b"wordOf:"),
        ("LOW32", b"low32"),
        ("HIGH32", b"high32"),
        ("FROM_BITS", b"fromBits:"),
        ("STRING", b"String"),
        ("SEQUENCE", b"sequence"),
        ("FROM_BYTES", b"fromBytes:"),
        ("GLOBAL", b"resolveGlobal:"),
    ] {
        writeln!(definitions, "#define S_{key} {}", e.name(name)?).unwrap();
    }
    let roots = e.name(b"Roots")?;
    let declared = e.name(b"declaredMethodAt:")?;
    if resumable {
        e.resumable_function(bytes, source, 0)?;
    } else {
        e.function(bytes, 0)?;
    }
    let execution = e.name(b"Execution")?;
    let register = e.name(b"fromStep:descriptor:selection:owner:")?;
    let method_names = (0..e.functions.len())
        .map(|i| e.name(format!("compiledFunction{i}").as_bytes()))
        .collect::<Result<Vec<_>, _>>()?;
    writeln!(
        definitions,
        "#define SYMBOL_COUNT {}\n#define METHOD_COUNT {}",
        e.names.len(),
        e.functions.len()
    )
    .unwrap();
    let mut out = definitions + include_str!("oop_prelude.c");
    for i in 0..e.functions.len() {
        writeln!(out, "static O function_{i}(O,O,O,const O*,size_t);").unwrap();
    }
    for f in &e.functions {
        out.push_str(f);
    }
    for (i, descriptor) in e.descriptors.iter().enumerate() {
        if let Some((descriptor, selection)) = descriptor {
            for (label, bytes) in [("descriptor", descriptor), ("selection", selection)] {
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
        writeln!(out, "{{{name},function_{i}}},").unwrap();
    }
    out.push_str("};\n");
    writeln!(out,"static O initialize(const P*p,O unit,const O*names,size_t count,const O*args,size_t n){{if(!p||p->version!=2||p->byte_size!=sizeof(P))return 0;if(n!=1||count!=SYMBOL_COUNT)return p->invalid;id=*p;ns=args[0];owner=unit;for(size_t i=0;i<count;i++)symbols[i]=names[i];O key=symbols[{roots}];roots=id.message(id.root,ns,symbols[S_AT],&key,1);if(failed(roots)||!roots)return id.invalid;Call c={{0}};").unwrap();
    for i in 0..e.functions.len() {
        writeln!(
            out,
            "{{O at=size_oop({i});O method=call(&c,owner,{declared},&at,1);"
        )
        .unwrap();
        if e.descriptors[i].is_some() {
            writeln!(out,"O descriptor=string(&c,descriptor_{i},sizeof(descriptor_{i}));O selection=string(&c,selection_{i},sizeof(selection_{i}));O a[]={{method,descriptor,selection,owner}};O execution=named(&c,{execution});method=call(&c,execution,{register},a,4);").unwrap();
        }
        writeln!(
            out,
            "methods[{i}]=method;raw(&c,owner,S_RETAIN,&method,1);if(c.error)return finish(&c,0);}}"
        )
        .unwrap();
    }
    out.push_str("return finish(&c,methods[0]);}\nconst Module birnam_native_module_v2={2,sizeof(Module),literals,SYMBOL_COUNT,declarations,METHOD_COUNT,initialize};\n");
    Ok(out.into_bytes())
}
