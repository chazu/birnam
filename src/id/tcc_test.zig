//! Proof for rung 3c (part 1): libtcc as an id module — compile C to native,
//! install the result as a method, and dispatch to it through the object model.

const std = @import("std");
const floor = @import("floor_adapter_test.zig");
const tcc = @import("tcc.zig");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;

fn str(api: *idapi.IdApi, bytes: []const u8) Oop {
    return api.mkString(bytes.ptr, bytes.len);
}

test "tcc 3c: compile C → native → install as method → dispatch" {
    const api = floor.api();
    tcc.bringUp(api);
    const Tcc = api.import("Tcc").?;

    // A function with the birnam Method ABI returning the Integer 42. Integers are
    // boxed now (no immediates), so the answer is built through the birnam_mkSmallInt
    // runtime shim — which needs the STAGED compile path (new → compile → bindRuntime
    // → relocate), since `compile:` alone relocates before a shim could be bound.
    const src =
        \\extern void* birnam_mkSmallInt(long);
        \\void* answer(void* self, void** args, long n) {
        \\  (void)self; (void)args; (void)n;
        \\  return birnam_mkSmallInt(42L);
        \\}
    ;
    const unit = api.send(Tcc, api.intern("new"), &[_]Oop{}, 0);
    try std.testing.expect(unit != api.nilObj);
    _ = api.send(unit, api.intern("compile:"), &[_]Oop{str(api, src)}, 1);
    _ = api.send(unit, api.intern("bindRuntime"), &[_]Oop{}, 0);
    try std.testing.expect(api.send(unit, api.intern("relocate"), &[_]Oop{}, 0) != api.nilObj);

    // Install `answer` as #answer on a fresh type, then dispatch to it.
    const vt = api.proto(api.object_vt);
    const sel = api.intern("answer");
    const ok = api.send(unit, api.intern("install:as:on:"), &[_]Oop{ str(api, "answer"), sel, vt }, 3);
    try std.testing.expect(ok == vt);

    const inst = api.alloc(vt, 0);
    const r = api.send(inst, sel, &[_]Oop{}, 0);
    try std.testing.expectEqual(@as(i64, 42), api.smallIntValue(r));
}

test "tcc: a disposable unit is reclaimed by the collector (frees its code)" {
    const api = floor.api();
    tcc.bringUp(api);
    const Tcc = api.import("Tcc").?;

    var anchor: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&anchor));
    _ = &anchor;
    _ = api.gcCollect(); // settle
    const before = api.gcLiveCount();

    // Compile + relocate many small units, marking each disposable and dropping
    // it. Each holds a TCCState (executable pages); the disposable finalizer must
    // let the collector reclaim both the unit object and (via tcc_delete) the code.
    makeDisposableUnits(api, Tcc, 60);
    _ = api.gcCollect();

    // Nearly all reclaimed — the live set returns near baseline rather than
    // growing by 60 (it would, if disposable units leaked their TccState forever).
    try std.testing.expect(api.gcLiveCount() <= before + 20);
}

noinline fn makeDisposableUnits(api: *idapi.IdApi, Tcc: Oop, count: usize) void {
    var i: usize = 0;
    var u: Oop = null;
    while (i < count) : (i += 1) {
        const src = "void* f(void* self, void** a, long n){ (void)self;(void)a;(void)n; return (void*)0; }";
        u = api.send(Tcc, api.intern("new"), &[_]Oop{}, 0);
        _ = api.send(u, api.intern("compile:"), &[_]Oop{str(api, src)}, 1);
        _ = api.send(u, api.intern("relocate"), &[_]Oop{}, 0);
        _ = api.send(u, api.intern("disposable"), &[_]Oop{}, 0);
    }
    std.mem.doNotOptimizeAway(&u);
}

test "tcc: compile C to a loadable .dylib on disk (AOT output)" {
    const api = floor.api();
    tcc.bringUp(api);
    const Tcc = api.import("Tcc").?;

    const path = "/tmp/birnam_tcc_answer.dylib";
    std.fs.cwd().deleteFile(path) catch {}; // start clean
    const src = "int answer(void){ return 42; }";
    const r = api.send(Tcc, api.intern("compile:output:as:"), &[_]Oop{ str(api, src), str(api, path), api.intern("dll") }, 3);

    if (r == api.nilObj) {
        // The vendored TinyCC could not write a shared library on this platform —
        // surface it loudly rather than silently passing.
        std.debug.print("tcc compile:output:as: #dll returned nil (no dylib support on this target)\n", .{});
        return error.DylibOutputUnsupported;
    }

    // It claims success — prove the artifact is a real, loadable, callable dylib.
    var lib = try std.DynLib.open(path);
    defer lib.close();
    const answer = lib.lookup(*const fn () callconv(.c) c_int, "answer") orelse return error.SymbolMissing;
    try std.testing.expectEqual(@as(c_int, 42), answer());

    // An unknown output kind is rejected; a compile error yields nil.
    try std.testing.expect(api.send(Tcc, api.intern("compile:output:as:"), &[_]Oop{ str(api, src), str(api, path), api.intern("widget") }, 3) == api.nilObj);
    try std.testing.expect(api.send(Tcc, api.intern("compile:output:as:"), &[_]Oop{ str(api, "not c"), str(api, path), api.intern("dll") }, 3) == api.nilObj);
}

/// A host-process export standing in for the `birnam_*` runtime shims: the
/// relocatable-artifact design (docs/40 §3a, slice 1) rests on an emitted dylib
/// leaving `birnam_*` as undefined externals that the dynamic loader resolves
/// against the running process at dlopen.
export fn birnam_probe() callconv(.c) i64 {
    return 4242;
}

