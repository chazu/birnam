//! Tests for the image (docs/40, slice 3). Two levels: FORMAT tests exercise the
//! header + record framing with no running system (via `writeHeaderOnly` /
//! `validate`); the ROUND-TRIP test brings up a real id stack in-process and
//! proves store-reachable state — leaf globals, a vector trie, and a HAMT —
//! survives `saveImage` → `loadImage` (a wholesale replace, so an intervening
//! mutation is undone by the load).

const std = @import("std");
const image = @import("image.zig");
const id = @import("id.zig");
const store = @import("store.zig");
const collection = @import("collection.zig");
const prelude = @import("prelude.zig");
const peg = @import("peg.zig");
const lang = @import("lang.zig");
const tcc = @import("tcc.zig");
const codegen = @import("codegen.zig");
// The module manager is birnam now (src/id/modules.birnam, docs/44 Phase 5b);
// tests that need it eval this source on the bare floor — which is exactly the
// driver's from-zero fallback path, so the test doubles as its proof.
const MODULES_SRC = @embedFile("modules.birnam");
const STDLIB_SRC = @embedFile("stdlib.birnam");
const COMPILED_UNIT_SRC = @embedFile("compiled-unit.birnam");
const err_mod = @import("error.zig");
const relational = @import("relational.zig");
const alien = @import("alien.zig");
const isolate = @import("isolate.zig");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;

test "abiHash is deterministic and non-trivial" {
    try std.testing.expectEqual(image.abiHash(), image.abiHash());
    try std.testing.expect(image.abiHash() != 0);
}

test "a header-only image is valid; load walks its empty record stream" {
    // The format floor, exercised without a running system: writeHeaderOnly emits
    // just the header, and load (finding no records) accepts it. The full
    // store-integrated round-trip is the birnam-level test in S3.5.
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const path = try tmp.dir.realpathAlloc(std.testing.allocator, ".");
    defer std.testing.allocator.free(path);
    const file = try std.fs.path.join(std.testing.allocator, &.{ path, "empty.image" });
    defer std.testing.allocator.free(file);

    try std.testing.expect(image.writeHeaderOnly(file));
    try std.testing.expect(image.validate(file));
}

test "load rejects a missing file, garbage, and a bad magic" {
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const dir = try tmp.dir.realpathAlloc(std.testing.allocator, ".");
    defer std.testing.allocator.free(dir);

    const missing = try std.fs.path.join(std.testing.allocator, &.{ dir, "nope.image" });
    defer std.testing.allocator.free(missing);
    try std.testing.expect(!image.validate(missing));

    const garbage = try std.fs.path.join(std.testing.allocator, &.{ dir, "garbage.image" });
    defer std.testing.allocator.free(garbage);
    {
        const f = try std.fs.cwd().createFile(garbage, .{});
        defer f.close();
        try f.writeAll("not an image at all");
    }
    try std.testing.expect(!image.validate(garbage));

    // Right magic length, wrong bytes → rejected.
    const badmagic = try std.fs.path.join(std.testing.allocator, &.{ dir, "badmagic.image" });
    defer std.testing.allocator.free(badmagic);
    {
        const f = try std.fs.cwd().createFile(badmagic, .{});
        defer f.close();
        try f.writeAll("XXXXXXXX" ++ &[_]u8{0} ** 20);
    }
    try std.testing.expect(!image.validate(badmagic));
}

test "load rejects a truncated header" {
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const dir = try tmp.dir.realpathAlloc(std.testing.allocator, ".");
    defer std.testing.allocator.free(dir);
    const trunc = try std.fs.path.join(std.testing.allocator, &.{ dir, "trunc.image" });
    defer std.testing.allocator.free(trunc);
    {
        const f = try std.fs.cwd().createFile(trunc, .{});
        defer f.close();
        try f.writeAll(image.MAGIC); // magic only — header incomplete
    }
    try std.testing.expect(!image.validate(trunc));
}

// -- The round-trip: a real stack, in-process --------------------------------

var gapi: *idapi.IdApi = undefined;

fn setup() *idapi.IdApi {
    const api = id.id_init();
    gapi = api;
    prelude.bringUp(api); // exports SystemVt (image extends it)
    store.bringUp(api); // seeds the schema; exports Store + Globals
    collection.bringUp(api); // Vector + Map factories
    image.bringUp(api); // saveImage:/loadImage: + the fact collector
    return api;
}

fn send(rcv: Oop, sel: [*:0]const u8, args: []const Oop) Oop {
    return gapi.send(rcv, gapi.intern(sel), args.ptr, args.len);
}
fn gPut(globals: Oop, name: [*:0]const u8, val: Oop) void {
    _ = send(globals, "at:put:", &.{ gapi.intern(name), val });
}
fn gAt(globals: Oop, name: [*:0]const u8) Oop {
    return send(globals, "at:", &.{gapi.intern(name)});
}

