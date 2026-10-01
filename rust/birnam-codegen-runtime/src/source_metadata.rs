//! Compiler metadata is ordinary relational data, owned and persisted by Store.
//! No AST, native cache, or parse fallback participates in compiled admission.
use super::*;
use birnam_codegen::{resumable::*, source_metadata::Metadata, source_revision::*};

const ATTRIBUTE: &[u8] = b"compiledSourceMetadata";
unsafe fn symbol(s: &State, q: &Scope<'_>, bytes: &[u8]) -> Result<Oop, Outcome> {
    unsafe { q.keep(s.id.intern_name(bytes)?) }
}
unsafe fn seq(s: &State, q: &Scope<'_>, value: Oop) -> Result<Vec<Oop>, Outcome> {
    unsafe {
        if !s.exact(q, value, b"VectorVt")? {
            return Err(invalid());
        }
        let n = s.index(q, value, b"size")?;
        if n > 1_000_000 {
            return Err(invalid());
        }
        (0..n)
            .map(|i| {
                let i = integer(s, q, i)?;
                q.keep(object(s.send(value, b"at:", &[Operand::object(i)])?)?)
            })
            .collect()
    }
}
unsafe fn integer(s: &State, q: &Scope<'_>, n: usize) -> Result<Oop, Outcome> {
    unsafe {
        q.keep(object(s.send(
            s.named(b"Integer")?,
            b"fromWord:",
            &[Operand::word(n)],
        )?)?)
    }
}
unsafe fn number(s: &State, value: Oop) -> Result<u32, Outcome> {
    unsafe {
        s.send(s.named(b"Integer")?, b"wordOf:", &[Operand::object(value)])?
            .as_word()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(invalid)
    }
}
unsafe fn bytes(s: &State, q: &Scope<'_>, b: &[u8]) -> Result<Oop, Outcome> {
    unsafe {
        q.keep(object(s.send(
            s.named(b"Bytes")?,
            b"fromBytes:",
            &[Operand::bytes(b)],
        )?)?)
    }
}
unsafe fn symbols(s: &State, q: &Scope<'_>, values: &[Vec<u8>]) -> Result<Oop, Outcome> {
    unsafe {
        let values = values
            .iter()
            .map(|b| symbol(s, q, b))
            .collect::<Result<Vec<_>, _>>()?;
        s.vector(q, &values)
    }
}
unsafe fn texts(s: &State, q: &Scope<'_>, values: &[Vec<u8>]) -> Result<Oop, Outcome> {
    unsafe {
        let values = values
            .iter()
            .map(|b| s.text(q, b))
            .collect::<Result<Vec<_>, _>>()?;
        s.vector(q, &values)
    }
}
unsafe fn read_symbol(s: &State, q: &Scope<'_>, v: Oop) -> Result<Vec<u8>, Outcome> {
    unsafe { s.symbol_bytes(q, v)?.ok_or_else(invalid) }
}
unsafe fn read_text(s: &State, q: &Scope<'_>, v: Oop) -> Result<Vec<u8>, Outcome> {
    unsafe { s.source_bytes(q, v)?.ok_or_else(invalid) }
}
unsafe fn read_bytes(s: &State, q: &Scope<'_>, v: Oop) -> Result<Vec<u8>, Outcome> {
    unsafe {
        if !s.exact(q, v, b"BytesVt")? {
            return Err(invalid());
        }
        s.bytes(v)
    }
}
unsafe fn read_symbols(s: &State, q: &Scope<'_>, v: Oop) -> Result<Vec<Vec<u8>>, Outcome> {
    unsafe {
        seq(s, q, v)?
            .into_iter()
            .map(|v| read_symbol(s, q, v))
            .collect()
    }
}
unsafe fn read_texts(s: &State, q: &Scope<'_>, v: Oop) -> Result<Vec<Vec<u8>>, Outcome> {
    unsafe {
        seq(s, q, v)?
            .into_iter()
            .map(|v| read_text(s, q, v))
            .collect()
    }
}
unsafe fn digest(s: &State, q: &Scope<'_>, v: Oop) -> Result<[u8; 64], Outcome> {
    unsafe {
        let b: [u8; 64] = read_text(s, q, v)?.try_into().map_err(|_| invalid())?;
        if !b
            .iter()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
        {
            return Err(invalid());
        }
        Ok(b)
    }
}

