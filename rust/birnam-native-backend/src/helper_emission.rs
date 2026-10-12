//! Check real TinyCC objects, not just the spelling of generated C.
use birnam_native_compiler::Unit;
use std::{collections::BTreeMap, ffi::CString};

// Independent ELF64 symbol inventory for the compiler's relocatable output.
fn symbols(bytes: &[u8], functions_only: bool) -> BTreeMap<String, u64> {
    assert_eq!(&bytes[..6], b"\x7fELF\x02\x01");
    let u16_at = |p| u16::from_le_bytes(bytes[p..p + 2].try_into().unwrap()) as usize;
    let u32_at = |p| u32::from_le_bytes(bytes[p..p + 4].try_into().unwrap()) as usize;
    let u64_at = |p| u64::from_le_bytes(bytes[p..p + 8].try_into().unwrap());
    let sections = u64_at(40) as usize;
    let stride = u16_at(58);
    let mut result = BTreeMap::new();
    for i in 0..u16_at(60) {
        let section = sections + i * stride;
        if u32_at(section + 4) != 2 {
            continue;
        } // SHT_SYMTAB
        let strings = sections + u32_at(section + 40) * stride;
        let strings_start = u64_at(strings + 24) as usize;
        let start = u64_at(section + 24) as usize;
        let size = u64_at(section + 32) as usize;
        let entry_size = u64_at(section + 56) as usize;
        for symbol in (start..start + size).step_by(entry_size) {
            if (functions_only && bytes[symbol + 4] & 15 != 2) || u16_at(symbol + 6) == 0 {
                continue;
            }
            let name = &bytes[strings_start + u32_at(symbol)..];
            let end = name.iter().position(|&b| b == 0).unwrap();
            let name = String::from_utf8(name[..end].to_vec()).unwrap();
            result.insert(
                name.strip_prefix("_").unwrap_or(&name).to_owned(),
                u64_at(symbol + 16),
            );
        }
    }
    result
}

#[test]
fn tinycc_omits_unused_helpers_and_keeps_transitive_dependencies() {
    let source = crate::native::compile_named_synchronous(1).unwrap();
    let object = Unit::compile_object(&CString::new(source).unwrap()).unwrap();
    let names = symbols(&object, true);
    assert!(symbols(&object, false).contains_key("birnam_native_module_v1"));
    eprintln!(
        "named sync: {} object bytes, {} functions, {} function bytes",
        object.len(),
        names.len(),
        names.values().sum::<u64>()
    );
    for absent in [
        "float_bits",
        "load_captures",
        "try_frame_call_copies",
        "put",
    ] {
        assert!(
            !names.contains_key(absent),
            "unused helper {absent} remains"
        );
    }
    for needed in [
        "raw",
        "selector",
        "cached_name",
        "keep",
        "finish",
        "return_call",
        "sync_0",
        "initialize",
    ] {
        assert!(
            names.contains_key(needed),
            "transitive or exported function {needed} missing"
        );
    }
}

#[test]
fn address_taken_helper_remains_callable() {
    let mut source = include_str!("native_prelude.c").to_owned();
    source.push_str("\ntypedef A (*WordHelper)(size_t); static WordHelper chosen=word;\nsize_t helper_address_probe(size_t value){return chosen(value).data.word;}\n");
    let source = CString::new(source).unwrap();
    let object = Unit::compile_object(&source).unwrap();
    let names = symbols(&object, true);
    assert!(names.contains_key("word"));
    assert!(!names.contains_key("load_captures"));
    let unit = unsafe { Unit::load_object(&object) }.unwrap();
    let entry: unsafe extern "C" fn(usize) -> usize =
        unsafe { std::mem::transmute(unit.symbol(c"helper_address_probe").unwrap().as_ptr()) };
    for value in [0, 73, usize::MAX] {
        assert_eq!(unsafe { entry(value) }, value);
    }
}

#[test]
fn small_units_do_not_reserve_512_selector_entries() {
    let source = crate::native::compile_named_synchronous(1).unwrap();
    let object = Unit::compile_object(&CString::new(source).unwrap()).unwrap();
    let size = symbols(&object, false)["selectors"];
    assert!(
        size <= (128 * 2 * size_of::<usize>()) as u64,
        "small adapter reserved {size} bytes of selector storage"
    );
}