fn tmpImagePath(a: std.mem.Allocator, tmp: *std.testing.TmpDir) ![]u8 {
    const dir = try tmp.dir.realpathAlloc(a, ".");
    defer a.free(dir);
    return std.fs.path.join(a, &.{ dir, "rt.image" });
}

fn corruptFirstCodeUnit(bytes: []u8) !void {
    var i: usize = image.MAGIC.len + 4 + 4 + 4 + 8;
    while (i < bytes.len) {
        if (i + 12 > bytes.len) return error.TruncatedRecord;
        const kind = std.mem.readInt(u32, bytes[i..][0..4], .little);
        const len: usize = @intCast(std.mem.readInt(u64, bytes[i + 4 ..][0..8], .little));
        const payload_start = i + 12;
        if (payload_start + len > bytes.len) return error.TruncatedRecord;
        if (kind == 4) {
            if (len <= 8) return error.EmptyCodeUnit;
            // v3 payloads are [u64 checksum][dylib bytes]. Preserve the record
            // framing while changing a dylib byte, so only integrity catches it.
            bytes[payload_start + 8] ^= 0x80;
            return;
        }
        i = payload_start + len;
    }
    return error.NoCodeUnit;
}

test "round-trip: leaf + vector + map globals survive save/load" {
    const api = setup();
    const globals = api.import("Globals").?;

    // int, string, vector #[10 20 30], map {#k -> 7}
    gPut(globals, "gx", api.mkSmallInt(42));
    gPut(globals, "gs", api.mkString("hi", 2));

    const vfac = api.import("Vector").?;
    var v = send(vfac, "new", &.{});
    v = send(v, "add:", &.{api.mkSmallInt(10)});
    v = send(v, "add:", &.{api.mkSmallInt(20)});
    v = send(v, "add:", &.{api.mkSmallInt(30)});
    gPut(globals, "gv", v);

    const mfac = api.import("Map").?;
    var m = send(mfac, "new", &.{});
    m = send(m, "at:put:", &.{ api.intern("k"), api.mkSmallInt(7) });
    gPut(globals, "gm", m);

    // Save, then MUTATE gx — the load must undo it (wholesale replace).
    const a = std.testing.allocator;
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const path = try tmpImagePath(a, &tmp);
    defer a.free(path);

    try std.testing.expect(image.saveImage(path));
    gPut(globals, "gx", api.mkSmallInt(999));
    try std.testing.expect(image.loadImage(path));

    // Leaves.
    try std.testing.expectEqual(@as(i64, 42), api.smallIntValue(gAt(globals, "gx")));
    const gs = gAt(globals, "gs");
    var slen: usize = 0;
    const sp = api.stringBytes(gs, &slen);
    try std.testing.expectEqualStrings("hi", sp[0..slen]);

    // Vector trie: size + an element (0-indexed).
    const gv = gAt(globals, "gv");
    try std.testing.expectEqual(@as(i64, 3), api.smallIntValue(send(gv, "size", &.{})));
    try std.testing.expectEqual(@as(i64, 20), api.smallIntValue(send(gv, "at:", &.{api.mkSmallInt(1)})));

    // HAMT: the symbol key resolves after re-interning (the keyHash fix).
    const gm = gAt(globals, "gm");
    try std.testing.expectEqual(@as(i64, 7), api.smallIntValue(send(gm, "at:", &.{api.intern("k")})));
}

