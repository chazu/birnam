//! `Module` — the module manager (docs/42 direction; stage 3 of the module-system
//! arc). A dependency graph IS relational data, so this module is store-native: it
//! records `requires` edges as facts on the well-known `db`, and answers a
//! topological load order by walking those facts. No new store core — `Module` is
//! a libid object that forwards to the store, exactly like `Globals` (store.zig).
//!
//! Two tiers of module (docs, the module-system memory):
//!   * KERNEL — the Zig primitive dylibs (store, peg, …). REPRESENTED here (so the
//!     graph is complete + queryable) via `[Module kernel: #name]`, but NOT loaded
//!     by this manager; they are already up by the time it runs.
//!   * SELF-HOSTED — birnam-source modules (stdlib, toml, …). Each declares its own
//!     dependencies in its source via `[Module named: #x requires: #[ … ]]`, which
//!     records the edges as it loads.
//!
//! `[Module loadOrder: #[ … ]]` topologically sorts a set of names (DFS post-order),
//! deps first; a dependency cycle answers a `#circularDependency` kinded error
//! (mirroring the G4 left-recursion catch) rather than looping. The graph
//! serializes into the image for free (it is ordinary store facts), like rules.

const std = @import("std");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;
const ca = std.heap.c_allocator;

var g_api: *idapi.IdApi = undefined;

pub var module_vt: Oop = null; // the Module vtable (a proto delegating to Object)
pub var module_obj: Oop = null; // exported as "Module"

var store: Oop = null; // the well-known store (`db`), imported at bringUp
var vector_factory: Oop = null; // `Vector`, imported lazily

// The entity of a module `m` is the interned symbol `#m` itself; its attributes:
var s_requires: Oop = null; // `#requires` — a Vector of dependency-name symbols
var s_kind: Oop = null; //     `#kind` — `#kernel` or `#selfHosted`
var s_registered: Oop = null; // `#registered` — the boolean true once declared
var s_kernel: Oop = null; //    the `#kernel` kind value
var s_selfHosted: Oop = null; // the `#selfHosted` kind value

// Store + collection selectors we drive.
var s_assert: Oop = null; //        `assert:attr:value:`
var s_currentValue: Oop = null; //  `currentValue:attr:`
var s_new: Oop = null; //           `new`
var s_add: Oop = null; //           `add:`
var s_size: Oop = null; //          `size`
var s_at: Oop = null; //            `at:`

fn send0(rcv: Oop, sel: Oop) Oop {
    return g_api.send(rcv, sel, &[_]Oop{}, 0);
}
fn send1(rcv: Oop, sel: Oop, a: Oop) Oop {
    return g_api.send(rcv, sel, &[_]Oop{a}, 1);
}
fn assert3(e: Oop, a: Oop, v: Oop) void {
    _ = g_api.send(store, s_assert, &[_]Oop{ e, a, v }, 3);
}
fn currentValue(e: Oop, a: Oop) Oop {
    return g_api.send(store, s_currentValue, &[_]Oop{ e, a }, 2);
}
fn vectorFactory() Oop {
    if (vector_factory == null) vector_factory = g_api.import("Vector");
    return vector_factory;
}

/// A `#kind:message:` kinded error (F7 error model), or nil if the error module
/// isn't loaded — the honest signal, never a silent wrong answer.
fn modErr(comptime kind: [:0]const u8, message: []const u8) Oop {
    const ef = g_api.import("Error") orelse return g_api.nilObj;
    return g_api.send(ef, g_api.intern("kind:message:"), &[_]Oop{
        g_api.intern(kind), g_api.mkString(message.ptr, message.len),
    }, 2);
}

// -- declaration + query -----------------------------------------------------

/// `[Module named: #x requires: #[ #a #b ]]` — a self-hosted module declaring its
/// own dependencies (its source's first statement). Records the edges + marks it
/// registered as a self-hosted module. Answers self.
fn mNamedRequires(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const name = args[0];
    assert3(name, s_registered, g_api.trueObj);
    assert3(name, s_kind, s_selfHosted);
    assert3(name, s_requires, args[1]);
    return self;
}

/// `[Module kernel: #store]` — represent a kernel (Zig primitive) module in the
/// graph: registered, kind `#kernel`, no dependencies. It is NOT loaded here (it
/// is already up); this just completes the graph. Answers self.
fn mKernel(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const name = args[0];
    assert3(name, s_registered, g_api.trueObj);
    assert3(name, s_kind, s_kernel);
    return self;
}

/// `[Module requiresOf: #x]` — the declared dependency Vector, or an empty Vector
/// when `x` has none (or isn't registered — treated as a leaf).
fn mRequiresOf(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    // The store answers Zig `null` (not the nil singleton) for an absent fact —
    // the G6 distinction — so guard against BOTH: an unregistered or kernel module
    // has no `#requires` fact and is a leaf (empty Vector).
    const r = currentValue(args[0], s_requires);
    return if (r == null or r == g_api.nilObj) send0(vectorFactory(), s_new) else r;
}

/// `[Module kindOf: #x]` — `#kernel` / `#selfHosted`, or nil if unregistered.
fn mKindOf(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    const r = currentValue(args[0], s_kind); // normalise the store's absence (null) to nil
    return if (r == null) g_api.nilObj else r;
}

