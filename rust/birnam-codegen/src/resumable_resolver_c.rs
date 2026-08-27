//! Unit-local generated block-vtable resolver C.

use crate::Emitter;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockVtableSpec<'a> {
    pub manifest_entry_index: u32,
    pub symbol_index: u32,
    pub selector: &'a [u8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Empty,
    Order,
    DuplicateSymbol,
    Selector,
}

/// Emit the one private resolver exported by a unit containing block entries.
///
/// Registration remains eager and metadata-only. This resolver lazily creates
/// each private vtable, installs the already-registered entry thunk through the
/// ordinary method path, and returns the same identity on every later call.
pub fn emit_block_vtable_resolver_v1(entries: &[BlockVtableSpec<'_>]) -> Result<Vec<u8>, Error> {
    if entries.is_empty() {
        return Err(Error::Empty);
    }
    let mut previous_index = None;
    for (position, entry) in entries.iter().enumerate() {
        if previous_index.is_some_and(|previous| previous >= entry.manifest_entry_index) {
            return Err(Error::Order);
        }
        if entries[..position]
            .iter()
            .any(|prior| prior.symbol_index == entry.symbol_index)
        {
            return Err(Error::DuplicateSymbol);
        }
        if entry.selector.is_empty() || entry.selector.contains(&0) {
            return Err(Error::Selector);
        }
        previous_index = Some(entry.manifest_entry_index);
    }

    let mut output = Emitter::with_capacity(entries.len().saturating_mul(512));
    output.push(b"void* __birnam_resolve_block_vtable_v1(unsigned int manifest_entry_index){\nswitch(manifest_entry_index){\n");
    for entry in entries {
        let cache_index = (1_u64 << 62) | u64::from(entry.manifest_entry_index);
        output.push(b"case ");
        output.push(entry.manifest_entry_index.to_string().as_bytes());
        output.push(b"U:{void* v=birnam_vtable_cache_get(__birnam_code_generation_identity,");
        output.push(cache_index.to_string().as_bytes());
        output.push(b"ULL);if(!v){v=birnam_vtable_cache_set(__birnam_code_generation_identity,");
        output.push(cache_index.to_string().as_bytes());
        output.push(b"ULL,birnam_proto(birnam_blockVt()));if(!v)return (void*)0;birnam_method(v,birnam_intern(\"");
        output.push_c_string_body(entry.selector);
        output.push(b"\"),(void*)&__birnam_resumable_thunk_");
        output.push(entry.symbol_index.to_string().as_bytes());
        output.push(b");birnam_layoutAllOops(v);}return v;}\n");
    }
    output.push(b"default:return (void*)0;\n}}\n");
    Ok(output.finish())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Command;

    use super::*;

    #[test]
    fn emits_one_bounded_idempotent_unit_resolver() {
        let source = emit_block_vtable_resolver_v1(&[
            BlockVtableSpec {
                manifest_entry_index: 2,
                symbol_index: 7,
                selector: b"value:",
            },
            BlockVtableSpec {
                manifest_entry_index: 5,
                symbol_index: 9,
                selector: b"value:\xff",
            },
        ])
        .unwrap();
        let text = String::from_utf8(source).unwrap();
        assert!(text.contains("__birnam_resolve_block_vtable_v1"));
        assert!(text.contains("case 2U:"));
        assert!(text.contains("case 5U:"));
        assert!(text.contains("__birnam_resumable_thunk_7"));
        assert!(text.contains("value:\\377"));
        assert!(text.contains("default:return (void*)0"));
        assert!(!text.contains("if(!birnam_method"));
        assert!(!text.contains("if(!birnam_layoutAllOops"));
    }

    #[test]
    fn rejects_ambiguous_or_non_canonical_tables() {
        let entry = BlockVtableSpec {
            manifest_entry_index: 2,
            symbol_index: 7,
            selector: b"value",
        };
        assert_eq!(emit_block_vtable_resolver_v1(&[]), Err(Error::Empty));
        assert_eq!(
            emit_block_vtable_resolver_v1(&[entry, entry]),
            Err(Error::Order)
        );
        assert_eq!(
            emit_block_vtable_resolver_v1(&[
                entry,
                BlockVtableSpec {
                    manifest_entry_index: 3,
                    ..entry
                },
            ]),
            Err(Error::DuplicateSymbol)
        );
        assert_eq!(
            emit_block_vtable_resolver_v1(&[BlockVtableSpec {
                selector: b"bad\0selector",
                ..entry
            }]),
            Err(Error::Selector)
        );
    }

    #[test]
    fn emitted_resolver_is_c11_warnings_clean() {
        let resolver = emit_block_vtable_resolver_v1(&[BlockVtableSpec {
            manifest_entry_index: 0,
            symbol_index: 3,
            selector: b"value:",
        }])
        .unwrap();
        let directory = std::env::temp_dir();
        let path = directory.join(format!(
            "birnam-resumable-resolver-{}-{}.c",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let mut source = br#"extern void* birnam_proto(void*);
extern void* birnam_blockVt(void);
extern void birnam_method(void*,void*,void*);
extern void* birnam_intern(char const*);
extern void birnam_layoutAllOops(void*);
extern void* birnam_vtable_cache_get(unsigned long long,unsigned long long);
extern void* birnam_vtable_cache_set(unsigned long long,unsigned long long,void*);
unsigned long long __birnam_code_generation_identity;
extern void* __birnam_resumable_thunk_3(void*,void* const*,unsigned long);
"#
        .to_vec();
        source.extend_from_slice(&resolver);
        fs::write(&path, source).unwrap();
        let status = Command::new("cc")
            .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-fsyntax-only"])
            .arg(&path)
            .status()
            .unwrap();
        let _ = fs::remove_file(path);
        assert!(status.success());
    }
}
