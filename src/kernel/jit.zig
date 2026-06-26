//! libtcc JIT — the mode-1 (live, malleable + native) code-generation backend.
//!
//! Per docs/10a (the cell refounding, "native ALWAYS; the dial is
//! codegen-residency"): birnam compiles to **C**, and that C is turned into
//! in-memory native machine code by TinyCC-as-a-library in ~ms — fast enough to
//! re-JIT on every redefine, so a method can be *malleable AND native* at once.
//! No dylib, no `dlopen`: `tcc_compile_string` → `tcc_relocate` → a callable
//! function pointer via `tcc_get_symbol`.
//!
//! This is the thin Zig wrapper over the libtcc C API. The birnam→C codegen pass
//! that feeds it (and the dispatch wiring that installs the resulting fn-ptrs as
//! method closures — the same relink-on-load spine W1 already proved) live above
//! this. The mode-3 ship backend (`clang`/`zig cc` → frozen dylib) reuses the
//! same birnam→C pass with a different compiler; see `freeze.zig`.
//!
//! Validated 2026-06-25 on arm64-apple-darwin (libtcc 0.9.27 install): a
//! recursive `fib` and a 2-arg `add` JIT + call correctly.

const std = @import("std");
const c = @cImport({
    @cInclude("libtcc.h");
});

pub const Error = error{
    StateInit,
    Compile,
    Relocate,
    SymbolNotFound,
};

/// A single libtcc compilation unit. Holds the in-memory native code alive: the
/// function pointers `getSymbol` returns are valid only until `deinit`. One Jit
/// per code-cell (or per batch of co-compiled cells) so a redefine can drop the
/// old unit and stand up a new one.
pub const Jit = struct {
    state: *c.TCCState,
    relocated: bool = false,

    /// `runtime_lib_dir` is where libtcc's own runtime archive (`libtcc1.a`,
    /// providing helpers like 64-bit division on some targets) lives — passed to
    /// `tcc_add_library_path`. On the dev install that is `/usr/local/lib/tcc`;
    /// a shipped birnam bundles it and points here. Pass null to skip (only safe
    /// when the generated C needs no libtcc runtime helpers).
    pub fn init(runtime_lib_dir: ?[*:0]const u8) Error!Jit {
        const s = c.tcc_new() orelse return Error.StateInit;
        errdefer c.tcc_delete(s);
        _ = c.tcc_set_output_type(s, c.TCC_OUTPUT_MEMORY);
        if (runtime_lib_dir) |dir| _ = c.tcc_add_library_path(s, dir);
        return .{ .state = s };
    }

    pub fn deinit(self: *Jit) void {
        c.tcc_delete(self.state);
        self.* = undefined;
    }

    /// Compile a C translation unit (null-terminated). May be called multiple
    /// times before `relocate` to accumulate sources into one unit.
    pub fn compile(self: *Jit, c_src: [*:0]const u8) Error!void {
        if (self.relocated) return Error.Compile; // already finalized
        if (c.tcc_compile_string(self.state, c_src) == -1) return Error.Compile;
    }

    /// Register a host symbol (function or data pointer) so generated C can call
    /// back into the **birnam runtime as a library** by name — the send entry
    /// points, allocation, HAMT/RRB ops, GC roots, etc. The codegen emits
    /// `extern` declarations for these and we satisfy them here; the JIT'd method
    /// body is then just C that orchestrates runtime calls over `Ref`s. Must run
    /// before `relocate`.
    pub fn addSymbol(self: *Jit, name: [*:0]const u8, ptr: *const anyopaque) Error!void {
        if (self.relocated) return Error.Compile;
        if (c.tcc_add_symbol(self.state, name, ptr) < 0) return Error.Compile;
    }

    /// Finalize: lay the compiled code out as executable memory. Must run once,
    /// after all `compile` calls and before any `getSymbol`.
    pub fn relocate(self: *Jit) Error!void {
        if (self.relocated) return;
        if (c.tcc_relocate(self.state) < 0) return Error.Relocate;
        self.relocated = true;
    }

    /// Resolve a top-level symbol (function or global) to its in-memory address.
    /// Requires `relocate` first. The returned pointer stays valid until `deinit`.
    pub fn getSymbol(self: *Jit, name: [*:0]const u8) Error!*anyopaque {
        if (!self.relocated) return Error.SymbolNotFound;
        return c.tcc_get_symbol(self.state, name) orelse Error.SymbolNotFound;
    }

    /// Typed convenience: resolve `name` and cast to a C-callconv function
    /// pointer of type `F` (e.g. `*const fn (c_int, c_int) callconv(.c) c_int`).
    pub fn getFn(self: *Jit, comptime F: type, name: [*:0]const u8) Error!F {
        const sym = try self.getSymbol(name);
        return @ptrCast(@alignCast(sym));
    }
};
