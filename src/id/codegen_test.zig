//! Proof for rung 3c (part 2): the birnam → C emitter.
//!
//! Boots the whole front end (peg → lang → tcc → codegen), then evaluates birnam
//! arithmetic expressions: each is parsed, emitted as Method-ABI C, compiled by
//! libtcc, and run — the answer comes back from native code through the id api.

const std = @import("std");
const id = @import("id.zig");
const store = @import("store.zig");
const peg = @import("peg.zig");
const lang = @import("lang.zig");
const tcc = @import("tcc.zig");
const codegen = @import("codegen.zig");
const prelude = @import("prelude.zig");
const collection = @import("collection.zig");
const errmod = @import("error.zig");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;

var gapi: *idapi.IdApi = undefined;

fn setup() *idapi.IdApi {
    const api = id.id_init();
    gapi = api;
    store.bringUp(api); // codegen resolves globals through the store
    peg.bringUp(api);
    lang.bringUp(api);
    tcc.bringUp(api);
    codegen.bringUp(api);
    // The prelude provides the SmallInt protocol. Arithmetic `[a + b]` compiles to
    // a real send now (not inlined C math), so it dispatches here — without the
    // prelude a compiled `+` would have no method to reach.
    prelude.bringUp(api);
    return api;
}

/// `setup` + the collection factories (`Vector`/`Map`) that `#[…]`/`{…}` literals
/// lower onto, and the F7 error model (so `[10 / 0]` yields a #zeroDivide value).
fn setupFull() *idapi.IdApi {
    const api = setup();
    collection.bringUp(api);
    errmod.bringUp(api);
    return api;
}

// Primitive methods to install on smallint_vt so emitted birnam sends have real
// targets to dispatch to (the floor's SmallInt has no protocol of its own).
fn mDouble(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = args;
    _ = n;
    return gapi.mkSmallInt(gapi.smallIntValue(self) * 2);
}
fn mPlus(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return gapi.mkSmallInt(gapi.smallIntValue(self) + gapi.smallIntValue(args[0]));
}
fn mTilde(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n; // non-arithmetic binary op: self*10 + arg, just to be distinguishable
    return gapi.mkSmallInt(gapi.smallIntValue(self) * 10 + gapi.smallIntValue(args[0]));
}
fn mCommitDrift(_: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return gapi.mkSmallInt(77);
}
fn mFromTo(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    return gapi.mkSmallInt(gapi.smallIntValue(args[0]) * 100 + gapi.smallIntValue(args[1]));
}
// `[x with: aBlock]` invokes the block argument as `[aBlock value: x]`.
fn mWith(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return gapi.send(args[0], gapi.intern("value:"), &[_]Oop{self}, 1);
}

fn eval(api: *idapi.IdApi, src: []const u8) Oop {
    const Codegen = api.import("Codegen").?;
    return api.send(Codegen, api.intern("eval:"), &[_]Oop{api.mkString(src.ptr, src.len)}, 1);
}
fn evalInt(api: *idapi.IdApi, src: []const u8) !i64 {
    const r = eval(api, src);
    if (r == api.nilObj) return error.EvalFailed;
    return api.smallIntValue(r);
}
fn evalStr(api: *idapi.IdApi, src: []const u8) ![]const u8 {
    const r = eval(api, src);
    if (r == api.nilObj) return error.EvalFailed;
    var len: usize = 0;
    const p = api.stringBytes(r, &len);
    return p[0..len];
}

fn directTestMethod(api: *idapi.IdApi, vt: Oop, sel: Oop) Oop {
    var i: usize = 0;
    while (api.vtableSelectorAt(vt, i)) |bound| : (i += 1) {
        if (bound == sel) return api.send(vt, api.intern("lookup"), &.{sel}, 1);
    }
    return null;
}

fn vectorAt(api: *idapi.IdApi, vector: Oop, index: i64) Oop {
    return api.send(vector, api.intern("at:"), &.{api.mkSmallInt(index)}, 1);
}

fn stringBytes(api: *idapi.IdApi, string: Oop) []const u8 {
    var len: usize = 0;
    const p = api.stringBytes(string, &len);
    return p[0..len];
}

fn sourceRevision(api: *idapi.IdApi, source: []const u8) Oop {
    return api.send(api.import("Codegen").?, api.intern("sourceRevisionOf:"), &.{api.mkString(source.ptr, source.len)}, 1);
}

test "module metadata scanner accepts literal declarations anywhere and ignores legacy source" {
    const api = setupFull();
    const Codegen = api.import("Codegen").?;
    const source = "// prose first\n[Globals at: #MetadataProbe put: [Object clone]];\n[Module named: #metadataProbe requires: #[#file #time]];\n";
    const metadata = api.send(Codegen, api.intern("moduleMetadataOf:"), &.{api.mkString(source.ptr, source.len)}, 1);
    try std.testing.expect(metadata != api.nilObj);
    try std.testing.expectEqualStrings("metadataProbe", std.mem.span(api.symbolName(vectorAt(api, metadata, 1))));
    const requires = vectorAt(api, metadata, 2);
    try std.testing.expectEqual(@as(i64, 2), api.smallIntValue(api.send(requires, api.intern("size"), &.{}, 0)));

    const legacy = "[Globals at: #LegacyProbe put: [Object clone]];\n";
    const ignored = api.send(Codegen, api.intern("moduleMetadataOf:"), &.{api.mkString(legacy.ptr, legacy.len)}, 1);
    try std.testing.expect(ignored == api.nilObj);
}

test "source revision projects complete documents and isolates method changes" {
    const api = setupFull();
    const before =
        "let bootValue := 1;\n" ++
        "[Module named: #projected requires: #[#core]];\n" ++
        "[Reflect defineMethod: #answer on: Integer as: [:self | 41]];\n" ++
        "[Store assert: #projected attr: #bootValue value: bootValue]";
    const method_edit =
        "let bootValue := 1;\n" ++
        "[Module named: #projected requires: #[#core]];\n" ++
        "[Reflect defineMethod: #answer on: Integer as: [:self | 42]];\n" ++
        "[Store assert: #projected attr: #bootValue value: bootValue]";
    const top_level_edit =
        "let bootValue := 2;\n" ++
        "[Module named: #projected requires: #[#core]];\n" ++
        "[Reflect defineMethod: #answer on: Integer as: [:self | 42]];\n" ++
        "[Store assert: #projected attr: #bootValue value: bootValue]";

    const r0 = sourceRevision(api, before);
    const r1 = sourceRevision(api, method_edit);
    const r2 = sourceRevision(api, top_level_edit);
    try std.testing.expect(vectorAt(api, r0, 0) == api.intern("sourceRevision"));
    try std.testing.expect(vectorAt(api, r0, 1) == api.intern("projected"));
    try std.testing.expectEqualStrings(before, stringBytes(api, vectorAt(api, r0, 3)));
    try std.testing.expectEqualStrings(stringBytes(api, vectorAt(api, r0, 4)), stringBytes(api, vectorAt(api, r1, 4)));
    try std.testing.expect(!std.mem.eql(u8, stringBytes(api, vectorAt(api, r0, 5)), stringBytes(api, vectorAt(api, r1, 5))));
    try std.testing.expect(!std.mem.eql(u8, stringBytes(api, vectorAt(api, r0, 6)), stringBytes(api, vectorAt(api, r1, 6))));
    try std.testing.expect(!std.mem.eql(u8, stringBytes(api, vectorAt(api, r1, 4)), stringBytes(api, vectorAt(api, r2, 4))));
}

test "source revision accepts the real module manager document with declaration last" {
    const api = setupFull();
    const bytes = try std.fs.cwd().readFileAlloc(std.testing.allocator, "src/id/modules.birnam", 1 << 20);
    defer std.testing.allocator.free(bytes);
    const revision = sourceRevision(api, bytes);
    try std.testing.expect(api.send(revision, api.intern("isError"), &.{}, 0) != api.trueObj);
    try std.testing.expect(vectorAt(api, revision, 1) == api.intern("modules"));
    try std.testing.expectEqualStrings(bytes, stringBytes(api, vectorAt(api, revision, 3)));
}

test "codegen 3c.2: birnam arithmetic → C → native → value" {
    const api = setup();

    try std.testing.expectEqual(@as(i64, 42), try evalInt(api, "42"));
    try std.testing.expectEqual(@as(i64, 7), try evalInt(api, "[3 + 4]"));
    try std.testing.expectEqual(@as(i64, 14), try evalInt(api, "[[3 + 4] * 2]"));
    try std.testing.expectEqual(@as(i64, 5), try evalInt(api, "[[10 - 3] - 2]"));
    try std.testing.expectEqual(@as(i64, 25), try evalInt(api, "[100 / 4]"));
    try std.testing.expectEqual(@as(i64, -8), try evalInt(api, "[-10 + 2]"));

    // Deeper nesting recurses correctly.
    try std.testing.expectEqual(@as(i64, 20), try evalInt(api, "[[2 * 3] + [7 * 2]]"));
}

