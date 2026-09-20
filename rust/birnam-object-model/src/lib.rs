//! Private-behavior convenience above Id. The association field and its GC
//! semantics remain in the existing default-vtable representation.
use birnam_abi::{IdApi, Oop};
use core::ffi::{c_char, c_void};
use std::sync::OnceLock;

/// Rust adapter for the existing Id operations; no new native table or state.
/// The production adapter uses IdApi and ordinary seed messages. Floor unit
/// tests use the same algorithm with their independently constructed runtime.
pub trait BehaviorAccess {
    fn prepare(&self, object: Oop) -> Oop;
    fn metavtable(&self) -> Oop;
    fn owner(&self, behavior: Oop) -> Oop;
    fn delegate(&self, parent: Oop) -> Oop;
    fn set_owner(&self, behavior: Oop, object: Oop);
    fn attach(&self, object: Oop, behavior: Oop);
}

pub fn ensure_private_vtable(id: &impl BehaviorAccess, object: Oop) -> Oop {
    let current = id.prepare(object);
    if current == id.metavtable() {
        return object;
    }
    if id.owner(current) == object {
        return current;
    }
    let private = id.delegate(current);
    id.set_owner(private, object);
    id.attach(object, private);
    private
}

struct NativeAccess {
    api: usize,
    prepare: usize,
    attach: usize,
}
static ACCESS: OnceLock<NativeAccess> = OnceLock::new();
impl NativeAccess {
    fn api(&self) -> &IdApi {
        // SAFETY: module_init receives the owner-thread, process-lived floor.
        unsafe { &*(self.api as *const IdApi) }
    }
    fn send(&self, selector: usize, args: &[Oop]) -> Oop {
        let api = self.api();
        // SAFETY: all operands come from the same live Id runtime.
        unsafe { (api.send)(api.vtable_vt, selector as Oop, args.as_ptr(), args.len()) }
    }
}
impl BehaviorAccess for NativeAccess {
    fn prepare(&self, object: Oop) -> Oop {
        self.send(self.prepare, &[object])
    }
    fn metavtable(&self) -> Oop {
        self.api().vtable_vt
    }
    fn owner(&self, behavior: Oop) -> Oop {
        unsafe { birnam_abi::vtable_owner(self.api(), behavior) }
    }
    fn delegate(&self, parent: Oop) -> Oop {
        unsafe { (self.api().proto)(parent) }
    }
    fn set_owner(&self, behavior: Oop, object: Oop) {
        unsafe { birnam_abi::set_vtable_owner(self.api(), behavior, object) };
    }
    fn attach(&self, object: Oop, behavior: Oop) {
        self.send(self.attach, &[object, behavior]);
    }
}
unsafe extern "C" fn ensure(_: Oop, args: *const Oop, count: usize) -> Oop {
    let access = ACCESS.get().expect("object-model module is initialized");
    let Some([object]) = (unsafe { birnam_abi::borrowed_oop_args(args, count) }) else {
        eprintln!("birnam object-model: ensurePrivateVtableFor: requires one object");
        std::process::abort();
    };
    ensure_private_vtable(access, *object)
}

/// # Safety
/// The floor API and this library remain live for the runtime's lifetime, as
/// for the other native rungs. Invoke only on the floor's owner thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api: *mut IdApi, _: *mut c_void, _presence: *const birnam_abi::namespace_presence::ApiV1, _imports: *const birnam_abi::namespace_import::ApiV1, _exports: *const birnam_abi::namespace_export::ApiV1) -> *mut c_void {
    assert!(!api.is_null());
    let id = unsafe { &*api };
    let access = ACCESS.get_or_init(|| NativeAccess {
        api: api as usize,
        prepare: unsafe { (id.intern)(c"prepareBehaviorMutationOf:".as_ptr()) } as usize,
        attach: unsafe { (id.intern)(c"reassociateBehaviorOf:with:".as_ptr()) } as usize,
    });
    assert_eq!(
        access.api, api as usize,
        "object-model mounted on another floor"
    );
    unsafe {
        (id.method)(
            id.vtable_vt,
            (id.intern)(c"ensurePrivateVtableFor:".as_ptr()),
            ensure,
        )
    };
    core::ptr::null_mut()
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_object_model_rust_identity() -> *const c_char {
    c"rust-object-model-v1".as_ptr()
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> core::ffi::c_int {
    1
}
