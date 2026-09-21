//! Raw export bindings and their storage, independently owned per Runtime.
#![allow(clippy::missing_safety_doc)]
mod native;
mod index;
mod enumeration;
mod names;
mod lookup;
mod presence;
mod import;
mod export;
mod publication;
mod state;
use birnam_abi::namespace::{ApiV8, ViewV1};
use birnam_abi::Oop;
use core::ffi::c_void;
use state::Environment;
unsafe extern "C" fn create() -> *mut c_void {
    Box::into_raw(Box::new(Environment::new())).cast()
}
unsafe extern "C" fn destroy(context: *mut c_void) {
    unsafe { drop(Box::from_raw(context.cast::<Environment>())) };
}
unsafe extern "C" fn bind(context: *mut c_void, name: Oop, value: Oop) -> Oop {
    unsafe { &mut *context.cast::<Environment>() }.export(name, value)
}
unsafe extern "C" fn find(context: *const c_void, name: Oop) -> Oop {
    unsafe { &*context.cast::<Environment>() }.import(name)
}
unsafe extern "C" fn reverse(context: *const c_void, value: Oop) -> Oop {
    unsafe { &*context.cast::<Environment>() }.exported_name_of(value)
}
unsafe extern "C" fn count(context: *const c_void) -> usize {
    unsafe { &*context.cast::<Environment>() }.count()
}
unsafe extern "C" fn name_at(context: *const c_void, index: usize) -> Oop {
    unsafe { &*context.cast::<Environment>() }.name_at(index)
}
unsafe extern "C" fn bindings(context: *const c_void) -> ViewV1 {
    unsafe { &*context.cast::<Environment>() }.bindings()
}
static API: ApiV8 = ApiV8 {
    version: 8,
    byte_size: size_of::<ApiV8>(),
    create: Some(create),
    destroy: Some(destroy),
    bind: Some(bind),
    find: Some(find),
    reverse: Some(reverse),
    count: Some(count),
    name_at: Some(name_at),
    bindings: Some(bindings),
    create_names: Some(names::create),
    destroy_names: Some(names::destroy),
    create_enumeration: Some(enumeration::create),
    destroy_enumeration: Some(enumeration::destroy),
    lookup: Some(lookup::lookup),
    create_presence: Some(presence::create),
    destroy_presence: Some(presence::destroy),
    publish: Some(publication::publish),
    create_import: Some(import::create),
    destroy_import: Some(import::destroy),
    create_export: Some(export::create),
    destroy_export: Some(export::destroy),
};
/// Preparation has no side effects and needs no Id or namespace instance.
pub fn namespace_provider() -> *const ApiV8 {
    &API
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}
