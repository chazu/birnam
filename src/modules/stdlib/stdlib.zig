//! stdlib — a thin dylib shell that files in the birnam-authored stdlib units.
//!
//! Loads as `libbirnam_stdlib.dylib` AFTER peg + lang. At init it files in each
//! embedded `.birnam` unit (Test, Stats, Csv, Log), defining their prototypes.
//! All logic lives in birnam; this file only bootstraps it. One shim for all
//! units (vs one per module) — see the dedup note in TASKS.md.

const std = @import("std");

const abi = @import("module_abi");
const Ref = abi.Ref;
const IdApi = abi.IdApi;
const ShellContext = abi.ShellContext;
const mkEid = abi.mkEid;

const UNITS = [_][]const u8{
    @embedFile("test.birnam"),
    @embedFile("stats.birnam"),
    @embedFile("csv.birnam"),
    @embedFile("logging.birnam"),
};

export fn birnam_module_init(
    ctx: *ShellContext,
    api: *const IdApi,
    module_eid: u64,
) void {
    _ = module_eid;
    ctx.log("[stdlib] initializing (birnam units)");
    const sel = api.intern("fileInSource:", 13);
    for (UNITS) |src| {
        _ = api.send1(mkEid(abi.LANGUAGE_PROTO), sel, api.make_string(src.ptr, src.len));
    }
    ctx.log("[stdlib] ready");
}
