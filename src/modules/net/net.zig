//! Net — TCP networking over zig's `std.net`/`std.posix`.
//!
//! Loads as `libbirnam_net.dylib`. Installs a `Net` prototype + socket/server
//! vtables. Blocking I/O (first cut). Sockets/servers are entities holding the
//! OS fd; they are runtime-only (never persisted), so eids are minted at init.
//!
//!   let s := [Net connect: "example.com" port: 80]
//!   [s send: "GET / HTTP/1.0\r\n\r\n"]
//!   [s recv: 4096]            " → string (up to N bytes; "" at EOF) "
//!   [s close]
//!
//!   let srv := [Net listen: 8080]
//!   let c := [srv accept]      " blocks; → a socket "

const std = @import("std");
const abi = @import("module_abi");
const Ref = abi.Ref;
const IdApi = abi.IdApi;
const ShellContext = abi.ShellContext;
const mkEid = abi.mkEid;
const NIL = abi.NIL;

const Send = extern struct {
    selector: u64,
    receiver: Ref,
    n_args: u8,
    state: Ref,
    closure: ?*anyopaque,
};

var g_api: *const IdApi = undefined;
var g_module_eid: u64 = 0;
var g_socket_vt: u64 = 0;
var g_server_vt: u64 = 0;
var g_attr_fd: u64 = 0;

const alloc = std.heap.page_allocator;

// -- tag helpers (ABI-stable subset) ----------------------------------------
fn mkSmallInt(n: i64) Ref {
    return @as(u64, @bitCast(n << 3)) | 0b001;
}
fn untagSmallInt(r: Ref) i64 {
    return @as(i64, @bitCast(r)) >> 3;
}
fn untagEid(r: Ref) u64 {
    return r >> 3;
}
fn isString(r: Ref) bool {
    return (r & 0b111) == 0b100;
}
fn isSmallInt(r: Ref) bool {
    return (r & 0b111) == 0b001;
}
const StringHeader = extern struct { len: u64 };
fn strBytes(r: Ref) []const u8 {
    const addr = r & ~@as(u64, 0b111);
    const h: *const StringHeader = @ptrFromInt(addr);
    const payload: [*]const u8 = @as([*]const u8, @ptrCast(h)) + @sizeOf(StringHeader);
    return payload[0..h.len];
}

fn fdOf(recv: Ref) ?std.posix.fd_t {
    const v = g_api.query_eavt(untagEid(recv), g_attr_fd);
    if (!isSmallInt(v)) return null;
    return @intCast(untagSmallInt(v));
}
fn makeSocket(fd: std.posix.fd_t, vt: u64) Ref {
    const e = g_api.alloc_eid();
    _ = g_api.assert_fact(e, g_api.attr_vtable, mkEid(vt), g_module_eid);
    _ = g_api.assert_fact(e, g_attr_fd, mkSmallInt(@intCast(fd)), g_module_eid);
    return mkEid(e);
}

fn errStr(comptime msg: []const u8) Ref {
    return g_api.make_string(msg.ptr, msg.len);
}

// -- Net (class-side) -------------------------------------------------------

/// `[Net connect: host port: n]` → a socket (resolves DNS, connects).
fn connectImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, n };
    if (!isString(args[0]) or !isSmallInt(args[1])) return errStr("Net connect:port: — host string + int port");
    const host = strBytes(args[0]);
    const port: u16 = @intCast(untagSmallInt(args[1]));
    const stream = std.net.tcpConnectToHost(alloc, host, port) catch return errStr("Net connect: failed");
    return makeSocket(stream.handle, g_socket_vt);
}

/// `[Net listen: port]` → a server bound to 127.0.0.1:port (backlog 16).
fn listenImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, n };
    if (!isSmallInt(args[0])) return errStr("Net listen: — int port");
    const port: u16 = @intCast(untagSmallInt(args[0]));
    const addr = std.net.Address.initIp4(.{ 127, 0, 0, 1 }, port);
    const server = addr.listen(.{ .reuse_address = true }) catch return errStr("Net listen: failed");
    // Keep the listener fd; std.net.Server is just a wrapper over it.
    return makeSocket(server.stream.handle, g_server_vt);
}

// -- Socket -----------------------------------------------------------------

/// `[sock send: aString]` → bytes written (or an error string).
fn sendImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    const fd = fdOf(s.receiver) orelse return errStr("send: not a socket");
    if (!isString(args[0])) return errStr("send: expects a string");
    const bytes = strBytes(args[0]);
    const w = std.posix.write(fd, bytes) catch return errStr("send: failed");
    return mkSmallInt(@intCast(w));
}