test "Reflect removeMethod:on: removes only an existing stable direct binding" {
    const api = setupFull();
    const target = api.send(api.import("Object").?, api.intern("clone"), &.{}, 0);
    _ = api.send(store.globals_obj, api.intern("at:put:"), &[_]Oop{ api.intern("RemovalTarget"), target }, 2);

    const defined = eval(api, "[Reflect defineMethod: #answer on: RemovalTarget as: [:self | 42]]");
    try std.testing.expect(defined != api.nilObj);
    try std.testing.expectEqual(@as(i64, 42), try evalInt(api, "[RemovalTarget answer]"));

    const vt = api.vtableOf(target);
    const removed = eval(api, "[Reflect removeMethod: #answer on: RemovalTarget]");
    try std.testing.expect(removed == store.store_instance);
    try std.testing.expect(api.send(store.store_instance, api.intern("rawCurrentValue:attr:"), &[_]Oop{ vt, api.intern("answer") }, 2) == null);

    const unstable = api.send(api.import("Object").?, api.intern("clone"), &.{}, 0);
    _ = api.send(store.globals_obj, api.intern("at:put:"), &[_]Oop{ api.intern("UnstableRemovalTarget"), unstable }, 2);
    const err = eval(api, "[Reflect removeMethod: #answer on: UnstableRemovalTarget]");
    try std.testing.expect(api.send(err, api.intern("isError"), &.{}, 0) == api.trueObj);
    try std.testing.expect(api.send(err, api.intern("kind"), &.{}, 0) == api.intern("unstableMethodTarget"));
}

test "CompiledUnit candidate stages add replace remove and rolls back exactly" {
    const api = setupFull();
    const Codegen = api.import("Codegen").?;
    _ = eval(api, "[Reflect defineMethod: #cuReplace on: Integer as: [:self | 1]]; [Reflect defineMethod: #cuRemove on: Integer as: [:self | 2]]");

    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const dir = try tmp.dir.realpathAlloc(std.testing.allocator, ".");
    defer std.testing.allocator.free(dir);
    const path = try std.fs.path.join(std.testing.allocator, &.{ dir, "candidate.dylib" });
    defer std.testing.allocator.free(path);
    const source =
        "let candidateMetadata := 7; " ++
        "[Module named: #candidateTest requires: #[]]; " ++
        "[Reflect defineMethod: #cuReplace on: Integer as: [:self | 100]]; " ++
        "[Reflect defineMethod: #cuAdd on: Integer as: [:self | 200]]; " ++
        "[Reflect removeMethod: #cuRemove on: Integer]";
    const compiled = api.send(Codegen, api.intern("compileCandidate:named:output:"), &.{
        api.mkString(source.ptr, source.len), api.intern("candidateTest"), api.mkString(path.ptr, path.len),
    }, 3);
    try std.testing.expect(compiled != api.nilObj);
    try std.testing.expect(api.send(compiled, api.intern("isError"), &.{}, 0) != api.trueObj);

    const unit = api.send(api.import("Object").?, api.intern("clone"), &.{}, 0);
    const inspected = api.send(Codegen, api.intern("inspectCandidate:into:"), &.{ api.mkString(path.ptr, path.len), unit }, 2);
    try std.testing.expect(inspected == unit);
    try std.testing.expectEqual(@as(i64, 1), try evalInt(api, "[3 cuReplace]"));
    try std.testing.expectEqual(@as(i64, 2), try evalInt(api, "[3 cuRemove]"));

    const mapped = api.send(Codegen, api.intern("mapCandidate:"), &.{unit}, 1);
    try std.testing.expect(mapped == unit);
    try std.testing.expectEqual(@as(i64, 1), try evalInt(api, "[3 cuReplace]"));

    const abandoned = api.send(Codegen, api.intern("prepareActivation:"), &.{unit}, 1);
    try std.testing.expect(api.send(Codegen, api.intern("discardPrepared:"), &.{abandoned}, 1) == api.trueObj);
    const abandoned_commit = api.send(Codegen, api.intern("commitPrepared:"), &.{abandoned}, 1);
    try std.testing.expect(api.send(abandoned_commit, api.intern("kind"), &.{}, 0) == api.intern("invalidPreparedToken"));
    try std.testing.expectEqual(@as(i64, 1), try evalInt(api, "[3 cuReplace]"));

    const token = api.send(Codegen, api.intern("prepareActivation:"), &.{unit}, 1);
    try std.testing.expect(token != api.nilObj);
    try std.testing.expect(api.send(token, api.intern("isError"), &.{}, 0) != api.trueObj);
    const committed = api.send(Codegen, api.intern("commitPrepared:"), &.{token}, 1);
    try std.testing.expect(api.send(committed, api.intern("isError"), &.{}, 0) != api.trueObj);
    const integer_vt = api.vtableOf(api.import("Integer").?);
    const installed = directTestMethod(api, integer_vt, api.intern("cuReplace"));
    try std.testing.expectEqual(@as(i64, 100), try evalInt(api, "[3 cuReplace]"));
    try std.testing.expect(installed == directTestMethod(api, integer_vt, api.intern("cuReplace")));
    try std.testing.expectEqual(@as(i64, 200), try evalInt(api, "[3 cuAdd]"));
    try std.testing.expect(installed == directTestMethod(api, integer_vt, api.intern("cuReplace")));
    const removed_send = api.send(api.mkSmallInt(3), api.intern("cuRemove"), &.{}, 0);
    try std.testing.expect(api.send(removed_send, api.intern("isError"), &.{}, 0) == api.trueObj);

    const rollback = api.send(Codegen, api.intern("prepareRollback:"), &.{unit}, 1);
    try std.testing.expect(rollback != api.nilObj);
    try std.testing.expect(api.send(rollback, api.intern("isError"), &.{}, 0) != api.trueObj);
    const rolled = api.send(Codegen, api.intern("commitPreparedRollback:"), &.{rollback}, 1);
    try std.testing.expect(rolled == unit);
    try std.testing.expectEqual(@as(i64, 1), try evalInt(api, "[3 cuReplace]"));
    try std.testing.expectEqual(@as(i64, 2), try evalInt(api, "[3 cuRemove]"));
    var found_add = false;
    var i: usize = 0;
    while (api.vtableSelectorAt(api.vtableOf(api.mkSmallInt(0)), i)) |sel| : (i += 1) {
        if (sel == api.intern("cuAdd")) found_add = true;
    }
    try std.testing.expect(!found_add);

    // The symmetric stale check protects an intervening supported live edit.
    const token2 = api.send(Codegen, api.intern("prepareActivation:"), &.{unit}, 1);
    _ = api.send(Codegen, api.intern("commitPrepared:"), &.{token2}, 1);
    _ = eval(api, "[Reflect defineMethod: #cuUnrelated on: Integer as: [:self | 77]]");
    try std.testing.expect(api.send(store.store_instance, api.intern("currentValue:attr:"), &.{ unit, api.intern("state") }, 2) == api.intern("active"));
    _ = eval(api, "[Reflect defineMethod: #cuReplace on: Integer as: [:self | 999]]");
    // No state query or rollback reconciliation is needed: Store's completed
    // tracked-MOP observer marks the managed generation immediately.
    try std.testing.expect(api.send(store.store_instance, api.intern("currentValue:attr:"), &.{ unit, api.intern("state") }, 2) == api.intern("drifted"));
    try std.testing.expect(api.send(store.store_instance, api.intern("currentValue:attr:"), &.{ unit, api.intern("rollbackAvailable") }, 2) == api.falseObj);
    const stale_rollback = api.send(Codegen, api.intern("prepareRollback:"), &.{unit}, 1);
    try std.testing.expect(api.send(stale_rollback, api.intern("isError"), &.{}, 0) == api.trueObj);
    try std.testing.expect(api.send(stale_rollback, api.intern("kind"), &.{}, 0) == api.intern("candidateDrifted"));
    try std.testing.expectEqual(@as(i64, 999), try evalInt(api, "[3 cuReplace]"));
}

test "CompiledUnit exported validator rejects a mechanically invalid mapped candidate" {
    const api = setupFull();
    const Codegen = api.import("Codegen").?;
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const dir = try tmp.dir.realpathAlloc(std.testing.allocator, ".");
    defer std.testing.allocator.free(dir);
    const path = try std.fs.path.join(std.testing.allocator, &.{ dir, "invalid-candidate.dylib" });
    defer std.testing.allocator.free(path);
    const source = "[Module named: #invalidCandidate requires: #[]]; [Reflect removeMethod: #cuDefinitelyAbsent on: Integer]";
    const compiled = api.send(Codegen, api.intern("compileCandidate:named:output:"), &.{
        api.mkString(source.ptr, source.len), api.intern("invalidCandidate"), api.mkString(path.ptr, path.len),
    }, 3);
    try std.testing.expect(api.send(compiled, api.intern("isError"), &.{}, 0) != api.trueObj);
    const unit = api.send(api.import("Object").?, api.intern("clone"), &.{}, 0);
    try std.testing.expect(api.send(Codegen, api.intern("inspectCandidate:into:"), &.{ api.mkString(path.ptr, path.len), unit }, 2) == unit);
    const mapped = api.send(Codegen, api.intern("mapCandidate:"), &.{unit}, 1);
    try std.testing.expect(api.send(mapped, api.intern("isError"), &.{}, 0) == api.trueObj);
    try std.testing.expect(api.send(mapped, api.intern("kind"), &.{}, 0) == api.intern("candidateValidationFailed"));
}