pub(super) unsafe fn reflect(s: &State, q: &Scope<'_>, m: &Metadata) -> Result<Oop, Outcome> {
    unsafe {
        let nil = s.named(b"nil")?;
        let (name, requires) = if let Some((name, requires)) = &m.module {
            (symbol(s, q, name)?, symbols(s, q, requires)?)
        } else {
            (nil, s.vector(q, &[])?)
        };
        let revision = if let Some(r) = &m.revision {
            let mut operations = Vec::new();
            for op in &r.operations {
                let row = [
                    symbol(s, q, &op.target)?,
                    symbol(s, q, &op.selector)?,
                    s.named(if op.install { b"true" } else { b"false" })?,
                    s.text(q, &op.source)?,
                    if let Some(p) = &op.program {
                        bytes(s, q, p)?
                    } else {
                        nil
                    },
                ];
                operations.push(s.vector(q, &row)?);
            }
            let mut globals = Vec::new();
            for g in &r.projected_globals {
                globals.push(s.vector(q, &[symbol(s, q, &g.name)?, bytes(s, q, &g.program)?])?);
            }
            let mut inventory = Vec::new();
            for i in &r.inventory {
                let path = i
                    .path
                    .iter()
                    .map(|n| integer(s, q, *n as usize))
                    .collect::<Result<Vec<_>, _>>()?;
                let row = [
                    symbol(
                        s,
                        q,
                        match i.root_kind {
                            InventoryRootKind::NonMethodProgram => b"nonMethodProgram",
                            InventoryRootKind::MethodOperation => b"methodOperation",
                        },
                    )?,
                    integer(s, q, i.root_index as usize)?,
                    s.vector(q, &path)?,
                    symbol(
                        s,
                        q,
                        match i.implementation_kind {
                            InventoryImplementationKind::NamedAdapter => b"namedAdapter",
                            InventoryImplementationKind::BlockBody => b"blockBody",
                        },
                    )?,
                ];
                inventory.push(s.vector(q, &row)?);
            }
            s.vector(
                q,
                &[
                    symbol(s, q, &r.name)?,
                    symbols(s, q, &r.requires)?,
                    s.vector(q, &operations)?,
                    s.vector(q, &globals)?,
                    s.vector(q, &inventory)?,
                    s.text(q, &r.initializer_residual_digest)?,
                    s.text(q, &r.non_method_digest)?,
                    s.text(q, &r.method_digest)?,
                    s.text(q, &r.revision_digest)?,
                ],
            )?
        } else {
            nil
        };
        s.vector(
            q,
            &[
                symbol(s, q, b"compiledSourceMetadataV1")?,
                s.text(q, &m.source_digest)?,
                name,
                requires,
                revision,
                texts(s, q, &m.method_sources)?,
                texts(s, q, &m.embedded_files)?,
            ],
        )
    }
}
unsafe fn decode(s: &State, q: &Scope<'_>, value: Oop) -> Result<Metadata, Outcome> {
    unsafe {
        let fields = seq(s, q, value)?;
        let [
            tag,
            source_digest,
            name,
            requires,
            revision,
            methods,
            embeds,
        ] = fields.as_slice()
        else {
            return Err(invalid());
        };
        if read_symbol(s, q, *tag)? != b"compiledSourceMetadataV1" {
            return Err(invalid());
        }
        let nil = s.named(b"nil")?;
        let requires = read_symbols(s, q, *requires)?;
        let module = if *name == nil {
            if !requires.is_empty() {
                return Err(invalid());
            }
            None
        } else {
            Some((read_symbol(s, q, *name)?, requires))
        };
        let revision = if *revision == nil {
            None
        } else {
            let fields = seq(s, q, *revision)?;
            let [
                name,
                requires,
                ops,
                globals,
                inventory,
                residual,
                non_method,
                method,
                revision,
            ] = fields.as_slice()
            else {
                return Err(invalid());
            };
            let mut operations = Vec::new();
            for op in seq(s, q, *ops)? {
                let fields = seq(s, q, op)?;
                let [target, selector, install, source, program] = fields.as_slice() else {
                    return Err(invalid());
                };
                let install = if *install == s.named(b"true")? {
                    true
                } else if *install == s.named(b"false")? {
                    false
                } else {
                    return Err(invalid());
                };
                let program = if *program == nil {
                    None
                } else {
                    Some(read_bytes(s, q, *program)?)
                };
                if install != program.is_some() {
                    return Err(invalid());
                }
                operations.push(RevisionOperation {
                    target: read_symbol(s, q, *target)?,
                    selector: read_symbol(s, q, *selector)?,
                    install,
                    source: read_text(s, q, *source)?,
                    program,
                });
            }
            let mut projected_globals = Vec::new();
            for g in seq(s, q, *globals)? {
                let fields = seq(s, q, g)?;
                let [name, program] = fields.as_slice() else {
                    return Err(invalid());
                };
                projected_globals.push(ProjectedGlobal {
                    name: read_symbol(s, q, *name)?,
                    program: read_bytes(s, q, *program)?,
                });
            }
            let mut keys = Vec::new();
            for i in seq(s, q, *inventory)? {
                let fields = seq(s, q, i)?;
                let [root, index, path, kind] = fields.as_slice() else {
                    return Err(invalid());
                };
                let root_kind = match read_symbol(s, q, *root)?.as_slice() {
                    b"nonMethodProgram" => InventoryRootKind::NonMethodProgram,
                    b"methodOperation" => InventoryRootKind::MethodOperation,
                    _ => return Err(invalid()),
                };
                let implementation_kind = match read_symbol(s, q, *kind)?.as_slice() {
                    b"namedAdapter" => InventoryImplementationKind::NamedAdapter,
                    b"blockBody" => InventoryImplementationKind::BlockBody,
                    _ => return Err(invalid()),
                };
                let path = seq(s, q, *path)?
                    .into_iter()
                    .map(|v| number(s, v))
                    .collect::<Result<Vec<_>, _>>()?;
                keys.push(ImplementationKey {
                    root_kind,
                    root_index: number(s, *index)?,
                    path,
                    implementation_kind,
                });
            }
            if keys.windows(2).any(|w| w[0] >= w[1]) {
                return Err(invalid());
            }
            Some(RevisionMetadata {
                name: read_symbol(s, q, *name)?,
                requires: read_symbols(s, q, *requires)?,
                operations,
                projected_globals,
                inventory: keys,
                initializer_residual_digest: digest(s, q, *residual)?,
                non_method_digest: digest(s, q, *non_method)?,
                method_digest: digest(s, q, *method)?,
                revision_digest: digest(s, q, *revision)?,
            })
        };
        Ok(Metadata {
            source_digest: digest(s, q, *source_digest)?,
            module,
            revision,
            method_sources: read_texts(s, q, *methods)?,
            embedded_files: read_texts(s, q, *embeds)?,
        })
    }
}
pub(super) unsafe fn read(
    s: &State,
    q: &Scope<'_>,
    source: Oop,
) -> Result<Option<Metadata>, Outcome> {
    unsafe {
        let key = symbol(s, q, ATTRIBUTE)?;
        let value = q.keep(object(s.send(
            s.named(b"Store")?,
            b"currentValue:attr:",
            &[Operand::object(source), Operand::object(key)],
        )?)?)?;
        if value == s.named(b"nil")? {
            return Ok(None);
        }
        let metadata = decode(s, q, value)?;
        if !metadata.matches_source(&read_text(s, q, source)?) {
            return Err(invalid());
        }
        Ok(Some(metadata))
    }
}
pub(super) unsafe fn persist(
    s: &State,
    q: &Scope<'_>,
    source: Oop,
    metadata: &Metadata,
) -> Result<(), Outcome> {
    unsafe {
        if !metadata.matches_source(&read_text(s, q, source)?) {
            return Err(invalid());
        }
        if let Some(old) = read(s, q, source)? {
            return if old.encode() == metadata.encode() {
                Ok(())
            } else {
                Err(invalid())
            };
        }
        let value = reflect(s, q, metadata)?;
        let key = symbol(s, q, ATTRIBUTE)?;
        let store = s.named(b"Store")?;
        let result = object(s.send(
            store,
            b"assert:attr:value:",
            &[
                Operand::object(source),
                Operand::object(key),
                Operand::object(value),
            ],
        )?)?;
        if result != store {
            return Err(Outcome::loaded(Operand::object(result)));
        }
        Ok(())
    }
}
/// Explicit source compilation/inspection may parse; compiled admission calls read.
pub(super) unsafe fn analyze(
    s: &State,
    q: &Scope<'_>,
    source: Oop,
) -> Result<Option<Metadata>, Outcome> {
    unsafe {
        if let Some(metadata) = read(s, q, source)? {
            return Ok(Some(metadata));
        }
        let Some(bytes) = s.source_bytes(q, source)? else {
            return Ok(None);
        };
        let Some(tree) = s.source_tree(q, source, bytes.len())? else {
            return Ok(None);
        };
        let metadata = Metadata::inspect(&tree, &bytes);
        persist(s, q, source, &metadata)?;
        Ok(Some(metadata))
    }
}
