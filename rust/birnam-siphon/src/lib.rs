//! The single ad-hoc native handoff creates Id and its seed objects. All later
//! loading and service interactions use the returned Oop-only message interface.
use birnam_abi::{
    id_protocol::{Bootstrap, Protocol, Seed, VERSION},
    native_module,
};
pub fn load(path: &[u8], seeds: &[&[u8]]) -> Result<Protocol, &'static str> {
    unsafe {
        let handle = native_module::open(path).ok_or("cannot load Id")?;
        let entry = native_module::symbol(handle, c"birnam_id_bootstrap_v2");
        if entry.is_null() {
            native_module::close(handle);
            return Err("Id bootstrap entry missing");
        }
        let bootstrap: Bootstrap = core::mem::transmute(entry);
        let values: Vec<_> = seeds
            .iter()
            .map(|b| Seed {
                bytes: b.as_ptr(),
                length: b.len(),
            })
            .collect();
        let p = bootstrap(values.as_ptr(), values.len());
        if p.is_null() {
            return Err("Id bootstrap failed");
        }
        let p = *p;
        if p.version != VERSION || p.byte_size as usize != size_of::<Protocol>() {
            return Err("Id ABI mismatch");
        }
        Ok(p)
    }
}