test "prepared candidate rejects commit-time binding drift without partial publication" {
    const api = setupFull();
    const Codegen = api.import("Codegen").?;
    _ = eval(api, "[Reflect defineMethod: #cuCommitDrift on: Integer as: [:self | 10]]");

    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const dir = try tmp.dir.realpathAlloc(std.testing.allocator, ".");
    defer std.testing.allocator.free(dir);
    const path = try std.fs.path.join(std.testing.allocator, &.{ dir, "commit-drift.dylib" });
    defer std.testing.allocator.free(path);
    const source = "[Module named: #commitDriftTest requires: #[]]; [Reflect defineMethod: #cuCommitDrift on: Integer as: [:self | 20]]";
    const compiled = api.send(Codegen, api.intern("compileCandidate:named:output:"), &.{
        api.mkString(source.ptr, source.len), api.intern("commitDriftTest"), api.mkString(path.ptr, path.len),
    }, 3);
    try std.testing.expect(api.send(compiled, api.intern("isError"), &.{}, 0) != api.trueObj);
    const unit = api.send(api.import("Object").?, api.intern("clone"), &.{}, 0);
    try std.testing.expect(api.send(Codegen, api.intern("inspectCandidate:into:"), &.{ api.mkString(path.ptr, path.len), unit }, 2) == unit);
    try std.testing.expect(api.send(Codegen, api.intern("mapCandidate:"), &.{unit}, 1) == unit);
    const token = api.send(Codegen, api.intern("prepareActivation:"), &.{unit}, 1);
    try std.testing.expect(api.send(token, api.intern("isError"), &.{}, 0) != api.trueObj);

    api.method(api.vtableOf(api.mkSmallInt(0)), api.intern("cuCommitDrift"), &mCommitDrift);
    const rejected = api.send(Codegen, api.intern("commitPrepared:"), &.{token}, 1);
    try std.testing.expect(api.send(rejected, api.intern("isError"), &.{}, 0) == api.trueObj);
    try std.testing.expect(api.send(rejected, api.intern("kind"), &.{}, 0) == api.intern("candidateStaleBase"));
    try std.testing.expectEqual(@as(i64, 77), try evalInt(api, "[3 cuCommitDrift]"));
}

test "CompiledUnit inspection rejects unsafe and checksum-mismatched artifacts before mapping" {
    const api = setupFull();
    const Codegen = api.import("Codegen").?;
    _ = eval(api, "[Reflect defineMethod: #cuIntegrity on: Integer as: [:self | 5]]");

    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const dir = try tmp.dir.realpathAlloc(std.testing.allocator, ".");
    defer std.testing.allocator.free(dir);
    const path = try std.fs.path.join(std.testing.allocator, &.{ dir, "integrity.dylib" });
    defer std.testing.allocator.free(path);
    const source = "[Module named: #integrityTest requires: #[]]; [Reflect defineMethod: #cuIntegrity on: Integer as: [:self | 55]]";
    const source_oop = api.mkString(source.ptr, source.len);
    const path_oop = api.mkString(path.ptr, path.len);
    const name = api.intern("integrityTest");
    const compiled = api.send(Codegen, api.intern("compileCandidate:named:output:"), &.{ source_oop, name, path_oop }, 3);
    try std.testing.expect(api.send(compiled, api.intern("isError"), &.{}, 0) != api.trueObj);

    var bytes = try std.fs.cwd().readFileAlloc(std.testing.allocator, path, 1 << 30);
    defer std.testing.allocator.free(bytes);
    bytes[0] ^= 1;
    try std.fs.cwd().writeFile(.{ .sub_path = path, .data = bytes });
    const unsafe_unit = api.send(api.import("Object").?, api.intern("clone"), &.{}, 0);
    const unsafe = api.send(Codegen, api.intern("inspectCandidate:into:"), &.{ path_oop, unsafe_unit }, 2);
    try std.testing.expect(api.send(unsafe, api.intern("kind"), &.{}, 0) == api.intern("unsafeCandidateArtifact"));
    try std.testing.expectEqual(@as(i64, 5), try evalInt(api, "[3 cuIntegrity]"));

    _ = api.send(Codegen, api.intern("compileCandidate:named:output:"), &.{ source_oop, name, path_oop }, 3);
    std.testing.allocator.free(bytes);
    bytes = try std.fs.cwd().readFileAlloc(std.testing.allocator, path, 1 << 30);
    bytes[bytes.len - 1] ^= 1;
    try std.fs.cwd().writeFile(.{ .sub_path = path, .data = bytes });
    const mismatch_unit = api.send(api.import("Object").?, api.intern("clone"), &.{}, 0);
    const mismatch = api.send(Codegen, api.intern("inspectCandidate:into:"), &.{ path_oop, mismatch_unit }, 2);
    try std.testing.expect(api.send(mismatch, api.intern("kind"), &.{}, 0) == api.intern("candidateChecksumMismatch"));
    try std.testing.expectEqual(@as(i64, 5), try evalInt(api, "[3 cuIntegrity]"));
}

test "codegen: multi-char binary operators parse and dispatch (>=, <=)" {
    const api = setupFull(); // prelude has >= <= on Integer, & on Boolean

    try std.testing.expect(eval(api, "[5 >= 3]") == api.trueObj);
    try std.testing.expect(eval(api, "[3 >= 5]") == api.falseObj);
    try std.testing.expect(eval(api, "[5 <= 5]") == api.trueObj);
    try std.testing.expect(eval(api, "[6 <= 5]") == api.falseObj);
    // Combined with single-char ops and a boolean `&` (a between-style test).
    try std.testing.expect(eval(api, "[[4 >= 1] & [4 <= 10]]") == api.trueObj);
    try std.testing.expect(eval(api, "[[12 >= 1] & [12 <= 10]]") == api.falseObj);
}

test "codegen §G9: extended binary operators (% @ ^) dispatch as custom selectors" {
    const api = setup();

    // A user-defined binary method whose selector is one of the newly-admitted
    // operator chars (% @ ! ^) dispatches like any other — the charset in lang.zig
    // was the only gate; dispatch already handles any interned selector.
    _ = eval(api, "[Reflect defineMethod: #% on: Integer as: [:x :y | [[x * y] + 1]]]");
    try std.testing.expectEqual(@as(i64, 31), try evalInt(api, "[10 % 3]")); // 10*3+1

    _ = eval(api, "[Reflect defineMethod: #@ on: Integer as: [:x :y | [x - y]]]");
    try std.testing.expectEqual(@as(i64, 4), try evalInt(api, "[7 @ 3]"));

    // A RUN of operator chars is one selector, distinct from its single-char pieces.
    _ = eval(api, "[Reflect defineMethod: #@% on: Integer as: [:x :y | [x + y]]]");
    try std.testing.expectEqual(@as(i64, 9), try evalInt(api, "[4 @% 5]"));
}

test "codegen: arithmetic is dispatched — zeroDivide is a value and propagates" {
    const api = setupFull(); // prelude + collection + error model (F7)

    // `[10 / 0]` reaches the prelude's guarded `/`, yielding a #zeroDivide error
    // value — not undefined behavior from untagging-and-dividing in raw C.
    const e = eval(api, "[10 / 0]");
    try std.testing.expect(api.send(e, api.intern("isError"), &[_]Oop{}, 0) == api.trueObj);
    try std.testing.expect(api.send(e, api.intern("kind"), &[_]Oop{}, 0) == api.intern("zeroDivide"));

    // The error flows THROUGH further arithmetic (doesNotUnderstand: returns self),
    // rather than being untagged into garbage as the old inline path would.
    const e2 = eval(api, "[[10 / 0] + 1]");
    try std.testing.expect(api.send(e2, api.intern("isError"), &[_]Oop{}, 0) == api.trueObj);
    try std.testing.expect(api.send(e2, api.intern("kind"), &[_]Oop{}, 0) == api.intern("zeroDivide"));

    // Ordinary arithmetic still works end to end through the same dispatch path.
    try std.testing.expectEqual(@as(i64, 25), try evalInt(api, "[100 / 4]"));
}

