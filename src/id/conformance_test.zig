//! External IdApi conformance shared by every floor implementation.
//! This root must not import id.zig.

const std = @import("std");
const floor = @import("floor_adapter_test.zig");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;

var callback_api: ?*idapi.IdApi = null;
var dnu_selector: Oop = null;
var dnu_count: usize = 0;
var dnu_args: [16]Oop = undefined;

fn answer42(_: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return callback_api.?.mkSmallInt(42);
}

fn answer7(_: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return callback_api.?.mkSmallInt(7);
}

fn captureDnu(self: Oop, args: [*]const Oop, count: usize) callconv(.c) Oop {
    dnu_selector = args[0];
    dnu_count = count - 1;
    const forwarded = @min(dnu_args.len, dnu_count);
    if (forwarded > 0) @memcpy(dnu_args[0..forwarded], args[1 .. 1 + forwarded]);
    return self;
}

var null_zero_seen = false;
const exact_layout = [_]u8{ idapi.slot_oop, idapi.slot_raw, idapi.slot_code };
const precise_layout = [_]u8{ idapi.slot_oop, idapi.slot_raw };

var gc_roots: [3]Oop = .{ null, null, null };
var gc_provider_registered = false;
var gc_provider_calls: usize = 0;
var precise_followed_address: usize = 0;
var precise_raw_address: usize = 0;
var trace_target: Oop = null;
var trace_calls: usize = 0;
var finalizer_calls: usize = 0;

fn conformanceRootProvider(mark: idapi.MarkFn) callconv(.c) void {
    gc_provider_calls += 1;
    for (gc_roots) |root| if (root != null) mark(root);
}

fn ensureRootProvider(api: *idapi.IdApi) void {
    if (gc_provider_registered) return;
    api.gcRegisterRootProvider(&conformanceRootProvider);
    gc_provider_registered = true;
}

fn conformanceTraceHook(_: Oop, mark: idapi.MarkFn) callconv(.c) void {
    trace_calls += 1;
    if (trace_target != null) mark(trace_target);
}

fn conformanceFinalizer(_: Oop) callconv(.c) void {
    finalizer_calls += 1;
}

noinline fn makeGarbage(api: *idapi.IdApi, count: usize, payload_size: usize) void {
    var last: Oop = null;
    for (0..count) |_| last = api.alloc(api.object_vt, payload_size);
    std.mem.doNotOptimizeAway(&last);
}

noinline fn makeFinalizableGarbage(api: *idapi.IdApi, count: usize) void {
    var last: Oop = null;
    for (0..count) |_| {
        last = api.alloc(api.object_vt, 0);
        api.setFinalizer(last, &conformanceFinalizer);
    }
    std.mem.doNotOptimizeAway(&last);
}

noinline fn makePreciseGraph(api: *idapi.IdApi, container_vt: Oop) void {
    const followed = api.alloc(api.object_vt, 0);
    const raw = api.alloc(api.object_vt, 0);
    const container = api.alloc(container_vt, 2 * @sizeOf(Oop));
    const slots: [*]Oop = @ptrCast(@alignCast(container.?));
    slots[0] = followed;
    slots[1] = raw;
    precise_followed_address = @intFromPtr(followed.?);
    precise_raw_address = @intFromPtr(raw.?);
    gc_roots[0] = container;
}

noinline fn scrubConservativeStack() void {
    var scratch: [4096]usize = @splat(0);
    for (&scratch, 0..) |*word, i| word.* = i;
    std.mem.doNotOptimizeAway(&scratch);
}

fn captureNullZero(self: Oop, args: ?[*]const Oop, count: usize) callconv(.c) Oop {
    null_zero_seen = args == null and count == 0;
    return self;
}

test "conformance harness selects one identified floor and initializes it once" {
    const first = floor.api();
    const second = floor.api();

    try std.testing.expect(first == second);
    try std.testing.expectEqual(@as(usize, 1), floor.initCallCount());
    try std.testing.expect(floor.retainedHandle() != null);
    try std.testing.expect(first.import("Object") != null);
    try std.testing.expect(first.import("Integer") != null);
    try std.testing.expect(first.import("String") != null);
}

