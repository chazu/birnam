//! Oop spelling of the retained logical IR and canonical frame descriptor.
use super::*;
use birnam_artifact::stackless_selection::{
    self as selection, Disposition, ImplementationKind, RootKind, SelectionSpec,
};
use birnam_codegen::{resumable as ir, resumable_descriptor};
use sha2::{Digest, Sha256};
impl Emitter {
    pub(super) fn resumable_function(
        &mut self,
        bytes: &[u8],
        source: &[u8],
        depth: usize,
    ) -> Result<usize, Error> {
        if depth > 256 {
            return Err(Error::DepthLimit);
        }
        let program = ir::lower_portable_block(bytes).map_err(|_| Error::InvalidProgram)?;
        let layout = ir::color_slots(&program).map_err(|_| Error::InvalidProgram)?;
        let index = self.functions.len();
        self.functions.push(String::new());
        self.descriptors.push(None);
        let revision = Sha256::digest(source).into();
        let path = [index as u32];
        let authority = SelectionSpec {
            source_revision: revision,
            root_kind: RootKind::NonMethodProgram,
            root_index: 0,
            path: &path,
            implementation_kind: ImplementationKind::BlockBody,
            disposition: Disposition::ResumableRequired,
        };
        let descriptor = resumable_descriptor::compile_descriptor_v2(
            &program,
            &layout,
            authority,
            b"<compiler>",
            source,
            1,
        )
        .map_err(|_| Error::InvalidProgram)?;
        let selection = selection::encode_artifact_selection(&[revision], &[authority])
            .map_err(|_| Error::InvalidProgram)?;
        let get = self.name(b"oopAt:")?;
        let put = self.name(b"oopAt:put:")?;
        let slot = |v: ir::ValueId| layout.value_slots[v.0 as usize];
        // Preserve the existing step contract: the driver supplies a language
        // Integer PC and generated code reads it through its current owner.
        let mut out = format!(
            "static O function_{index}(O root,O context,O machine,const O*args,size_t n){{(void)root;(void)context;if(n!=1)return id.invalid;Call c={{0}};size_t pc=integer_index(&c,args[0]);if(c.error)return finish(&c,0);switch(pc){{\n"
        );
        for block in &program.blocks {
            writeln!(out, "case {}:{{", block.pc.0).unwrap();
            for operation in &block.operations {
                use ir::OperationKind as Op;
                let expr = match &operation.kind {
                    Op::EntryReceiver
                    | Op::EntryImplementation
                    | Op::EntryArgument { .. }
                    | Op::EntryCapture { .. } => continue,
                    Op::Literal(value) => {
                        self.resumable_literal(&mut out, value, operation.result.0)?
                    }
                    Op::GlobalLookup(name) => format!("global(&c,{})", self.name(name)?),
                    Op::Bind { source, .. } => {
                        format!("frame_get(&c,machine,{}, {get})", slot(*source))
                    }
                    Op::PortableBlockLiteral {
                        program, captures, ..
                    } => {
                        let child = self.resumable_function(program, source, depth + 1)?;
                        let v = operation.result.0;
                        writeln!(
                            out,
                            "O capture_{v}[{}];capture_{v}[0]=methods[{child}];",
                            captures.len() + 1
                        )
                        .unwrap();
                        for (i, capture) in captures.iter().enumerate() {
                            writeln!(
                                out,
                                "capture_{v}[{}]=frame_get(&c,machine,{}, {get});",
                                i + 1,
                                slot(*capture)
                            )
                            .unwrap();
                        }
                        writeln!(out, "O factory_{v}=named(&c,{});", self.name(b"Block")?).unwrap();
                        format!(
                            "call(&c,factory_{v},{},capture_{v},{})",
                            self.name(b"fromMethod:captureOperands:")?,
                            captures.len() + 1
                        )
                    }
                    Op::BlockLiteral { .. } => return Err(Error::InvalidProgram),
                };
                writeln!(out,"O value_{}={expr};frame_put(&c,machine,{},value_{},{put});if(c.error)return finish(&c,0);",operation.result.0,slot(operation.result),operation.result.0).unwrap();
                if depth == 0 {
                    if let Op::Bind { name, .. } = &operation.kind {
                        let name = self.name(name)?;
                        let globals = self.name(b"Globals")?;
                        let publish = self.name(b"at:put:")?;
                        let return_at = self.name(b"returnAt:")?;
                        writeln!(out,"{{O g=named(&c,{globals});O a[]={{symbols[{name}],value_{v}}};O published=call(&c,g,{publish},a,2);if(c.error)return finish(&c,0);if(published!=value_{v}){{frame_put(&c,machine,{slot},published,{put});O at=integer(&c,{slot});O result=call(&c,machine,{return_at},&at,1);return finish(&c,result);}}}}",v=operation.result.0,slot=slot(operation.result)).unwrap();
                    }
                }
            }
            use ir::Terminator as Term;
            let (name, arg) = match &block.terminator {
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
                    let start = layout.call_window_start.ok_or(Error::InvalidProgram)?;
                    writeln!(out,"frame_put(&c,machine,{start},frame_get(&c,machine,{}, {get}),{put});frame_put(&c,machine,{},symbols[{}],{put});",slot(*receiver),start+1,self.name(selector)?).unwrap();
                    for (i, arg) in arguments.iter().enumerate() {
                        writeln!(
                            out,
                            "frame_put(&c,machine,{},frame_get(&c,machine,{}, {get}),{put});",
                            start + 2 + i as u32,
                            slot(*arg)
                        )
                        .unwrap();
                    }
                    (
                        if matches!(block.terminator, Term::TailCall { .. }) {
                            b"tailCallSite:".as_slice()
                        } else {
                            b"callSite:".as_slice()
                        },
                        call_site.0,
                    )
                }
                Term::Return(value) => (b"returnAt:".as_slice(), slot(*value)),
                Term::Continue(pc) => (b"continueAt:".as_slice(), pc.0),
            };
            writeln!(out,"O argument=integer(&c,{arg});O result=call(&c,machine,{},&argument,1);return finish(&c,result);}}",self.name(name)?).unwrap();
        }
        out.push_str("default:c.error=id.invalid;return finish(&c,0);}}\n");
        self.functions[index] = out;
        self.descriptors[index] = Some((descriptor, selection));
        Ok(index)
    }
    fn resumable_literal(
        &mut self,
        out: &mut String,
        value: &ir::Literal<'_>,
        index: u32,
    ) -> Result<String, Error> {
        use ir::Literal;
        Ok(match value {
            Literal::SmallInt(n) => format!("integer(&c,UINT64_C({}))", *n as u64),
            Literal::SmallIntSource(bytes) => format!(
                "integer(&c,UINT64_C({}))",
                std::str::from_utf8(bytes)
                    .map_err(|_| Error::InvalidProgram)?
                    .parse::<i64>()
                    .map_err(|_| Error::InvalidProgram)? as u64
            ),
            Literal::FloatBits(bits) => format!("float_bits(&c,UINT64_C({bits}))"),
            Literal::Float(bytes) => format!(
                "float_bits(&c,UINT64_C({}))",
                std::str::from_utf8(bytes)
                    .map_err(|_| Error::InvalidProgram)?
                    .parse::<f64>()
                    .map_err(|_| Error::InvalidProgram)?
                    .to_bits()
            ),
            Literal::String(bytes) => {
                let values = if bytes.is_empty() {
                    "0".to_owned()
                } else {
                    bytes
                        .iter()
                        .map(u8::to_string)
                        .collect::<Vec<_>>()
                        .join(",")
                };
                writeln!(
                    out,
                    "static const unsigned char text{index}[]={{{values}}};"
                )
                .unwrap();
                format!("string(&c,text{index},{})", bytes.len())
            }
            Literal::StringSource(_) => return Err(Error::InvalidProgram),
            Literal::Symbol(name) => format!("symbols[{}]", self.name(name)?),
            Literal::Nil => format!("named(&c,{})", self.name(b"nil")?),
            Literal::True => format!("named(&c,{})", self.name(b"true")?),
            Literal::False => format!("named(&c,{})", self.name(b"false")?),
        })
    }
}
