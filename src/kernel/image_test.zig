//! Phase 12.1 slice 1 — image fact-layer round-trip.

const std = @import("std");
const testing = std.testing;
const image = @import("image.zig");
const fs = @import("fact_store.zig");
const ref = @import("ref.zig");
const boxed = @import("boxed.zig");
const strings = @import("strings.zig");
const kernel_eids = @import("kernel_eids.zig");
const compiler = @import("compiler.zig");
const object_table = @import("object_table.zig");
const object = @import("object.zig");
const bootstrap = @import("bootstrap.zig");
const interner_mod = @import("interner.zig");
const send_mod = @import("send.zig");

fn vi(n: i64) ref.Ref {
    return ref.mkSmallInt(n).?;
}

const M_USER: u64 = kernel_eids.M_USER;
const M_CODE: u64 = kernel_eids.M_LANG; // a code-module tag — must NOT persist
const E_A: u64 = 300;
const E_B: u64 = 301;
const A_REF: u64 = 400;
const A_NUM: u64 = 401;
const A_KW: u64 = 402;

test "image: immediate-valued facts round-trip with counters" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var src = fs.FactStore.init(a);
    _ = try src.assert(E_A, A_REF, ref.mkEid(E_B), M_USER);
    _ = try src.assert(E_A, A_NUM, vi(42), M_USER);
    _ = try src.assert(E_B, A_NUM, vi(-7), M_USER);
    _ = try src.assert(E_B, A_KW, ref.mkKeyword(99), M_USER);

    const SAVED_NEXT_EID: u64 = 512;
    const res = try image.write(a, &src, SAVED_NEXT_EID);

    try testing.expectEqual(@as(u64, 4), res.facts_written);
    try testing.expectEqual(@as(u64, 0), res.skipped_pointer_values);

    var dst = fs.FactStore.init(a);
    const loaded = try image.load(&dst, res.bytes);

    try testing.expectEqual(@as(u64, 4), loaded.facts_loaded);
    try testing.expectEqual(SAVED_NEXT_EID, loaded.next_eid);
    try testing.expectEqual(src.currentTx() + 1, loaded.next_tx);

    try testing.expectEqual(ref.mkEid(E_B), dst.currentValue(E_A, A_REF).?);
    try testing.expectEqual(vi(42), dst.currentValue(E_A, A_NUM).?);
    try testing.expectEqual(vi(-7), dst.currentValue(E_B, A_NUM).?);
    try testing.expectEqual(ref.mkKeyword(99), dst.currentValue(E_B, A_KW).?);

    // next_tx restored to the source high-water mark, not the replay count.
    try testing.expectEqual(src.currentTx(), dst.currentTx());
}

test "image: boxed float values round-trip (slice 3)" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var src = fs.FactStore.init(a);
    _ = try src.assert(E_A, A_REF, try boxed.mkFloat(a, 3.14159), M_USER);
    _ = try src.assert(E_B, A_REF, try boxed.mkFloat(a, -0.0), M_USER);

    const res = try image.write(a, &src, 256);
    try testing.expectEqual(@as(u64, 2), res.facts_written);
    try testing.expectEqual(@as(u64, 0), res.skipped_pointer_values);

    var dst = fs.FactStore.init(a);
    _ = try image.load(&dst, res.bytes);

    const got = dst.currentValue(E_A, A_REF).?;
    try testing.expect(boxed.isFloat(got));
    try testing.expectEqual(@as(f64, 3.14159), boxed.untagFloat(got));
    try testing.expectEqual(@as(f64, -0.0), boxed.untagFloat(dst.currentValue(E_B, A_REF).?));
}

test "image: RRB vector values round-trip incl mixed + nested (slice 3)" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var src = fs.FactStore.init(a);
    // Mixed-element vector: smallint, string, float — like a constant pool.
    const inner = try boxed.mkRrb(a, &.{
        vi(7),
        try strings.mkString(a, "pool-entry"),
        try boxed.mkFloat(a, 2.5),
    });
    // Nested: a vector containing the inner vector + an eid.
    const outer = try boxed.mkRrb(a, &.{ inner, ref.mkEid(E_B) });
    _ = try src.assert(E_A, A_REF, outer, M_USER);

    const res = try image.write(a, &src, 256);
    try testing.expectEqual(@as(u64, 1), res.facts_written);
    try testing.expectEqual(@as(u64, 0), res.skipped_pointer_values);

    var dst = fs.FactStore.init(a);
    _ = try image.load(&dst, res.bytes);

    const got_outer = dst.currentValue(E_A, A_REF).?;
    try testing.expect(boxed.isRrbRef(got_outer));
    const ro = boxed.untagRrbRef(got_outer);
    try testing.expectEqual(@as(usize, 2), ro.count());

    var oit = ro.iter();
    const got_inner = oit.next().?;
    try testing.expectEqual(ref.mkEid(E_B), oit.next().?);
    try testing.expect(boxed.isRrbRef(got_inner));
    const ri = boxed.untagRrbRef(got_inner);
    try testing.expectEqual(@as(usize, 3), ri.count());
    var iit = ri.iter();
    try testing.expectEqual(vi(7), iit.next().?);
    try testing.expectEqualStrings("pool-entry", strings.bytes(iit.next().?));
    try testing.expectEqual(@as(f64, 2.5), boxed.untagFloat(iit.next().?));
}