test "§G3: an object with an unknown slot in a NON-zero position rejects the image" {
    const api = setup();
    const globals = api.import("Globals").?;

    // Manufacture a pathological type: slot 0 has a KNOWN kind (oop) but slot 1 is
    // `slot_unknown` — a partially-described layout no real type produces today, so
    // this is the latent hole G3 names. The old guard inspected only slot 0, passed
    // this object, and `drainHeapFrom` would then dump slot 1 (a live Oop) as a raw
    // word → a dangling pointer on load. The tightened guard scans every slot and
    // defers the whole object instead.
    const vt = api.proto(api.object_vt);
    _ = api.exportObj("BadLayout", vt); // exported → not deferred for the image-born reason
    const kinds = [_]u8{ idapi.slot_oop, idapi.slot_unknown };
    api.setLayout(vt, &kinds, 2, idapi.slot_raw);

    const obj = api.alloc(vt, 2 * @sizeOf(usize));
    const live = api.mkString("i-would-dangle", 14); // a real, rooted Oop in the unknown slot
    const words: [*]usize = @ptrCast(@alignCast(obj.?));
    words[0] = @intFromPtr(api.nilObj.?); // slot 0: a legit oop
    words[1] = @intFromPtr(live.?); // slot 1: a live Oop under an UNKNOWN kind

    gPut(globals, "ggood", api.mkSmallInt(7));

    const a = std.testing.allocator;
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const path = try tmpImagePath(a, &tmp);
    defer a.free(path);

    // Establish a known-good image, then try to replace it with unsupported state.
    // A rejected save must preserve the old publication intact.
    try std.testing.expect(image.saveImage(path));
    const before = try std.fs.cwd().readFileAlloc(a, path, 1 << 30);
    defer a.free(before);

    gPut(globals, "gbad", obj); // reachable from a saved global
    // Globals use the default no-history `:one` policy, so this physically
    // removes the fixture from the shared process store after the assertion.
    // The image tests deliberately reuse that well-known store across setups.
    defer _ = send(api.import("Store").?, "retract:attr:value:", &.{ api.intern("__globals__"), api.intern("gbad"), obj });

    // The full layout guard rejects this durable root rather than serializing a
    // live pointer as a raw word or silently dropping state from the image.
    try std.testing.expect(!image.saveImage(path));
    const report = send(api.import("System").?, "imageSaveReport", &.{});
    try std.testing.expect(send(report, "status", &.{}) == api.intern("rejected"));
    try std.testing.expectEqual(@as(i64, 1), api.smallIntValue(send(report, "rejectedFacts", &.{})));
    const after = try std.fs.cwd().readFileAlloc(a, path, 1 << 30);
    defer a.free(after);
    try std.testing.expectEqualSlices(u8, before, after);
}

test "append images are not part of the format surface" {
    try std.testing.expect(!@hasDecl(image, "appendImage"));
}

// -- slice 4: the compiled .code section (map by default, replay on --rebuild) --

/// The full front end (peg → lang → tcc → codegen) atop `setup`, so an image can
/// carry a compiled `.code` section. This test file defines NO other methods, so
/// `__methods__` holds exactly what this test defines — a clean code section.
fn setupWithCodegen() *idapi.IdApi {
    const api = id.id_init();
    gapi = api;
    store.bringUp(api);
    peg.bringUp(api);
    lang.bringUp(api);
    tcc.bringUp(api);
    codegen.bringUp(api);
    prelude.bringUp(api);
    collection.bringUp(api);
    image.bringUp(api);
    return api;
}

fn setupWithCompiledUnits() *idapi.IdApi {
    const api = setupWithCodegen();
    err_mod.bringUp(api);
    relational.bringUp(api);
    const Codegen = api.import("Codegen").?;
    const Globals = api.import("Globals").?;
    const existing_generations = gAt(Globals, "DispatchGeneration");
    if (existing_generations != api.nilObj and
        api.send(existing_generations, api.intern("current"), &.{}, 0) != api.nilObj) return api;
    const fixture = "[Module named: #cuImage requires: #[]]; let ImageReceiver := [Object clone]; [Reflect defineMethod: #cuImageProbe on: Integer as: [:self | 41]]; [Reflect defineMethod: #cuImageHandle: on: ImageReceiver as: [:self :value | value]]";
    const sources = [_][]const u8{ MODULES_SRC, STDLIB_SRC, COMPILED_UNIT_SRC, fixture };
    const names = [_][*:0]const u8{ "modules", "stdlib", "compiledUnit", "cuImage" };
    for (sources, names) |src, name| {
        const staged = api.send(Codegen, api.intern("recordModuleSource:named:"), &.{ api.mkString(src.ptr, src.len), api.intern(name) }, 2);
        std.debug.assert(api.send(staged, api.intern("isError"), &.{}, 0) != api.trueObj);
        const eval_selector: Oop = if (std.mem.eql(u8, std.mem.span(name), "cuImage")) api.intern("evalPersisting:") else api.intern("eval:");
        const result = api.send(Codegen, eval_selector, &.{api.mkString(src.ptr, src.len)}, 1);
        std.debug.assert(result != api.nilObj);
    }
    const generations = gAt(Globals, "DispatchGeneration");
    const initialized = api.send(generations, api.intern("initializeCurrent"), &.{}, 0);
    std.debug.assert(api.send(initialized, api.intern("isError"), &.{}, 0) != api.trueObj);
    return api;
}

