//! Digla-local accelerator for `Reflect allMethodsOf:`.
//!
//! This application module preserves the core descriptor contract while using
//! a basis-keyed reverse-global index for target names. It deliberately lives
//! outside the language/runtime floor.

const std = @import("std");
const idapi = @import("idapi");

const Oop = idapi.Oop;
const ca = std.heap.c_allocator;

var g_api: *idapi.IdApi = undefined;
var g_store: Oop = null;
var g_globals: Oop = null;
var sel_current_value: Oop = null;
var sel_live_triples_do: Oop = null;
var global_collector_vt: Oop = null;
var global_names: std.AutoHashMapUnmanaged(Oop, Oop) = .{};
var global_names_basis: i64 = -1;

fn send0(receiver: Oop, selector: [*:0]const u8) Oop {
    return g_api.send(receiver, g_api.intern(selector), &.{}, 0);
}

fn send1(receiver: Oop, selector: [*:0]const u8, arg: Oop) Oop {
    return g_api.send(receiver, g_api.intern(selector), &[_]Oop{arg}, 1);
}

fn mapPut(map: Oop, key: Oop, value: Oop) Oop {
    return g_api.send(map, g_api.intern("at:put:"), &[_]Oop{ key, value }, 2);
}

fn mCollectGlobal(_: Oop, args: [*]const Oop, _: usize) callconv(.c) Oop {
    if (args[0] == g_globals and args[2] != g_api.nilObj and !global_names.contains(args[2]))
        global_names.put(ca, args[2], args[1]) catch @panic("digla-reflect: oom");
    return g_api.nilObj;
}

fn refreshGlobalNames() void {
    global_names.clearRetainingCapacity();
    const collector = g_api.alloc(global_collector_vt, 0);
    _ = g_api.send(g_store, sel_live_triples_do, &[_]Oop{collector}, 1);
    global_names_basis = g_api.smallIntValue(send0(g_store, "basis"));
}

fn globalNameForValue(value: Oop) Oop {
    if (value == null or value == g_api.nilObj) return g_api.nilObj;
    const basis = g_api.smallIntValue(send0(g_store, "basis"));
    if (basis != global_names_basis) refreshGlobalNames();
    return global_names.get(value) orelse g_api.nilObj;
}

fn exportedSymbolForValue(value: Oop) Oop {
    if (value == null) return g_api.nilObj;
    if (g_api.exportedNameOf(value)) |name| return g_api.intern(name);
    return g_api.nilObj;
}

fn methodTargetName(vtable: Oop) Oop {
    const owner = g_api.vtableOwner(vtable);
    if (owner != null) {
        const exported_owner = exportedSymbolForValue(owner);
        if (exported_owner != g_api.nilObj) return exported_owner;
        const global_owner = globalNameForValue(owner);
        if (global_owner != g_api.nilObj) return global_owner;
    }

    const exported_vtable = exportedSymbolForValue(vtable);
    if (exported_vtable != g_api.nilObj) return exported_vtable;
    return globalNameForValue(vtable);
}

fn methodDescriptor(vtable: Oop, selector: Oop, target_name: Oop) Oop {
    const mirror = g_api.send(g_store, sel_current_value, &[_]Oop{ vtable, selector }, 2);
    const primitive = g_api.intern("primitive");
    const method = if (mirror == g_api.nilObj)
        primitive
    else if (mirror == primitive or g_api.objectPayloadSize(mirror) != 0)
        mirror
    else
        g_api.intern("compiled");

    var descriptor = send0(g_api.import("Map"), "new");
    descriptor = mapPut(descriptor, g_api.intern("selector"), selector);
    descriptor = mapPut(descriptor, g_api.intern("owner"), g_api.vtableOwner(vtable) orelse g_api.nilObj);
    descriptor = mapPut(descriptor, g_api.intern("target"), target_name);
    descriptor = mapPut(descriptor, g_api.intern("method"), method);
    return descriptor;
}

fn mAllMethodsOf(_: Oop, args: [*]const Oop, _: usize) callconv(.c) Oop {
    g_api.gcSetAuto(0);
    defer g_api.gcSetAuto(1);

    var result = send0(g_api.import("Vector"), "new");
    var seen_selectors: std.AutoHashMapUnmanaged(Oop, void) = .{};
    defer seen_selectors.deinit(ca);
    var seen_vtables: std.AutoHashMapUnmanaged(Oop, void) = .{};
    defer seen_vtables.deinit(ca);

    var vtable = g_api.vtableOf(args[0]);
    while (vtable != null and !seen_vtables.contains(vtable)) : (vtable = g_api.vtableParent(vtable)) {
        seen_vtables.put(ca, vtable, {}) catch @panic("digla-reflect: oom");
        const target_name = methodTargetName(vtable);

        var i: usize = 0;
        while (g_api.vtableSelectorAt(vtable, i)) |selector| : (i += 1) {
            if (seen_selectors.contains(selector)) continue;
            seen_selectors.put(ca, selector, {}) catch @panic("digla-reflect: oom");
            result = send1(result, "add:", methodDescriptor(vtable, selector, target_name));
        }
    }
    return result;
}

fn bringUp(api: *idapi.IdApi) void {
    g_api = api;
    g_store = api.import("Store");
    g_globals = api.intern("__globals__");
    sel_current_value = api.intern("currentValue:attr:");
    sel_live_triples_do = api.intern("liveTriplesDo:");
    global_collector_vt = api.proto(api.object_vt);
    api.method(global_collector_vt, api.intern("value:value:value:"), &mCollectGlobal);
    const reflect = api.import("Reflect");
    api.method(api.vtableOf(reflect), api.intern("allMethodsOf:"), &mAllMethodsOf);
}

fn moduleInit(api: *idapi.IdApi, state: ?*anyopaque) callconv(.c) ?*anyopaque {
    _ = state;
    bringUp(api);
    return null;
}

comptime {
    @export(&moduleInit, .{ .name = "module_init" });
}
