const std = @import("std");

// birnam — the libid refounding (docs/30 §9).
//
// `birnam-id` is a minimal siphon (src/id/main.zig) that loads the system as a
// set of reloadable dylibs over a clean Piumarta object floor (libid: vtable
// pointers at [-1], no eids/facts). Each rung is its own module + test suite:
//
//   libid (id.zig) → store → peg → lang → tcc → codegen → prelude
//                  → collection → error → relational → alien
//
// The legacy fact-store kernel that preceded this lives only in git history; a
// handful of its files (image/persistence, gc, the libffi FFI, macho/elf,
// boxed) are parked in src/attic/ as reference for arcs not yet rebuilt on the
// id floor (persistence/images, struct-by-value FFI, value-slot globals). The
// attic is NOT part of the build.
pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    // -- libtcc static lib (the C → native backend) --------------------------
    // Vendored from the TinyCC source (vendor/libtcc-src/, LGPL — see COPYING),
    // built from source so the compiler is self-contained AND the basis for
    // cross-compile. `config.h` defaults to arm64 + Mach-O when no TCC_TARGET is
    // set; adding x86_64/linux is incremental (their gen/link .c + a define).
    const libtcc_lib = b.addLibrary(.{
        .name = "tcc",
        .linkage = .static,
        .root_module = b.createModule(.{ .target = target, .optimize = optimize, .link_libc = true }),
    });
    // `libtcc.c` defaults `ONE_SOURCE=1` and #includes the rest (tccpp/tccgen/…
    // + arm64-gen/link/asm + tccmacho), so it is the single translation unit.
    libtcc_lib.addCSourceFiles(.{
        .root = b.path("vendor/libtcc-src"),
        .files = &.{"libtcc.c"},
        .flags = &.{ "-std=gnu11", "-w" }, // -w: silence tcc's own (benign) warnings
    });
    libtcc_lib.addIncludePath(b.path("vendor/libtcc-src")); // config.h, tcc.h, …

    const test_step = b.step("test", "Run all id-system unit tests");

    // -- The id rungs: a dylib + its test suite, one row per module ----------
    // Every rung is a dynamic library the siphon dlopens at runtime, plus a test
    // that imports its siblings directly (relative imports unify the IdApi type
    // across the floor and the module). `*_links_tcc` rows statically link the
    // vendored libtcc and need its headers on the include path.
    const Mod = struct {
        name: []const u8, // dylib artifact name (lib<name>.dylib)
        src: []const u8, // dylib root source, relative to src/id/
        test_src: []const u8, // test root source, relative to src/id/
        step: []const u8, // test step name
        desc: []const u8, // test step description
        lib_links_tcc: bool = false,
        test_links_tcc: bool = false,
    };
    const mods = [_]Mod{
        .{ .name = "birnam_id", .src = "id.zig", .test_src = "id.zig", .step = "test-id", .desc = "libid: the Piumarta object-system bootstrap proof" },
        .{ .name = "birnam_counter", .src = "counter.zig", .test_src = "counter.zig", .step = "test-counter", .desc = "the reloadable demo module (opaque state survives hot reload)" },
        .{ .name = "birnam_store", .src = "store.zig", .test_src = "store_test.zig", .step = "test-store", .desc = "rung 2: the cell/relational store bootstrap fixpoint" },
        .{ .name = "birnam_id_peg", .src = "peg.zig", .test_src = "peg_test.zig", .step = "test-peg", .desc = "rung 3a: the PEG engine on the libid floor" },
        .{ .name = "birnam_id_lang", .src = "lang.zig", .test_src = "lang_test.zig", .step = "test-lang", .desc = "rung 3b: birnam's grammar + parser via the peg message api" },
        .{ .name = "birnam_id_tcc", .src = "tcc.zig", .test_src = "tcc_test.zig", .step = "test-tcc", .desc = "rung 3c: libtcc as an id module — C to native", .lib_links_tcc = true, .test_links_tcc = true },
        .{ .name = "birnam_id_codegen", .src = "codegen.zig", .test_src = "codegen_test.zig", .step = "test-codegen", .desc = "rung 3c: the birnam → C emitter", .test_links_tcc = true },
        .{ .name = "birnam_id_prelude", .src = "prelude.zig", .test_src = "prelude_test.zig", .step = "test-prelude", .desc = "the primitive stdlib: Integer/Boolean/System" },
        .{ .name = "birnam_id_collection", .src = "collection.zig", .test_src = "collection_test.zig", .step = "test-collection", .desc = "the id-native collection library: Vector trie + Range" },
        .{ .name = "birnam_id_error", .src = "error.zig", .test_src = "error_test.zig", .step = "test-error", .desc = "F7: the error model (propagating values + DNU swap)" },
        .{ .name = "birnam_id_relational", .src = "relational.zig", .test_src = "relational_test.zig", .step = "test-relational", .desc = "the relational reading: datalog join + pull" },
        .{ .name = "birnam_id_alien", .src = "alien.zig", .test_src = "alien_test.zig", .step = "test-alien", .desc = "the FFI: wrap + absorb C libraries via Tcc", .test_links_tcc = true },
        .{ .name = "birnam_id_image", .src = "image.zig", .test_src = "image_test.zig", .step = "test-image", .desc = "the image: store-reachable durability (docs/40, slice 3)" },
    };

    inline for (mods) |m| {
        // The dylib.
        const lib_mod = b.createModule(.{
            .root_source_file = b.path("src/id/" ++ m.src),
            .target = target,
            .optimize = optimize,
        });
        if (m.lib_links_tcc) lib_mod.addIncludePath(b.path("vendor/libtcc-src"));
        const lib = b.addLibrary(.{ .name = m.name, .root_module = lib_mod, .linkage = .dynamic });
        if (m.lib_links_tcc) lib.linkLibrary(libtcc_lib);
        lib.linkLibC();
        b.installArtifact(lib);

        // The test suite.
        const test_mod = b.createModule(.{
            .root_source_file = b.path("src/id/" ++ m.test_src),
            .target = target,
            .optimize = optimize,
        });
        if (m.test_links_tcc) test_mod.addIncludePath(b.path("vendor/libtcc-src"));
        const tests = b.addTest(.{ .root_module = test_mod });
        if (m.test_links_tcc) tests.linkLibrary(libtcc_lib);
        tests.linkLibC();
        const run = b.addRunArtifact(tests);
        b.step(m.step, m.desc).dependOn(&run.step);
        test_step.dependOn(&run.step);
    }

    // -- birnam-id: the siphon (default install) -----------------------------
    // A minimal host that imports only the IdApi (src/id/idapi.zig) and dlopens
    // the rung dylibs at runtime ($BIRNAM_LIB_DIR / zig-out/lib). `birnam-id
    // <file.birnam>` runs a script; bare invocation drops into the REPL.
    const siphon_mod = b.createModule(.{
        .root_source_file = b.path("src/id/main.zig"),
        .target = target,
        .optimize = optimize,
    });
    const siphon_exe = b.addExecutable(.{ .name = "birnam-id", .root_module = siphon_mod });
    siphon_exe.linkLibC();
    b.installArtifact(siphon_exe);

    // The siphon/REPL host's own unit tests (e.g. the REPL bracket balancer).
    const main_test_mod = b.createModule(.{
        .root_source_file = b.path("src/id/main.zig"),
        .target = target,
        .optimize = optimize,
    });
    const main_tests = b.addTest(.{ .root_module = main_test_mod });
    main_tests.linkLibC();
    const run_main_tests = b.addRunArtifact(main_tests);
    b.step("test-main", "siphon/REPL host unit tests").dependOn(&run_main_tests.step);
    test_step.dependOn(&run_main_tests.step);
}