test "Store-reachable isolate handles are explicitly non-durable" {
    const api = setupWithCompiledUnits();
    isolate.bringUp(api);
    const globals = api.import("Globals").?;
    const token_only_handle = api.alloc(api.import("IsolateHandleVt"), @sizeOf(u64));
    gPut(globals, "nonDurableHandle", token_only_handle);
    defer _ = send(api.import("Store").?, "retract:attr:value:", &.{ api.intern("__globals__"), api.intern("nonDurableHandle"), token_only_handle });

    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const path = try tmpImagePath(std.testing.allocator, &tmp);
    defer std.testing.allocator.free(path);
    try std.testing.expect(!image.saveImage(path));
    const report = send(api.import("System").?, "imageSaveReport", &.{});
    const detail = send(report, "detail", &.{});
    var len: usize = 0;
    const bytes = api.stringBytes(detail, &len)[0..len];
    try std.testing.expect(std.mem.indexOf(u8, bytes, "nonDurableIsolateHandle") != null);
}

test "round-trip: generation code is mapped by default and rebuilt from the same documents" {
    const api = setupWithCompiledUnits();
    const Codegen = api.import("Codegen").?;
    const globals = api.import("Globals").?;
    gPut(globals, "n", api.mkSmallInt(9));
    try std.testing.expectEqual(@as(i64, 41), api.smallIntValue(send(api.mkSmallInt(5), "cuImageProbe", &.{})));
    try std.testing.expectEqual(@as(i64, 17), api.smallIntValue(send(gAt(globals, "ImageReceiver"), "cuImageHandle:", &.{api.mkSmallInt(17)})));

    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const a = std.testing.allocator;
    const dir = try tmp.dir.realpathAlloc(a, ".");
    defer a.free(dir);
    const path = try std.fs.path.join(a, &.{ dir, "code.image" });
    defer a.free(path);

    try std.testing.expect(image.saveImage(path));

    try std.testing.expect(image.loadImage(path));
    try std.testing.expect(image.mappedCode());
    try std.testing.expectEqual(@as(i64, 41), api.smallIntValue(send(api.mkSmallInt(5), "cuImageProbe", &.{})));
    try std.testing.expectEqual(@as(i64, 9), api.smallIntValue(gAt(globals, "n")));
    try std.testing.expectEqual(@as(i64, 23), api.smallIntValue(send(gAt(globals, "ImageReceiver"), "cuImageHandle:", &.{api.mkSmallInt(23)})));

    // Forced rebuild takes the REPLAY path: the load maps NO code (answers false),
    // and the boot orchestration (here, as in runImage) rebuilds the library by
    // re-evaluating the image's own recorded source — after which the method works.
    const sys = api.import("System").?;
    const status = api.send(sys, api.intern("loadImage:rebuild:"), &.{ api.mkString(path.ptr, path.len), api.trueObj }, 2);
    try std.testing.expect(status == api.falseObj);
    try std.testing.expect(!image.mappedCode());
    _ = api.send(Codegen, api.intern("replayMethods"), &.{}, 0); // what runImage does on the false branch
    try std.testing.expectEqual(@as(i64, 41), api.smallIntValue(send(api.mkSmallInt(5), "cuImageProbe", &.{})));
    try std.testing.expectEqual(@as(i64, 29), api.smallIntValue(send(gAt(globals, "ImageReceiver"), "cuImageHandle:", &.{api.mkSmallInt(29)})));
}

test "round-trip: standalone script method sources are mapped and rebuilt" {
    const api = setupWithCodegen();
    const Codegen = api.import("Codegen").?;
    const globals = api.import("Globals").?;
    const source = "[Reflect defineMethod: #directImageProbe on: Integer as: [:self | 73]]";
    const source_path = "/tmp/direct-script-image.birnam";
    const eval_result = api.send(Codegen, api.intern("evalPersisting:from:"), &.{
        api.mkString(source.ptr, source.len),
        api.mkString(source_path.ptr, source_path.len),
    }, 2);
    try std.testing.expect(eval_result != api.nilObj);
    try std.testing.expectEqual(@as(i64, 73), api.smallIntValue(send(api.mkSmallInt(1), "directImageProbe", &.{})));

    const generations = gAt(globals, "DispatchGeneration");
    try std.testing.expect(send(generations, "initializeCurrent", &.{}) != api.nilObj);

    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const a = std.testing.allocator;
    const dir = try tmp.dir.realpathAlloc(a, ".");
    defer a.free(dir);
    const path = try std.fs.path.join(a, &.{ dir, "direct-script.image" });
    defer a.free(path);

    try std.testing.expect(image.saveImage(path));

    const reflect = api.import("Reflect").?;
    const integer = api.import("Integer").?;
    try std.testing.expect(send(reflect, "removeMethod:on:", &.{ api.intern("directImageProbe"), integer }) != api.nilObj);

    try std.testing.expect(image.loadImage(path));
    try std.testing.expect(image.mappedCode());
    try std.testing.expectEqual(@as(i64, 73), api.smallIntValue(send(api.mkSmallInt(1), "directImageProbe", &.{})));

    try std.testing.expect(send(reflect, "removeMethod:on:", &.{ api.intern("directImageProbe"), integer }) != api.nilObj);
    const system = api.import("System").?;
    const rebuilt = api.send(system, api.intern("loadImage:rebuild:"), &.{
        api.mkString(path.ptr, path.len), api.trueObj,
    }, 2);
    try std.testing.expect(rebuilt == api.falseObj);
    try std.testing.expect(!image.mappedCode());
    _ = api.send(Codegen, api.intern("replayMethods"), &.{}, 0);
    try std.testing.expectEqual(@as(i64, 73), api.smallIntValue(send(api.mkSmallInt(1), "directImageProbe", &.{})));
}

