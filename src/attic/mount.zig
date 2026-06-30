//! The identity/fact-layer mount seam (W7 Track C, docs/30 §9, docs/31).
//!
//! The destination decomposes the monolithic bootstrap into a siphon that mounts
//! an ordered tower (id → fact store → gc → peg → compiler). `mountIdentityLayer`
//! is the explicit boundary where the fact layer comes online over the id floor:
//! it projects/publishes everything that the floor does not need in order to
//! dispatch. W7.0 measured the boot is write-only (one read-before-mount), so the
//! work here is publication, not ordering-sensitive wiring.
//!
//! Today it is called at the tail of `siphon.boot`, so external behaviour is
//! unchanged — the win is that it is now a *named boundary* the tower can move
//! later. Steps land here incrementally:
//!   - A1 (done): flush the deferred `:name` reflection cells.
//!   - C2 (later): retrofit eids onto floor objects.
//!   - B3 (later): project the floor's struct metaobject graph into EAVT/AVET.

const std = @import("std");
const fact_store = @import("fact_store.zig");
const object_table = @import("object_table.zig");
const interner_mod = @import("interner.zig");

pub fn mountIdentityLayer(
    store: *fact_store.FactStore,
    table: *object_table.ObjectTable,
    interner: *interner_mod.Interner,
) !void {
    _ = table; // C2 (eid retrofit) attaches here later.

    // A1 — publish the deferred `:name` reflection shadow (the 171-write majority
    // of the boot, proven read-before-mount = 0 by the W7.0 probe).
    try interner.publishNames(store);

    // (The floor's delegation is now set struct-resident at the delegation sites
    // in `siphon.boot` — no mount-time `:delegate` sync needed. W7 B inversion.)
}
