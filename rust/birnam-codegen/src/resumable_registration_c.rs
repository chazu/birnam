//! Exact registration-envelope v1 C production.
//!
//! The common declaration is emitted once per translation unit. Each entry
//! owns descriptor bytes, a token cell, and Prepare/Abort helpers. Physical
//! generation identity and lease live only in the artifact's initializer-
//! scoped context, never in canonical artifact bytes. Candidate staging is
//! serialized by the owner-thread quiescence guard, so this context is
//! artifact-local rather than dynamic TLS (which TinyCC cannot relocate
//! correctly in a dlopened Mach-O image).

use birnam_artifact::stackless_selection::ImplementationKind;
use sha2::{Digest, Sha256};

use crate::Emitter;

#[derive(Clone, Copy, Debug)]
pub struct RegistrationSpec<'a> {
    pub symbol_index: u32,
    pub manifest_entry_index: u32,
    pub implementation_kind: ImplementationKind,
    pub descriptor: &'a [u8],
    pub portable_implementation_identity: u64,
    pub language_arity: u32,
    pub driver_argument_count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    EmptyDescriptor,
    InvalidDriverArgumentCount,
}

/// Exact target declaration shared by every emitted registration operation.
pub const fn registration_prelude_v1() -> &'static [u8] {
    br#"typedef void* BirnamOopV1;
typedef BirnamOopV1 (*BirnamMethodV1)(BirnamOopV1, BirnamOopV1 const*, unsigned long);
typedef BirnamMethodV1 BirnamStepMethodV1;
typedef struct BirnamResumableRegistrationV1 {
unsigned int carrier_version;
unsigned int struct_size;
unsigned int operation;
unsigned int flags;
unsigned int manifest_entry_index;
unsigned int reserved0;
unsigned long long registration_token;
unsigned char const* descriptor_bytes;
unsigned long long descriptor_length;
BirnamMethodV1 entry_thunk;
BirnamStepMethodV1 step_method;
BirnamOopV1 implementation_oop;
unsigned long long code_generation_identity;
unsigned long long code_generation_lease;
unsigned long long portable_implementation_identity;
unsigned int language_arity;
unsigned int driver_argument_count;
unsigned char descriptor_sha256[32];
} BirnamResumableRegistrationV1;
extern unsigned int birnam_register_resumable_v1(BirnamResumableRegistrationV1*);
typedef struct BirnamResumableContextV1 {
unsigned long long generation_identity;
unsigned long long generation_lease;
unsigned int failed;
} BirnamResumableContextV1;
static BirnamResumableContextV1 __birnam_resumable_context_v1;
unsigned int __birnam_resumable_context_begin_v1(unsigned long long generation_identity,unsigned long long generation_lease){
if(generation_identity==0ULL||generation_lease==0ULL||__birnam_resumable_context_v1.generation_identity!=0ULL||__birnam_resumable_context_v1.generation_lease!=0ULL)return 1U;
__birnam_resumable_context_v1.generation_identity=generation_identity;__birnam_resumable_context_v1.generation_lease=generation_lease;__birnam_resumable_context_v1.failed=0U;return 0U;
}
unsigned int __birnam_resumable_context_end_v1(void){
unsigned int status=(__birnam_resumable_context_v1.generation_identity==0ULL||__birnam_resumable_context_v1.generation_lease==0ULL||__birnam_resumable_context_v1.failed!=0U)?1U:0U;
__birnam_resumable_context_v1.generation_identity=0ULL;__birnam_resumable_context_v1.generation_lease=0ULL;__birnam_resumable_context_v1.failed=0U;return status;
}
_Static_assert(sizeof(void*)==8,"Birnam v1 Oop width");
_Static_assert(sizeof(unsigned long)==8,"Birnam v1 size_t width");
_Static_assert(sizeof(BirnamMethodV1)==8,"Birnam v1 Method width");
_Static_assert(_Alignof(BirnamResumableRegistrationV1)==8,"Birnam registration alignment");
_Static_assert(sizeof(BirnamResumableRegistrationV1)==136,"Birnam registration size");
_Static_assert(__builtin_offsetof(BirnamResumableRegistrationV1,carrier_version)==0x00,"carrier_version offset");
_Static_assert(__builtin_offsetof(BirnamResumableRegistrationV1,struct_size)==0x04,"struct_size offset");
_Static_assert(__builtin_offsetof(BirnamResumableRegistrationV1,operation)==0x08,"operation offset");
_Static_assert(__builtin_offsetof(BirnamResumableRegistrationV1,flags)==0x0c,"flags offset");
_Static_assert(__builtin_offsetof(BirnamResumableRegistrationV1,manifest_entry_index)==0x10,"manifest_entry_index offset");
_Static_assert(__builtin_offsetof(BirnamResumableRegistrationV1,reserved0)==0x14,"reserved0 offset");
_Static_assert(__builtin_offsetof(BirnamResumableRegistrationV1,registration_token)==0x18,"registration_token offset");
_Static_assert(__builtin_offsetof(BirnamResumableRegistrationV1,descriptor_bytes)==0x20,"descriptor_bytes offset");
_Static_assert(__builtin_offsetof(BirnamResumableRegistrationV1,descriptor_length)==0x28,"descriptor_length offset");
_Static_assert(__builtin_offsetof(BirnamResumableRegistrationV1,entry_thunk)==0x30,"entry_thunk offset");
_Static_assert(__builtin_offsetof(BirnamResumableRegistrationV1,step_method)==0x38,"step_method offset");
_Static_assert(__builtin_offsetof(BirnamResumableRegistrationV1,implementation_oop)==0x40,"implementation_oop offset");
_Static_assert(__builtin_offsetof(BirnamResumableRegistrationV1,code_generation_identity)==0x48,"code_generation_identity offset");
_Static_assert(__builtin_offsetof(BirnamResumableRegistrationV1,code_generation_lease)==0x50,"code_generation_lease offset");
_Static_assert(__builtin_offsetof(BirnamResumableRegistrationV1,portable_implementation_identity)==0x58,"portable_implementation_identity offset");
_Static_assert(__builtin_offsetof(BirnamResumableRegistrationV1,language_arity)==0x60,"language_arity offset");
_Static_assert(__builtin_offsetof(BirnamResumableRegistrationV1,driver_argument_count)==0x64,"driver_argument_count offset");
_Static_assert(__builtin_offsetof(BirnamResumableRegistrationV1,descriptor_sha256)==0x68,"descriptor_sha256 offset");
"#
}

