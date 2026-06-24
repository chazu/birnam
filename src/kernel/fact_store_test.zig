const std = @import("std");
const fs = @import("fact_store.zig");
const ref = @import("ref.zig");
const kernel_eids = @import("kernel_eids.zig");
const testing = std.testing;

// Convenience: build a value Ref from a small int.
fn vi(n: i64) ref.Ref {
    return ref.mkSmallInt(n).?;
}

// Module IDs we use across tests
const M_BOOT: u64 = 1;
const M_FOO: u64 = 2;
const M_BAR: u64 = 3;

// Entity / attribute IDs
const E1: u64 = 100;
const E2: u64 = 101;
const E3: u64 = 102;
const A_NAME: u64 = 200;
const A_AGE: u64 = 201;
const A_PARENT: u64 = 202;

// -- Basic assert / lookup ----------------------------------------------------

test "assert + currentValue round-trip" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());

    const tx = try store.assert(E1, A_NAME, vi(42), M_BOOT);
    try testing.expectEqual(@as(u64, 1), tx);
    try testing.expectEqual(vi(42), store.currentValue(E1, A_NAME).?);
}

test "currentValue null for never-asserted fact" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());
    try testing.expectEqual(@as(?ref.Ref, null), store.currentValue(E1, A_NAME));
}

test "tx counter monotonic across assert/retract" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());
    const t1 = try store.assert(E1, A_NAME, vi(1), M_BOOT);
    const t2 = try store.assert(E2, A_NAME, vi(2), M_BOOT);
    const t3 = try store.retract(E1, A_NAME, vi(1), M_BOOT);
    try testing.expect(t1 < t2);
    try testing.expect(t2 < t3);
}

// -- Retract -----------------------------------------------------------------

test "retract removes current value" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());
    _ = try store.assert(E1, A_AGE, vi(30), M_BOOT);
    try testing.expectEqual(vi(30), store.currentValue(E1, A_AGE).?);
    _ = try store.retract(E1, A_AGE, vi(30), M_BOOT);
    try testing.expectEqual(@as(?ref.Ref, null), store.currentValue(E1, A_AGE));
}

test "retract of never-asserted is no-op" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());
    _ = try store.retract(E1, A_NAME, vi(999), M_BOOT);
    try testing.expectEqual(@as(?ref.Ref, null), store.currentValue(E1, A_NAME));
}

test "retract of wrong-value is no-op" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());
    _ = try store.assert(E1, A_AGE, vi(30), M_BOOT);
    _ = try store.retract(E1, A_AGE, vi(99), M_BOOT); // wrong value
    try testing.expectEqual(vi(30), store.currentValue(E1, A_AGE).?);
}

// -- Supersession ------------------------------------------------------------

test "assert v2 over v1: current becomes v2" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());
    _ = try store.assert(E1, A_AGE, vi(30), M_BOOT);
    _ = try store.assert(E1, A_AGE, vi(31), M_BOOT);
    try testing.expectEqual(vi(31), store.currentValue(E1, A_AGE).?);
}

test "history walk via as-of-tx" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());
    const tx1 = try store.assert(E1, A_AGE, vi(30), M_BOOT);
    const tx2 = try store.assert(E1, A_AGE, vi(31), M_BOOT);
    const tx3 = try store.retract(E1, A_AGE, vi(31), M_BOOT);

    try testing.expectEqual(@as(?ref.Ref, null), store.currentValueAsOf(E1, A_AGE, 0));
    try testing.expectEqual(vi(30), store.currentValueAsOf(E1, A_AGE, tx1).?);
    try testing.expectEqual(vi(31), store.currentValueAsOf(E1, A_AGE, tx2).?);
    try testing.expectEqual(@as(?ref.Ref, null), store.currentValueAsOf(E1, A_AGE, tx3));
}

// -- Four-index agreement on current state ------------------------------------

test "EAVT and AEVT agree on current state" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());

    _ = try store.assert(E1, A_NAME, vi(100), M_BOOT);
    _ = try store.assert(E2, A_NAME, vi(200), M_BOOT);
    _ = try store.assert(E1, A_AGE, vi(30), M_BOOT);

    // EAVT-shaped: attributes of E1
    var attrs = store.attributesOf(E1);
    var seen_name = false;
    var seen_age = false;
    while (attrs.next()) |kv| {
        if (kv.a == A_NAME and kv.v == vi(100)) seen_name = true;
        if (kv.a == A_AGE and kv.v == vi(30)) seen_age = true;
    }
    try testing.expect(seen_name);
    try testing.expect(seen_age);

    // AEVT-shaped: entities with A_NAME
    var entities = store.entitiesWith(A_NAME);
    var saw_e1 = false;
    var saw_e2 = false;
    while (entities.next()) |kv| {
        if (kv.e == E1 and kv.v == vi(100)) saw_e1 = true;
        if (kv.e == E2 and kv.v == vi(200)) saw_e2 = true;
    }
    try testing.expect(saw_e1);
    try testing.expect(saw_e2);
}

