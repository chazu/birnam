//! PEG dylib façade — Phase 10.0.
//!
//! The PEG engine lives in `src/kernel/peg.zig` (P10 deviation noted there
//! and tracked in ROADMAP follow-up). This file is the dynamic-link
//! artifact loaded at runtime via the Phase 5 module manager; step 4 of
//! the Phase 10.0 work will replace the stub `birnam_module_init` here
//! with full send-dispatched method install on PEG_VT / GRAMMAR_VT,
//! calling back into the kernel engine through IdApi function pointers.

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
    ctx.log("[peg] initializing");
    ctx.log("[peg] ready");
}