test "dispatch is metacircular and a user method executes through IdApi" {
    const api = floor.api();
    callback_api = api;

    try std.testing.expect(api.vtableOf(api.vtable_vt) == api.vtable_vt);
    try std.testing.expect(api.vtableOf(api.object_vt) == api.vtable_vt);

    const point_vt = api.proto(api.object_vt);
    try std.testing.expect(api.vtableParent(point_vt) == api.object_vt);
    const answer = api.intern("conformanceDispatchAnswer");
    api.method(point_vt, answer, &answer42);
    const point = api.alloc(point_vt, 0);
    const result = api.send(point, answer, &.{}, 0);
    try std.testing.expect(api.isSmallInt(result) != 0);
    try std.testing.expectEqual(@as(i64, 42), api.smallIntValue(result));
}

test "direct method removal reveals live inheritance without a tombstone" {
    const api = floor.api();
    callback_api = api;

    const parent_vt = api.proto(api.object_vt);
    const child_vt = api.proto(parent_vt);
    const selector = api.intern("conformanceRemovableAnswer");
    api.method(parent_vt, selector, &answer42);
    api.method(child_vt, selector, &answer7);
    const object = api.alloc(child_vt, 0);

    try std.testing.expectEqual(@as(i64, 7), api.smallIntValue(api.send(object, selector, &.{}, 0)));
    const remove = api.intern("removeMethod:");
    const removed = api.send(child_vt, remove, &.{selector}, 1);
    const answer7_oop: Oop = @ptrCast(@constCast(&answer7));
    try std.testing.expect(removed == answer7_oop);
    try std.testing.expectEqual(@as(i64, 42), api.smallIntValue(api.send(object, selector, &.{}, 0)));

    // No copied parent binding: changing the parent remains immediately visible.
    api.method(parent_vt, selector, &answer7);
    try std.testing.expectEqual(@as(i64, 7), api.smallIntValue(api.send(object, selector, &.{}, 0)));
    try std.testing.expect(api.send(child_vt, remove, &.{selector}, 1) == null);
}

test "DNU is overridable and forwards exactly sixteen original arguments" {
    const api = floor.api();
    const proxy_vt = api.proto(api.object_vt);
    api.method(proxy_vt, api.intern("doesNotUnderstand:"), &captureDnu);
    const proxy = api.alloc(proxy_vt, 0);
    const missing = api.intern("conformanceMissing:with:many:arguments:");
    var original: [18]Oop = undefined;
    for (&original, 0..) |*arg, i| arg.* = api.mkSmallInt(@intCast(i));

    dnu_selector = null;
    dnu_count = 0;
    const result = api.send(proxy, missing, &original, original.len);
    try std.testing.expect(result == proxy);
    try std.testing.expect(dnu_selector == missing);
    try std.testing.expectEqual(@as(usize, 16), dnu_count);
    for (dnu_args, 0..) |arg, i| try std.testing.expect(arg == original[i]);
}

test "a null zero-argument buffer reaches the resolved Method unchanged" {
    const api = floor.api();
    const NullableMethod = *const fn (Oop, ?[*]const Oop, usize) callconv(.c) Oop;
    const NullableSend = *const fn (Oop, Oop, ?[*]const Oop, usize) callconv(.c) Oop;
    const vtable = api.proto(api.object_vt);
    const selector = api.intern("conformanceNullZeroArgs");
    const nullable_method: NullableMethod = &captureNullZero;
    api.method(vtable, selector, @ptrCast(nullable_method));
    const object = api.alloc(vtable, 0);
    const send: NullableSend = @ptrCast(api.send);

    null_zero_seen = false;
    try std.testing.expect(send(object, selector, null, 0) == object);
    try std.testing.expect(null_zero_seen);
}