test "AVET equality lookup" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());

    _ = try store.assert(E1, A_NAME, vi(7), M_BOOT);
    _ = try store.assert(E2, A_NAME, vi(7), M_BOOT);
    _ = try store.assert(E3, A_NAME, vi(8), M_BOOT);

    var it = store.entitiesMatching(A_NAME, vi(7));
    var seen_e1 = false;
    var seen_e2 = false;
    var count: usize = 0;
    while (it.next()) |e| {
        if (e == E1) seen_e1 = true;
        if (e == E2) seen_e2 = true;
        count += 1;
    }
    try testing.expectEqual(@as(usize, 2), count);
    try testing.expect(seen_e1);
    try testing.expect(seen_e2);
}

test "VAET reverse-ref walk" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());

    const parent_ref = ref.mkEid(E3);
    _ = try store.assert(E1, A_PARENT, parent_ref, M_BOOT);
    _ = try store.assert(E2, A_PARENT, parent_ref, M_BOOT);

    var it = try store.pointingAt(parent_ref, arena.allocator());
    defer it.deinit();
    var seen_e1 = false;
    var seen_e2 = false;
    while (it.next()) |ae| {
        try testing.expectEqual(A_PARENT, ae.a);
        if (ae.e == E1) seen_e1 = true;
        if (ae.e == E2) seen_e2 = true;
    }
    try testing.expect(seen_e1);
    try testing.expect(seen_e2);
}

// -- hasFact -----------------------------------------------------------------

test "hasFact reflects current state" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());
    try testing.expect(!store.hasFact(E1, A_AGE, vi(30)));
    _ = try store.assert(E1, A_AGE, vi(30), M_BOOT);
    try testing.expect(store.hasFact(E1, A_AGE, vi(30)));
    _ = try store.retract(E1, A_AGE, vi(30), M_BOOT);
    try testing.expect(!store.hasFact(E1, A_AGE, vi(30)));
}

test "supersession: prev value's hasFact becomes false" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());
    _ = try store.assert(E1, A_AGE, vi(30), M_BOOT);
    _ = try store.assert(E1, A_AGE, vi(31), M_BOOT);
    try testing.expect(!store.hasFact(E1, A_AGE, vi(30)));
    try testing.expect(store.hasFact(E1, A_AGE, vi(31)));
}

// -- entityExists ------------------------------------------------------------

test "entityExists true after assert, false after full retract" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());
    try testing.expect(!store.entityExists(E1));
    _ = try store.assert(E1, A_NAME, vi(7), M_BOOT);
    try testing.expect(store.entityExists(E1));
    _ = try store.retract(E1, A_NAME, vi(7), M_BOOT);
    try testing.expect(!store.entityExists(E1));
}

// -- Module column -----------------------------------------------------------

test "facts owned by different modules are independently queryable" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());

    _ = try store.assert(E1, A_NAME, vi(1), M_FOO);
    _ = try store.assert(E2, A_NAME, vi(2), M_FOO);
    _ = try store.assert(E3, A_NAME, vi(3), M_BAR);

    var foo_it = try store.factsOfModule(M_FOO, arena.allocator());
    defer foo_it.deinit();
    var foo_count: usize = 0;
    while (foo_it.next()) |_| foo_count += 1;
    try testing.expectEqual(@as(usize, 2), foo_count);

    var bar_it = try store.factsOfModule(M_BAR, arena.allocator());
    defer bar_it.deinit();
    var bar_count: usize = 0;
    while (bar_it.next()) |_| bar_count += 1;
    try testing.expectEqual(@as(usize, 1), bar_count);
}

test "module unload: walk MEAT, retract each (e, a, v) — fact disappears" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());

    _ = try store.assert(E1, A_NAME, vi(1), M_FOO);
    _ = try store.assert(E1, A_AGE, vi(30), M_FOO);
    _ = try store.assert(E2, A_NAME, vi(2), M_BAR);

    // Simulate module unload for FOO.
    var to_retract = std.ArrayList(fs.FactStore.ModuleFactIter.EAV){};
    defer to_retract.deinit(arena.allocator());
    var it = try store.factsOfModule(M_FOO, arena.allocator());
    defer it.deinit();
    while (it.next()) |triple| try to_retract.append(arena.allocator(), triple);
    for (to_retract.items) |t| _ = try store.retract(t.e, t.a, t.v, M_FOO);

    // FOO's facts gone.
    try testing.expectEqual(@as(?ref.Ref, null), store.currentValue(E1, A_NAME));
    try testing.expectEqual(@as(?ref.Ref, null), store.currentValue(E1, A_AGE));
    // BAR's fact untouched.
    try testing.expectEqual(vi(2), store.currentValue(E2, A_NAME).?);
}

// -- attributesOf / entitiesWith reflect supersession + retract ---------------

test "attributesOf does not include retracted attributes" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());
    _ = try store.assert(E1, A_NAME, vi(7), M_BOOT);
    _ = try store.assert(E1, A_AGE, vi(30), M_BOOT);
    _ = try store.retract(E1, A_AGE, vi(30), M_BOOT);

    var it = store.attributesOf(E1);
    var count: usize = 0;
    var saw_age = false;
    while (it.next()) |kv| {
        if (kv.a == A_AGE) saw_age = true;
        count += 1;
    }
    try testing.expectEqual(@as(usize, 1), count);
    try testing.expect(!saw_age);
}

