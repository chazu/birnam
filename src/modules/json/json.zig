//! JSON module — a thin dylib shell around a birnam-authored parser.
//!
//! Loads as `libbirnam_json.dylib` AFTER peg + lang (it needs the PEG engine,
//! the CST navigation surface, and `Object new`). On init it files in the
//! embedded `json.birnam`, which defines the `Json` prototype with `parse:`.
//! The parser logic lives in birnam — this zig file only bootstraps it.

const std = @import("std");

const abi = @import("module_abi");
const Ref = abi.Ref;
const IdApi = abi.IdApi;
const ShellContext = abi.ShellContext;
const mkEid = abi.mkEid;

/// The birnam source, embedded at build time.
const JSON_SRC = @embedFile("json.birnam");

export fn birnam_module_init(
    ctx: *ShellContext,
    api: *const IdApi,
    module_eid: u64,
) void {
    _ = module_eid;
    ctx.log("[json] initializing (birnam unit)");
    // `[language fileInSource: JSON_SRC]` — atomic file-in defines `Json`.
    const src = api.make_string(JSON_SRC.ptr, JSON_SRC.len);
    const sel = api.intern("fileInSource:", 13);
    _ = api.send1(mkEid(abi.LANGUAGE_PROTO), sel, src);
    ctx.log("[json] ready");
}