/// Emit one entry's immutable descriptor and context-consuming
/// Prepare/Abort operations.
pub fn emit_registration_v1(spec: RegistrationSpec<'_>) -> Result<Vec<u8>, Error> {
    if spec.descriptor.is_empty() {
        return Err(Error::EmptyDescriptor);
    }
    if spec.driver_argument_count > 1 {
        return Err(Error::InvalidDriverArgumentCount);
    }
    let digest: [u8; 32] = Sha256::digest(spec.descriptor).into();
    let mut output = Emitter::with_capacity(spec.descriptor.len() * 4 + 2048);

    output.push(b"static const unsigned char __birnam_resumable_descriptor_");
    decimal(&mut output, spec.symbol_index);
    output.push(b"[]={");
    emit_byte_list(&mut output, spec.descriptor);
    output.push(b"};\nstatic unsigned long long __birnam_resumable_token_");
    decimal(&mut output, spec.symbol_index);
    output.push(b"=0ULL;\n");

    output.push(b"unsigned int __birnam_resumable_prepare_");
    decimal(&mut output, spec.symbol_index);
    output.push(b"(void* implementation){\nif(__birnam_resumable_context_v1.generation_identity==0ULL||__birnam_resumable_context_v1.generation_lease==0ULL)return 1U;\n");
    if spec.implementation_kind == ImplementationKind::BlockBody {
        output.push(b"(void)implementation;\n");
    }
    output.push(b"BirnamResumableRegistrationV1 e={0};\ne.carrier_version=1U;e.struct_size=136U;e.operation=1U;e.manifest_entry_index=");
    decimal(&mut output, spec.manifest_entry_index);
    output.push(b"U;e.descriptor_bytes=__birnam_resumable_descriptor_");
    decimal(&mut output, spec.symbol_index);
    output.push(b";e.descriptor_length=");
    output.push(spec.descriptor.len().to_string().as_bytes());
    output.push(b"ULL;e.entry_thunk=&__birnam_resumable_thunk_");
    decimal(&mut output, spec.symbol_index);
    output.push(b";e.step_method=&__birnam_resumable_step_");
    decimal(&mut output, spec.symbol_index);
    output.push(b";e.implementation_oop=");
    match spec.implementation_kind {
        ImplementationKind::NamedAdapter => output.push(b"implementation"),
        ImplementationKind::BlockBody => output.push(b"(void*)0"),
    }
    output.push(b";e.code_generation_identity=__birnam_resumable_context_v1.generation_identity;e.code_generation_lease=__birnam_resumable_context_v1.generation_lease;e.portable_implementation_identity=");
    output.push(spec.portable_implementation_identity.to_string().as_bytes());
    output.push(b"ULL;e.language_arity=");
    decimal(&mut output, spec.language_arity);
    output.push(b"U;e.driver_argument_count=");
    decimal(&mut output, spec.driver_argument_count);
    output.push(b"U;\nstatic const unsigned char h[32]={");
    emit_byte_list(&mut output, &digest);
    output.push(b"};for(unsigned int i=0;i<32U;i++)e.descriptor_sha256[i]=h[i];\nunsigned int status=birnam_register_resumable_v1(&e);if(status==0U)__birnam_resumable_token_");
    decimal(&mut output, spec.symbol_index);
    output.push(
        b"=e.registration_token;else __birnam_resumable_context_v1.failed=1U;return status;\n}\n",
    );

    output.push(b"unsigned int __birnam_resumable_abort_");
    decimal(&mut output, spec.symbol_index);
    output.push(b"(void){\nif(__birnam_resumable_token_");
    decimal(&mut output, spec.symbol_index);
    output.push(b"==0ULL)return 1U;BirnamResumableRegistrationV1 e={0};e.carrier_version=1U;e.struct_size=136U;e.operation=2U;e.manifest_entry_index=");
    decimal(&mut output, spec.manifest_entry_index);
    output.push(b"U;e.registration_token=__birnam_resumable_token_");
    decimal(&mut output, spec.symbol_index);
    output.push(b";unsigned int status=birnam_register_resumable_v1(&e);if(status==0U)__birnam_resumable_token_");
    decimal(&mut output, spec.symbol_index);
    output.push(b"=0ULL;else __birnam_resumable_context_v1.failed=1U;return status;\n}\n");

    Ok(output.finish())
}