test "Integer preserves the full signed width" {
    const api = floor.api();
    const values = [_]i64{ std.math.minInt(i64), -1, 0, 1, 42, std.math.maxInt(i64) };
    for (values) |value| {
        const object = api.mkSmallInt(value);
        try std.testing.expect(api.isSmallInt(object) != 0);
        try std.testing.expect(api.vtableOf(object) == api.smallint_vt);
        try std.testing.expectEqual(value, api.smallIntValue(object));
        try std.testing.expect(api.isFloat(object) == 0);
        try std.testing.expect(api.isWord(object) == 0);
    }
}

test "Float preserves boxed f64 bit patterns" {
    const api = floor.api();
    const patterns = [_]u64{
        0x0000000000000000, // +0
        0x8000000000000000, // -0
        0x3ff4000000000000, // 1.25
        0x7ff0000000000000, // +infinity
        0x7ff8000000001234, // quiet NaN with payload
    };
    for (patterns) |bits| {
        const value: f64 = @bitCast(bits);
        const object = api.mkFloat(value);
        try std.testing.expect(api.isFloat(object) != 0);
        try std.testing.expect(api.vtableOf(object) == api.float_vt);
        try std.testing.expectEqual(bits, @as(u64, @bitCast(api.floatValue(object))));
        try std.testing.expect(api.isSmallInt(object) == 0);
        try std.testing.expect(api.isWord(object) == 0);
    }
}

test "Word preserves every opaque machine bit" {
    const api = floor.api();
    const values = [_]u64{ 0, 1, 0x7fffffffffffffff, 0x8000000000000000, 0xfedcba9876543210, std.math.maxInt(u64) };
    for (values) |value| {
        const object = api.mkWord(value);
        try std.testing.expect(api.isWord(object) != 0);
        try std.testing.expect(api.vtableOf(object) == api.word_vt);
        try std.testing.expectEqual(value, api.wordValue(object));
        try std.testing.expect(api.isSmallInt(object) == 0);
        try std.testing.expect(api.isFloat(object) == 0);
    }
}

test "String owns an uninterned byte-for-byte copy" {
    const api = floor.api();
    var source = [_]u8{ 0xff, 0x00, 'b', 'i', 'r', 'n', 'a', 'm' };
    const first = api.mkString(&source, source.len);
    const second = api.mkString(&source, source.len);
    source[0] = 0;

    var first_len: usize = 0;
    const first_bytes = api.stringBytes(first, &first_len);
    try std.testing.expectEqual(@as(usize, 8), first_len);
    try std.testing.expectEqualSlices(u8, &.{ 0xff, 0x00, 'b', 'i', 'r', 'n', 'a', 'm' }, first_bytes[0..first_len]);
    try std.testing.expect(first != second);
    try std.testing.expect(api.vtableOf(first) == api.string_vt);
    try std.testing.expect(api.isSmallInt(first) == 0);

    const empty = api.mkString("", 0);
    var empty_len: usize = 1;
    _ = api.stringBytes(empty, &empty_len);
    try std.testing.expectEqual(@as(usize, 0), empty_len);
}

test "nil and Boolean are distinct canonical heap singletons" {
    const api = floor.api();
    try std.testing.expect(api.nilObj != null);
    try std.testing.expect(api.trueObj != null);
    try std.testing.expect(api.falseObj != null);
    try std.testing.expect(api.nilObj != api.trueObj);
    try std.testing.expect(api.nilObj != api.falseObj);
    try std.testing.expect(api.trueObj != api.falseObj);
    try std.testing.expect(api.vtableOf(api.nilObj) != null);
    try std.testing.expect(api.vtableOf(api.trueObj) == api.vtableOf(api.falseObj));
    try std.testing.expect(api.import("Boolean") == api.trueObj);
}