// -- Scale -------------------------------------------------------------------

test "10k assertions over 1k entities" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());

    const N_E: u64 = 1000;
    const N_A: u64 = 10;
    var e: u64 = 0;
    while (e < N_E) : (e += 1) {
        var a: u64 = 0;
        while (a < N_A) : (a += 1) {
            _ = try store.assert(1000 + e, 5000 + a, vi(@as(i64, @intCast(e * 100 + a))), M_BOOT);
        }
    }

    // Sample lookups
    try testing.expectEqual(vi(0), store.currentValue(1000, 5000).?);
    try testing.expectEqual(vi(50_007), store.currentValue(1500, 5007).?);
    try testing.expectEqual(vi(99_909), store.currentValue(1999, 5009).?);

    // Tx counter = 10000 + 1
    try testing.expectEqual(@as(u64, 10_000), store.currentTx());
}

// -- Phase 14 --strip=history: frozen read-only store ------------------------

test "frozenInsert builds indexes; freeze rejects further mutation" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());

    // Bulk-build via the frozen path (no tx churn — counter stays at 1).
    try store.frozenInsert(E1, A_NAME, vi(42), M_FOO);
    try store.frozenInsert(E2, A_AGE, vi(7), M_FOO);
    try testing.expectEqual(vi(42), store.currentValue(E1, A_NAME).?);
    try testing.expectEqual(vi(7), store.currentValue(E2, A_AGE).?);
    try testing.expectEqual(@as(u64, 0), store.currentTx()); // never bumped (assert would)

    // Freeze → writes rejected.
    store.freeze();
    try testing.expectError(error.ReadOnly, store.assert(E3, A_NAME, vi(99), M_FOO));
    try testing.expectError(error.ReadOnly, store.retract(E1, A_NAME, vi(42), M_FOO));
    // Reads still work after freeze.
    try testing.expectEqual(vi(42), store.currentValue(E1, A_NAME).?);
}

// -- Phase 15.I — index epoch arena -----------------------------------------

const KeepAll = struct {
    fn keep(_: void, _: u64, _: u64, _: u64, _: ref.Ref) bool {
        return true;
    }
};

test "index epoch arena: compact frees old epoch wholesale; values survive" {
    // Store directly on the leak-checked allocator. The index lives on the
    // epoch arena; if compact fails to free the old epoch (or deinit the
    // current one), testing.allocator flags the leak. Reify maps freed below.
    const a = testing.allocator;
    var store = fs.FactStore.init(a);
    defer store.fact_eids.deinit();
    defer store.fact_modules.deinit();
    try store.enableIndexArena();
    defer store.deinitIndexArena();

    // 1000 asserts (each (e,a) rewritten once → spine garbage + history churn,
    // all accumulating in epoch 1).
    var i: u64 = 0;
    while (i < 500) : (i += 1) {
        _ = try store.assert(100 + i, 5, vi(@intCast(i)), M_BOOT);
        _ = try store.assert(100 + i, 5, vi(@intCast(i + 1000)), M_BOOT);
    }
    try testing.expectEqual(vi(1000), store.currentValue(100, 5).?);

    const old_epoch_ptr = store.index_arena.?;
    const stats = try store.compact(a, {}, KeepAll.keep);
    try testing.expectEqual(@as(usize, 500), stats.kept); // current-only collapse
    // Epoch swapped (old freed; pointer value compared, not dereferenced).
    try testing.expect(store.index_arena.? != old_epoch_ptr);

    // No corruption / UAF across the swap+free.
    try testing.expectEqual(vi(1000), store.currentValue(100, 5).?);
    try testing.expectEqual(vi(1499), store.currentValue(599, 5).?);
    try testing.expect(store.currentValue(99, 5) == null);
}

test "epoch registry (R1b): pin retains old epoch across compact; unpin frees it" {
    // Leak-checked allocator: a retained epoch that is never freed (or freed
    // twice) trips the allocator. Proves the refcount gate keeps a pinned
    // epoch alive across compact and frees it exactly once on unpin.
    const a = testing.allocator;
    var store = fs.FactStore.init(a);
    defer store.fact_eids.deinit();
    defer store.fact_modules.deinit();
    try store.enableIndexArena();
    defer store.deinitIndexArena();

    _ = try store.assert(1, 5, vi(10), M_BOOT);
    _ = try store.assert(2, 5, vi(20), M_BOOT);

    // Pin the current (writable) epoch, then compact. The old epoch is pinned
    // → moved to retained_epochs, NOT freed.
    const gen = store.pinCurrentEpoch();
    const old_ptr = store.index_arena.?;
    _ = try store.compact(a, {}, KeepAll.keep);

    try testing.expect(store.index_arena.? != old_ptr); // swapped to fresh epoch
    try testing.expectEqual(@as(usize, 1), store.retained_epochs.items.len);
    try testing.expectEqual(@as(u32, 1), store.retained_epochs.items[0].refcount);
    // New epoch serves current values fine.
    try testing.expectEqual(vi(10), store.currentValue(1, 5).?);
    try testing.expectEqual(vi(20), store.currentValue(2, 5).?);

    // Unpin → refcount 0 and below the (current-only) horizon → freed + dropped.
    store.unpinEpoch(gen);
    try testing.expectEqual(@as(usize, 0), store.retained_epochs.items.len);
    try testing.expectEqual(vi(10), store.currentValue(1, 5).?); // no UAF
}

