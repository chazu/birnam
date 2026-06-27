const std = @import("std");
const testing = std.testing;

const ref_mod = @import("ref.zig");
const fact_store = @import("fact_store.zig");
const object_table = @import("object_table.zig");
const interner_mod = @import("interner.zig");
const id_api_mod = @import("id_api.zig");
const module = @import("module.zig");
const kernel_eids = @import("kernel_eids.zig");
const siphon = @import("siphon.zig");
const send_mod = @import("send.zig");
const strings = @import("strings.zig");

const Ref = ref_mod.Ref;

// -- Output capture ---------------------------------------------------------

var captured: std.ArrayList(u8) = .{};
var captured_alloc: ?std.mem.Allocator = null;

fn capturingLog(msg: [*:0]const u8) callconv(.c) void {
    const slice = std.mem.span(msg);
    if (captured_alloc) |a| {
        captured.appendSlice(a, slice) catch {};
        captured.append(a, '\n') catch {};
    }
}

fn setReplHandlerStub(handler: ?*const fn ([*:0]const u8) callconv(.c) void) callconv(.c) void {
    g_handler = handler;
}

var g_handler: ?*const fn ([*:0]const u8) callconv(.c) void = null;
var shell_ctx = id_api_mod.ShellContext{
    .log = &capturingLog,
    .set_repl_handler = &setReplHandlerStub,
};

fn fullSetup(arena: *std.heap.ArenaAllocator) !struct {
    store: *fact_store.FactStore,
    table: *object_table.ObjectTable,
    interner: *interner_mod.Interner,
    manager: *module.Manager,
} {
    const allocator = arena.allocator();
    captured = .{};
    captured_alloc = allocator;
    g_handler = null;

    const store = try allocator.create(fact_store.FactStore);
    store.* = fact_store.FactStore.init(allocator);
    const table = try allocator.create(object_table.ObjectTable);
    table.* = object_table.ObjectTable.init(allocator);
    const interner = try allocator.create(interner_mod.Interner);
    interner.* = interner_mod.Interner.init(allocator);
    _ = try siphon.boot(allocator, store, table, interner);
    id_api_mod.setInterner(interner);

    const api = id_api_mod.populate();
    const manager = try allocator.create(module.Manager);
    manager.* = module.Manager.init(allocator, &shell_ctx, api);
    return .{ .store = store, .table = table, .interner = interner, .manager = manager };
}

fn dispatchLine(line: [:0]const u8) void {
    const h = g_handler orelse return;
    h(@ptrCast(line.ptr));
}

fn loadHelloAndRepl(arena: *std.heap.ArenaAllocator, s: anytype) !struct { hello: u64, repl: u64 } {
    _ = arena;
    const hello_eid = (try s.manager.load("./zig-out/lib/libbirnam_hello.dylib")) orelse return error.LoadFailed;
    const repl_eid = (try s.manager.load("./zig-out/lib/libbirnam_repl.dylib")) orelse return error.LoadFailed;
    return .{ .hello = hello_eid, .repl = repl_eid };
}

fn lastLines(text: []const u8, n: usize) []const u8 {
    // Strip a single trailing newline so the last line is real content.
    var end = text.len;
    if (end > 0 and text[end - 1] == '\n') end -= 1;
    var count: usize = 0;
    var i: usize = end;
    while (i > 0 and count < n) {
        i -= 1;
        if (text[i] == '\n') {
            count += 1;
            if (count == n) {
                i += 1; // start AFTER the newline
                break;
            }
        }
    }
    return text[i..end];
}

// -- Tests ------------------------------------------------------------------

test "repl: registering handler via set_repl_handler" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const s = try fullSetup(&arena);

    const _ids = try loadHelloAndRepl(&arena, s);
    _ = _ids;

    try testing.expect(g_handler != null);
}

test "repl: unary `count` after `target: <counter_eid>` returns 0" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const s = try fullSetup(&arena);
    const ids = try loadHelloAndRepl(&arena, s);

    // Discover counter eid from the hello module's exports.
    const sel_counter = try s.interner.intern(s.store, s.table, "counter");
    const counter_ref = s.store.currentValue(ids.hello, sel_counter).?;
    const counter_eid = ref_mod.untagEid(counter_ref);

    // Build a `target: N` line then `count`.
    var buf: [64]u8 = undefined;
    const t_line = try std.fmt.bufPrintZ(&buf, "target: {d}", .{counter_eid});
    dispatchLine(t_line);
    dispatchLine("count");

    // Final printed line should be "0".
    const tail = lastLines(captured.items, 1);
    try testing.expect(std.mem.indexOf(u8, tail, "0") != null);
}

test "repl: `inc` mutates counter; subsequent `count` reflects it" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const s = try fullSetup(&arena);
    const ids = try loadHelloAndRepl(&arena, s);

    const sel_counter = try s.interner.intern(s.store, s.table, "counter");
    const counter_ref = s.store.currentValue(ids.hello, sel_counter).?;
    const counter_eid = ref_mod.untagEid(counter_ref);

    var buf: [64]u8 = undefined;
    const t_line = try std.fmt.bufPrintZ(&buf, "target: {d}", .{counter_eid});
    dispatchLine(t_line);
    dispatchLine("inc");
    dispatchLine("inc");
    dispatchLine("inc");
    dispatchLine("count");

    const tail = lastLines(captured.items, 1);
    try testing.expect(std.mem.indexOf(u8, tail, "3") != null);
}

test "repl: `greet` on hello vt instance returns the greeting string" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const s = try fullSetup(&arena);
    const ids = try loadHelloAndRepl(&arena, s);

    // We need a hello-vt-typed entity. Allocate one ourselves: mint EID,
    // assert :vtable = Hello_vt.
    const sel_hello_vt = try s.interner.intern(s.store, s.table, "hello_vt");
    const hello_vt_ref = s.store.currentValue(ids.hello, sel_hello_vt).?;
    const inst_eid = s.table.alloc();
    _ = try s.store.assert(inst_eid, kernel_eids.ATTR_VTABLE, hello_vt_ref, ids.hello);

    var buf: [64]u8 = undefined;
    const t_line = try std.fmt.bufPrintZ(&buf, "target: {d}", .{inst_eid});
    dispatchLine(t_line);
    dispatchLine("greet");

    try testing.expect(std.mem.indexOf(u8, captured.items, "hello, birnam!") != null);
}

test "repl: unknown selector prints error, loop continues" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const s = try fullSetup(&arena);
    const ids = try loadHelloAndRepl(&arena, s);

    const sel_counter = try s.interner.intern(s.store, s.table, "counter");
    const counter_ref = s.store.currentValue(ids.hello, sel_counter).?;
    const counter_eid = ref_mod.untagEid(counter_ref);

    var buf: [64]u8 = undefined;
    const t_line = try std.fmt.bufPrintZ(&buf, "target: {d}", .{counter_eid});
    dispatchLine(t_line);

    captured = .{};
    captured_alloc = arena.allocator();

    dispatchLine("notARealSelector");
    // Send returned NIL (cold path failed to find method); printRef prints "nil".
    try testing.expect(std.mem.indexOf(u8, captured.items, "nil") != null);

    // Loop continues: `count` still works after.
    dispatchLine("count");
    try testing.expect(std.mem.indexOf(u8, captured.items, "0") != null);
}

test "repl: `quit` prints farewell, no crash" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const s = try fullSetup(&arena);
    _ = try loadHelloAndRepl(&arena, s);

    dispatchLine("quit");
    try testing.expect(std.mem.indexOf(u8, captured.items, "bye") != null);
}