test "active candidate image restores the exact authoritative generation" {
    const api = setupWithCompiledUnits();
    const Codegen = api.import("Codegen").?;
    const Globals = api.import("Globals").?;
    const System = api.import("System").?;
    const CompiledUnit = gAt(Globals, "CompiledUnit");
    try std.testing.expect(CompiledUnit != api.nilObj);
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const a = std.testing.allocator;
    const dir = try tmp.dir.realpathAlloc(a, ".");
    defer a.free(dir);
    const candidate_path = try std.fs.path.join(a, &.{ dir, "image-candidate.dylib" });
    defer a.free(candidate_path);
    const image_path = try std.fs.path.join(a, &.{ dir, "active.image" });
    defer a.free(image_path);
    const candidate2_path = try std.fs.path.join(a, &.{ dir, "image-candidate-2.dylib" });
    defer a.free(candidate2_path);
    const successor_image_path = try std.fs.path.join(a, &.{ dir, "successor.image" });
    defer a.free(successor_image_path);
    const candidate_source = "[Module named: #cuImage requires: #[]]; let ImageReceiver := [Object clone]; [Reflect defineMethod: #cuImageProbe on: Integer as: [:self | 410]]; [Reflect defineMethod: #cuImageHandle: on: ImageReceiver as: [:self :value | value]]";
    const compiled = api.send(CompiledUnit, api.intern("compile:named:output:"), &.{
        api.mkString(candidate_source.ptr, candidate_source.len), api.intern("cuImage"), api.mkString(candidate_path.ptr, candidate_path.len),
    }, 3);
    try std.testing.expect(api.send(compiled, api.intern("isError"), &.{}, 0) != api.trueObj);
    const origin = api.send(CompiledUnit, api.intern("open:"), &.{api.mkString(candidate_path.ptr, candidate_path.len)}, 1);
    try std.testing.expect(api.send(origin, api.intern("map"), &.{}, 0) == origin);
    const intent = api.send(origin, api.intern("activate"), &.{}, 0);
    try std.testing.expect(api.send(intent, api.intern("fulfill"), &.{}, 0) == origin);
    try std.testing.expectEqual(@as(i64, 410), api.smallIntValue(send(api.mkSmallInt(3), "cuImageProbe", &.{})));
    try std.testing.expect(image.saveImage(image_path));

    try std.testing.expect(image.loadImage(image_path));
    try std.testing.expect(image.mappedCode());
    const LoadedCompiledUnit = gAt(Globals, "CompiledUnit");
    const loaded_origin = api.send(LoadedCompiledUnit, api.intern("activeFor:"), &.{api.intern("cuImage")}, 1);
    try std.testing.expect(loaded_origin != api.nilObj);
    try std.testing.expectEqual(@as(i64, 410), api.smallIntValue(send(api.mkSmallInt(3), "cuImageProbe", &.{})));

    // A restored provenance object has no native handle, but it is not an
    // authority bottleneck: a fresh complete successor can activate over the
    // restored generation, save, and boot normally.
    const candidate2_source = "[Module named: #cuImage requires: #[]]; let ImageReceiver := [Object clone]; [Reflect defineMethod: #cuImageProbe on: Integer as: [:self | 411]]; [Reflect defineMethod: #cuImageHandle: on: ImageReceiver as: [:self :value | value]]";
    _ = api.send(LoadedCompiledUnit, api.intern("compile:named:output:"), &.{
        api.mkString(candidate2_source.ptr, candidate2_source.len), api.intern("cuImage"), api.mkString(candidate2_path.ptr, candidate2_path.len),
    }, 3);
    const successor = api.send(LoadedCompiledUnit, api.intern("open:"), &.{api.mkString(candidate2_path.ptr, candidate2_path.len)}, 1);
    try std.testing.expect(api.send(successor, api.intern("map"), &.{}, 0) == successor);
    const successor_intent = api.send(successor, api.intern("activate"), &.{}, 0);
    try std.testing.expect(api.send(successor_intent, api.intern("fulfill"), &.{}, 0) == successor);
    try std.testing.expectEqual(@as(i64, 411), api.smallIntValue(send(api.mkSmallInt(3), "cuImageProbe", &.{})));
    try std.testing.expect(image.saveImage(successor_image_path));
    try std.testing.expect(image.loadImage(successor_image_path));
    try std.testing.expectEqual(@as(i64, 411), api.smallIntValue(send(api.mkSmallInt(3), "cuImageProbe", &.{})));

    const rebuilt = api.send(System, api.intern("loadImage:rebuild:"), &.{ api.mkString(image_path.ptr, image_path.len), api.trueObj }, 2);
    try std.testing.expect(rebuilt == api.falseObj);
    _ = api.send(Codegen, api.intern("replayMethods"), &.{}, 0);
    try std.testing.expect(api.send(LoadedCompiledUnit, api.intern("activeFor:"), &.{api.intern("cuImage")}, 1) != api.nilObj);
    try std.testing.expectEqual(@as(i64, 410), api.smallIntValue(send(api.mkSmallInt(3), "cuImageProbe", &.{})));
    // The remaining tests reuse this process as independent scenarios.
    codegen.beginImageDispatchRecovery();
    codegen.endImageDispatchRecovery();
}