test "epoch registry (R1b): compact does not advance basis_t" {
    const a = testing.allocator;
    var store = fs.FactStore.init(a);
    defer store.fact_eids.deinit();
    defer store.fact_modules.deinit();
    try store.enableIndexArena();
    defer store.deinitIndexArena();

    _ = try store.assert(1, 5, vi(10), M_BOOT); // rewrite (1,5) thrice → churn
    _ = try store.assert(1, 5, vi(11), M_BOOT);
    _ = try store.assert(1, 5, vi(12), M_BOOT);
    _ = try store.assert(2, 5, vi(20), M_BOOT);
    const basis_before = store.currentBasisT(); // 4 commits
    _ = try store.compact(a, {}, KeepAll.keep); // keeps 2 current facts
    try testing.expectEqual(basis_before, store.currentBasisT()); // unchanged
    // next_tx legitimately reset to the per-epoch coordinate (2 < 4).
    try testing.expect(store.currentTx() < basis_before);
}

test "snapshot (R1c): frozen view is isolated from later live mutation" {
    const a = testing.allocator;
    var store = fs.FactStore.init(a);
    defer store.fact_eids.deinit();
    defer store.fact_modules.deinit();
    try store.enableIndexArena();
    defer store.deinitIndexArena();

    _ = try store.assert(1, 5, vi(10), M_BOOT);
    const snap = try store.snapshotNow();
    defer snap.deinit(a);

    // Mutate live AFTER the snapshot.
    _ = try store.assert(1, 5, vi(20), M_BOOT);
    _ = try store.assert(2, 5, vi(99), M_BOOT); // entity not present at snapshot

    // Snapshot still sees the captured state; live sees the new state.
    try testing.expectEqual(vi(10), snap.view.currentValue(1, 5).?);
    try testing.expectEqual(vi(20), store.currentValue(1, 5).?);
    try testing.expect(snap.view.currentValue(2, 5) == null); // didn't exist yet
    try testing.expectEqual(vi(99), store.currentValue(2, 5).?);
    try testing.expectEqual(@as(u64, 1), snap.basis_t); // one commit at capture
}

test "snapshot (R1c): pin survives compact; release frees the epoch" {
    const a = testing.allocator; // leak-checked
    var store = fs.FactStore.init(a);
    defer store.fact_eids.deinit();
    defer store.fact_modules.deinit();
    try store.enableIndexArena();
    defer store.deinitIndexArena();

    _ = try store.assert(1, 5, vi(10), M_BOOT);
    _ = try store.assert(2, 5, vi(20), M_BOOT);
    const snap = try store.snapshotNow();

    // Live churns then compacts — the snapshot's epoch is pinned, so compact
    // retains it rather than freeing it.
    _ = try store.assert(1, 5, vi(11), M_BOOT);
    _ = try store.compact(a, {}, KeepAll.keep);
    try testing.expectEqual(@as(usize, 1), store.retained_epochs.items.len);

    // Snapshot reads remain valid across the compact (no UAF) — it reads the
    // RETAINED old epoch's nodes, not the new live epoch.
    try testing.expectEqual(vi(10), snap.view.currentValue(1, 5).?);
    try testing.expectEqual(vi(20), snap.view.currentValue(2, 5).?);
    // Live shows the post-churn current value.
    try testing.expectEqual(vi(11), store.currentValue(1, 5).?);

    // Release → epoch refcount hits 0, below horizon → freed + dropped.
    snap.deinit(a);
    try testing.expectEqual(@as(usize, 0), store.retained_epochs.items.len);
    try testing.expectEqual(vi(11), store.currentValue(1, 5).?); // live intact
}

test "snapshot (R1c): datalog query runs against the frozen view" {
    const a = testing.allocator;
    var store = fs.FactStore.init(a);
    defer store.fact_eids.deinit();
    defer store.fact_modules.deinit();
    try store.enableIndexArena();
    defer store.deinitIndexArena();

    const attr_kind: u64 = 5;
    _ = try store.assert(100, attr_kind, vi(7), M_BOOT);
    _ = try store.assert(101, attr_kind, vi(7), M_BOOT);
    const snap = try store.snapshotNow();
    defer snap.deinit(a);

    // After the snapshot, a third entity gets the same value on the LIVE store.
    _ = try store.assert(102, attr_kind, vi(7), M_BOOT);

    // entitiesMatching against the view sees exactly the 2 captured entities;
    // the live store sees 3. (Iterator-level datalog primitive over the view.)
    var snap_count: usize = 0;
    var sit = snap.view.entitiesMatching(attr_kind, vi(7));
    while (sit.next()) |_| snap_count += 1;
    try testing.expectEqual(@as(usize, 2), snap_count);

    var live_count: usize = 0;
    var lit = store.entitiesMatching(attr_kind, vi(7));
    while (lit.next()) |_| live_count += 1;
    try testing.expectEqual(@as(usize, 3), live_count);
}

