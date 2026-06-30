//! Proof for the FFI: wrap C library functions as birnam methods by generating C
//! shims and compiling them with Tcc. Uses libc functions (resolvable in the
//! process, no external lib needed) to exercise int / string-in / string-out.

const std = @import("std");
const id = @import("id.zig");
const tcc = @import("tcc.zig");
const alien = @import("alien.zig");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;

fn setup() *idapi.IdApi {
    const api = id.id_init();
    tcc.bringUp(api);
    alien.bringUp(api);
    return api;
}

fn str(api: *idapi.IdApi, s: []const u8) Oop {
    return api.mkString(s.ptr, s.len);
}

test "alien: wrap libc strlen — receiver (a String) is the C argument" {
    const api = setup();
    const Alien = api.import("Alien").?;
    // Wrap onto the String vtable so a String receiver IS the const char* argument.
    const ok = api.send(Alien, api.intern("wrap:signature:as:on:"), &[_]Oop{ str(api, "strlen"), str(api, "l s"), api.intern("clen"), api.string_vt }, 4);
    try std.testing.expect(ok == api.string_vt);

    const r = api.send(str(api, "hello, world"), api.intern("clen"), &[_]Oop{}, 0);
    try std.testing.expectEqual(@as(i64, 12), api.smallIntValue(r));
}

test "alien: wrap libc abs — receiver (an Integer) is the C argument" {
    const api = setup();
    const Alien = api.import("Alien").?;
    _ = api.send(Alien, api.intern("wrap:signature:as:on:"), &[_]Oop{ str(api, "abs"), str(api, "i i"), api.intern("absVia"), api.smallint_vt }, 4);
    const r = api.send(api.mkSmallInt(-7), api.intern("absVia"), &[_]Oop{}, 0);
    try std.testing.expectEqual(@as(i64, 7), api.smallIntValue(r));
}

test "alien: wrap libc getenv (string receiver → string return; nil when absent)" {
    const api = setup();
    const Alien = api.import("Alien").?;
    _ = api.send(Alien, api.intern("wrap:signature:as:on:"), &[_]Oop{ str(api, "getenv"), str(api, "s s"), api.intern("getenvVia"), api.string_vt }, 4);

    // PATH is always set → a non-empty String; an absent var returns nil.
    const r = api.send(str(api, "PATH"), api.intern("getenvVia"), &[_]Oop{}, 0);
    try std.testing.expect(api.vtableOf(r) == api.string_vt);
    var len: usize = 0;
    _ = api.stringBytes(r, &len);
    try std.testing.expect(len > 0);

    const miss = api.send(str(api, "BIRNAM_NO_SUCH_VAR_XYZ"), api.intern("getenvVia"), &[_]Oop{}, 0);
    try std.testing.expect(miss == api.nilObj);
}

test "alien: an invalid signature is rejected (nil, no crash)" {
    const api = setup();
    const Alien = api.import("Alien").?;
    const vt = api.proto(api.object_vt);
    // 'q' is not a known type char.
    const bad = api.send(Alien, api.intern("wrap:signature:as:on:"), &[_]Oop{ str(api, "strlen"), str(api, "i q"), api.intern("nope:"), vt }, 4);
    try std.testing.expect(bad == api.nilObj);
    // An empty signature is rejected too.
    const empty = api.send(Alien, api.intern("wrap:signature:as:on:"), &[_]Oop{ str(api, "strlen"), str(api, ""), api.intern("nope2:"), vt }, 4);
    try std.testing.expect(empty == api.nilObj);
}
