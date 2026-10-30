//! Oop spelling of the retained logical IR and canonical frame descriptor.
use super::*;
use birnam_artifact::stackless_selection::{
    self as selection, Disposition, ImplementationKind, RootKind, SelectionSpec,
};
use birnam_codegen::{resumable as ir, resumable_descriptor};
impl Emitter {
    pub(super) fn resumable_function(
        &mut self,
        bytes: &[u8],
        source: &[u8],
        filename: &[u8],
        depth: usize,
    ) -> Result<usize, Error> {
        if depth > 256 {
            return Err(Error::DepthLimit);
        }
        let program = ir::lower_portable_block(bytes).map_err(|_| Error::InvalidProgram)?;
        let layout = ir::color_slots(&program).map_err(|_| Error::InvalidProgram)?;
        let index = self.functions.len();
        self.functions.push(String::new());
        self.programs.push(bytes.to_vec());
        self.descriptors.push(None);
        let revision = self.source_revision;
        let path = [index as u32];
        let authority = SelectionSpec {
            source_revision: revision,
            root_kind: RootKind::NonMethodProgram,
            root_index: 0,
            path: &path,
            implementation_kind: ImplementationKind::BlockBody,
            disposition: Disposition::ResumableRequired,
        };
        let descriptor = resumable_descriptor::compile_oop_descriptor_v2(
            &program, &layout, authority, filename, source,
        )
        .map_err(|_| Error::InvalidProgram)?;
        let selection = selection::encode_artifact_selection(&[revision], &[authority])
            .map_err(|_| Error::InvalidProgram)?;
        let get = self.name(b"oopAt:")?;
        let put = self.name(b"oopAt:put:")?;
        let slot = |v: ir::ValueId| layout.value_slots[v.0 as usize];
        // The descriptor requests an immediate Oop counter. Internal routing must
        // not call the Integer methods that this generated body may implement.
        let mut out = format!(
            "static O function_{index}(O root,O context,O machine,const O*args,size_t n){{(void)root;(void)context;if(n!=1||!((uintptr_t)args[0]&1))return id.invalid;Call c={{0}};size_t pc=(uintptr_t)args[0]>>1;switch(pc){{\n"
        );
        let mut plans = Vec::new();
        let mut plan_count = 0u32;
        for block in &program.blocks {
            writeln!(out, "case {}:{{", block.pc.0).unwrap();
            use ir::OperationKind as Op;
            let copies = if !(depth == 0 && self.persistent)
                && matches!(&block.terminator, ir::Terminator::Call { arguments, .. } | ir::Terminator::TailCall { arguments, .. } if arguments.len() <= 16)
            {
                block
                    .operations
                    .iter()
                    .rev()
                    .take_while(|op| matches!(op.kind, Op::Bind { .. }))
                    .count()
            } else {
                0
            };
            let copies = if copies <= 32 { copies } else { 0 };
            for operation in &block.operations[..block.operations.len() - copies] {
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
                        let child =
                            self.selected_function(program, source, filename, depth + 1, false)?;
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
                if depth == 0 && self.persistent {
                    if let Op::Bind { name, .. } = &operation.kind {
                        let name = self.name(name)?;
                        let globals = self.name(b"Globals")?;
                        let publish = self.name(b"at:put:")?;
                        let return_at = self.name(b"returnAt:")?;
                        writeln!(out,"{{O g=named(&c,{globals});O a[]={{symbols[{name}],value_{v}}};O published=call(&c,g,{publish},a,2);if(c.error)return finish(&c,0);if(published!=value_{v}){{frame_put(&c,machine,{slot},published,{put});O at=size_oop({slot});O result=call(&c,machine,{return_at},&at,1);return finish(&c,result);}}}}",v=operation.result.0,slot=slot(operation.result)).unwrap();
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
                    if call_site.0 != plan_count {
                        return Err(Error::InvalidProgram);
                    }
                    plan_count += 1;
                    for value in [block.pc.0, arguments.len() as u32 + 1, copies as u32] {
                        plans.extend_from_slice(&value.to_le_bytes());
                    }
                    for source in std::iter::once(receiver).chain(arguments.iter()) {
                        plans.extend_from_slice(&slot(*source).to_le_bytes());
                    }
                    for op in &block.operations[block.operations.len() - copies..] {
                        let Op::Bind { source, .. } = op.kind else {
                            unreachable!()
                        };
                        plans.extend_from_slice(&slot(source).to_le_bytes());
                        plans.extend_from_slice(&slot(op.result).to_le_bytes());
                    }
                    let prepare = self.name(b"prepareCall:selector:")?;
                    let selector = self.name(selector)?;
                    writeln!(out,"O a[]={{size_oop({}),symbols[{selector}]}};O result=call(&c,machine,{prepare},a,2);return finish(&c,result);}}",call_site.0).unwrap();
                    continue;
                }
                Term::Return(value) => (b"returnAt:".as_slice(), slot(*value)),
                Term::Continue(pc) => (b"continueAt:".as_slice(), pc.0),
            };
            writeln!(out,"O argument=size_oop({arg});O result=call(&c,machine,{},&argument,1);return finish(&c,result);}}",self.name(name)?).unwrap();
        }
        out.push_str("default:c.error=id.invalid;return finish(&c,0);}}\n");
        self.functions[index] = out;
        let mut call_plans = 1u32.to_le_bytes().to_vec();
        call_plans.extend_from_slice(&plan_count.to_le_bytes());
        call_plans.extend_from_slice(&plans);
        self.descriptors[index] = Some(ResumableMetadata {
            descriptor,
            selection,
            call_plans,
        });
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