test "retention (R2): currentValueAsOfBasis is exact per basis" {
    var backing = std.heap.ArenaAllocator.init(testing.allocator);
    defer backing.deinit();
    const a = backing.allocator();
    var store = fs.FactStore.init(a);

    _ = try store.assert(1, 5, vi(10), M_BOOT); // basis 1
    _ = try store.assert(1, 5, vi(20), M_BOOT); // basis 2
    _ = try store.assert(1, 5, vi(30), M_BOOT); // basis 3

    try testing.expect(store.currentValueAsOfBasis(1, 5, 0) == null);
    try testing.expectEqual(vi(10), store.currentValueAsOfBasis(1, 5, 1).?);
    try testing.expectEqual(vi(20), store.currentValueAsOfBasis(1, 5, 2).?);
    try testing.expectEqual(vi(30), store.currentValueAsOfBasis(1, 5, 3).?);
    try testing.expectEqual(vi(30), store.currentValueAsOfBasis(1, 5, 99).?);
}

test "retention (R2): keep_all retains archive epoch; asOf reads past basis" {
    const a = testing.allocator;
    var store = fs.FactStore.init(a);
    defer store.fact_eids.deinit();
    defer store.fact_modules.deinit();
    try store.enableIndexArena();
    defer store.deinitIndexArena();

    store.setRetention(.keep_all);
    _ = try store.assert(1, 5, vi(10), M_BOOT); // basis 1
    _ = try store.assert(1, 5, vi(20), M_BOOT); // basis 2

    // Compact: keep_all → old epoch retained as a history archive (not freed).
    _ = try store.compact(a, {}, KeepAll.keep);
    try testing.expectEqual(@as(usize, 1), store.retained_epochs.items.len);

    // asOf a past basis routes to the retained archive and reads exactly.
    const past = try store.snapshotAsOf(1);
    defer past.deinit(a);
    try testing.expectEqual(vi(10), past.get(1, 5).?);
    try testing.expectEqual(@as(u64, 1), past.basis_t);

    // asOf the collapse basis routes to the current epoch.
    const at2 = try store.snapshotAsOf(2);
    defer at2.deinit(a);
    try testing.expectEqual(vi(20), at2.get(1, 5).?);

    // Live still current.
    try testing.expectEqual(vi(20), store.currentValue(1, 5).?);
}

test "retention (R2): current_only frees history; asOf past errors" {
    const a = testing.allocator;
    var store = fs.FactStore.init(a);
    defer store.fact_eids.deinit();
    defer store.fact_modules.deinit();
    try store.enableIndexArena();
    defer store.deinitIndexArena();

    // Default current_only.
    _ = try store.assert(1, 5, vi(10), M_BOOT); // basis 1
    _ = try store.assert(1, 5, vi(20), M_BOOT); // basis 2
    _ = try store.compact(a, {}, KeepAll.keep); // frees old epoch
    try testing.expectEqual(@as(usize, 0), store.retained_epochs.items.len);

    // History below the collapse is gone → asOf(1) has no archive.
    try testing.expectError(error.HistoryNotRetained, store.snapshotAsOf(1));
}

test "excision (R3): removes entity + history; other entities untouched" {
    var backing = std.heap.ArenaAllocator.init(testing.allocator);
    defer backing.deinit();
    const a = backing.allocator();
    var store = fs.FactStore.init(a);

    _ = try store.assert(1, 5, vi(10), M_BOOT); // e=1 history: 10 then 20
    _ = try store.assert(1, 5, vi(20), M_BOOT);
    _ = try store.assert(1, 6, vi(77), M_BOOT); // second attr on e=1
    _ = try store.assert(2, 5, vi(30), M_BOOT); // bystander

    try store.exciseEntity(1, a);

    // e=1 wholly gone (current + both attrs).
    try testing.expect(!store.entityExists(1));
    try testing.expect(store.currentValue(1, 5) == null);
    try testing.expect(store.currentValue(1, 6) == null);
    // Bystander intact.
    try testing.expectEqual(vi(30), store.currentValue(2, 5).?);

    // AVET scrubbed: neither historical value of e=1 maps back to it.
    var m20 = store.entitiesMatching(5, vi(20));
    try testing.expect(m20.next() == null);
    var m10 = store.entitiesMatching(5, vi(10));
    try testing.expect(m10.next() == null);
    // VAET scrubbed: nothing points via the old values to e=1.
    var pa = try store.pointingAt(vi(20), a);
    defer pa.deinit();
    try testing.expect(pa.next() == null);
    // Bystander still AVET-findable.
    var m30 = store.entitiesMatching(5, vi(30));
    try testing.expectEqual(@as(u64, 2), m30.next().?);
}

test "excision (R3): refuses while a live snapshot pins an epoch" {
    const a = testing.allocator;
    var store = fs.FactStore.init(a);
    defer store.fact_eids.deinit();
    defer store.fact_modules.deinit();
    try store.enableIndexArena();
    defer store.deinitIndexArena();

    _ = try store.assert(1, 5, vi(10), M_BOOT);
    const snap = try store.snapshotNow();

    try testing.expectError(error.SnapshotPins, store.exciseEntity(1, a));

    snap.deinit(a); // drop the snapshot...
    try store.exciseEntity(1, a); // ...now it succeeds
    try testing.expect(!store.entityExists(1));
}

