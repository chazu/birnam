//! Durable native-object artifacts: the existing inert descriptor plus generated
//! code whose initializer returns an owned Method and its original source.
use crate::{OwnedCst, source_revision};
use birnam_artifact::{ABI, DESCRIPTOR_MAGIC, TARGET};
use std::fmt::Write;

pub const CAPABILITY: &[u8] = b"id-object-v1";
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
pub fn emit(tree: &OwnedCst, source: &[u8], generated: &[u8], path: &[u8]) -> Option<Vec<u8>> {
    let revision = source_revision::inspect(tree, source);
    let module = source_revision::artifact_module(tree, source);
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
        &source_revision::native_requirements(tree, source),
    );
    list(
        &mut payload,
        &[
            b"activate".to_vec(),
            b"source".to_vec(),
            b"reflection".to_vec(),
            CAPABILITY.to_vec(),
        ],
    );
    let mut descriptor = DESCRIPTOR_MAGIC.to_vec();
    descriptor.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    descriptor.extend_from_slice(&payload);
    let mut out = "#define birnam_native_module_v1 artifact_code_module\n".to_owned();
    out.push_str(std::str::from_utf8(generated).ok()?);
    out.push_str("\n#undef birnam_native_module_v1\n");
    array(&mut out, "__birnam_artifact_descriptor", &descriptor);
    array(&mut out, "__birnam_artifact_source", source);
    out.push_str("static R artifact_initialize(const P*p,const A*a,size_t n){\nR r=artifact_code_module.initialize(p,a,n);if(r.status||r.value.kind!=1)return r;Call c={0};O method=keep(&c,r.value.data.object);\n");
    out.push_str("if(method==named(&c,\"nil\")||call(&c,method,\"isError\",0,0)==named(&c,\"true\"))return finish(&c,obj(method));\n");
    writeln!(out, "O source=string(&c,__birnam_artifact_source,{});O result=call(&c,named(&c,\"Vector\"),\"new\",0,0);A v=obj(method);result=call(&c,result,\"add:\",&v,1);v=obj(source);result=call(&c,result,\"add:\",&v,1);return finish(&c,obj(result));}}\nconst Module birnam_native_module_v1={{1,sizeof(Module),artifact_initialize}};", source.len()).unwrap();
    Some(out.into_bytes())
}
