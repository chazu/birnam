//! PEG dylib façade — Phase 10.0 step 4.
//!
//! The PEG engine lives in `src/kernel/peg.zig` (P10 deviation noted there).
//! This file is the dynamic-link artifact loaded at runtime via the
//! Phase 5 module manager. At module-init it calls
//! `api.peg_install_send_api(module_eid)` which dispatches into the
//! kernel-side install routine, installing `parse:from:` on GRAMMAR_VT.
//!
//! IdApi layout is duplicated and MUST stay layout-compatible with
//! src/kernel/id_api.zig (Phase 9.5.5 ABI discipline — new fields are
//! appended at the end of the struct, never inserted mid-struct).

const std = @import("std");

const abi = @import("../../kernel/module_abi.zig");
const Ref = abi.Ref;
const NIL = abi.NIL;

const ShellContext = abi.ShellContext;

const FactCallback = abi.FactCallback;

const IdApi = abi.IdApi;

// Phase 14 M4: export the C entry only for dylib builds. When statically
// `@import`ed into the AOT binary (build_opts.export_init = false), the
// caller invokes `moduleInitImpl` directly — avoids a duplicate
// `birnam_module_init` symbol across statically-linked modules.
const build_opts = @import("build_opts");

pub fn moduleInitImpl(
    ctx: *ShellContext,
    api: *const IdApi,
    module_eid: u64,
) callconv(.c) void {
    ctx.log("[peg] initializing");
    api.peg_install_send_api(module_eid);
    ctx.log("[peg] ready");
}

comptime {
    if (build_opts.export_init) @export(&moduleInitImpl, .{ .name = "birnam_module_init" });
}