test "public pointer predicates reject non-object Oop carriers" {
    const api = floor.api();
    const foreign_words: [2]usize align(16) = .{ 0xfeed, 0xbeef };
    const candidates = [_]Oop{
        null,
        @ptrCast(@constCast(&answer42)),
        @ptrCast(@constCast(&foreign_words)),
        @ptrFromInt(0x1000),
        @ptrFromInt(0x1001),
    };
    for (candidates) |candidate| {
        try std.testing.expect(api.vtableOf(candidate) == null);
        try std.testing.expect(api.isSmallInt(candidate) == 0);
        try std.testing.expect(api.isFloat(candidate) == 0);
        try std.testing.expect(api.isWord(candidate) == 0);
        try std.testing.expect(api.vtableOwner(candidate) == null);
    }
}

test "canonical prototypes own their families and instances split privately" {
    const api = floor.api();
    callback_api = api;

    const object_proto = api.import("Object");
    const integer_proto = api.import("Integer");
    const string_proto = api.import("String");
    try std.testing.expect(object_proto != null);
    try std.testing.expect(api.vtableOf(object_proto) == api.object_vt);
    try std.testing.expect(api.vtableOwner(api.object_vt) == object_proto);
    try std.testing.expect(api.vtableOf(integer_proto) == api.smallint_vt);
    try std.testing.expect(api.vtableOwner(api.smallint_vt) == integer_proto);
    try std.testing.expect(api.vtableOf(string_proto) == api.string_vt);
    try std.testing.expect(api.vtableOwner(api.string_vt) == string_proto);

    // A family prototype is already its family's owner, so it does not split.
    try std.testing.expect(api.ensurePrivateVtable(integer_proto) == api.smallint_vt);
    const family_selector = api.intern("conformanceIntegerFamilyAnswer");
    api.method(api.smallint_vt, family_selector, &answer42);
    try std.testing.expectEqual(@as(i64, 42), api.smallIntValue(api.send(api.mkSmallInt(9), family_selector, &.{}, 0)));

    // An ordinary instance gets one stable private vtable; its sibling stays on
    // the shared family and the split delegates to that family.
    const first = api.alloc(api.object_vt, 0);
    const sibling = api.alloc(api.object_vt, 0);
    const private_vt = api.ensurePrivateVtable(first);
    try std.testing.expect(private_vt != api.object_vt);
    try std.testing.expect(api.ensurePrivateVtable(first) == private_vt);
    try std.testing.expect(api.vtableOf(first) == private_vt);
    try std.testing.expect(api.vtableOf(sibling) == api.object_vt);
    try std.testing.expect(api.vtableParent(private_vt) == api.object_vt);
    try std.testing.expect(api.vtableOwner(private_vt) == first);
}

test "clone and new preserve prototype-family semantics and String ownership" {
    const api = floor.api();
    const clone = api.intern("clone");
    const new = api.intern("new");

    const object_proto = api.import("Object");
    const object_clone = api.send(object_proto, clone, &.{}, 0);
    const object_new = api.send(object_proto, new, &.{}, 0);
    try std.testing.expect(object_clone != object_proto);
    try std.testing.expect(object_new != object_proto);
    try std.testing.expect(api.vtableOf(object_clone) == api.object_vt);
    try std.testing.expect(api.vtableOf(object_new) == api.object_vt);

    const original = api.mkString("owned bytes", 11);
    const copied = api.send(original, clone, &.{}, 0);
    var original_len: usize = 0;
    var copied_len: usize = 0;
    const original_bytes = api.stringBytes(original, &original_len);
    const copied_bytes = api.stringBytes(copied, &copied_len);
    try std.testing.expect(copied != original);
    try std.testing.expect(original_bytes != copied_bytes);
    try std.testing.expectEqualStrings(original_bytes[0..original_len], copied_bytes[0..copied_len]);
}

