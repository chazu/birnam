//! Phase 11.7.v — consolidated transaction commit path.
//!
//! Single entry point for fact-store mutations. Today's body delegates to
//! `fact_store.assert` / `fact_store.retract`; under the Datomic shape
//! locked in ADR-11 #7 the body becomes a channel send to the designated
//! transactor thread. Funnelling through one function means that swap is
//! one edit instead of an audit across every assert/retract call site.
//!
//! Existing `store.assert(...)` / `store.retract(...)` calls remain valid
//! during the transition; new code should prefer `tx_engine.commit(...)`.

const std = @import("std");
const fact_store_mod = @import("fact_store.zig");
const ref_mod = @import("ref.zig");

pub const Ref = ref_mod.Ref;
pub const FactStore = fact_store_mod.FactStore;

pub const OpKind = enum(u8) {
    assert,
    retract,
};

pub const Op = struct {
    kind: OpKind,
    e: u64,
    a: u64,
    v: Ref,
    m: u64,
};

/// Apply a single fact mutation. Returns the transaction id assigned to
/// the operation. Single-threaded today — direct call into the store.
/// Under the Datomic shape this becomes a blocking send into the
/// transactor's tx queue.
pub fn commit(store: *FactStore, op: Op) !u64 {
    return switch (op.kind) {
        .assert => store.assert(op.e, op.a, op.v, op.m),
        .retract => store.retract(op.e, op.a, op.v, op.m),
    };
}

/// Convenience builder for assert.
pub inline fn opAssert(e: u64, a: u64, v: Ref, m: u64) Op {
    return .{ .kind = .assert, .e = e, .a = a, .v = v, .m = m };
}

/// Convenience builder for retract.
pub inline fn opRetract(e: u64, a: u64, v: Ref, m: u64) Op {
    return .{ .kind = .retract, .e = e, .a = a, .v = v, .m = m };
}