test "codegen: evalPersisting: top-level lets survive across evaluations (REPL)" {
    const api = setupFull();
    const Codegen = api.import("Codegen").?;
    const evalP = struct {
        fn e(a: *idapi.IdApi, cg: Oop, src: []const u8) Oop {
            return a.send(cg, a.intern("evalPersisting:"), &[_]Oop{a.mkString(src.ptr, src.len)}, 1);
        }
    }.e;

    // Bind in one evaluation, use in the next — the binding persists as a global.
    _ = evalP(api, Codegen, "let x := 40");
    try std.testing.expectEqual(@as(i64, 42), api.smallIntValue(evalP(api, Codegen, "[x + 2]")));
    // Rebinding updates it; a later line sees the new value.
    _ = evalP(api, Codegen, "let x := [x * 2]"); // x := 80
    try std.testing.expectEqual(@as(i64, 80), api.smallIntValue(evalP(api, Codegen, "x")));

    // Plain `eval:` does NOT persist (a block-local let stays local), so this name
    // is undeclared → nil.
    try std.testing.expect(eval(api, "y") == api.nilObj);
    _ = eval(api, "let y := 5"); // eval: (non-persisting)
    try std.testing.expect(eval(api, "y") == api.nilObj); // still undeclared next time
}

test "codegen: parse failure / empty / undeclared global all yield nil (no crash)" {
    const api = setup();

    try std.testing.expect(eval(api, "@#$") == api.nilObj); // parse failure
    try std.testing.expect(eval(api, "") == api.nilObj); // empty program
    try std.testing.expect(eval(api, "hello") == api.nilObj); // undeclared global → nil
}

// Counter primitives for the whileTrue:/whileFalse: proofs: observable loop
// state without the relational store (the grammar has no local re-assignment,
// so a birnam-side loop must mutate SOMETHING these tests can read back).
var g_loop_counter: i64 = 0;
fn mCounterBump(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = args;
    _ = n;
    g_loop_counter += 1;
    return self;
}
// `[n counterBelow]` — true while the counter has not reached the receiver.
fn mCounterBelow(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = args;
    _ = n;
    return if (g_loop_counter < gapi.smallIntValue(self)) gapi.trueObj else gapi.falseObj;
}
// `[n counterReached]` — the whileFalse: mirror (false until the counter gets there).
fn mCounterReached(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = args;
    _ = n;
    return if (g_loop_counter >= gapi.smallIntValue(self)) gapi.trueObj else gapi.falseObj;
}

test "prelude whileTrue:: a native loop — flat stack across 100k iterations" {
    const api = setupFull();
    api.method(api.smallint_vt, api.intern("bumpCounter"), &mCounterBump);
    api.method(api.smallint_vt, api.intern("counterBelow"), &mCounterBelow);
    api.method(api.smallint_vt, api.intern("counterReached"), &mCounterReached);

    // 100k iterations: far past any recursion depth the native stack survives,
    // so completing at all proves the loop frame is flat. Answers nil.
    g_loop_counter = 0;
    const r = eval(api, "[[| [100000 counterBelow]] whileTrue: [| [0 bumpCounter]]]");
    try std.testing.expect(r == api.nilObj);
    try std.testing.expectEqual(@as(i64, 100_000), g_loop_counter);

    // A condition that is false on arrival never runs the body.
    g_loop_counter = 0;
    _ = eval(api, "[[| [0 counterBelow]] whileTrue: [| [0 bumpCounter]]]");
    try std.testing.expectEqual(@as(i64, 0), g_loop_counter);

    // whileFalse: mirrors — loop until the condition turns true.
    g_loop_counter = 0;
    _ = eval(api, "[[| [5 counterReached]] whileFalse: [| [0 bumpCounter]]]");
    try std.testing.expectEqual(@as(i64, 5), g_loop_counter);
}

test "prelude whileTrue:: error semantics — cond must be boolean, errors propagate" {
    const api = setupFull();
    api.method(api.smallint_vt, api.intern("counterBelow"), &mCounterBelow);

    // A non-boolean condition answers a #mustBeBoolean error (not a silent exit,
    // not a loop) — the same strictness a non-boolean gets from ifTrue: (DNU).
    const e = eval(api, "[[| 42] whileTrue: [| 0]]");
    try std.testing.expect(api.send(e, api.intern("isError"), &[_]Oop{}, 0) == api.trueObj);
    try std.testing.expect(api.send(e, api.intern("kind"), &[_]Oop{}, 0) == api.intern("mustBeBoolean"));

    // An error VALUE from the condition propagates as itself (Result-style).
    const e2 = eval(api, "[[| [nil fooBar]] whileTrue: [| 0]]");
    try std.testing.expect(api.send(e2, api.intern("kind"), &[_]Oop{}, 0) == api.intern("doesNotUnderstand"));

    // A body that answers an error stops the loop and propagates it — the
    // Result-style reading of Smalltalk's signal-aborts-the-loop. (Completing
    // this eval at all also proves the error broke what is otherwise an
    // infinite `[| true]` loop.)
    g_loop_counter = 0;
    const e3 = eval(api, "[[| [10 counterBelow]] whileTrue: [| [1 / 0]]]");
    try std.testing.expect(api.send(e3, api.intern("kind"), &[_]Oop{}, 0) == api.intern("zeroDivide"));
}

test "prelude whileTrue:: an overridable send — defineMethod on: Block wins" {
    const api = setupFull();

    // The docs/41 lock: control flow is real sends, never codegen special forms.
    // Overriding the family method changes what EVERY block's whileTrue: does.
    _ = eval(api, "[Reflect defineMethod: #whileTrue: on: Block as: [:c :b | 99]]");
    try std.testing.expectEqual(@as(i64, 99), try evalInt(api, "[[| true] whileTrue: [| 0]]"));
}

test "codegen 3c.3: sends dispatch through libid (unary / binary / keyword)" {
    const api = setup();

    // Give SmallInt a protocol to dispatch to.
    api.method(api.smallint_vt, api.intern("double"), &mDouble);
    api.method(api.smallint_vt, api.intern("plus:"), &mPlus);
    api.method(api.smallint_vt, api.intern("~"), &mTilde);
    api.method(api.smallint_vt, api.intern("from:to:"), &mFromTo);

    // unary
    try std.testing.expectEqual(@as(i64, 42), try evalInt(api, "[21 double]"));
    // keyword, one arg
    try std.testing.expectEqual(@as(i64, 42), try evalInt(api, "[20 plus: 22]"));
    // keyword, two parts → selector "from:to:", two args
    try std.testing.expectEqual(@as(i64, 102), try evalInt(api, "[0 from: 1 to: 2]"));
    // non-arithmetic binary op routes through send (not the inline fast path)
    try std.testing.expectEqual(@as(i64, 53), try evalInt(api, "[5 ~ 3]"));

    // Receivers and args may themselves be sends or inline arithmetic.
    try std.testing.expectEqual(@as(i64, 42), try evalInt(api, "[[21 double] plus: 0]"));
    try std.testing.expectEqual(@as(i64, 22), try evalInt(api, "[20 plus: [1 + 1]]"));
    try std.testing.expectEqual(@as(i64, 84), try evalInt(api, "[[20 plus: 22] double]"));
}

test "codegen 3c.3: let-bindings, identifiers, and multi-statement programs" {
    const api = setup();
    api.method(api.smallint_vt, api.intern("double"), &mDouble);
    api.method(api.smallint_vt, api.intern("plus:"), &mPlus);

    // let introduces a variable; later statements see it; value = last statement.
    try std.testing.expectEqual(@as(i64, 21), try evalInt(api, "let x := 21"));
    try std.testing.expectEqual(@as(i64, 42), try evalInt(api, "let x := 21; [x double]"));
    try std.testing.expectEqual(@as(i64, 42), try evalInt(api, "let x := 20; let y := 22; [x plus: y]"));
    // a binding's RHS may reference earlier bindings
    try std.testing.expectEqual(@as(i64, 84), try evalInt(api, "let x := 21; let y := [x double]; [y double]"));
    // identifier as both receiver and argument
    try std.testing.expectEqual(@as(i64, 10), try evalInt(api, "let x := 5; [x plus: x]"));

    // an undeclared identifier resolves to a nil global (forward/undeclared ref)
    try std.testing.expect(eval(api, "y") == api.nilObj);
}

test "codegen 3c.3: string literals construct String objects" {
    const api = setup();

    try std.testing.expectEqualStrings("hello", try evalStr(api, "\"hello\""));
    try std.testing.expectEqualStrings("", try evalStr(api, "\"\""));
    // chars that must be escaped into the emitted C literal but ARE valid in a
    // birnam string (the grammar allows any byte but `"`): backslash, newline.
    try std.testing.expectEqualStrings("a\\c", try evalStr(api, "\"a\\c\""));
    try std.testing.expectEqualStrings("a\nb", try evalStr(api, "\"a\nb\""));
    // strings flow through let + are returned as the program value
    try std.testing.expectEqualStrings("hi", try evalStr(api, "let s := \"hi\"; s"));
}

