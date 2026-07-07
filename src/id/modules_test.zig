//! Proof for the module manager (stage 3): the dependency graph is store facts,
//! and `loadOrder:` topologically sorts it (deps first) or answers a
//! `#circularDependency` error on a cycle — no loop.

const std = @import("std");
const id = @import("id.zig");
const store = @import("store.zig");
const collection = @import("collection.zig");
const errmod = @import("error.zig");
const modules = @import("modules.zig");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;

var gapi: *idapi.IdApi = undefined;

fn setup() *idapi.IdApi {
    const api = id.id_init();
    gapi = api;
    store.bringUp(api); // exports Store/db (the fact store the graph lives in)
    collection.bringUp(api); // Vector factory
    errmod.bringUp(api); // so a cycle yields a kinded error, not nil
    modules.bringUp(api); // the manager under test
    return api;
}

fn sym(name: [:0]const u8) Oop {
    return gapi.intern(name.ptr);
}
/// Build a `#[ … ]` Vector of symbols.
fn vec(syms: []const [:0]const u8) Oop {
    var v = gapi.send(gapi.import("Vector"), gapi.intern("new"), &.{}, 0);
    for (syms) |s| v = gapi.send(v, gapi.intern("add:"), &.{sym(s)}, 1);
    return v;
}
fn named(api: *idapi.IdApi, name: [:0]const u8, reqs: []const [:0]const u8) void {
    _ = api.send(modules.module_obj, api.intern("named:requires:"), &.{ sym(name), vec(reqs) }, 2);
}
fn at(v: Oop, i: i64) Oop {
    return gapi.send(v, gapi.intern("at:"), &.{gapi.mkSmallInt(i)}, 1);
}
fn size(v: Oop) i64 {
    return gapi.smallIntValue(gapi.send(v, gapi.intern("size"), &.{}, 0));
}

test "modules: loadOrder topologically sorts — every dependency precedes its dependent" {
    const api = setup();
    // a → {b, c}, b → {c}, c → {} . The only valid order is c, b, a.
    named(api, "a", &.{ "b", "c" });
    named(api, "b", &.{"c"});
    named(api, "c", &.{});

    const order = api.send(modules.module_obj, api.intern("loadOrder:"), &.{vec(&.{"a"})}, 1);
    try std.testing.expectEqual(@as(i64, 3), size(order));
    try std.testing.expect(at(order, 0) == sym("c")); // no deps → first
    try std.testing.expect(at(order, 1) == sym("b")); // needs c
    try std.testing.expect(at(order, 2) == sym("a")); // needs b, c → last

    // A shared dependency is emitted exactly once even when reached twice.
    var seen_c: usize = 0;
    var i: i64 = 0;
    while (i < 3) : (i += 1) if (at(order, i) == sym("c")) {
        seen_c += 1;
    };
    try std.testing.expectEqual(@as(usize, 1), seen_c);
}

test "modules: a dependency cycle answers #circularDependency, not an infinite loop" {
    const api = setup();
    named(api, "x", &.{"y"});
    named(api, "y", &.{"x"}); // x ⇄ y

    const r = api.send(modules.module_obj, api.intern("loadOrder:"), &.{vec(&.{"x"})}, 1);
    try std.testing.expect(api.send(r, api.intern("isError"), &.{}, 0) == api.trueObj);
    try std.testing.expect(api.send(r, api.intern("kind"), &.{}, 0) == sym("circularDependency"));
}

test "modules: kernel modules are represented (registered, kind #kernel), not required to be loaded" {
    const api = setup();
    _ = api.send(modules.module_obj, api.intern("kernel:"), &.{sym("store")}, 1);

    try std.testing.expect(api.send(modules.module_obj, api.intern("isRegistered:"), &.{sym("store")}, 1) == api.trueObj);
    try std.testing.expect(api.send(modules.module_obj, api.intern("kindOf:"), &.{sym("store")}, 1) == sym("kernel"));
    // An unregistered name is not registered and has an empty requires set.
    try std.testing.expect(api.send(modules.module_obj, api.intern("isRegistered:"), &.{sym("ghost")}, 1) == api.falseObj);
    try std.testing.expectEqual(@as(i64, 0), size(api.send(modules.module_obj, api.intern("requiresOf:"), &.{sym("ghost")}, 1)));

    // A self-hosted module that requires a kernel one sorts after it (kernel = leaf).
    named(api, "app", &.{"store"});
    const order = api.send(modules.module_obj, api.intern("loadOrder:"), &.{vec(&.{"app"})}, 1);
    try std.testing.expectEqual(@as(i64, 2), size(order));
    try std.testing.expect(at(order, 0) == sym("store"));
    try std.testing.expect(at(order, 1) == sym("app"));
}
