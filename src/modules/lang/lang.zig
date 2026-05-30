//! Lang — Phase 10.1 dylib module.
//!
//! Loads the birnam language layer atop the kernel + PEG engine. Scope of
//! Phase 10.1 (per ROADMAP): asserts ~50 PEG rules forming the birnam
//! grammar (G2-locked syntax) and registers them under M_LANG ownership.
//!
//! Phase 10.2 then adds the parser runner (slot resolution + AST
//! generation); 10.3 evaluator; 10.4 method install; 10.5 REPL surface.
//!
//! 10.1.a scope (this scaffolding commit): module loads, logs ready,
//! prepares to receive grammar-construction calls. Actual grammar
//! assertion lands in 10.1.b once the IdApi surface or send-dispatched
//! PEG calls have been validated end-to-end at dylib-load time.

const std = @import("std");

const Ref = u64;

const ShellContext = extern struct {
    log: *const fn ([*:0]const u8) callconv(.c) void,
    set_repl_handler: *const fn (?*const fn ([*:0]const u8) callconv(.c) void) callconv(.c) void,
};

const IdApiOpaque = opaque {};

export fn birnam_module_init(
    ctx: *ShellContext,
    api: *const IdApiOpaque,
    module_eid: u64,
) void {
    _ = api;
    _ = module_eid;
    ctx.log("[lang] initializing");
    ctx.log("[lang] ready");
}