test "vtable registry, parent mutation, and direct selectors are ordered" {
    const api = floor.api();
    const before = api.vtableCount();
    const parent_a = api.proto(api.object_vt);
    const parent_b = api.proto(api.object_vt);
    const child = api.proto(parent_a);
    try std.testing.expectEqual(before + 3, api.vtableCount());
    try std.testing.expect(api.vtableAt(before) == parent_a);
    try std.testing.expect(api.vtableAt(before + 1) == parent_b);
    try std.testing.expect(api.vtableAt(before + 2) == child);
    try std.testing.expect(api.vtableAt(api.vtableCount()) == null);
    try std.testing.expect(api.vtableParent(child) == parent_a);

    const first = api.intern("conformanceDirectFirst");
    const second = api.intern("conformanceDirectSecond");
    api.method(child, first, &answer42);
    api.method(child, second, &answer7);
    try std.testing.expect(api.vtableSelectorAt(child, 0) == first);
    try std.testing.expect(api.vtableSelectorAt(child, 1) == second);
    try std.testing.expect(api.vtableSelectorAt(child, 2) == null);

    // Replacement preserves dictionary order; parent mutation changes only the
    // delegation edge, not the direct protocol.
    api.method(child, first, &answer7);
    api.setParent(child, parent_b);
    try std.testing.expect(api.vtableParent(child) == parent_b);
    try std.testing.expect(api.vtableSelectorAt(child, 0) == first);
    try std.testing.expect(api.vtableSelectorAt(child, 1) == second);
}

test "exports preserve deterministic alias and rebinding behavior" {
    const api = floor.api();
    const first = api.alloc(api.object_vt, 0);
    const second = api.alloc(api.object_vt, 0);
    const primary = "ConformanceExportPrimary";
    const alias = "ConformanceExportAlias";

    try std.testing.expect(api.isExported(primary) == 0);
    try std.testing.expect(api.exportObj(primary, first) == first);
    try std.testing.expect(api.exportObj(alias, first) == first);
    try std.testing.expect(api.isExported(primary) != 0);
    try std.testing.expect(api.import(primary) == first);
    try std.testing.expectEqualStrings(primary, std.mem.span(api.exportedNameOf(first).?));

    // Same-name rebinding moves the primary name to the new value. The old value
    // remains reachable by its later alias, which becomes its deterministic name.
    try std.testing.expect(api.exportObj(primary, second) == second);
    try std.testing.expect(api.import(primary) == second);
    try std.testing.expectEqualStrings(primary, std.mem.span(api.exportedNameOf(second).?));
    try std.testing.expectEqualStrings(alias, std.mem.span(api.exportedNameOf(first).?));
}

test "payload sizes and exact slot layouts are externally observable" {
    const api = floor.api();
    const unconfigured = api.proto(api.object_vt);
    try std.testing.expectEqual(idapi.slot_unknown, api.slotKind(unconfigured, 0));

    const laid_out = api.proto(api.object_vt);
    api.setLayout(laid_out, &exact_layout, exact_layout.len, idapi.slot_raw);
    try std.testing.expectEqual(idapi.slot_oop, api.slotKind(laid_out, 0));
    try std.testing.expectEqual(idapi.slot_raw, api.slotKind(laid_out, 1));
    try std.testing.expectEqual(idapi.slot_code, api.slotKind(laid_out, 2));
    try std.testing.expectEqual(idapi.slot_raw, api.slotKind(laid_out, 3));
    try std.testing.expectEqual(idapi.slot_raw, api.slotKind(laid_out, 100));

    const empty = api.alloc(laid_out, 0);
    const rounded = api.alloc(laid_out, 1);
    const exact = api.alloc(laid_out, 3 * @sizeOf(usize));
    try std.testing.expectEqual(@as(usize, 0), api.objectPayloadSize(empty));
    try std.testing.expectEqual(@sizeOf(usize), api.objectPayloadSize(rounded));
    try std.testing.expectEqual(3 * @sizeOf(usize), api.objectPayloadSize(exact));
    try std.testing.expectEqual(@as(usize, 0), api.objectPayloadSize(@ptrFromInt(0x1000)));
}

