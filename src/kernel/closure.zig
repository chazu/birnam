//! Closure representation.
//!
//! A method in birnam is a Zig function pointer plus optional captured
//! state. Materialized as a small extern struct, registered in the
//! object table at an EID, and referenced from facts as `mkEid(eid)`.
//!
//! Cold-path bind dereferences `[vt sel ?closure_ref]` -> closure_eid
//! -> object_table.get(closure_eid) -> *Closure -> call `.method`.
//! Hot path caches the resolved `*Closure` in mcache.

const std = @import("std");
const ref_mod = @import("ref.zig");

pub const Ref = ref_mod.Ref;

pub const Closure = extern struct {
    /// Zig function pointer. Cast to the right `Imp*` signature at the
    /// call site.
    method: *const anyopaque,
    /// Captured state. NIL for plain kernel methods.
    data: Ref,
};