test "image: still-deferred pointer classes counted as skipped, never dropped" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var src = fs.FactStore.init(a);
    _ = try src.assert(E_A, A_NUM, vi(1), M_USER);
    // FFI pointer is a heap-pointer class not yet handled by the image.
    const ffi = try boxed.mkFfiPtr(a, 0xCAFE);
    try testing.expect(ref.isPointer(ffi));
    _ = try src.assert(E_A, A_REF, ffi, M_USER);

    const res = try image.write(a, &src, 256);
    try testing.expectEqual(@as(u64, 1), res.facts_written);
    try testing.expectEqual(@as(u64, 1), res.skipped_pointer_values);

    var dst = fs.FactStore.init(a);
    _ = try image.load(&dst, res.bytes);
    try testing.expectEqual(vi(1), dst.currentValue(E_A, A_NUM).?);
    try testing.expect(dst.currentValue(E_A, A_REF) == null);
}

test "image: string-valued facts round-trip by bytes (slice 2)" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var src = fs.FactStore.init(a);
    const s1 = try strings.mkString(a, "hello world");
    const s2 = try strings.mkString(a, ""); // empty string edge case
    _ = try src.assert(E_A, A_REF, s1, M_USER);
    _ = try src.assert(E_B, A_REF, s2, M_USER);
    _ = try src.assert(E_A, A_NUM, vi(5), M_USER); // mixed immediate + string

    const res = try image.write(a, &src, 256);
    try testing.expectEqual(@as(u64, 3), res.facts_written);
    try testing.expectEqual(@as(u64, 0), res.skipped_pointer_values);

    var dst = fs.FactStore.init(a);
    _ = try image.load(&dst, res.bytes);

    const got1 = dst.currentValue(E_A, A_REF).?;
    const got2 = dst.currentValue(E_B, A_REF).?;
    try testing.expect(strings.isString(got1));
    try testing.expectEqualStrings("hello world", strings.bytes(got1));
    try testing.expectEqualStrings("", strings.bytes(got2));
    try testing.expectEqual(vi(5), dst.currentValue(E_A, A_NUM).?);

    // Rebuilt string is a fresh allocation, not the original pointer.
    try testing.expect(got1 != s1);
}

test "image: persists M_USER facts only, skips code-module provenance" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var src = fs.FactStore.init(a);
    _ = try src.assert(E_A, A_NUM, vi(1), M_USER); // user data — persist
    _ = try src.assert(E_B, A_NUM, vi(2), M_CODE); // code fact — reconstructed on load, skip

    const res = try image.write(a, &src, 256);
    try testing.expectEqual(@as(u64, 1), res.facts_written);

    var dst = fs.FactStore.init(a);
    _ = try image.load(&dst, res.bytes);
    try testing.expectEqual(vi(1), dst.currentValue(E_A, A_NUM).?);
    // The code-module fact was not persisted (init rebuilds it).
    try testing.expect(dst.currentValue(E_B, A_NUM) == null);
}

test "provenance: module unload (retractAllOf) leaves M_USER data intact" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store = fs.FactStore.init(a);
    // A code-module install fact + a user-data fact on the same entity.
    _ = try store.assert(E_A, A_NUM, vi(100), M_CODE);
    _ = try store.assert(E_A, A_REF, ref.mkEid(E_B), M_USER);

    // Mirror Manager.retractAllOf(M_CODE): retract every fact owned by the
    // code module. M_USER facts live in a disjoint meat subtree, so they
    // are unreachable from this walk.
    var it = try store.factsOfModule(M_CODE, a);
    defer it.deinit();
    while (it.next()) |t| _ = try store.retract(t.e, t.a, t.v, M_CODE);

    // Code fact gone; user data survives the unload.
    try testing.expect(store.currentValue(E_A, A_NUM) == null);
    try testing.expectEqual(ref.mkEid(E_B), store.currentValue(E_A, A_REF).?);
}