test "codegen 3c.3: globals — define, reference from native, late binding" {
    const api = setup();
    api.method(api.smallint_vt, api.intern("double"), &mDouble);
    api.method(api.smallint_vt, api.intern("plus:"), &mPlus);

    // Define a global via the Globals facade (an ordinary send), then reference
    // it as a bare identifier from compiled native code.
    try std.testing.expectEqual(@as(i64, 21), try evalInt(api, "[Globals at: #x put: 21]"));
    try std.testing.expectEqual(@as(i64, 21), try evalInt(api, "x"));
    try std.testing.expectEqual(@as(i64, 42), try evalInt(api, "[x double]"));

    // Late binding: one compiled block reads x through its binding on each call,
    // so it sees a redefinition that happens between calls.
    try std.testing.expectEqual(@as(i64, 3), try evalInt(api, "let g := [| x]; [Globals at: #x put: 1]; let a := [g value]; [Globals at: #x put: 2]; let b := [g value]; [a plus: b]"));

    // A global can hold a block; fetch and invoke it.
    try std.testing.expectEqual(@as(i64, 42), try evalInt(api, "[Globals at: #f put: [:n | [n double]]]; [[Globals at: #f] value: 21]"));
}

test "codegen 3c.3: symbol literals intern to selectors" {
    const api = setup();
    try std.testing.expect(eval(api, "#double") == api.intern("double"));
    try std.testing.expect(eval(api, "#plus:") == api.intern("plus:"));
    try std.testing.expect(eval(api, "#fooWithBar:andBaz:andQuux:") == api.intern("fooWithBar:andBaz:andQuux:"));
    try std.testing.expect(eval(api, "#+") == api.intern("+"));
}

test "codegen 3c.3: blocks compile to closures (params, capture, invoke)" {
    const api = setup();
    api.method(api.smallint_vt, api.intern("double"), &mDouble);
    api.method(api.smallint_vt, api.intern("plus:"), &mPlus);

    // one-param block, invoked with value:
    try std.testing.expectEqual(@as(i64, 42), try evalInt(api, "let f := [:n | [n double]]; [f value: 21]"));
    // zero-param block capturing an enclosing variable, invoked with value
    try std.testing.expectEqual(@as(i64, 42), try evalInt(api, "let x := 21; let f := [| [x double]]; [f value]"));
    // param + capture together
    try std.testing.expectEqual(@as(i64, 42), try evalInt(api, "let x := 10; let f := [:y | [x plus: y]]; [f value: 32]"));
    // two params → selector value:value:
    try std.testing.expectEqual(@as(i64, 42), try evalInt(api, "let g := [:a :b | [a plus: b]]; [g value: 20 value: 22]"));
    // a block is a value: stored, then invoked later via an identifier
    try std.testing.expectEqual(@as(i64, 84), try evalInt(api, "let x := 21; let f := [| [x double]]; let r := [f value]; [r double]"));
    // nested blocks: inner captures the outer param transitively
    try std.testing.expectEqual(@as(i64, 42), try evalInt(api, "let mk := [:a | [:b | [a plus: b]]]; let add20 := [mk value: 20]; [add20 value: 22]"));

    // a block literal passed directly as a message argument (a statement-
    // expression nested inside the call's compound-literal arg array)
    api.method(api.smallint_vt, api.intern("with:"), &mWith);
    try std.testing.expectEqual(@as(i64, 42), try evalInt(api, "[21 with: [:x | [x double]]]"));
    // capturing block passed as an argument: [20 with: blk] → [[20+2] double] = 44
    try std.testing.expectEqual(@as(i64, 44), try evalInt(api, "let k := 2; [20 with: [:x | [[x plus: k] double]]]"));
}