test "excision (R3): scrubs retained history archives too (GDPR)" {
    const a = testing.allocator;
    var store = fs.FactStore.init(a);
    defer store.fact_eids.deinit();
    defer store.fact_modules.deinit();
    try store.enableIndexArena();
    defer store.deinitIndexArena();

    store.setRetention(.keep_all);
    _ = try store.assert(1, 5, vi(10), M_BOOT); // basis 1
    _ = try store.assert(1, 5, vi(20), M_BOOT); // basis 2
    _ = try store.assert(2, 5, vi(99), M_BOOT); // bystander, basis 3
    _ = try store.compact(a, {}, KeepAll.keep); // archive retained
    try testing.expectEqual(@as(usize, 1), store.retained_epochs.items.len);

    try store.exciseEntity(1, a);

    // Gone from the live store AND from the retained archive.
    try testing.expect(!store.entityExists(1));
    const past = try store.snapshotAsOf(1); // basis 1 → retained archive
    defer past.deinit(a);
    try testing.expect(past.get(1, 5) == null); // scrubbed from the archive
    try testing.expect(!past.view.entityExists(1)); // fully gone from archive
    // Bystander survives in the live store (it was asserted at basis 3).
    try testing.expectEqual(vi(99), store.currentValue(2, 5).?);
}

test "cold tier (R4): spill epoch to disk, fault back with exact fidelity" {
    const a = testing.allocator; // leak-checked
    var store = fs.FactStore.init(a);
    defer store.fact_eids.deinit();
    defer store.fact_modules.deinit();
    try store.enableIndexArena();
    defer store.deinitIndexArena();

    store.setRetention(.keep_all);
    _ = try store.assert(1, 5, vi(10), M_BOOT); // basis 1
    _ = try store.assert(1, 5, vi(20), M_BOOT); // basis 2  (e=1 history: 10,20)
    _ = try store.assert(2, 5, vi(30), M_BOOT); // basis 3  (e=2)
    _ = try store.compact(a, {}, KeepAll.keep); // archive epoch gen 0 retained
    try testing.expectEqual(@as(usize, 1), store.retained_epochs.items.len);
    const gen = store.retained_epochs.items[0].gen;

    // Spill the archive to disk → in-RAM arena freed, marked cold.
    try store.spillEpoch(gen, ".");
    try testing.expect(store.retained_epochs.items[0].arena == null);
    try testing.expect(store.retained_epochs.items[0].cold_path != null);

    // asOf a past basis auto-faults the cold epoch back and reads exactly.
    {
        const past = try store.snapshotAsOf(1);
        defer past.deinit(a);
        try testing.expectEqual(vi(10), past.get(1, 5).?); // e=1 as of basis 1
        try testing.expect(past.get(2, 5) == null); // e=2 didn't exist yet
    }
    // Now hot again; reads still exact at basis 2.
    try testing.expect(store.retained_epochs.items[0].arena != null);
    try testing.expect(store.retained_epochs.items[0].cold_path == null);
    {
        const at2 = try store.snapshotAsOf(2);
        defer at2.deinit(a);
        try testing.expectEqual(vi(20), at2.get(1, 5).?);
    }
    // Live store unaffected.
    try testing.expectEqual(vi(20), store.currentValue(1, 5).?);
    try testing.expectEqual(vi(30), store.currentValue(2, 5).?);
}

test "cold tier (R4): excision faults in + scrubs a cold archive" {
    const a = testing.allocator;
    var store = fs.FactStore.init(a);
    defer store.fact_eids.deinit();
    defer store.fact_modules.deinit();
    try store.enableIndexArena();
    defer store.deinitIndexArena();

    store.setRetention(.keep_all);
    _ = try store.assert(1, 5, vi(10), M_BOOT); // basis 1
    _ = try store.assert(1, 5, vi(20), M_BOOT); // basis 2
    _ = try store.compact(a, {}, KeepAll.keep);
    try store.spillEpoch(store.retained_epochs.items[0].gen, ".");
    try testing.expect(store.retained_epochs.items[0].arena == null);

    // GDPR excise must reach into the cold archive.
    try store.exciseEntity(1, a);
    const past = try store.snapshotAsOf(1);
    defer past.deinit(a);
    try testing.expect(past.get(1, 5) == null); // scrubbed from the faulted archive
}

test "exciseTxRange (R3): removing latest event reverts current value" {
    const a = testing.allocator;
    var store = fs.FactStore.init(a);
    defer store.fact_eids.deinit();
    defer store.fact_modules.deinit();
    try store.enableIndexArena();
    defer store.deinitIndexArena();

    _ = try store.assert(1, 5, vi(10), M_BOOT); // basis 1
    _ = try store.assert(1, 5, vi(20), M_BOOT); // basis 2
    _ = try store.assert(1, 5, vi(30), M_BOOT); // basis 3
    _ = try store.assert(2, 6, vi(99), M_BOOT); // basis 4 (bystander)

    // Excise basis 3 (the assert of 30) → current reverts to 20.
    try store.exciseTxRange(3, 3, a);
    try testing.expectEqual(vi(20), store.currentValue(1, 5).?);
    try testing.expectEqual(vi(10), store.currentValueAsOfBasis(1, 5, 1).?);
    try testing.expectEqual(vi(20), store.currentValueAsOfBasis(1, 5, 99).?);
    try testing.expectEqual(vi(99), store.currentValue(2, 6).?);
    var m30 = store.entitiesMatching(5, vi(30));
    try testing.expect(m30.next() == null);
    var m20 = store.entitiesMatching(5, vi(20));
    try testing.expectEqual(@as(u64, 1), m20.next().?);
}