test "tcc slice-1: a dylib's undefined external resolves against the host at dlopen" {
    const api = floor.api();
    tcc.bringUp(api);
    const Tcc = api.import("Tcc").?;

    const path = "/tmp/birnam_tcc_undef.dylib";
    std.fs.cwd().deleteFile(path) catch {};
    // `birnam_probe` is UNDEFINED in the artifact — provided by this test binary.
    const src =
        \\extern long birnam_probe(void);
        \\long call_probe(void){ return birnam_probe(); }
    ;
    const r = api.send(Tcc, api.intern("compile:output:as:"), &[_]Oop{ str(api, src), str(api, path), api.intern("dll") }, 3);
    if (r == api.nilObj) return error.DylibOutputRefusedUndefinedExternal;

    var lib = try std.DynLib.open(path);
    defer lib.close();
    const call_probe = lib.lookup(*const fn () callconv(.c) i64, "call_probe") orelse return error.SymbolMissing;
    try std.testing.expectEqual(@as(i64, 4242), call_probe());
}

test "tcc 3c: bad C returns nil; a missing symbol returns nil" {
    const api = floor.api();
    tcc.bringUp(api);
    const Tcc = api.import("Tcc").?;

    try std.testing.expect(api.send(Tcc, api.intern("compile:"), &[_]Oop{str(api, "this is not valid c")}, 1) == api.nilObj);

    const unit = api.send(Tcc, api.intern("compile:"), &[_]Oop{str(api, "int present(void){return 1;}")}, 1);
    try std.testing.expect(unit != api.nilObj);
    try std.testing.expect(api.send(unit, api.intern("symbol:"), &[_]Oop{str(api, "absent")}, 1) == api.nilObj);
}

test "tcc 3c.3: staged compile opens a pre-relocate symbol window" {
    const api = floor.api();
    tcc.bringUp(api);
    const Tcc = api.import("Tcc").?;

    // Emitted C that calls BACK INTO libid: it constructs its SmallInt result via
    // the runtime shim rather than hand-tagging. `birnam_mkSmallInt` is an
    // unresolved external until `bindRuntime` patches it before `relocate`.
    const src =
        \\extern void* birnam_mkSmallInt(long);
        \\void* viaRuntime(void* self, void** args, long n) {
        \\  (void)self; (void)args; (void)n;
        \\  return birnam_mkSmallInt(123);
        \\}
    ;
    const unit = api.send(Tcc, api.intern("new"), &[_]Oop{}, 0);
    try std.testing.expect(unit != api.nilObj);
    try std.testing.expect(api.send(unit, api.intern("compile:"), &[_]Oop{str(api, src)}, 1) == unit);
    try std.testing.expect(api.send(unit, api.intern("bindRuntime"), &[_]Oop{}, 0) == unit);
    try std.testing.expect(api.send(unit, api.intern("relocate"), &[_]Oop{}, 0) == unit);

    const vt = api.proto(api.object_vt);
    const sel = api.intern("viaRuntime");
    const ok = api.send(unit, api.intern("install:as:on:"), &[_]Oop{ str(api, "viaRuntime"), sel, vt }, 3);
    try std.testing.expect(ok == vt);

    const inst = api.alloc(vt, 0);
    const r = api.send(inst, sel, &[_]Oop{}, 0);
    try std.testing.expectEqual(@as(i64, 123), api.smallIntValue(r));
}

test "tcc 3c.3: emitted C can send a message back through libid" {
    const api = floor.api();
    tcc.bringUp(api);
    const Tcc = api.import("Tcc").?;

    // A method that re-dispatches #double to self via birnam_send — proving the
    // full callback (intern a selector + send) survives the relocate window.
    const src =
        \\extern void* birnam_send(void* rcv, void* sel, void** args, long n);
        \\extern void* birnam_intern(const char* name);
        \\extern long birnam_smallIntValue(void*);
        \\extern void* birnam_mkSmallInt(long);
        \\void* callDouble(void* self, void** args, long n) {
        \\  (void)args; (void)n;
        \\  return birnam_send(self, birnam_intern("double"), 0, 0);
        \\}
        \\void* doubleIt(void* self, void** args, long n) {
        \\  (void)args; (void)n;
        \\  return birnam_mkSmallInt(birnam_smallIntValue(self) * 2);  /* Integer is boxed now */
        \\}
    ;
    const unit = api.send(Tcc, api.intern("new"), &[_]Oop{}, 0);
    try std.testing.expect(api.send(unit, api.intern("compile:"), &[_]Oop{str(api, src)}, 1) == unit);
    try std.testing.expect(api.send(unit, api.intern("bindRuntime"), &[_]Oop{}, 0) == unit);
    try std.testing.expect(api.send(unit, api.intern("relocate"), &[_]Oop{}, 0) == unit);

    // Install both on the SmallInt vtable, then send #callDouble to a fixnum.
    _ = api.send(unit, api.intern("install:as:on:"), &[_]Oop{ str(api, "callDouble"), api.intern("callDouble"), api.smallint_vt }, 3);
    _ = api.send(unit, api.intern("install:as:on:"), &[_]Oop{ str(api, "doubleIt"), api.intern("double"), api.smallint_vt }, 3);

    const r = api.send(api.mkSmallInt(21), api.intern("callDouble"), &[_]Oop{}, 0);
    try std.testing.expectEqual(@as(i64, 42), api.smallIntValue(r));
}