test "loader: bytecode-method closure rebuilds from facts (Approach B)" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    // Build + materialize a trivial method (`^42`) under M_USER in store A.
    var src = fs.FactStore.init(a);
    var tbl_a = object_table.ObjectTable.init(a);
    const ast: u64 = 1000;
    _ = try src.assert(ast, kernel_eids.ATTR_VTABLE, ref.mkEid(kernel_eids.AST_LITERAL_VT), M_USER);
    _ = try src.assert(ast, kernel_eids.ATTR_AST_VALUE, vi(42), M_USER);

    var m = try compiler.compileMethod(a, &src, ast);
    defer m.deinit(a);
    const method_eid = try compiler.materialize(a, &src, &tbl_a, &m, M_USER);
    try testing.expect(tbl_a.get(method_eid) != null); // closure live in A

    // Round-trip facts into a fresh store B (no object table content).
    const res = try image.write(a, &src, 256);
    var dst = fs.FactStore.init(a);
    var tbl_b = object_table.ObjectTable.init(a);
    _ = try image.load(&dst, res.bytes);

    // Method facts present, but the closure is NOT (object table not saved).
    try testing.expect(dst.currentValue(method_eid, kernel_eids.ATTR_BYTECODE_STREAM) != null);
    try testing.expect(tbl_b.get(method_eid) == null);

    // Approach B: rebuild the fn-ptr closure from facts.
    const n = try image.rematerializeClosures(a, &dst, &tbl_b);
    try testing.expectEqual(@as(usize, 1), n);

    // Closure now live with the correct libid-object shape: vt at -1 ==
    // CLOSURE_VT, data == mkEid(method_eid).
    const ptr_ref = tbl_b.get(method_eid).?;
    try testing.expect(ref.isPointer(ptr_ref));
    // Closure { method, data }: slot 0 = fn ptr, slot 1 = data.
    const obj = object.fromRef(ptr_ref);
    try testing.expectEqual(ref.mkEid(kernel_eids.CLOSURE_VT), object.vtableOf(obj));
    try testing.expect(object.slotAt(obj, 0) != 0); // method fn ptr present
    try testing.expectEqual(ref.mkEid(method_eid), object.slotAt(obj, 1)); // data

    // Idempotent: a second pass rebuilds nothing.
    try testing.expectEqual(@as(usize, 0), try image.rematerializeClosures(a, &dst, &tbl_b));
}

test "end-to-end: user method survives save -> reboot -> load -> dispatch" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const al = arena.allocator();

    // An arbitrary selector eid, stable across boots, above the kernel
    // range and far above any allocated user eid. bind() uses it purely as
    // an attribute key, so no interned name is required.
    const SEL: u64 = 1 << 20;
    const AST: u64 = 700;

    // ---- Boot A: define `answer = ^42` on SMALLINT_VT, save image. ----
    const store_a = try al.create(fs.FactStore);
    store_a.* = fs.FactStore.init(al);
    const table_a = try al.create(object_table.ObjectTable);
    table_a.* = object_table.ObjectTable.init(al);
    const intern_a = try al.create(interner_mod.Interner);
    intern_a.* = interner_mod.Interner.init(al);
    _ = try bootstrap.boot(al, store_a, table_a, intern_a);

    // Method body AST = literal 42 (compiles to PUSH 42; RETURN).
    _ = try store_a.assert(AST, kernel_eids.ATTR_VTABLE, ref.mkEid(kernel_eids.AST_LITERAL_VT), M_USER);
    _ = try store_a.assert(AST, kernel_eids.ATTR_AST_VALUE, vi(42), M_USER);
    var m = try compiler.compileMethod(al, store_a, AST);
    defer m.deinit(al);
    const method_eid = try compiler.materialize(al, store_a, table_a, &m, M_USER);
    // Install on SMALLINT_VT under the chosen selector (user DATA).
    _ = try store_a.assert(kernel_eids.SMALLINT_VT, SEL, ref.mkEid(method_eid), M_USER);

    // Pre-save dispatch works.
    var no_args: [0]ref.Ref = .{};
    try testing.expectEqual(vi(42), send_mod.sendN(vi(0), SEL, &no_args, 0));

    const res = try image.write(al, store_a, table_a.next_eid.load(.monotonic));

    // ---- Reboot B: fresh kernel (no lang dylib), load image. ----
    const store_b = try al.create(fs.FactStore);
    store_b.* = fs.FactStore.init(al);
    const table_b = try al.create(object_table.ObjectTable);
    table_b.* = object_table.ObjectTable.init(al);
    const intern_b = try al.create(interner_mod.Interner);
    intern_b.* = interner_mod.Interner.init(al);
    _ = try bootstrap.boot(al, store_b, table_b, intern_b); // initDispatch: globals->B, mcache cleared, Vm reset

    const loaded = try image.load(store_b, res.bytes);
    table_b.next_eid.store(loaded.next_eid, .monotonic); // restore eid allocator
    _ = try image.rematerializeClosures(al, store_b, table_b); // Approach B

    // The fn ptr was never serialized; it was rebuilt against this boot's
    // kernel. Re-sending the selector dispatches the restored method.
    try testing.expectEqual(vi(42), send_mod.sendN(vi(0), SEL, &no_args, 0));
}

test "image: rejects bad magic and version" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var dst = fs.FactStore.init(arena.allocator());

    try testing.expectError(error.BadMagic, image.load(&dst, "not an image"));
    try testing.expectError(error.Truncated, image.load(&dst, "BRN"));
}