test "exciseTxRange (R3): refuses while a live snapshot pins" {
    const a = testing.allocator;
    var store = fs.FactStore.init(a);
    defer store.fact_eids.deinit();
    defer store.fact_modules.deinit();
    try store.enableIndexArena();
    defer store.deinitIndexArena();

    _ = try store.assert(1, 5, vi(10), M_BOOT);
    const snap = try store.snapshotNow();
    try testing.expectError(error.SnapshotPins, store.exciseTxRange(1, 1, a));
    snap.deinit(a);
    try store.exciseTxRange(1, 1, a); // now succeeds
    try testing.expect(store.currentValue(1, 5) == null);
}

test "auto-eviction (R4): maybeSpillEpochs keeps N newest hot, spills rest" {
    const a = testing.allocator;
    var store = fs.FactStore.init(a);
    defer store.fact_eids.deinit();
    defer store.fact_modules.deinit();
    try store.enableIndexArena();
    defer store.deinitIndexArena();

    store.setRetention(.keep_all);
    // Three compacts → three retained archive epochs (gens 0,1,2).
    var i: u64 = 0;
    while (i < 3) : (i += 1) {
        _ = try store.assert(1, 5, vi(@intCast(10 + i)), M_BOOT);
        _ = try store.compact(a, {}, KeepAll.keep);
    }
    try testing.expectEqual(@as(usize, 3), store.retained_epochs.items.len);

    // Keep 1 newest hot → spill the 2 older.
    const spilled = try store.maybeSpillEpochs(".", 1, a);
    try testing.expectEqual(@as(usize, 2), spilled);
    var hot: usize = 0;
    var cold: usize = 0;
    for (store.retained_epochs.items) |re| {
        if (re.arena != null) hot += 1 else cold += 1;
    }
    try testing.expectEqual(@as(usize, 1), hot);
    try testing.expectEqual(@as(usize, 2), cold);

    // Idempotent: re-running spills nothing new.
    try testing.expectEqual(@as(usize, 0), try store.maybeSpillEpochs(".", 1, a));
    // deinitIndexArena cleans hot arenas + cold segment files.
}

test "index epoch arena: retain_history gate skips the free (no swap)" {
    // Backing arena reclaims the intentionally-retained epoch at teardown.
    var backing = std.heap.ArenaAllocator.init(testing.allocator);
    defer backing.deinit();
    const a = backing.allocator();
    var store = fs.FactStore.init(a);
    try store.enableIndexArena();
    store.retain_history = true;

    _ = try store.assert(1, 2, vi(3), M_BOOT);
    const epoch = store.index_arena.?;
    _ = try store.compact(a, {}, KeepAll.keep);

    try testing.expect(store.index_arena.? == epoch); // NOT swapped, old kept
    try testing.expectEqual(vi(3), store.currentValue(1, 2).?);
}

// -- Code-fact storage tiers (docs/19a-code-fact-tiers-plan.md) ---------------
// A_CODE is a real code attribute (ATTR_BYTECODE_STREAM); A_NAME/A_AGE/A_PARENT
// are data attrs (outside the code bands). Tests inject the real predicate.

const A_CODE: u64 = kernel_eids.ATTR_BYTECODE_STREAM; // 165 — in the code band

// Helper: history depth of (e,a) in EAVT (fields have no visibility in Zig).
fn histLen(store: *const fs.FactStore, e: u64, a: u64) usize {
    const inner = store.eavt.get(e) orelse return 0;
    const cell = inner.get(a) orelse return 0;
    return cell.history.count();
}

test "tier (Lever 1): code attr drops history; current value preserved; data attr historizes" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());
    store.setCodeAttrPred(&kernel_eids.isCodeAttr);

    _ = try store.assert(E1, A_CODE, vi(10), M_BOOT);
    _ = try store.assert(E1, A_CODE, vi(20), M_BOOT); // rewrite — would normally grow history

    // current_v preserved; history vec never grew.
    try testing.expectEqual(vi(20), store.currentValue(E1, A_CODE).?);
    try testing.expectEqual(@as(usize, 0), histLen(&store, E1, A_CODE));

    // asOf walks history, so a no-history attr is unreachable by time-travel
    // (current is still readable via currentValue). Documents the §1.3 nuance.
    try testing.expectEqual(@as(?ref.Ref, null), store.currentValueAsOf(E1, A_CODE, 1));

    // A data attr on the same entity still historizes normally.
    _ = try store.assert(E1, A_NAME, vi(30), M_BOOT);
    _ = try store.assert(E1, A_NAME, vi(40), M_BOOT);
    try testing.expect(histLen(&store, E1, A_NAME) > 0);
    try testing.expectEqual(vi(30), store.currentValueAsOf(E1, A_NAME, 3).?);
}