test "codegen §E2: the defineMethod:on:as: intercept is gated on the Reflect receiver" {
    const api = setupFull();
    // A non-Reflect object gets its OWN 3-arg defineMethod:on:as:. Were the compiler
    // still intercepting receiver-blind, `[fake defineMethod:…]` would be rewritten to
    // the 4-arg `…:source:` form and dispatch to a selector `fake` lacks → a DNU error
    // (an error value, whose smallIntValue is not 42), rather than running fake's method.
    const prog =
        \\let fake := [Object clone];
        \\[Reflect defineMethod: #defineMethod:on:as: on: fake as: [:self :a :b :c | 42]];
        \\[fake defineMethod: #x on: #y as: #z]
    ;
    try std.testing.expectEqual(@as(i64, 42), try evalInt(api, prog));
    // Control: the real `[Reflect defineMethod:…]` still installs (source-recorded) and
    // the method runs — the receiver-gated intercept didn't break the Reflect path.
    api.method(api.smallint_vt, api.intern("plus:"), &mPlus);
    const prog2 =
        \\[Reflect defineMethod: #triple on: Integer as: [:self | [[self plus: self] plus: self]]];
        \\[14 triple]
    ;
    try std.testing.expectEqual(@as(i64, 42), try evalInt(api, prog2));
}

test "codegen §B9: lastError reflects only the top-level eval, cleared on success" {
    const api = setupFull();
    const Codegen = api.import("Codegen").?;
    const lastErr = api.intern("lastError");

    // A failed top-level compile records an error.
    _ = eval(api, "let :=");
    try std.testing.expect(api.send(Codegen, lastErr, &[_]Oop{}, 0) != api.nilObj);

    // A subsequent SUCCESSFUL eval clears it — so a program that internally probes
    // a bad compile (aot.birnam) can't leave a false "compile error" for the CLI's
    // post-eval check. (A failed top-level compile returns before the clear, so its
    // note survives.)
    try std.testing.expectEqual(@as(i64, 7), try evalInt(api, "[3 + 4]"));
    try std.testing.expect(api.send(Codegen, lastErr, &[_]Oop{}, 0) == api.nilObj);
}

test "docs/42 §D1: backfillMirror makes the MOP total over the native protocol" {
    const api = setupFull();
    store.backfillMirror(api);
    const primitive = api.intern("primitive");
    const string_vt = api.string_vt;

    // A native type now reflects its protocol through the fact store: enumerate its
    // selectors, and each native binding reads back as the GC-safe #primitive marker.
    const nsel: usize = @intCast(api.smallIntValue(api.send(store.store_instance, store.s_attrCount, &[_]Oop{string_vt}, 1)));
    try std.testing.expect(nsel > 0);
    const sel0 = api.send(store.store_instance, store.s_attrAt, &[_]Oop{ string_vt, api.mkSmallInt(0) }, 2);
    try std.testing.expect(api.send(store.store_instance, store.s_currentValue, &[_]Oop{ string_vt, sel0 }, 2) == primitive);

    // Idempotent: a second backfill changes nothing (assert-if-absent).
    store.backfillMirror(api);
    const nsel2: usize = @intCast(api.smallIntValue(api.send(store.store_instance, store.s_attrCount, &[_]Oop{string_vt}, 1)));
    try std.testing.expectEqual(nsel, nsel2);

    // Internal dicts (the intern table / export env) are NOT types — they delegate
    // to nothing — so their data keys never become mirror facts.
}

test "Reflect enumerates effective methods and current facts without raw pointers" {
    const api = setupFull();
    store.backfillMirror(api);
    const Reflect = api.import("Reflect").?;

    codegen.resetReflectTargetResolutionCountForTesting();
    const methods = api.send(Reflect, api.intern("allMethodsOf:"), &[_]Oop{api.import("String").?}, 1);
    try std.testing.expect(api.smallIntValue(api.send(methods, api.intern("size"), &.{}, 0)) > 0);
    var saw_clone = false;
    var selectors: std.ArrayList(Oop) = .{};
    defer selectors.deinit(std.heap.c_allocator);
    const count = api.smallIntValue(api.send(methods, api.intern("size"), &.{}, 0));
    try std.testing.expect(codegen.reflectTargetResolutionCountForTesting() < count);
    try std.testing.expectEqual(@as(usize, 1), codegen.reflectGlobalSnapshotCountForTesting());
    var i: i64 = 0;
    while (i < count) : (i += 1) {
        const descriptor = vectorAt(api, methods, i);
        const selector = api.send(descriptor, api.intern("at:"), &[_]Oop{api.intern("selector")}, 1);
        for (selectors.items) |seen| try std.testing.expect(seen != selector);
        selectors.append(std.heap.c_allocator, selector) catch @panic("test: oom");
        if (selector == api.intern("clone")) {
            saw_clone = true;
            try std.testing.expect(api.send(descriptor, api.intern("at:"), &[_]Oop{api.intern("method")}, 1) == api.intern("primitive"));
            try std.testing.expect(api.send(Reflect, api.intern("methodSource:"), &[_]Oop{descriptor}, 1) == api.nilObj);
        }
    }
    try std.testing.expect(saw_clone);

    const person = api.send(api.import("Object").?, api.intern("clone"), &.{}, 0);
    const name = api.intern("name");
    const ada = api.mkString("Ada", 3);
    _ = api.send(store.store_instance, api.intern("assert:attr:value:"), &[_]Oop{ person, name, ada }, 3);
    const facts = api.send(Reflect, api.intern("factsOf:"), &[_]Oop{person}, 1);
    try std.testing.expectEqual(@as(i64, 1), api.smallIntValue(api.send(facts, api.intern("size"), &.{}, 0)));
    const fact = vectorAt(api, facts, 0);
    try std.testing.expect(api.send(fact, api.intern("at:"), &[_]Oop{api.intern("entity")}, 1) == person);
    try std.testing.expect(api.send(fact, api.intern("at:"), &[_]Oop{api.intern("attribute")}, 1) == name);
    try std.testing.expect(api.send(fact, api.intern("at:"), &[_]Oop{api.intern("value")}, 1) == ada);
}

test "codegen E1: a captured var in a chained send is not dropped by capture analysis" {
    const api = setupFull();

    // `x + base + 1` is a same-category CHAIN — one r_message node with two
    // binaryMsg children — which the emitter folds via emitSendChain. The capture
    // pass used to inspect only the single-message shape, so a free variable in a
    // chain's arg position (`base`) was never captured; the generated C then
    // referenced an undeclared name and failed to compile (→ nil block). Regression.
    try std.testing.expectEqual(@as(i64, 42), try evalInt(api, "let base := 31; let f := [:x | [x + base + 1]]; [f value: 10]"));
    // longer chain, capture is not the first arg either.
    try std.testing.expectEqual(@as(i64, 42), try evalInt(api, "let a := 10; let b := 20; let f := [:x | [x + a + b + 2]]; [f value: 10]"));
}

test "codegen 3c.3: define-method installs a block as a native method on a type" {
    const api = setup();

    // unary method, body uses inline arithmetic (self + self)
    _ = eval(api, "[Reflect defineMethod: #twice on: Integer as: [:self | [self + self]]]");
    try std.testing.expectEqual(@as(i64, 10), try evalInt(api, "[5 twice]"));

    // keyword method, two args → selector combine:and:
    _ = eval(api, "[Reflect defineMethod: #combine:and: on: Integer as: [:self :a :b | [[a * b] + self]]]");
    try std.testing.expectEqual(@as(i64, 26), try evalInt(api, "[2 combine: 3 and: 8]")); // 3*8+2

    // the multi-keyword selector specifically requested
    _ = eval(api, "[Reflect defineMethod: #fooWithBar:andBaz:andQuux: on: Integer as: [:self :bar :baz :quux | [[bar + baz] + quux]]]");
    try std.testing.expectEqual(@as(i64, 15), try evalInt(api, "[0 fooWithBar: 4 andBaz: 5 andQuux: 6]")); // 4+5+6

    // a defined method whose body references a global is late-bound to it
    _ = eval(api, "[Globals at: #bump put: 100]");
    _ = eval(api, "[Reflect defineMethod: #bumped on: Integer as: [:self | [self + bump]]]");
    try std.testing.expectEqual(@as(i64, 105), try evalInt(api, "[5 bumped]"));
    _ = eval(api, "[Globals at: #bump put: 0]");
    try std.testing.expectEqual(@as(i64, 5), try evalInt(api, "[5 bumped]"));

    // a defined method is a first-class dispatch: callable on any SmallInt, nests
    try std.testing.expectEqual(@as(i64, 20), try evalInt(api, "[[5 twice] twice]"));
}

test "codegen 3c.4: vector literals lower to a persistent add: chain" {
    const api = setupFull();

    // empty + non-empty, via the collection protocol (size / at: / inject:into:).
    try std.testing.expectEqual(@as(i64, 0), try evalInt(api, "[#[] size]"));
    try std.testing.expectEqual(@as(i64, 3), try evalInt(api, "[#[10 20 30] size]"));
    try std.testing.expectEqual(@as(i64, 20), try evalInt(api, "[#[10 20 30] at: 1]"));
    try std.testing.expectEqual(@as(i64, 60), try evalInt(api, "[#[10 20 30] inject: 0 into: [:a :x | [a + x]]]"));

    // elements are arbitrary exprs (sends, idents), evaluated in order.
    try std.testing.expectEqual(@as(i64, 9), try evalInt(api, "let y := 4; [#[[1 + 1] y 3] inject: 0 into: [:a :x | [a + x]]]"));

    // printString round-trips the literal syntax.
    try std.testing.expectEqualStrings("#[10 20 30]", try evalStr(api, "[#[10 20 30] printString]"));
    try std.testing.expectEqualStrings("#[]", try evalStr(api, "[#[] printString]"));

    // nested vector literal.
    try std.testing.expectEqualStrings("#[#[1 2] #[3 4]]", try evalStr(api, "[#[#[1 2] #[3 4]] printString]"));

    // a literal whose elements capture an enclosing variable inside a block. Uses
    // the native fold primitive `inject:into:` + `add:` directly — the higher-order
    // `collect:` now lives on Collection in stdlib.birnam (docs/design/41 §B), which
    // this stdlib-less codegen harness does not load.
    try std.testing.expectEqualStrings("#[101 102 103]", try evalStr(api, "let b := 100; [[#[1 2 3] inject: #[] into: [:a :x | [a add: [x + b]]]] printString]"));
}

test "codegen 3c.4: the Collection protocol unifies over inject:into: (stdlib)" {
    const api = setupFull();
    // Load the real self-hosted stdlib: it defines the whole higher-order protocol
    // ONCE on the shared Collection parent over the single native fold `inject:into:`
    // (docs/design/41 §B). Vector, Range, Map and String each provide only that
    // primitive and inherit collect:/select:/detect:/count:/… through delegation.
    _ = eval(api, @embedFile("stdlib.birnam"));

    // collect: reaches all four families uniformly (Map folds over its values).
    try std.testing.expectEqualStrings("#[1 4 9]", try evalStr(api, "[[#[1 2 3] collect: [:x | [x * x]]] printString]"));
    try std.testing.expectEqualStrings("#[1 4 9]", try evalStr(api, "[[[1 to: 3] collect: [:x | [x * x]]] printString]"));
    try std.testing.expectEqualStrings("#[60]", try evalStr(api, "[[{#a -> 6} collect: [:v | [v * 10]]] printString]"));
    try std.testing.expectEqualStrings("#[a! b! c!]", try evalStr(api, "[[\"abc\" collect: [:c | [c , \"!\"]]] printString]"));

    // The rest of the protocol on types that previously had NO higher-order methods.
    try std.testing.expectEqualStrings("#[2 4]", try evalStr(api, "[[#[1 2 3 4] select: [:x | [x even]]] printString]"));
    try std.testing.expectEqualStrings("2", try evalStr(api, "[[#[1 2 3 4] count: [:x | [x even]]] printString]"));
    try std.testing.expectEqualStrings("true", try evalStr(api, "[[\"hello\" includes: \"e\"] printString]"));
    try std.testing.expectEqualStrings("true", try evalStr(api, "[[[1 to: 5] anySatisfy: [:x | [x > 4]]] printString]"));
    try std.testing.expectEqualStrings("false", try evalStr(api, "[[[1 to: 5] isEmpty] printString]"));
}

test "codegen B8: literal true/false/nil are real singletons, not late-bound globals" {
    const api = setupFull();
    // A literal boolean must be the actual Boolean object (with the full protocol),
    // not an unbound-global reference that reads nil. (docs/design/41 §B8.)
    try std.testing.expectEqualStrings("true", try evalStr(api, "[true printString]"));
    try std.testing.expectEqualStrings("false", try evalStr(api, "[false printString]"));
    try std.testing.expectEqualStrings("false", try evalStr(api, "[[true not] printString]"));
    try std.testing.expectEqualStrings("false", try evalStr(api, "[[true & false] printString]"));
    try std.testing.expectEqualStrings("true", try evalStr(api, "[[true | false] printString]"));
    try std.testing.expectEqualStrings("yes", try evalStr(api, "[true ifTrue: [| \"yes\"] ifFalse: [| \"no\"]]"));
    // nil is still the nil singleton (isNil answers true).
    try std.testing.expectEqualStrings("true", try evalStr(api, "[[nil isNil] printString]"));
    // A literal boolean passed INTO a block is captured/forwarded, never treated as a
    // free-variable global — the block's `not` sees a real Boolean.
    try std.testing.expectEqualStrings("false", try evalStr(api, "[[[:b | [b not]] value: true] printString]"));
}

test "codegen B3/B4/B5/B7: equality, integer math, range indexing, nil printString" {
    const api = setupFull();
    _ = eval(api, @embedFile("stdlib.birnam"));

    // `==` is pointer identity: distinct boxed integers are NOT identical, but a
    // literal singleton is identical to itself.
    try std.testing.expectEqualStrings("false", try evalStr(api, "[[3 == 3] printString]"));
    try std.testing.expectEqualStrings("true", try evalStr(api, "[[nil == nil] printString]"));

    // B3: structural `=` via double dispatch — by kind, then element-wise; element
    // comparison recurses through ordinary `=` (nested + mixed types work).
    try std.testing.expectEqualStrings("true", try evalStr(api, "[[#[1 2 3] = #[1 2 3]] printString]"));
    try std.testing.expectEqualStrings("false", try evalStr(api, "[[#[1 2 3] = #[1 2 4]] printString]"));
    try std.testing.expectEqualStrings("false", try evalStr(api, "[[#[1 2] = #[1 2 3]] printString]"));
    try std.testing.expectEqualStrings("true", try evalStr(api, "[[#[] = #[]] printString]"));
    try std.testing.expectEqualStrings("true", try evalStr(api, "[[#[#[1] \"a\"] = #[#[1] \"a\"]] printString]"));
    try std.testing.expectEqualStrings("false", try evalStr(api, "[[#[1 2 3] = 5] printString]")); // non-collection
    try std.testing.expectEqualStrings("false", try evalStr(api, "[[#[1 2 3] = [1 to: 3]] printString]")); // cross-kind
    try std.testing.expectEqualStrings("true", try evalStr(api, "[[[1 to: 3] = [1 to: 3]] printString]"));
    // Maps: order-independent, compared by key→value content.
    try std.testing.expectEqualStrings("true", try evalStr(api, "[[{#a -> 1, #b -> 2} = {#b -> 2, #a -> 1}] printString]"));
    try std.testing.expectEqualStrings("false", try evalStr(api, "[[{#a -> 1} = {#a -> 2}] printString]"));
    // Booleans/nil are singletons: value equality IS identity (inherited Object `=`).
    try std.testing.expectEqualStrings("true", try evalStr(api, "[[true = true] printString]"));
    try std.testing.expectEqualStrings("false", try evalStr(api, "[[true = false] printString]"));
    try std.testing.expectEqualStrings("true", try evalStr(api, "[[nil = nil] printString]"));

    // B4: integer rounding/coercion are identity on a whole number; sqrt via Float.
    try std.testing.expectEqualStrings("3.0", try evalStr(api, "[[9 sqrt] printString]"));
    try std.testing.expectEqualStrings("7", try evalStr(api, "[[7 floor] printString]"));
    try std.testing.expectEqualStrings("7", try evalStr(api, "[[7 asInteger] printString]"));

    // B5: range indexing (0-based) + first/last/reversed.
    try std.testing.expectEqualStrings("2", try evalStr(api, "[[[2 to: 5] at: 0] printString]"));
    try std.testing.expectEqualStrings("5", try evalStr(api, "[[[2 to: 5] last] printString]"));
    try std.testing.expectEqualStrings("#[5 4 3 2]", try evalStr(api, "[[[2 to: 5] reversed] printString]"));

    // B7: nil prints as "nil", not "anObject".
    try std.testing.expectEqualStrings("nil", try evalStr(api, "[nil printString]"));
}

test "codegen 3c.4: map literals lower to a persistent at:put: chain" {
    const api = setupFull();

    try std.testing.expectEqual(@as(i64, 0), try evalInt(api, "[{} size]"));
    try std.testing.expectEqual(@as(i64, 2), try evalInt(api, "[{\"a\" -> 1, \"b\" -> 2} size]"));
    try std.testing.expectEqual(@as(i64, 5), try evalInt(api, "[{\"apple\" -> 3, \"banana\" -> 5} at: \"banana\"]"));

    // absent key runs the ifAbsent: block.
    try std.testing.expectEqual(@as(i64, 0), try evalInt(api, "[{\"a\" -> 1} at: \"z\" ifAbsent: [| 0]]"));

    // later duplicate key wins (persistent at:put: overwrite).
    try std.testing.expectEqual(@as(i64, 9), try evalInt(api, "[{\"k\" -> 1, \"k\" -> 9} at: \"k\"]"));

    // keys/values that are themselves exprs.
    try std.testing.expectEqual(@as(i64, 30), try evalInt(api, "let n := 10; [{\"x\" -> [n + 20]} at: \"x\"]"));

    // symbol keys including keyword-selector symbols (the reason for `->`).
    try std.testing.expectEqual(@as(i64, 7), try evalInt(api, "[{#at:put: -> 7} at: #at:put:]"));
}

test "codegen: float literals + the numeric tower (int↔float promotion)" {
    const api = setupFull();

    const evalFloat = struct {
        fn f(a: *idapi.IdApi, src: []const u8) !f64 {
            const r = eval(a, src);
            if (a.isFloat(r) == 0) return error.NotAFloat;
            return a.floatValue(r);
        }
    }.f;

    // A bare float literal compiles to birnam_mkFloat and round-trips.
    try std.testing.expectEqual(@as(f64, 3.5), try evalFloat(api, "3.5"));
    try std.testing.expectEqual(@as(f64, -0.25), try evalFloat(api, "-0.25"));
    // Float arithmetic, and Integer→Float promotion in BOTH directions.
    try std.testing.expectEqual(@as(f64, 5.0), try evalFloat(api, "[2.5 + 2.5]"));
    try std.testing.expectEqual(@as(f64, 3.5), try evalFloat(api, "[1 + 2.5]"));
    try std.testing.expectEqual(@as(f64, 3.5), try evalFloat(api, "[2.5 + 1]"));
    try std.testing.expectEqual(@as(f64, 2.5), try evalFloat(api, "[10.0 / 4]"));
    // Integer / Integer stays exact (no accidental promotion).
    try std.testing.expectEqual(@as(i64, 2), try evalInt(api, "[10 / 4]"));
    // Cross-kind ORDERING coerces (producing/relating values widens)...
    try std.testing.expectEqualStrings("true", try evalStr(api, "[[1.5 < 2] printString]"));
    // ...but cross-kind EQUALITY does NOT: integer and real are different domains, so
    // `1 = 1.0` is FALSE (docs/design/41 §B). Cross explicitly with `asFloat`.
    try std.testing.expectEqualStrings("false", try evalStr(api, "[[1 = 1.0] printString]"));
    try std.testing.expectEqualStrings("true", try evalStr(api, "[[[1 asFloat] = 1.0] printString]"));
    // printString forces a decimal point so a whole-valued float still reads float.
    try std.testing.expectEqualStrings("4.0", try evalStr(api, "[4.0 printString]"));
    // Conversions.
    try std.testing.expectEqual(@as(i64, 7), try evalInt(api, "[7.5 asInteger]"));
    try std.testing.expectEqual(@as(f64, 3.0), try evalFloat(api, "[3 asFloat]"));
}

// -- integration: a compiled birnam block as a C callback (alien) ---------------

const alien = @import("alien.zig");

// A C harness the callback test drives: invoke the fn pointer with 20 and 22.
export fn birnam_cg_apply2(f: *const fn (c_long, c_long) callconv(.c) c_long) c_long {
    return f(20, 22);
}

test "integration: [Alien callback:] on a REAL compiled birnam block" {
    const api = setupFull();
    alien.bringUp(api);

    // A capturing birnam block, compiled to native by codegen.
    const blk = eval(api, "let base := 100; [:a :b | [[a + b] + base]]");
    try std.testing.expect(blk != api.nilObj);

    // Alien turns it into a C function pointer…
    const Alien = api.import("Alien").?;
    const sig = api.mkString("l l l", 5);
    const cb = api.send(Alien, api.intern("callback:signature:"), &[_]Oop{ blk, sig }, 2);
    try std.testing.expect(cb != api.nilObj);

    // …which the C harness calls like any C function: 20 + 22 + 100.
    const wrapOk = api.send(Alien, api.intern("wrap:signature:as:on:"), &[_]Oop{
        api.mkString("birnam_cg_apply2", 16), api.mkString("l p", 3),
        api.intern("cgApply"),                api.smallint_vt,
    }, 4);
    try std.testing.expect(wrapOk == api.smallint_vt);
    const r = api.send(cb, api.intern("cgApply"), &[_]Oop{}, 0);
    try std.testing.expectEqual(@as(i64, 142), api.smallIntValue(r));
}

// -- slice 1 of the images arc (docs/40 §3a): relocatable artifacts -------------

test "slice-1: compile birnam to a relocatable .dylib, load it, run it" {
    const api = setupFull();
    const Codegen = api.import("Codegen").?;

    const path = "/tmp/birnam_codegen_artifact.dylib";
    std.fs.cwd().deleteFile(path) catch {};

    // Exercises every relocatable piece at once: baked global handles (`Vector`
    // behind the #[…] literal and `Integer`'s protocol — re-derived by name in
    // __birnam_unit_init), a capturing closure (a file-scope block fn inside the
    // artifact), and dispatch through the undefined `birnam_*` externals the
    // dynamic loader resolves against this process at dlopen.
    const src =
        "let base := 40; let add := [:y | [base + y]]; " ++
        "let v := #[[add value: 1] [add value: 2]]; [[v size] + [add value: 0]]";
    const psrc = api.mkString(src.ptr, src.len);
    const ppath = api.mkString(path, path.len);
    const out = api.send(Codegen, api.intern("compile:output:"), &[_]Oop{ psrc, ppath }, 2);
    try std.testing.expect(out != api.nilObj);

    // The artifact must be pure: no `__g` handle may remain an undefined dynamic
    // import (nothing exports those — a baked JIT handle leaking into the artifact
    // would poison dlopen), and __birnam_unit_init must be present.
    const r = api.send(Codegen, api.intern("load:"), &[_]Oop{ppath}, 1);
    try std.testing.expectEqual(@as(i64, 42), api.smallIntValue(r)); // 2 + 40
}

test "embedFile: embeds binary bytes into a dylib relative to the source file" {
    const api = setupFull();
    const Codegen = api.import("Codegen").?;
    const a = std.testing.allocator;
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    try tmp.dir.writeFile(.{ .sub_path = "asset.bin", .data = "a\x00b\xff" });
    const dir = try tmp.dir.realpathAlloc(a, ".");
    defer a.free(dir);
    const source_path = try std.fs.path.join(a, &.{ dir, "module.birnam" });
    defer a.free(source_path);
    const output_path = try std.fs.path.join(a, &.{ dir, "libbirnam_embedProbe.dylib" });
    defer a.free(output_path);

    const source =
        "[Module named: #embedProbe requires: #[]];\n" ++
        "[Globals at: #EmbedProbe put: [Object clone]];\n" ++
        "[Reflect defineMethod: #payload on: EmbedProbe as: [:self | [System embedFile: \"asset.bin\"]]];\n";
    const result = api.send(Codegen, api.intern("compile:output:from:"), &[_]Oop{
        api.mkString(source.ptr, source.len),
        api.mkString(output_path.ptr, output_path.len),
        api.mkString(source_path.ptr, source_path.len),
    }, 3);
    try std.testing.expect(result != api.nilObj);

    // Prove the loaded dylib no longer depends on the authoring file.
    try tmp.dir.deleteFile("asset.bin");
    const loaded = api.send(Codegen, api.intern("load:"), &[_]Oop{api.mkString(output_path.ptr, output_path.len)}, 1);
    try std.testing.expect(loaded != api.nilObj);
    const globals = api.import("Globals").?;
    const probe = api.send(globals, api.intern("at:"), &[_]Oop{api.intern("EmbedProbe")}, 1);
    const payload = api.send(probe, api.intern("payload"), &.{}, 0);
    var payload_len: usize = 0;
    const payload_bytes = api.stringBytes(payload, &payload_len);
    try std.testing.expectEqualSlices(u8, "a\x00b\xff", payload_bytes[0..payload_len]);
}

test "embedFile: source evaluation resolves relative to its source path" {
    const api = setupFull();
    const Codegen = api.import("Codegen").?;
    const a = std.testing.allocator;
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    try tmp.dir.writeFile(.{ .sub_path = "asset.txt", .data = "from-source" });
    const dir = try tmp.dir.realpathAlloc(a, ".");
    defer a.free(dir);
    const source_path = try std.fs.path.join(a, &.{ dir, "script.birnam" });
    defer a.free(source_path);
    const source = "[System embedFile: \"asset.txt\"]";
    const result = api.send(Codegen, api.intern("eval:from:"), &[_]Oop{
        api.mkString(source.ptr, source.len),
        api.mkString(source_path.ptr, source_path.len),
    }, 2);
    try std.testing.expectEqualStrings("from-source", stringBytes(api, result));
}

test "embedFile: static analysis reports distinct literal inputs" {
    const api = setupFull();
    const Codegen = api.import("Codegen").?;
    const source = "[System embedFile: \"a.txt\"]; [[:x | [System embedFile: \"b.bin\"]] value: 1]; [System embedFile: \"a.txt\"]";
    const files = api.send(Codegen, api.intern("embeddedFilesOf:"), &.{api.mkString(source.ptr, source.len)}, 1);
    try std.testing.expectEqual(@as(i64, 2), api.smallIntValue(api.send(files, api.intern("size"), &.{}, 0)));
    try std.testing.expectEqualStrings("a.txt", stringBytes(api, vectorAt(api, files, 0)));
    try std.testing.expectEqualStrings("b.bin", stringBytes(api, vectorAt(api, files, 1)));
}

test "slice-1: artifact failures are nil — bad source, missing library" {
    const api = setupFull();
    const Codegen = api.import("Codegen").?;

    const bad = api.mkString("let :=", 6);
    const ppath = api.mkString("/tmp/birnam_codegen_never.dylib", 31);
    try std.testing.expect(api.send(Codegen, api.intern("compile:output:"), &[_]Oop{ bad, ppath }, 2) == api.nilObj);

    const absent = api.mkString("/tmp/birnam_codegen_absent.dylib", 32);
    try std.testing.expect(api.send(Codegen, api.intern("load:"), &[_]Oop{absent}, 1) == api.nilObj);
}

// -- slice 2 of the images arc (docs/40 §3c): exact layouts for block captures --

noinline fn churnStrings(api: *idapi.IdApi, count: usize) void {
    var i: usize = 0;
    var last: Oop = null;
    while (i < count) : (i += 1) {
        last = api.mkString("throwaway garbage string", 24);
    }
    std.mem.doNotOptimizeAway(&last);
}

test "slice-2: a compiled closure's heap capture survives precise collection" {
    const api = setupFull();
    var anchor: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&anchor));
    _ = &anchor;
    defer api.gcSetPrecise(0);
    api.gcSetPrecise(1);

    // The capture is a HEAP object (a String) — if the emitted
    // birnam_layoutAllOops on the block's vtable were missing or wrong, the
    // precise trace would skip the capture slot and the churn+collect below
    // would free the string out from under the closure.
    const blk = eval(api, "let base := \"birnam\"; [:a | [base , a]]");
    try std.testing.expect(blk != api.nilObj);

    churnStrings(api, 20_000);
    _ = api.gcCollect();

    const r = api.send(blk, api.intern("value:"), &[_]Oop{api.mkString(" rocks", 6)}, 1);
    var len: usize = 0;
    const p = api.stringBytes(r, &len);
    try std.testing.expectEqualStrings("birnam rocks", p[0..len]);
}

// -- slice 4 of the images arc (docs/40 §7.4): mapped code — install PRE-COMPILED --

test "slice-4: a defineMethod artifact installs a method with no recompile" {
    const api = setupFull();
    const Codegen = api.import("Codegen").?;

    const path = "/tmp/birnam_codegen_defs.dylib";
    std.fs.cwd().deleteFile(path) catch {};

    // In ARTIFACT mode, defineMethod lowers to a static trampoline (__amtramp) +
    // a direct birnam_method install — not the compiling runtime send. So loading
    // the artifact installs MAPPED code: the trampoline pointer comes from the
    // dlopen'd unit, and dispatch runs it with no recompile. (The live/JIT path
    // still uses the source-recording send — that's slice 3's replay truth.)
    const src = "[Reflect defineMethod: #triple on: Integer as: [:x | [x + [x + x]]]]; [7 triple]";
    const psrc = api.mkString(src.ptr, src.len);
    const ppath = api.mkString(path, path.len);
    try std.testing.expect(api.send(Codegen, api.intern("compile:output:"), &[_]Oop{ psrc, ppath }, 2) != api.nilObj);

    const r = api.send(Codegen, api.intern("load:"), &[_]Oop{ppath}, 1);
    try std.testing.expectEqual(@as(i64, 21), api.smallIntValue(r)); // 7 * 3
}

// -- fileIn contract: errors are values; DISCARDING one is the only sin ---------

test "fileIn contract: a discarded top-level error ABORTS the program and propagates" {
    const api = setupFull();
    // [5 frobnicate] answers a #doesNotUnderstand error VALUE; it is not the
    // last form, so the old emit dropped it on the floor and the file limped
    // on. Now the program aborts with the error — the final form never runs.
    const r = eval(api, "[5 frobnicate]; 42");
    try std.testing.expect(api.send(r, api.intern("isError"), &[_]Oop{}, 0) == api.trueObj);
    try std.testing.expect(api.send(r, api.intern("kind"), &[_]Oop{}, 0) == api.intern("doesNotUnderstand"));
}

test "fileIn contract: a let-BOUND error is intentional capture, not a swallow" {
    const api = setupFull();
    // Binding an error value is the errors-as-values idiom for keeping one to
    // inspect — the program continues.
    try std.testing.expectEqual(@as(i64, 42), try evalInt(api, "let e := [5 frobnicate]; 42"));
}

test "fileIn contract: block bodies keep discard semantics" {
    const api = setupFull();
    // Inside a block/method, ignoring a returned error is an authoring choice
    // (on:do:/ifError: is the handled path) — only the TOP level aborts.
    try std.testing.expectEqual(@as(i64, 42), try evalInt(api, "[[:x | [x frobnicate]; 42] value: 5]"));
}

test "fileIn contract: [System load:] answers a kinded #syntaxError on a broken file" {
    const api = setupFull();
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    try tmp.dir.writeFile(.{ .sub_path = "bad.birnam", .data = "let x := [3 +" });
    const a = std.testing.allocator;
    const dir = try tmp.dir.realpathAlloc(a, ".");
    defer a.free(dir);
    const p = try std.fs.path.join(a, &.{ dir, "bad.birnam" });
    defer a.free(p);
    const System = api.import("System").?;
    const r = api.send(System, api.intern("load:"), &[_]Oop{api.mkString(p.ptr, p.len)}, 1);
    // Not a bare nil with the detail hidden in [Codegen lastError] — the load
    // itself answers the kinded failure (and names the file on stderr).
    try std.testing.expect(api.send(r, api.intern("isError"), &[_]Oop{}, 0) == api.trueObj);
    try std.testing.expect(api.send(r, api.intern("kind"), &[_]Oop{}, 0) == api.intern("syntaxError"));
}
