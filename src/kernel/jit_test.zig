//! libtcc JIT wrapper tests (W1 → libtcc, docs/10a). Proves the mode-1 backend's
//! core round-trip: a C string → in-memory native code → callable fn-ptr.

const std = @import("std");
const testing = std.testing;
const jit = @import("jit.zig");

// The dev install keeps libtcc's runtime archive here. A shipped birnam bundles
// it; tests run against the system install.
const RUNTIME_LIB_DIR: [*:0]const u8 = "/usr/local/lib/tcc";

test "jit: compile + call a simple C function" {
    var j = try jit.Jit.init(RUNTIME_LIB_DIR);
    defer j.deinit();
    try j.compile("int add(int a, int b) { return a + b; }");
    try j.relocate();
    const add = try j.getFn(*const fn (c_int, c_int) callconv(.c) c_int, "add");
    try testing.expectEqual(@as(c_int, 5), add(2, 3));
    try testing.expectEqual(@as(c_int, -1), add(9, -10));
}

test "jit: recursive function JITs and runs" {
    var j = try jit.Jit.init(RUNTIME_LIB_DIR);
    defer j.deinit();
    try j.compile("long fib(long n) { return n < 2 ? n : fib(n-1) + fib(n-2); }");
    try j.relocate();
    const fib = try j.getFn(*const fn (c_long) callconv(.c) c_long, "fib");
    try testing.expectEqual(@as(c_long, 55), fib(10));
    try testing.expectEqual(@as(c_long, 6765), fib(20));
}

test "jit: multiple compiles accumulate into one unit; cross-call works" {
    var j = try jit.Jit.init(RUNTIME_LIB_DIR);
    defer j.deinit();
    try j.compile("int dbl(int x) { return x + x; }");
    try j.compile("extern int dbl(int); int quad(int x) { return dbl(dbl(x)); }");
    try j.relocate();
    const quad = try j.getFn(*const fn (c_int) callconv(.c) c_int, "quad");
    try testing.expectEqual(@as(c_int, 20), quad(5));
}

test "jit: missing symbol is an error, not a crash" {
    var j = try jit.Jit.init(RUNTIME_LIB_DIR);
    defer j.deinit();
    try j.compile("int present(void) { return 1; }");
    try j.relocate();
    try testing.expectError(jit.Error.SymbolNotFound, j.getSymbol("absent"));
}

test "jit: a syntax error surfaces as Error.Compile" {
    var j = try jit.Jit.init(RUNTIME_LIB_DIR);
    defer j.deinit();
    try testing.expectError(jit.Error.Compile, j.compile("int broken(void) { return }"));
}

// Stand-in for a runtime entry point a generated method body would call —
// e.g. `birnam_send2(recv, sel, arg)`. Here a u64-in/u64-out (the `Ref` ABI).
fn hostRuntimeOp(x: u64) callconv(.c) u64 {
    return x *% 10 +% 7;
}

// The codegen foundation: generated C declares the runtime entry points `extern`
// and calls them; `addSymbol` binds those names to the host's fn-ptrs, so the
// JIT'd method body orchestrates the birnam runtime "as a library". Proves the
// C → runtime callback path the birnam→C pass depends on.
test "jit: generated C calls back into a host runtime function (add_symbol)" {
    var j = try jit.Jit.init(RUNTIME_LIB_DIR);
    defer j.deinit();
    try j.addSymbol("birnam_runtime_op", @ptrCast(&hostRuntimeOp));
    try j.compile(
        \\extern unsigned long birnam_runtime_op(unsigned long);
        \\unsigned long method_body(unsigned long recv) {
        \\    return birnam_runtime_op(recv) + birnam_runtime_op(recv);
        \\}
    );
    try j.relocate();
    const body = try j.getFn(*const fn (u64) callconv(.c) u64, "method_body");
    // hostRuntimeOp(4) = 47; called twice → 94.
    try testing.expectEqual(@as(u64, 94), body(4));
}
