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
const modules = @import("modules.zig");
const err_mod = @import("error.zig");
const alien = @import("alien.zig");
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

test "§G3: an object with an unknown slot in a NON-zero position is deferred, not dumped raw" {
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

    gPut(globals, "gbad", obj); // reachable from a saved global
    gPut(globals, "ggood", api.mkSmallInt(7)); // a normal sibling that MUST survive

    const a = std.testing.allocator;
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const path = try tmpImagePath(a, &tmp);
    defer a.free(path);

    // Save must SUCCEED without hitting drainHeapFrom's `unreachable` (the guard
    // deferred gbad before it reached the drain), and load must apply cleanly.
    try std.testing.expect(image.saveImage(path));
    try std.testing.expect(image.loadImage(path));

    // The normal global round-trips; the pathological one was deferred → absent.
    try std.testing.expectEqual(@as(i64, 7), api.smallIntValue(gAt(globals, "ggood")));
    try std.testing.expect(gAt(globals, "gbad") == api.nilObj);
}

test "journal: appendImage writes deltas; load applies base + deltas cumulatively" {
    const api = setup();
    const globals = api.import("Globals").?;

    const a = std.testing.allocator;
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const path = try tmpImagePath(a, &tmp);
    defer a.free(path);

    // Base: one leaf global. saveImage establishes the append baseline.
    gPut(globals, "base_g", api.mkSmallInt(1));
    try std.testing.expect(image.saveImage(path));

    // Delta 1: a new leaf. Delta 2: a new vector (a heap object born in a delta —
    // its `.heap` record's ids must continue the base's numbering). Each append
    // writes only its own change, no rewrite.
    gPut(globals, "d_i", api.mkSmallInt(2));
    try std.testing.expect(image.appendImage(path));

    const vfac = api.import("Vector").?;
    var v = send(vfac, "new", &.{});
    v = send(v, "add:", &.{api.mkSmallInt(7)});
    v = send(v, "add:", &.{api.mkSmallInt(8)});
    gPut(globals, "d_v", v);
    // Also REBIND the base global in this delta — newest tx must win on load.
    gPut(globals, "base_g", api.mkSmallInt(11));
    try std.testing.expect(image.appendImage(path));

    // Clobber live state, then load: the load must restore base + every delta.
    gPut(globals, "base_g", api.mkSmallInt(999));
    gPut(globals, "d_i", api.mkSmallInt(999));
    try std.testing.expect(image.loadImage(path));

    try std.testing.expectEqual(@as(i64, 11), api.smallIntValue(gAt(globals, "base_g"))); // rebind won
    try std.testing.expectEqual(@as(i64, 2), api.smallIntValue(gAt(globals, "d_i")));
    const dv = gAt(globals, "d_v");
    try std.testing.expectEqual(@as(i64, 2), api.smallIntValue(send(dv, "size", &.{})));
    try std.testing.expectEqual(@as(i64, 8), api.smallIntValue(send(dv, "at:", &.{api.mkSmallInt(1)})));

    // The file is a base generation plus appended generations, not a rewrite: more
    // than one `.facts` record survives structural validation.
    try std.testing.expect(image.validate(path));
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

test "round-trip: a user method's compiled code is mapped (default), replayed (--rebuild)" {
    const api = setupWithCodegen();
    const Codegen = api.import("Codegen").?;

    // Define a user method live (JIT records its source), and bind a datum.
    const src = "[Reflect defineMethod: #quad on: Integer as: [:x | [[x + x] + [x + x]]]]";
    _ = api.send(Codegen, api.intern("eval:"), &.{api.mkString(src.ptr, src.len)}, 1);
    const globals = api.import("Globals").?;
    gPut(globals, "n", api.mkSmallInt(9));
    try std.testing.expectEqual(@as(i64, 20), api.smallIntValue(send(api.mkSmallInt(5), "quad", &.{})));

    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const a = std.testing.allocator;
    const dir = try tmp.dir.realpathAlloc(a, ".");
    defer a.free(dir);
    const path = try std.fs.path.join(a, &.{ dir, "code.image" });
    defer a.free(path);

    try std.testing.expect(image.saveImage(path));

    // Default load MAPS the compiled .code — the method comes back from mapped
    // code (no recompile) and the datum survives.
    try std.testing.expect(image.loadImage(path));
    try std.testing.expect(image.mappedCode());
    try std.testing.expectEqual(@as(i64, 20), api.smallIntValue(send(api.mkSmallInt(5), "quad", &.{})));
    try std.testing.expectEqual(@as(i64, 9), api.smallIntValue(gAt(globals, "n")));

    // Forced rebuild takes the REPLAY path: the load maps NO code (answers false),
    // and the boot orchestration (here, as in runImage) rebuilds the library by
    // re-evaluating the image's own recorded source — after which the method works.
    const sys = api.import("System").?;
    const status = api.send(sys, api.intern("loadImage:rebuild:"), &.{ api.mkString(path.ptr, path.len), api.trueObj }, 2);
    try std.testing.expect(status == api.falseObj);
    try std.testing.expect(!image.mappedCode());
    _ = api.send(Codegen, api.intern("replayMethods"), &.{}, 0); // what runImage does on the false branch
    try std.testing.expectEqual(@as(i64, 20), api.smallIntValue(send(api.mkSmallInt(5), "quad", &.{})));
}

/// setupWithCodegen + error + Alien: the FFI stack, for wrap-replay round-trips.
fn setupWithAlien() *idapi.IdApi {
    const api = setupWithCodegen();
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

test "round-trip: a module singleton (private vtable) survives; boot installs land on it" {
    const api = setupWithCodegen();
    const Codegen = api.import("Codegen").?;
    const globals = api.import("Globals").?;

    // A module-style singleton: a Globals-bound `[Object clone]`, specialised the
    // moment a method lands on it (ensurePrivateVtable). Before the serial-vtable
    // walk its value fact DEFERRED (unexported private vtable) — the binding
    // vanished at boot and the mapped installs landed on nil. Now it serializes
    // under the private vtable's exported parent (ObjectVt): the vtable itself is
    // DERIVED state, re-minted by the replayed installs.
    const s1 = "[Globals at: #Single put: [Object clone]]";
    _ = api.send(Codegen, api.intern("eval:"), &.{api.mkString(s1.ptr, s1.len)}, 1);
    const s2 = "[Reflect defineMethod: #answer on: Single as: [:s | 42]]";
    _ = api.send(Codegen, api.intern("eval:"), &.{api.mkString(s2.ptr, s2.len)}, 1);
    try std.testing.expectEqual(@as(i64, 42), api.smallIntValue(send(gAt(globals, "Single"), "answer", &.{})));

    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const a = std.testing.allocator;
    const dir = try tmp.dir.realpathAlloc(a, ".");
    defer a.free(dir);
    const path = try std.fs.path.join(a, &.{ dir, "single.image" });
    defer a.free(path);

    try std.testing.expect(image.saveImage(path));
    try std.testing.expect(image.loadImage(path));

    // The binding survived, and the mapped `.code` install (which runs AFTER the
    // fact stream) re-minted the restored object's private vtable and method.
    const single = gAt(globals, "Single");
    try std.testing.expect(single != api.nilObj);
    try std.testing.expectEqual(@as(i64, 42), api.smallIntValue(send(single, "answer", &.{})));
}

test "codegen: separately-compiled unit dylibs don't clobber each other's method blocks (salt)" {
    const api = setupWithCodegen();
    const Codegen = api.import("Codegen").?;

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
    modules.bringUp(api);
    const Codegen = api.import("Codegen").?;
    const Module = api.import("Module").?;

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

    // A missing module fails gracefully (no crash) and is NOT marked loaded. (The
    // #loadFailed error KIND is verified end-to-end via the CLI, where the error
    // module is up — setupWithCodegen omits it to keep failure paths nil-valued.)
    _ = api.send(Module, api.intern("load:"), &.{api.intern("tnope")}, 1);
    try std.testing.expect(api.send(Module, api.intern("isLoaded:"), &.{api.intern("tnope")}, 1) == api.falseObj);
}

test "round-trip: a block-valued global is re-derived from its recorded source" {
    const api = setupWithCodegen();
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

test "§G2: a method REDEFINED in an append re-emits code; the new definition wins on default load" {
    const api = setupWithCodegen();
    const Codegen = api.import("Codegen").?;

    // Base: `bump` adds 1. saveImage establishes the append baseline AND writes a
    // `.code` record compiled from this (V1) source.
    const v1 = "[Reflect defineMethod: #bump on: Integer as: [:x | [x + 1]]]";
    _ = api.send(Codegen, api.intern("eval:"), &.{api.mkString(v1.ptr, v1.len)}, 1);
    try std.testing.expectEqual(@as(i64, 6), api.smallIntValue(send(api.mkSmallInt(5), "bump", &.{})));

    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const a = std.testing.allocator;
    const dir = try tmp.dir.realpathAlloc(a, ".");
    defer a.free(dir);
    const path = try std.fs.path.join(a, &.{ dir, "redef.image" });
    defer a.free(path);

    try std.testing.expect(image.saveImage(path));

    // Delta: REDEFINE `bump` to add 100. The append must re-emit a composing `.code`
    // record (§G2) — not just the new `__methods__` fact — so the image is
    // self-consistent without a --rebuild.
    const v2 = "[Reflect defineMethod: #bump on: Integer as: [:x | [x + 100]]]";
    _ = api.send(Codegen, api.intern("eval:"), &.{api.mkString(v2.ptr, v2.len)}, 1);
    try std.testing.expectEqual(@as(i64, 105), api.smallIntValue(send(api.mkSmallInt(5), "bump", &.{})));
    try std.testing.expect(image.appendImage(path));

    // Default load MAPS code (no --rebuild): base V1 installs first, then the
    // appended V2 install composes over it. The redefinition wins: 5 bump == 105.
    try std.testing.expect(image.loadImage(path));
    try std.testing.expect(image.mappedCode());
    try std.testing.expectEqual(@as(i64, 105), api.smallIntValue(send(api.mkSmallInt(5), "bump", &.{})));

    // Structurally: base + delta, and the delta carried its own `.code` record —
    // more than one `.code` survives validation (a base plus appended code).
    try std.testing.expect(image.validate(path));
}