test "manual GC refuses to sweep without a registered stack bottom" {
    const api = floor.api();
    api.gcSetAuto(0);
    api.gcSetStackBottom(0);
    makeGarbage(api, 128, 0);
    const live = api.gcLiveCount();
    try std.testing.expectEqual(@as(usize, 0), api.gcCollect());
    try std.testing.expectEqual(live, api.gcLiveCount());
}

test "root providers, precise layouts, and trace hooks retain exactly their edges" {
    const api = floor.api();
    var stack_bottom: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&stack_bottom));
    api.gcSetAuto(0);
    ensureRootProvider(api);
    gc_roots = .{ null, null, null };
    _ = api.gcCollect();

    const container_vt = api.proto(api.object_vt);
    api.setLayout(container_vt, &precise_layout, precise_layout.len, idapi.slot_raw);
    makePreciseGraph(api, container_vt);
    gc_provider_calls = 0;
    api.gcSetPrecise(1);
    for (0..4) |_| {
        scrubConservativeStack();
        _ = api.gcCollect();
    }
    try std.testing.expect(gc_provider_calls > 0);
    try std.testing.expect(api.vtableOf(@ptrFromInt(precise_followed_address)) == api.object_vt);
    try std.testing.expect(api.vtableOf(@ptrFromInt(precise_raw_address)) == null);

    const trace_vt = api.proto(api.object_vt);
    api.setLayout(trace_vt, null, 0, idapi.slot_raw);
    api.setTraceHook(trace_vt, &conformanceTraceHook);
    const traced = api.alloc(trace_vt, 0);
    trace_target = api.alloc(api.object_vt, 0);
    const target_address = @intFromPtr(trace_target.?);
    gc_roots = .{ traced, null, null };
    trace_calls = 0;
    _ = api.gcCollect();
    try std.testing.expect(trace_calls > 0);
    try std.testing.expect(api.vtableOf(@ptrFromInt(target_address)) == api.object_vt);

    gc_roots = .{ null, null, null };
    trace_target = null;
    api.gcSetPrecise(0);
    std.mem.doNotOptimizeAway(&stack_bottom);
}

test "manual collection finalizes once and never sweeps permanents" {
    const api = floor.api();
    var stack_bottom: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&stack_bottom));
    api.gcSetAuto(0);
    gc_roots = .{ null, null, null };
    _ = api.gcCollect();

    const permanent_index = api.vtableCount();
    const permanent_vtable = api.proto(api.object_vt);
    const permanent_symbol = api.intern("conformancePermanentSymbol");
    finalizer_calls = 0;
    makeFinalizableGarbage(api, 1000);
    for (0..4) |_| {
        scrubConservativeStack();
        _ = api.gcCollect();
    }
    try std.testing.expectEqual(@as(usize, 1000), finalizer_calls);
    const finalized = finalizer_calls;
    scrubConservativeStack();
    _ = api.gcCollect();
    try std.testing.expectEqual(finalized, finalizer_calls);

    try std.testing.expect(api.vtableAt(permanent_index) == permanent_vtable);
    try std.testing.expect(api.vtableOf(permanent_vtable) == api.vtable_vt);
    try std.testing.expect(api.intern("conformancePermanentSymbol") == permanent_symbol);
    try std.testing.expect(api.vtableOf(api.nilObj) != null);
    std.mem.doNotOptimizeAway(&stack_bottom);
}

test "automatic GC bounds the live set during allocation churn" {
    const api = floor.api();
    var stack_bottom: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&stack_bottom));
    api.gcSetAuto(0);
    gc_roots = .{ null, null, null };
    _ = api.gcCollect();
    const before = api.gcLiveCount();

    api.gcSetAuto(1);
    defer api.gcSetAuto(0);
    makeGarbage(api, 6000, 4096);
    try std.testing.expect(api.gcLiveCount() < before + 5000);
    std.mem.doNotOptimizeAway(&stack_bottom);
}
