//! Proof for rung 3c (part 1): libtcc as an id module — compile C to native,
//! install the result as a method, and dispatch to it through the object model.

const std = @import("std");
const id = @import("id.zig");
const tcc = @import("tcc.zig");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;

fn str(api: *idapi.IdApi, bytes: []const u8) Oop {
    return api.mkString(bytes.ptr, bytes.len);
}

test "tcc 3c: compile C → native → install as method → dispatch" {
    const api = id.id_init();
    tcc.bringUp(api);
    const Tcc = api.import("Tcc").?;

    // A function with the birnam Method ABI returning the SmallInt 42 (tagged:
    // (42<<1)|1). This is what codegen will emit (for now hand-tagged; later it
    // calls back into the IdApi for value construction).
    const src =
        \\void* answer(void* self, void** args, long n) {
        \\  (void)self; (void)args; (void)n;
        \\  return (void*)((42L << 1) | 1L);
        \\}
    ;
    const unit = api.send(Tcc, api.intern("compile:"), &[_]Oop{str(api, src)}, 1);
    try std.testing.expect(unit != api.nilObj);

    // Install `answer` as #answer on a fresh type, then dispatch to it.
    const vt = api.proto(api.object_vt);
    const sel = api.intern("answer");
    const ok = api.send(unit, api.intern("install:as:on:"), &[_]Oop{ str(api, "answer"), sel, vt }, 3);
    try std.testing.expect(ok == vt);

    const inst = api.alloc(vt, 0);
    const r = api.send(inst, sel, &[_]Oop{}, 0);
    try std.testing.expectEqual(@as(i64, 42), api.smallIntValue(r));
}

test "tcc 3c: bad C returns nil; a missing symbol returns nil" {
    const api = id.id_init();
    tcc.bringUp(api);
    const Tcc = api.import("Tcc").?;

    try std.testing.expect(api.send(Tcc, api.intern("compile:"), &[_]Oop{str(api, "this is not valid c")}, 1) == api.nilObj);

    const unit = api.send(Tcc, api.intern("compile:"), &[_]Oop{str(api, "int present(void){return 1;}")}, 1);
    try std.testing.expect(unit != api.nilObj);
    try std.testing.expect(api.send(unit, api.intern("symbol:"), &[_]Oop{str(api, "absent")}, 1) == api.nilObj);
}