fn emit_byte_list(output: &mut Emitter, bytes: &[u8]) {
    for (index, byte) in bytes.iter().enumerate() {
        if index != 0 {
            output.push(b",");
        }
        output.push(byte.to_string().as_bytes());
    }
}

fn decimal(output: &mut Emitter, value: u32) {
    output.push(value.to_string().as_bytes());
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Command;

    use super::*;

    #[test]
    fn emits_exact_prepare_and_abort_shapes_without_baking_physical_context() {
        let source = String::from_utf8(
            emit_registration_v1(RegistrationSpec {
                symbol_index: 4,
                manifest_entry_index: 7,
                implementation_kind: ImplementationKind::BlockBody,
                descriptor: b"descriptor",
                portable_implementation_identity: 23,
                language_arity: 2,
                driver_argument_count: 1,
            })
            .unwrap(),
        )
        .unwrap();
        assert!(source.contains("__birnam_resumable_prepare_4(void* implementation)"));
        assert!(source.contains("e.manifest_entry_index=7U"));
        assert!(source.contains("e.implementation_oop=(void*)0"));
        assert!(source.contains(
            "e.code_generation_identity=__birnam_resumable_context_v1.generation_identity"
        ));
        assert!(
            source
                .contains("e.code_generation_lease=__birnam_resumable_context_v1.generation_lease")
        );
        assert!(source.contains("__birnam_resumable_abort_4(void)"));
        assert!(!source.contains("16909060ULL"));
    }

    #[test]
    fn prelude_exports_fail_closed_artifact_local_context_brackets() {
        let prelude = String::from_utf8(registration_prelude_v1().to_vec()).unwrap();
        assert!(prelude.contains("static BirnamResumableContextV1 __birnam_resumable_context_v1"));
        assert!(!prelude.contains("_Thread_local"));
        assert!(prelude.contains("__birnam_resumable_context_begin_v1(unsigned long long generation_identity,unsigned long long generation_lease)"));
        assert!(prelude.contains("__birnam_resumable_context_end_v1(void)"));
        assert!(prelude.contains("__birnam_resumable_context_v1.failed!=0U"));
    }

    #[test]
    fn emitted_context_brackets_execute_the_frozen_state_machine() {
        let directory = std::env::temp_dir();
        let stem = format!(
            "birnam-resumable-context-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        );
        let source_path = directory.join(format!("{stem}.c"));
        let executable_path = directory.join(stem);
        let mut source = registration_prelude_v1().to_vec();
        source.extend_from_slice(
            br#"int main(void){
if(__birnam_resumable_context_begin_v1(0ULL,13ULL)==0U)return 1;
if(__birnam_resumable_context_end_v1()==0U)return 2;
if(__birnam_resumable_context_begin_v1(11ULL,13ULL)!=0U)return 3;
if(__birnam_resumable_context_begin_v1(17ULL,19ULL)==0U)return 4;
if(__birnam_resumable_context_end_v1()!=0U)return 5;
if(__birnam_resumable_context_begin_v1(17ULL,19ULL)!=0U)return 6;
__birnam_resumable_context_v1.failed=1U;
if(__birnam_resumable_context_end_v1()==0U)return 7;
if(__birnam_resumable_context_begin_v1(23ULL,29ULL)!=0U)return 8;
return __birnam_resumable_context_end_v1()==0U?0:9;
}
"#,
        );
        fs::write(&source_path, source).unwrap();
        let compiled = Command::new("cc")
            .args(["-std=c11", "-Wall", "-Wextra", "-Werror"])
            .arg(&source_path)
            .arg("-o")
            .arg(&executable_path)
            .status()
            .unwrap();
        assert!(compiled.success());
        let ran = Command::new(&executable_path).status().unwrap();
        let _ = fs::remove_file(source_path);
        let _ = fs::remove_file(executable_path);
        assert!(ran.success());
    }

    #[test]
    fn named_registration_uses_only_the_supplied_live_implementation() {
        let source = emit_registration_v1(RegistrationSpec {
            symbol_index: 1,
            manifest_entry_index: 0,
            implementation_kind: ImplementationKind::NamedAdapter,
            descriptor: b"x",
            portable_implementation_identity: 1,
            language_arity: 0,
            driver_argument_count: 0,
        })
        .unwrap();
        assert!(
            source
                .windows(b"e.implementation_oop=implementation".len())
                .any(|window| window == b"e.implementation_oop=implementation")
        );
    }
}