test "tier (Lever 2): code attr skips VAET; data attr keeps it; reverse-refs still work via AVET" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());
    store.setCodeAttrPred(&kernel_eids.isCodeAttr);

    const target = ref.mkEid(E3); // an eid-valued ref, the kind that lands in VAET

    // A code attr pointing at E3 — must NOT enter VAET.
    _ = try store.assert(E1, A_CODE, target, M_BOOT);
    try testing.expectEqual(@as(usize, 0), store.vaet.count());

    // The same value via a data attr — DOES enter VAET.
    _ = try store.assert(E2, A_PARENT, target, M_BOOT);
    try testing.expectEqual(@as(usize, 1), store.vaet.count());

    // pointingAt (the only VAET reader) sees only the data referrer, not the code one.
    var it = try store.pointingAt(target, arena.allocator());
    defer it.deinit();
    var seen_data = false;
    var seen_code = false;
    while (it.next()) |ae| {
        if (ae.a == A_PARENT and ae.e == E2) seen_data = true;
        if (ae.a == A_CODE) seen_code = true;
    }
    try testing.expect(seen_data);
    try testing.expect(!seen_code); // VAET-skipped

    // Reverse-ref-by-value over the code attr STILL works — it runs on the
    // retained AVET (this is what cReverseRefs/pull actually use), not VAET.
    var em = store.entitiesMatching(A_CODE, target);
    var found_e1 = false;
    while (em.next()) |e| {
        if (e == E1) found_e1 = true;
    }
    try testing.expect(found_e1);
}

test "tier (Lever 2): retract of a code fact still works (hasFact reads retained AVET)" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());
    store.setCodeAttrPred(&kernel_eids.isCodeAttr);

    _ = try store.assert(E1, A_CODE, vi(7), M_BOOT);
    try testing.expect(store.hasFact(E1, A_CODE, vi(7)));
    _ = try store.retract(E1, A_CODE, vi(7), M_BOOT);
    try testing.expect(!store.hasFact(E1, A_CODE, vi(7)));
    try testing.expectEqual(@as(?ref.Ref, null), store.currentValue(E1, A_CODE));
}

test "tier (Lever 2): VAET stays empty for code attrs across compact (the hot rebuild path)" {
    const a = testing.allocator;
    var store = fs.FactStore.init(a);
    defer store.fact_eids.deinit();
    defer store.fact_modules.deinit();
    store.setCodeAttrPred(&kernel_eids.isCodeAttr);
    try store.enableIndexArena();
    defer store.deinitIndexArena();

    // Many code-attr facts (eid-valued, so they would populate VAET if indexed).
    var i: u64 = 0;
    while (i < 200) : (i += 1) {
        _ = try store.assert(1000 + i, A_CODE, ref.mkEid(500 + i), M_BOOT);
    }
    try testing.expectEqual(@as(usize, 0), store.vaet.count());

    // compactInsert must carry the same tier, or the rebuild re-materializes VAET.
    _ = try store.compact(a, {}, KeepAll.keep);
    try testing.expectEqual(@as(usize, 0), store.vaet.count()); // still empty post-compact
    try testing.expectEqual(ref.mkEid(500), store.currentValue(1000, A_CODE).?); // values survive
    try testing.expectEqual(@as(usize, 0), histLen(&store, 1000, A_CODE)); // and no history
}

test "compact tightens the epoch: arena shrinks to live size, all values survive" {
    const a = testing.allocator;
    var store = fs.FactStore.init(a);
    defer store.fact_eids.deinit();
    defer store.fact_modules.deinit();
    try store.enableIndexArena();
    defer store.deinitIndexArena();

    // Build a non-trivial index: N distinct facts (per-assert path-copy fills
    // the live arena with abandoned spine garbage) plus a rewrite so a cell
    // carries history.
    const N: u64 = 6000;
    var i: u64 = 0;
    while (i < N) : (i += 1) {
        _ = try store.assert(1000 + i, 5, vi(@intCast(i)), M_BOOT);
    }
    _ = try store.assert(1000, 5, vi(424242), M_BOOT);

    const cap_before = store.index_arena.?.queryCapacity();
    const stats = try store.compact(a, {}, KeepAll.keep);
    const cap_after = store.index_arena.?.queryCapacity();
    try testing.expectEqual(@as(usize, N), stats.kept); // current-only collapse

    // Tightening: the rebuilt epoch holds only live structure, not the
    // per-fact path-copy garbage. Without it, the rebuild path-copies to a size
    // comparable to cap_before; with it, cap_after is a small fraction.
    try testing.expect(cap_after * 2 < cap_before);

    // Every current value survived the deep clone (the rewrite kept its v2).
    try testing.expectEqual(vi(424242), store.currentValue(1000, 5).?);
    i = 1;
    while (i < N) : (i += 1) {
        try testing.expectEqual(vi(@intCast(i)), store.currentValue(1000 + i, 5).?);
    }
}