test "compiled-unit integrity rejects corruption before restoring image data" {
    const api = setupWithCompiledUnits();
    const globals = api.import("Globals").?;
    gPut(globals, "integrityDatum", api.mkSmallInt(7));

    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const a = std.testing.allocator;
    const dir = try tmp.dir.realpathAlloc(a, ".");
    defer a.free(dir);
    const path = try std.fs.path.join(a, &.{ dir, "integrity.image" });
    defer a.free(path);

    try std.testing.expect(image.saveImage(path));
    const bytes = try std.fs.cwd().readFileAlloc(a, path, 1 << 30);
    defer a.free(bytes);
    try corruptFirstCodeUnit(bytes);
    const f = try std.fs.cwd().createFile(path, .{ .truncate = true });
    defer f.close();
    try f.writeAll(bytes);

    // A failed preflight leaves the pre-existing live store untouched.
    gPut(globals, "integrityDatum", api.mkSmallInt(99));
    try std.testing.expect(!image.validate(path));
    try std.testing.expect(!image.loadImage(path));
    try std.testing.expectEqual(@as(i64, 99), api.smallIntValue(gAt(globals, "integrityDatum")));
}

/// setupWithCodegen + error + Alien: the FFI stack, for wrap-replay round-trips.
fn setupWithAlien() *idapi.IdApi {
    const api = setupWithCompiledUnits();
    err_mod.bringUp(api);
    alien.bringUp(api);
    return api;
}

test "round-trip: an Alien wrap is recorded (`__aliens__`), deduped, and replayed" {
    const api = setupWithAlien();
    const Codegen = api.import("Codegen").?;
    const Store = api.import("Store").?;
    const aliens = api.intern("__aliens__");
    const attrCount = api.intern("attrCount:");

    // A successful wrap records its send as replayable source (a native
    // trampoline can't ride the image). Re-running the same wrap records
    // NOTHING new (by-value dedupe); a wrap on an ANONYMOUS target — resolvable
    // under no name at replay time — is installed but deliberately unrecorded.
    const w = "[Alien wrap: \"strlen\" signature: \"l s\" as: #cStrlen on: String]";
    _ = api.send(Codegen, api.intern("eval:"), &.{api.mkString(w.ptr, w.len)}, 1);
    const hello = api.mkString("hello", 5);
    try std.testing.expectEqual(@as(i64, 5), api.smallIntValue(send(hello, "cStrlen", &.{})));
    try std.testing.expectEqual(@as(i64, 1), api.smallIntValue(api.send(Store, attrCount, &.{aliens}, 1)));
    _ = api.send(Codegen, api.intern("eval:"), &.{api.mkString(w.ptr, w.len)}, 1);
    try std.testing.expectEqual(@as(i64, 1), api.smallIntValue(api.send(Store, attrCount, &.{aliens}, 1)));
    const anon = "[Alien wrap: \"strlen\" signature: \"l s\" as: #anonLen on: [Object clone]]";
    _ = api.send(Codegen, api.intern("eval:"), &.{api.mkString(anon.ptr, anon.len)}, 1);
    try std.testing.expectEqual(@as(i64, 1), api.smallIntValue(api.send(Store, attrCount, &.{aliens}, 1)));

    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const a = std.testing.allocator;
    const dir = try tmp.dir.realpathAlloc(a, ".");
    defer a.free(dir);
    const path = try std.fs.path.join(a, &.{ dir, "wrap.image" });
    defer a.free(path);

    try std.testing.expect(image.saveImage(path));
    try std.testing.expect(image.loadImage(path));

    // The record survived the round-trip; replay re-evaluates it cleanly (the
    // boot path calls this after code resolution) and re-enters the recorder,
    // whose dedupe keeps the journal quiet. The trampoline works after.
    try std.testing.expectEqual(@as(i64, 1), api.smallIntValue(api.send(Store, attrCount, &.{aliens}, 1)));
    const n = api.send(Codegen, api.intern("replayAliens"), &.{}, 0);
    try std.testing.expectEqual(@as(i64, 1), api.smallIntValue(n));
    try std.testing.expectEqual(@as(i64, 1), api.smallIntValue(api.send(Store, attrCount, &.{aliens}, 1)));
    const world = api.mkString("worlds!", 7);
    try std.testing.expectEqual(@as(i64, 7), api.smallIntValue(send(world, "cStrlen", &.{})));
}