/// `[sock recv: maxBytes]` → string of up to maxBytes (empty string at EOF).
fn recvImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    const fd = fdOf(s.receiver) orelse return errStr("recv: not a socket");
    if (!isSmallInt(args[0])) return errStr("recv: expects an int");
    var max: usize = @intCast(untagSmallInt(args[0]));
    if (max == 0) max = 1;
    if (max > 1 << 20) max = 1 << 20;
    const buf = alloc.alloc(u8, max) catch return errStr("recv: out of memory");
    defer alloc.free(buf);
    const r = std.posix.read(fd, buf) catch return errStr("recv: failed");
    return g_api.make_string(buf.ptr, r);
}

/// `[sock close]` → nil.
fn closeImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ args, n };
    if (fdOf(s.receiver)) |fd| std.posix.close(fd);
    return NIL;
}

// -- Server -----------------------------------------------------------------

/// `[srv accept]` → a socket for the next incoming connection (blocks).
fn acceptImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ args, n };
    const fd = fdOf(s.receiver) orelse return errStr("accept: not a server");
    const conn = std.posix.accept(fd, null, null, 0) catch return errStr("accept: failed");
    return makeSocket(conn, g_socket_vt);
}

// -- init -------------------------------------------------------------------

export fn birnam_module_init(ctx: *ShellContext, api: *const IdApi, module_eid: u64) void {
    g_api = api;
    g_module_eid = module_eid;
    ctx.log("[net] initializing");
    g_attr_fd = api.intern("net-fd", 6);

    // Socket vtable: send:/recv:/close (+ accept lives on the server vt).
    g_socket_vt = api.alloc_eid();
    _ = api.assert_fact(g_socket_vt, api.attr_vtable, mkEid(api.vtable_vt_eid), module_eid);
    _ = api.assert_fact(g_socket_vt, api.attr_delegate, mkEid(api.object_vt_eid), module_eid);
    _ = api.assert_fact(g_socket_vt, api.intern("send:", 5), mkEid(api.make_closure(@ptrCast(&sendImpl), NIL)), module_eid);
    _ = api.assert_fact(g_socket_vt, api.intern("recv:", 5), mkEid(api.make_closure(@ptrCast(&recvImpl), NIL)), module_eid);
    _ = api.assert_fact(g_socket_vt, api.intern("close", 5), mkEid(api.make_closure(@ptrCast(&closeImpl), NIL)), module_eid);

    // Server vtable: accept + close.
    g_server_vt = api.alloc_eid();
    _ = api.assert_fact(g_server_vt, api.attr_vtable, mkEid(api.vtable_vt_eid), module_eid);
    _ = api.assert_fact(g_server_vt, api.attr_delegate, mkEid(api.object_vt_eid), module_eid);
    _ = api.assert_fact(g_server_vt, api.intern("accept", 6), mkEid(api.make_closure(@ptrCast(&acceptImpl), NIL)), module_eid);
    _ = api.assert_fact(g_server_vt, api.intern("close", 5), mkEid(api.make_closure(@ptrCast(&closeImpl), NIL)), module_eid);

    // Net prototype (class-side connect:/listen:).
    const net_vt = api.alloc_eid();
    _ = api.assert_fact(net_vt, api.attr_vtable, mkEid(api.vtable_vt_eid), module_eid);
    _ = api.assert_fact(net_vt, api.attr_delegate, mkEid(api.object_vt_eid), module_eid);
    _ = api.assert_fact(net_vt, api.intern("connect:port:", 13), mkEid(api.make_closure(@ptrCast(&connectImpl), NIL)), module_eid);
    _ = api.assert_fact(net_vt, api.intern("listen:", 7), mkEid(api.make_closure(@ptrCast(&listenImpl), NIL)), module_eid);
    const net_proto = api.alloc_eid();
    _ = api.assert_fact(net_proto, api.attr_vtable, mkEid(net_vt), module_eid);

    // Bind `Net` as a global. The lang frontend binds globals as facts on the
    // GLOBALS scope; expose it on the module entity too for discovery.
    _ = api.assert_fact(module_eid, api.intern("Net", 3), mkEid(net_proto), module_eid);
    api.bind_global(api.intern("Net", 3), mkEid(net_proto));
    ctx.log("[net] ready");
}