/// `[Module isRegistered: #x]` — true iff `x` has been declared (kernel or
/// self-hosted).
fn mIsRegistered(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    return if (currentValue(args[0], s_registered) == g_api.trueObj) g_api.trueObj else g_api.falseObj;
}

// -- topological load order --------------------------------------------------

// DFS post-order scratch (single-transactor, so process-global is safe; reset at
// the top of every loadOrder). `g_cycle` holds the offending name on a cycle.
var g_order: std.ArrayList(Oop) = .{};
var g_done: std.ArrayList(Oop) = .{};
var g_inprog: std.ArrayList(Oop) = .{};
var g_cycle: Oop = null;

fn contains(list: *const std.ArrayList(Oop), x: Oop) bool {
    for (list.items) |it| if (it == x) return true; // interned symbols: identity
    return false;
}

fn visit(name: Oop) void {
    if (g_cycle != null) return; // already failed — unwind
    if (contains(&g_done, name)) return; // finished earlier
    if (contains(&g_inprog, name)) { // re-entered on the stack → a cycle
        g_cycle = name;
        return;
    }
    g_inprog.append(ca, name) catch @panic("modules: oom");
    const reqs = mRequiresOf(module_obj, &[_]Oop{name}, 1);
    const nreq: usize = @intCast(g_api.smallIntValue(send0(reqs, s_size)));
    var i: usize = 0;
    while (i < nreq) : (i += 1) {
        visit(send1(reqs, s_at, g_api.mkSmallInt(@intCast(i))));
        if (g_cycle != null) return;
    }
    _ = g_inprog.pop(); // name is the top of the in-progress stack
    g_done.append(ca, name) catch @panic("modules: oom");
    g_order.append(ca, name) catch @panic("modules: oom"); // post-order: deps first
}

/// `[Module loadOrder: #[ #toml #stdlib ]]` — the given names plus their transitive
/// dependencies, topologically sorted so every module's deps precede it. A
/// dependency cycle answers a `#circularDependency` error (naming the offending
/// module) rather than looping. The store graph is the single source of truth.
fn mLoadOrder(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    g_order.clearRetainingCapacity();
    g_done.clearRetainingCapacity();
    g_inprog.clearRetainingCapacity();
    g_cycle = null;

    const names = args[0];
    const count: usize = @intCast(g_api.smallIntValue(send0(names, s_size)));
    var i: usize = 0;
    while (i < count) : (i += 1) {
        visit(send1(names, s_at, g_api.mkSmallInt(@intCast(i))));
        if (g_cycle != null) break;
    }
    if (g_cycle != null) {
        const nm = g_api.symbolName(g_cycle);
        var mbuf: [128]u8 = undefined;
        const msg = std.fmt.bufPrint(&mbuf, "circular dependency at module: {s}", .{std.mem.span(nm)}) catch "circular dependency";
        return modErr("circularDependency", msg);
    }
    var vec = send0(vectorFactory(), s_new);
    for (g_order.items) |nm| vec = send1(vec, s_add, nm);
    return vec;
}

// -- bringup -----------------------------------------------------------------

/// Wire `Module` onto libid: intern the vocabulary, (re)build the vtable, install
/// its methods, and — on first load — mint + export the singleton. Hot-reload safe
/// like the store: methods reinstall each load, the instance persists via the id
/// environment. Requires the store (`db`) to be up first.
pub fn bringUp(api: *idapi.IdApi) void {
    g_api = api;
    store = api.import("Store");

    s_requires = api.intern("requires");
    s_kind = api.intern("kind");
    s_registered = api.intern("registered");
    s_kernel = api.intern("kernel");
    s_selfHosted = api.intern("selfHosted");

    s_assert = api.intern("assert:attr:value:");
    s_currentValue = api.intern("currentValue:attr:");
    s_new = api.intern("new");
    s_add = api.intern("add:");
    s_size = api.intern("size");
    s_at = api.intern("at:");

    const reloading = api.isExported("Module") != 0;
    if (reloading) {
        module_vt = api.import("ModuleVt");
        module_obj = api.import("Module");
    } else {
        module_vt = api.proto(api.object_vt);
    }

    api.method(module_vt, api.intern("named:requires:"), &mNamedRequires);
    api.method(module_vt, api.intern("kernel:"), &mKernel);
    api.method(module_vt, api.intern("requiresOf:"), &mRequiresOf);
    api.method(module_vt, api.intern("kindOf:"), &mKindOf);
    api.method(module_vt, api.intern("isRegistered:"), &mIsRegistered);
    api.method(module_vt, api.intern("loadOrder:"), &mLoadOrder);

    if (!reloading) {
        module_obj = api.alloc(module_vt, 0);
        _ = api.exportObj("ModuleVt", module_vt);
        _ = api.exportObj("Module", module_obj);
    }
}

/// The reloadable-module entrypoint. Like the store, nothing threads through the
/// opaque state channel — `Module` persists as a libid object via the id env.
/// Gated out of test builds so a suite can import several modules without the
/// `module_init` symbol colliding (the store keeps it via bringUp directly).
fn moduleInit(api: *idapi.IdApi, state: ?*anyopaque) callconv(.c) ?*anyopaque {
    _ = state;
    bringUp(api);
    return null;
}
comptime {
    if (!@import("builtin").is_test) @export(&moduleInit, .{ .name = "module_init" });
}