test "codegen: separately-compiled unit dylibs don't clobber each other's method blocks (salt)" {
    const api = setupWithCodegen();
    const Codegen = api.import("Codegen").?;
    const digest = api.send(Codegen, api.intern("currentGenerationDigest"), &.{}, 0);
    if (api.send(digest, api.intern("isError"), &.{}, 0) != api.trueObj) return error.SkipZigTest;

    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const a = std.testing.allocator;
    const dir = try tmp.dir.realpathAlloc(a, ".");
    defer a.free(dir);
    const pathA = try std.fs.path.join(a, &.{ dir, "unitA.dylib" });
    defer a.free(pathA);
    const pathB = try std.fs.path.join(a, &.{ dir, "unitB.dylib" });
    defer a.free(pathB);

    // Two DIFFERENT units, each defining a method whose body IS its own block. Each
    // compile resets the artifact block counter, so both methods would key on
    // `__amblk0` in the process-wide `birnam_global` namespace — the per-unit salt
    // (a source hash) keeps them distinct, so loading B does not overwrite A's block.
    const srcA = "[Reflect defineMethod: #saltA on: Integer as: [:x | [x + 1]]]";
    const srcB = "[Reflect defineMethod: #saltB on: Integer as: [:x | [x + 100]]]";
    const co = api.intern("compile:output:");
    try std.testing.expect(api.send(Codegen, co, &.{ api.mkString(srcA.ptr, srcA.len), api.mkString(pathA.ptr, pathA.len) }, 2) != api.nilObj);
    try std.testing.expect(api.send(Codegen, co, &.{ api.mkString(srcB.ptr, srcB.len), api.mkString(pathB.ptr, pathB.len) }, 2) != api.nilObj);

    const ld = api.intern("load:");
    try std.testing.expect(api.send(Codegen, ld, &.{api.mkString(pathA.ptr, pathA.len)}, 1) != api.nilObj);
    try std.testing.expect(api.send(Codegen, ld, &.{api.mkString(pathB.ptr, pathB.len)}, 1) != api.nilObj);

    // Without the salt, B (loaded last) would clobber A's `__amblk0`, so `saltA`
    // would run B's block and answer 105. With it, each keeps its own.
    try std.testing.expectEqual(@as(i64, 6), api.smallIntValue(send(api.mkSmallInt(5), "saltA", &.{})));
    try std.testing.expectEqual(@as(i64, 105), api.smallIntValue(send(api.mkSmallInt(5), "saltB", &.{})));
}

