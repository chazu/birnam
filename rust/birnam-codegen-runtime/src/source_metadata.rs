//! Retained metadata belongs to ordinary compilation records. Source registries
//! and revision snapshots retain those records; no Store or global metadata cache
//! participates. Compiled admission never falls back to parsing.
use super::*;
use birnam_codegen::{resumable::*, source_metadata::Metadata, source_revision::*};

unsafe fn symbol(s: &State, q: &Scope<'_>, bytes: &[u8]) -> Result<Oop, Oop> {
    unsafe { Ok(q.keep(s.intern(bytes)?)) }
}
pub(super) unsafe fn seq(s: &State, q: &Scope<'_>, value: Oop) -> Result<Vec<Oop>, Oop> {
    unsafe {
        if !s.exact(q, value, b"VectorVt")? {
            return Err(invalid());
        }
        let n = s.index(q.keep(s.send(value, b"size", &[])?))?;
        if n > 1_000_000 {
            return Err(invalid());
        }
        (0..n)
            .map(|i| {
                let i = integer(s, q, i)?;
                Ok(q.keep(s.send(value, b"at:", &[i])?))
            })
            .collect()
    }
}
unsafe fn integer(s: &State, q: &Scope<'_>, n: usize) -> Result<Oop, Oop> {
    unsafe {
        Ok(q.keep(s.send(
            s.named(b"Integer")?,
            b"fromWord:",
            &[abi::size(n).ok_or_else(invalid)?],
        )?))
    }
}
unsafe fn number(s: &State, value: Oop) -> Result<u32, Oop> {
    unsafe {
        let q = s.scope(&[value]);
        let word = q.keep(s.send(s.named(b"Integer")?, b"wordOf:", &[value])?);
        let bits = if let Some(v) = abi::as_size(word) {
            v
        } else {
            let lo = abi::as_size(s.send(word, b"low32", &[])?).ok_or_else(invalid)?;
            if abi::as_size(s.send(word, b"high32", &[])?) != Some(0) {
                return Err(invalid());
            }
            lo
        };
        u32::try_from(bits).map_err(|_| invalid())
    }
}
unsafe fn bytes(s: &State, q: &Scope<'_>, b: &[u8]) -> Result<Oop, Oop> {
    unsafe {
        let factory = q.keep(s.named(b"Bytes")?);
        let values = q.keep(s.byte_values(b)?);
        Ok(q.keep(s.send(factory, b"fromBytes:", &[values])?))
    }
}
unsafe fn symbols(s: &State, q: &Scope<'_>, values: &[Vec<u8>]) -> Result<Oop, Oop> {
    unsafe {
        let values = values
            .iter()
            .map(|b| symbol(s, q, b))
            .collect::<Result<Vec<_>, _>>()?;
        s.vector(q, &values)
    }
}
unsafe fn texts(s: &State, q: &Scope<'_>, values: &[Vec<u8>]) -> Result<Oop, Oop> {
    unsafe {
        let values = values
            .iter()
            .map(|b| s.source_text(q, b))
            .collect::<Result<Vec<_>, _>>()?;
        s.vector(q, &values)
    }
}
unsafe fn read_symbol(s: &State, q: &Scope<'_>, v: Oop) -> Result<Vec<u8>, Oop> {
    unsafe { s.symbol_bytes(q, v)?.ok_or_else(invalid) }
}
unsafe fn read_text(s: &State, q: &Scope<'_>, v: Oop) -> Result<Vec<u8>, Oop> {
    unsafe { s.source_bytes(q, v)?.ok_or_else(invalid) }
}
unsafe fn read_bytes(s: &State, q: &Scope<'_>, v: Oop) -> Result<Vec<u8>, Oop> {
    unsafe {
        if !s.exact(q, v, b"BytesVt")? {
            return Err(invalid());
        }
        s.bytes(v)
    }
}
unsafe fn read_symbols(s: &State, q: &Scope<'_>, v: Oop) -> Result<Vec<Vec<u8>>, Oop> {
    unsafe {
        seq(s, q, v)?
            .into_iter()
            .map(|v| read_symbol(s, q, v))
            .collect()
    }
}
unsafe fn read_texts(s: &State, q: &Scope<'_>, v: Oop) -> Result<Vec<Vec<u8>>, Oop> {
    unsafe {
        seq(s, q, v)?
            .into_iter()
            .map(|v| read_text(s, q, v))
            .collect()
    }
}
unsafe fn digest(s: &State, q: &Scope<'_>, v: Oop) -> Result<[u8; 64], Oop> {
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

pub(super) unsafe fn reflect(s: &State, q: &Scope<'_>, m: &Metadata) -> Result<Oop, Oop> {
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
                    s.source_text(q, &op.source)?,
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
                    s.source_text(q, &r.initializer_residual_digest)?,
                    s.source_text(q, &r.non_method_digest)?,
                    s.source_text(q, &r.method_digest)?,
                    s.source_text(q, &r.revision_digest)?,
                ],
            )?
        } else {
            nil
        };
        s.vector(
            q,
            &[
                symbol(s, q, b"compiledSourceMetadataV1")?,
                s.source_text(q, &m.source_digest)?,
                name,
                requires,
                revision,
                texts(s, q, &m.method_sources)?,
                texts(s, q, &m.embedded_files)?,
            ],
        )
    }
}
unsafe fn decode(s: &State, q: &Scope<'_>, value: Oop) -> Result<Metadata, Oop> {
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
pub(super) unsafe fn read(s: &State, q: &Scope<'_>, source: Oop) -> Result<Option<Metadata>, Oop> {
    unsafe {
        let owner = q.keep(s.named(b"Codegen")?);
        let record = q.keep(s.send(owner, b"compiledSourceFor:", &[source])?);
        if record == s.named(b"nil")? {
            return Ok(None);
        }
        let value = q.keep(s.send(owner, b"retainedSourceMetadataOf:", &[record])?);
        if value == s.named(b"nil")? {
            return Ok(None);
        }
        let metadata = decode(s, q, value)?;
        if !metadata.matches_source(&s.source_bytes(q, source)?.ok_or_else(invalid)?) {
            return Err(invalid());
        }
        Ok(Some(metadata))
    }
}
/// Construct an imageable owner without changing a database or global registry.
pub(super) unsafe fn retain_metadata(
    s: &State,
    q: &Scope<'_>,
    source: Oop,
    metadata: &Metadata,
) -> Result<Oop, Oop> {
    unsafe {
        let text = q.keep(s.send(s.named(b"Codegen")?, b"sourceTextOf:", &[source])?);
        if !metadata.matches_source(&s.bytes(text)?) {
            return Err(invalid());
        }
        let value = reflect(s, q, metadata)?;
        Ok(q.keep(s.send(
            s.named(b"Codegen")?,
            b"compiledSource:metadata:",
            &[text, value],
        )?))
    }
}

/// Prepare once and hand the owning record to the caller; no implicit retention.
pub(super) unsafe fn prepare(s: &State, q: &Scope<'_>, source: Oop) -> Result<Option<Oop>, Oop> {
    unsafe {
        if compiled_source::is_record(s, q, source)? {
            return Ok(read(s, q, source)?.map(|_| source));
        }
        let Some(metadata) = inspect_fresh(s, q, source)? else {
            return Ok(None);
        };
        retain_metadata(s, q, source, &metadata).map(Some)
    }
}

/// Parse an explicit source request into operation-local compiler data.
/// The CST and metadata contain copied bytes, not retained language objects.
unsafe fn inspect_fresh(s: &State, q: &Scope<'_>, source: Oop) -> Result<Option<Metadata>, Oop> {
    unsafe {
        let Some(bytes) = s.source_bytes(q, source)? else {
            return Ok(None);
        };
        let Some(tree) = s.source_tree(q, source, bytes.len())? else {
            return Ok(None);
        };
        Ok(Some(Metadata::inspect(&tree, &bytes)))
    }
}

/// Inspect text locally, or read a caller-supplied compilation record. Raw text
/// has no attached metadata; inspection never registers or repairs a compilation.
pub(super) unsafe fn inspect(
    s: &State,
    q: &Scope<'_>,
    source: Oop,
) -> Result<Option<Metadata>, Oop> {
    unsafe {
        if compiled_source::is_record(s, q, source)? {
            return read(s, q, source);
        }
        inspect_fresh(s, q, source)
    }
}
