//! Durable native-object artifacts: the existing inert descriptor plus generated
//! code whose initializer returns an owned Method and its original source.
use crate::{OwnedCst, source_revision};
use birnam_artifact::{ABI, DESCRIPTOR_MAGIC, TARGET};
use std::fmt::Write;

pub const CAPABILITY: &[u8] = b"id-object-v3";
fn field(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(bytes);
}
fn list(out: &mut Vec<u8>, values: &[Vec<u8>]) {
    out.extend_from_slice(&(values.len() as u32).to_le_bytes());
    for value in values {
        field(out, value);
    }
}
fn array(out: &mut String, name: &str, bytes: &[u8]) {
    write!(out, "const unsigned char {name}[]={{").unwrap();
    for byte in bytes {
        write!(out, "{byte},").unwrap();
    }
    out.push_str("0};\n");
}
pub fn emit(
    tree: &OwnedCst,
    source: &[u8],
    generated: &[u8],
    path: &[u8],
    metadata: &crate::source_metadata::Metadata,
    adapters: &[(u32, Vec<u8>)],
    synchronous_adapters: bool,
) -> Option<Vec<u8>> {
    emit_with_embedded(tree, source, generated, path, metadata, adapters, synchronous_adapters, &[])
}

pub fn emit_with_embedded(
    tree: &OwnedCst, source: &[u8], generated: &[u8], path: &[u8],
    metadata: &crate::source_metadata::Metadata, adapters: &[(u32, Vec<u8>)],
    synchronous_adapters: bool, embedded: &[(Vec<u8>, Vec<u8>)],
) -> Option<Vec<u8>> {
    let encoded_metadata = metadata.encode()?;
    let revision = &metadata.revision;
    let module = &metadata.module;
    let fallback = || {
        let mut name = path.rsplit(|b| *b == b'/').next().unwrap_or(path);
        if let Some(dot) = name.iter().rposition(|b| *b == b'.') {
            name = &name[..dot];
        }
        name.strip_prefix(b"libbirnam_")
            .or_else(|| name.strip_prefix(b"lib"))
            .unwrap_or(name)
            .to_vec()
    };
    let identity = module
        .as_ref()
        .map(|m| m.0.clone())
        .unwrap_or_else(fallback);
    if identity.is_empty() {
        return None;
    }
    let digest = source_revision::source_digest(source);
    let mut payload = Vec::new();
    for value in [
        b"module".as_slice(),
        &identity,
        ABI,
        TARGET,
        &digest,
        revision
            .as_ref()
            .map(|m| m.revision_digest.as_slice())
            .unwrap_or(&digest),
    ] {
        field(&mut payload, value);
    }
    list(
        &mut payload,
        &module.as_ref().map(|m| m.1.clone()).unwrap_or_default(),
    );
    list(
        &mut payload,
        &source_revision::native_requirements(tree, source).into_iter().filter(|name| !embedded.iter().any(|(n, _)| n == name)).collect::<Vec<_>>(),
    );
    let mut capabilities = vec![b"activate".to_vec(), b"source".to_vec(),
        b"reflection".to_vec(), CAPABILITY.to_vec(), b"prepared-alien-v1".to_vec()];
    if !embedded.is_empty() { capabilities.push(b"embedded-native-hex-v1".to_vec()); }
    list(&mut payload, &capabilities);
    let mut descriptor = DESCRIPTOR_MAGIC.to_vec();
    descriptor.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    descriptor.extend_from_slice(&payload);
    let mut out = "#define birnam_native_module_v1 artifact_code_module\n".to_owned();
    out.push_str(std::str::from_utf8(generated).ok()?);
    out.push_str("\n#undef birnam_native_module_v1\n");
    array(&mut out, "__birnam_artifact_descriptor", &descriptor);
    array(&mut out, "__birnam_artifact_source", source);
    array(&mut out, "__birnam_artifact_metadata", &encoded_metadata);
    for (index, (name, bytes)) in embedded.iter().enumerate() {
        array(&mut out, &format!("__birnam_native_name_{index}"), name);
        // Native binaries can themselves contain descriptor magic. An inert
        // hex carrier keeps arbitrary payload bytes out of descriptor scanning.
        let encoded: Vec<u8> = bytes.iter().flat_map(|b| [b"0123456789abcdef"[(b >> 4) as usize], b"0123456789abcdef"[(b & 15) as usize]]).collect();
        array(&mut out, &format!("__birnam_native_bytes_{index}"), &encoded);
    }
    for (arity, bytes) in adapters {
        array(
            &mut out,
            &format!("__birnam_artifact_adapter_{arity}"),
            bytes,
        );
    }
    out.push_str("static R artifact_initialize(const P*p,const A*a,size_t n){\nR r=artifact_code_module.initialize(p,a,n);if(r.status||r.value.kind!=1)return r;Call c={0};O method=keep(&c,r.value.data.object);\n");
    out.push_str("if(method==named(&c,\"nil\")||is_error(&c,method))return finish(&c,obj(method));\n");
    writeln!(out, "O source=string(&c,__birnam_artifact_source,{});O metadata=string(&c,__birnam_artifact_metadata,{});O adapters=call(&c,named(&c,\"Vector\"),\"new\",0,0);A v;", source.len(), encoded_metadata.len()).unwrap();
    for (arity, bytes) in adapters {
        writeln!(out, "O row_{arity}=call(&c,named(&c,\"Vector\"),\"new\",0,0);v=obj(integer(&c,{arity}));row_{arity}=call(&c,row_{arity},\"add:\",&v,1);v=obj(string(&c,__birnam_artifact_adapter_{arity},{}));row_{arity}=call(&c,row_{arity},\"add:\",&v,1);v=obj(named(&c,\"{}\"));row_{arity}=call(&c,row_{arity},\"add:\",&v,1);v=obj(row_{arity});adapters=call(&c,adapters,\"add:\",&v,1);", bytes.len(), if synchronous_adapters { "true" } else { "false" }).unwrap();
    }
    out.push_str("O result=call(&c,named(&c,\"Vector\"),\"new\",0,0);v=obj(method);result=call(&c,result,\"add:\",&v,1);v=obj(source);result=call(&c,result,\"add:\",&v,1);v=obj(metadata);result=call(&c,result,\"add:\",&v,1);v=obj(adapters);result=call(&c,result,\"add:\",&v,1);");
    if !embedded.is_empty() {
        out.push_str("O libraries=call(&c,named(&c,\"Vector\"),\"new\",0,0);");
        for (index, (name, bytes)) in embedded.iter().enumerate() {
            writeln!(out,"O lib_{index}=call(&c,named(&c,\"Vector\"),\"new\",0,0);v=obj(string(&c,__birnam_native_name_{index},{}));lib_{index}=call(&c,lib_{index},\"add:\",&v,1);v=obj(string(&c,__birnam_native_bytes_{index},{}));lib_{index}=call(&c,lib_{index},\"add:\",&v,1);v=obj(lib_{index});libraries=call(&c,libraries,\"add:\",&v,1);",name.len(),bytes.len()*2).unwrap();
        }
        out.push_str("v=obj(libraries);result=call(&c,result,\"add:\",&v,1);");
    }
    out.push_str("return finish(&c,obj(result));}\nconst Module birnam_native_module_v1={1,sizeof(Module),artifact_initialize};");
    Some(out.into_bytes())
}