test "modules: [Module load:] loads a compiled unit dylib and its deps at runtime" {
    const api = setupWithCodegen();
    err_mod.bringUp(api); // modules.birnam answers kinded errors + sends isError
    relational.bringUp(api); // …and rides the well-known `db` (exported here)
    const Codegen = api.import("Codegen").?;
    const digest = api.send(Codegen, api.intern("currentGenerationDigest"), &.{}, 0);
    if (api.send(digest, api.intern("isError"), &.{}, 0) != api.trueObj) return error.SkipZigTest;
    _ = api.send(Codegen, api.intern("eval:"), &.{api.mkString(MODULES_SRC.ptr, MODULES_SRC.len)}, 1);
    const Globals = api.import("Globals").?;
    const Module = api.send(Globals, api.intern("at:"), &.{api.intern("Module")}, 1);
    try std.testing.expect(Module != api.nilObj); // the manager came up from source

    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const a = std.testing.allocator;
    const dir = try tmp.dir.realpathAlloc(a, ".");
    defer a.free(dir);
    _ = api.send(Module, api.intern("libDir:"), &.{api.mkString(dir.ptr, dir.len)}, 1); // search tmp

    // A dependency module + a dependent that uses its method. Compiled to
    // libbirnam_<name>.dylib in tmp; each carries a `__birnam_module_meta` symbol.
    const co = api.intern("compile:output:");
    const depPath = try std.fs.path.join(a, &.{ dir, "libbirnam_tdep.dylib" });
    defer a.free(depPath);
    const modPath = try std.fs.path.join(a, &.{ dir, "libbirnam_tmod.dylib" });
    defer a.free(modPath);
    const depSrc = "[Module named: #tdep requires: #[]]; [Reflect defineMethod: #td on: Integer as: [:x | [x + 5]]]";
    const modSrc = "[Module named: #tmod requires: #[#tdep]]; [Reflect defineMethod: #tm on: Integer as: [:x | [[x td] * 2]]]";
    try std.testing.expect(api.send(Codegen, co, &.{ api.mkString(depSrc.ptr, depSrc.len), api.mkString(depPath.ptr, depPath.len) }, 2) != api.nilObj);
    try std.testing.expect(api.send(Codegen, co, &.{ api.mkString(modSrc.ptr, modSrc.len), api.mkString(modPath.ptr, modPath.len) }, 2) != api.nilObj);

    // Neither is loaded yet; loading tmod must pull in tdep first (from the meta
    // symbol), so tmod's method — which calls tdep's `td` — works.
    try std.testing.expect(api.send(Module, api.intern("isLoaded:"), &.{api.intern("tmod")}, 1) == api.falseObj);
    try std.testing.expect(api.send(Module, api.intern("isLoaded:"), &.{api.intern("tdep")}, 1) == api.falseObj);
    _ = api.send(Module, api.intern("load:"), &.{api.intern("tmod")}, 1);
    try std.testing.expect(api.send(Module, api.intern("isLoaded:"), &.{api.intern("tmod")}, 1) == api.trueObj);
    try std.testing.expect(api.send(Module, api.intern("isLoaded:"), &.{api.intern("tdep")}, 1) == api.trueObj); // dep auto-loaded
    try std.testing.expectEqual(@as(i64, 16), api.smallIntValue(send(api.mkSmallInt(3), "tm", &.{}))); // (3+5)*2

    // A missing module fails gracefully: a kinded #loadFailed error (the error
    // module is up in this test — modules.birnam requires it), NOT marked loaded.
    const miss = api.send(Module, api.intern("load:"), &.{api.intern("tnope")}, 1);
    try std.testing.expect(api.send(miss, api.intern("isError"), &.{}, 0) == api.trueObj);
    try std.testing.expect(api.send(miss, api.intern("kind"), &.{}, 0) == api.intern("loadFailed"));
    try std.testing.expect(api.send(Module, api.intern("isLoaded:"), &.{api.intern("tnope")}, 1) == api.falseObj);
}

test "round-trip: a block-valued global is re-derived from its recorded source" {
    const api = setupWithCompiledUnits();
    const Codegen = api.import("Codegen").?;
    const globals = api.import("Globals").?;

    // A block bound to a name — a closure that captures a leaf global. Its VALUE
    // can't be serialized (an image-born vtable + code pointer), so the save
    // defers it; but `evalPersisting:` records the binding's SOURCE, from which
    // the image re-derives the closure on load (docs/40 §6).
    const src = "let base := 40; let addBase := [:x | [x + base]]";
    _ = api.send(Codegen, api.intern("evalPersisting:"), &.{api.mkString(src.ptr, src.len)}, 1);
    try std.testing.expectEqual(@as(i64, 42), api.smallIntValue(send(gAt(globals, "addBase"), "value:", &.{api.mkSmallInt(2)})));

    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const a = std.testing.allocator;
    const dir = try tmp.dir.realpathAlloc(a, ".");
    defer a.free(dir);
    const path = try std.fs.path.join(a, &.{ dir, "blk.image" });
    defer a.free(path);

    try std.testing.expect(image.saveImage(path));
    try std.testing.expect(image.loadImage(path)); // maps code + facts; block value deferred

    // The leaf `base` mapped via the fact stream; the block `addBase` did NOT
    // (its value fact was deferred), so it comes back unbound until re-derived.
    try std.testing.expectEqual(@as(i64, 40), api.smallIntValue(gAt(globals, "base")));

    // Re-derive block globals from source — what runImage does after code resolves.
    // Answers the count re-derived: exactly `addBase` (the leaf `base` stays bound).
    const n = api.send(Codegen, api.intern("deriveImageGlobals"), &.{}, 0);
    try std.testing.expectEqual(@as(i64, 1), api.smallIntValue(n));

    // The re-derived closure works and still sees the restored capture.
    try std.testing.expectEqual(@as(i64, 42), api.smallIntValue(send(gAt(globals, "addBase"), "value:", &.{api.mkSmallInt(2)})));
}
