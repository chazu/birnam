const std = @import("std");

// birnam — the libid refounding (docs/30 §9).
//
// `birnam` is a minimal siphon (src/id/main.zig) that loads the system as a
// set of reloadable dylibs over a clean Piumarta object floor (libid: vtable
// pointers at [-1], no eids/facts). Each rung is its own module + test suite:
//
//   libid (id.zig) → store → peg → lang → tcc → codegen → prelude
//                  → collection → error → isolate → relational → alien → image
//
// The legacy fact-store kernel that preceded this lives only in git history; a
// handful of its files (image/persistence, gc, the libffi FFI, macho/elf,
// boxed) are parked in src/attic/ as reference for arcs not yet rebuilt on the
// id floor (persistence/images, struct-by-value FFI, value-slot globals). The
// attic is NOT part of the build.
pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    // libtcc is presently vendored and configured for arm64 Mach-O, and the
    // runtime resolves Mach-O dylibs by name. Reject other targets before Zig
    // spends time compiling artifacts that cannot link or boot correctly.
    if (target.result.cpu.arch != .aarch64 or target.result.os.tag != .macos) {
        std.process.fatal(
            "birnam currently supports only arm64 macOS; requested {s}-{s}. " ++
                "A Linux or Intel port needs a target-specific libtcc build and runtime shared-library loading.",
            .{ @tagName(target.result.cpu.arch), @tagName(target.result.os.tag) },
        );
    }

    // The release version, read from the repo's VERSION file at build time so the
    // siphon's `--version` never drifts from what `scripts/release.sh` tags.
    const version = std.mem.trim(u8, @embedFile("VERSION"), " \t\r\n");
    const build_opts = b.addOptions();
    build_opts.addOption([]const u8, "version", version);

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
    const verify_step = b.step(
        "verify",
        "Run the complete native, self-hosted, and REPL verification gate",
    );

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
        .{ .name = "birnam_store", .src = "store.zig", .test_src = "store_test.zig", .step = "test-store", .desc = "rung 2: the cell/relational store bootstrap fixpoint" },
        .{ .name = "birnam_id_peg", .src = "peg.zig", .test_src = "peg_test.zig", .step = "test-peg", .desc = "rung 3a: the PEG engine on the libid floor" },
        .{ .name = "birnam_id_lang", .src = "lang.zig", .test_src = "lang_test.zig", .step = "test-lang", .desc = "rung 3b: birnam's grammar + parser via the peg message api" },
        .{ .name = "birnam_id_tcc", .src = "tcc.zig", .test_src = "tcc_test.zig", .step = "test-tcc", .desc = "rung 3c: libtcc as an id module — C to native", .lib_links_tcc = true, .test_links_tcc = true },
        .{ .name = "birnam_id_codegen", .src = "codegen.zig", .test_src = "codegen_test.zig", .step = "test-codegen", .desc = "rung 3c: the birnam → C emitter", .test_links_tcc = true },
        .{ .name = "birnam_id_prelude", .src = "prelude.zig", .test_src = "prelude_test.zig", .step = "test-prelude", .desc = "the primitive stdlib: Integer/Boolean/System" },
        .{ .name = "birnam_id_collection", .src = "collection.zig", .test_src = "collection_test.zig", .step = "test-collection", .desc = "the id-native collection library: Vector trie + Range" },
        .{ .name = "birnam_id_error", .src = "error.zig", .test_src = "error_test.zig", .step = "test-error", .desc = "F7: the error model (propagating values + DNU swap)" },
        .{ .name = "birnam_id_isolate", .src = "isolate.zig", .test_src = "isolate_test.zig", .step = "test-isolate", .desc = "Blocking isolates, wire-v1, and L2 evolution records" },
        .{ .name = "birnam_id_relational", .src = "relational.zig", .test_src = "relational_test.zig", .step = "test-relational", .desc = "the relational reading: datalog join + pull" },
        .{ .name = "birnam_id_alien", .src = "alien.zig", .test_src = "alien_test.zig", .step = "test-alien", .desc = "the FFI: wrap + absorb C libraries via Tcc", .test_links_tcc = true },
        .{ .name = "birnam_id_image", .src = "image.zig", .test_src = "image_test.zig", .step = "test-image", .desc = "the image: store-reachable durability + mapped code (docs/40)", .test_links_tcc = true },
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

    // -- birnam: the siphon (default install) --------------------------------
    // A minimal host that imports only the IdApi (src/id/idapi.zig) and dlopens
    // the rung dylibs at runtime ($BIRNAM_LIB_DIR / zig-out/lib). `birnam
    // <file.birnam>` runs a script; bare invocation drops into the REPL.
    const siphon_mod = b.createModule(.{
        .root_source_file = b.path("src/id/main.zig"),
        .target = target,
        .optimize = optimize,
    });
    siphon_mod.addOptions("build_opts", build_opts);
    const siphon_exe = b.addExecutable(.{ .name = "birnam", .root_module = siphon_mod });
    siphon_exe.linkLibC();
    b.installArtifact(siphon_exe);

    // Source-backed tools are part of an installed birnam, not checkout-only
    // extras. Keep the same relative tree that Cli/Doc use in development:
    //   modules/../src/id     (Zig-floor source docs)
    //   modules/../docs/guide (the authored guide chapters)
    b.installDirectory(.{
        .source_dir = b.path("modules"),
        .install_dir = .prefix,
        .install_subdir = "share/birnam/modules",
        .include_extensions = &.{".birnam"},
    });
    b.installDirectory(.{
        .source_dir = b.path("src/id"),
        .install_dir = .prefix,
        .install_subdir = "share/birnam/src/id",
        .include_extensions = &.{ ".zig", ".birnam" },
    });
    b.installDirectory(.{
        .source_dir = b.path("docs/guide"),
        .install_dir = .prefix,
        .install_subdir = "share/birnam/docs/guide",
        .include_extensions = &.{".md"},
    });

    // -- stage 1: AOT-compile birnam-source modules to installable dylibs -------
    // The default build ("stage 0") ships a binary that embeds the stdlib SOURCE
    // and evals it at boot. `zig build stage1` then RUNS that binary to compile
    // the source into `libbirnam_stdlib.dylib`, which a subsequent boot prefers
    // (installing methods with no recompile) and falls back to source if absent.
    // Kept opt-in: a plain `zig build` still works via the source fallback. Points
    // the binary at the just-installed lib dir so it finds the fresh kernel dylibs.
    const stage1 = b.step("stage1", "AOT-compile the dependency-ordered self-hosted modules to installable dylibs");
    // Each self-hosted module: `birnam --compile-module <src> <lib>/libbirnam_<name>.dylib`.
    const self_hosted = [_]struct { src: []const u8, name: []const u8 }{
        .{ .src = "src/id/modules.birnam", .name = "modules" },
        .{ .src = "src/id/stdlib.birnam", .name = "stdlib" },
        .{ .src = "src/id/compiled-unit.birnam", .name = "compiledUnit" },
        .{ .src = "src/id/live-modules.birnam", .name = "liveModules" },
        .{ .src = "src/id/toml.birnam", .name = "toml" },
        .{ .src = "src/id/config.birnam", .name = "config" },
        .{ .src = "src/id/isolate.birnam", .name = "isolate" },
    };
    inline for (self_hosted) |m| {
        const compile = b.addRunArtifact(siphon_exe);
        compile.setEnvironmentVariable("BIRNAM_LIB_DIR", b.getInstallPath(.lib, ""));
        compile.addArg("--compile-module");
        compile.addFileArg(b.path(m.src));
        compile.addArg(b.getInstallPath(.lib, "libbirnam_" ++ m.name ++ ".dylib"));
        compile.step.dependOn(b.getInstallStep()); // exe + all kernel dylibs installed first
        stage1.dependOn(&compile.step);
    }

    // -- Self-hosted and console integration gates ---------------------------
    // Keep these ordered after stage1: a stale compiled stdlib can mask source
    // changes, while both suites must exercise the exact installed artifact a
    // user receives. `test` remains independently runnable for fast Zig-only
    // feedback; `verify` composes every gate below.
    const birnam_tests = b.addSystemCommand(&.{
        b.getInstallPath(.bin, "birnam"),
        "modules/run-tests.birnam",
    });
    birnam_tests.step.dependOn(stage1);
    b.step("test-birnam", "Run all self-hosted birnam test suites").dependOn(&birnam_tests.step);

    const project_check_smoke = b.addSystemCommand(&.{ "bash", "scripts/project-check-smoke.sh" });
    project_check_smoke.setEnvironmentVariable("BIRNAM_BIN", b.getInstallPath(.bin, "birnam"));
    project_check_smoke.setEnvironmentVariable("BIRNAM_MODULES_DIR", b.pathFromRoot("modules"));
    project_check_smoke.step.dependOn(stage1);
    const project_check_step = b.step("test-project-check", "Run source-project resolution and build smoke tests");
    project_check_step.dependOn(&project_check_smoke.step);

    const repl_goldens = b.addSystemCommand(&.{ "bash", "scripts/repl-golden.sh" });
    repl_goldens.setEnvironmentVariable("BIRNAM_BIN", b.getInstallPath(.bin, "birnam"));
    repl_goldens.step.dependOn(&birnam_tests.step);
    b.step("test-repl", "Run console REPL end-to-end golden transcripts").dependOn(&repl_goldens.step);

    const source_fallback = b.addSystemCommand(&.{ "bash", "scripts/source-fallback.sh" });
    source_fallback.setEnvironmentVariable("BIRNAM_BIN", b.getInstallPath(.bin, "birnam"));
    source_fallback.step.dependOn(stage1);
    b.step("test-source-fallback", "Compare AOT and embedded-source G0 authority").dependOn(&source_fallback.step);

    const image_save_smoke = b.addSystemCommand(&.{ "bash", "scripts/image-save-smoke.sh" });
    image_save_smoke.setEnvironmentVariable("BIRNAM_BIN", b.getInstallPath(.bin, "birnam"));
    image_save_smoke.setEnvironmentVariable("BIRNAM_LIB_DIR", b.getInstallPath(.lib, ""));
    image_save_smoke.step.dependOn(stage1);
    b.step("test-image-save", "Run the script-host image-save regression smoke").dependOn(&image_save_smoke.step);

    const install_smoke = b.addSystemCommand(&.{ "bash", "scripts/install-smoke.sh" });
    install_smoke.setEnvironmentVariable("BIRNAM_PREFIX", b.getInstallPath(.prefix, ""));
    install_smoke.step.dependOn(stage1);
    b.step("test-install", "Run source-backed tools from an isolated installed tree").dependOn(&install_smoke.step);

    const isolate_smoke = b.addSystemCommand(&.{ "bash", "scripts/isolate-smoke.sh" });
    isolate_smoke.setEnvironmentVariable("BIRNAM_BIN", b.getInstallPath(.bin, "birnam"));
    isolate_smoke.setEnvironmentVariable("BIRNAM_LIB_DIR", b.getInstallPath(.lib, ""));
    isolate_smoke.step.dependOn(stage1);
    b.step("test-isolate-process", "Boot a worker image and round-trip a copied value").dependOn(&isolate_smoke.step);

    const isolate_replacement_smoke = b.addSystemCommand(&.{ "bash", "scripts/isolate-replacement-smoke.sh" });
    isolate_replacement_smoke.setEnvironmentVariable("BIRNAM_BIN", b.getInstallPath(.bin, "birnam"));
    isolate_replacement_smoke.step.dependOn(stage1);
    b.step("test-isolate-replacement", "Health-check and replace one selected worker").dependOn(&isolate_replacement_smoke.step);

    const isolate_rollout_smoke = b.addSystemCommand(&.{ "bash", "scripts/isolate-rollout-smoke.sh" });
    isolate_rollout_smoke.setEnvironmentVariable("BIRNAM_BIN", b.getInstallPath(.bin, "birnam"));
    isolate_rollout_smoke.setEnvironmentVariable("BIRNAM_LIB_DIR", b.getInstallPath(.lib, ""));
    isolate_rollout_smoke.step.dependOn(stage1);
    b.step("test-isolate-rollout", "Activate and roll back one worker between requests").dependOn(&isolate_rollout_smoke.step);

    const live_modules_smoke = b.addSystemCommand(&.{ "bash", "scripts/live-modules-smoke.sh" });
    live_modules_smoke.setEnvironmentVariable("BIRNAM_BIN", b.getInstallPath(.bin, "birnam"));
    live_modules_smoke.setEnvironmentVariable("BIRNAM_LIB_DIR", b.getInstallPath(.lib, ""));
    live_modules_smoke.step.dependOn(stage1);
    b.step("test-live-modules", "Exercise L1b host activation and mapped/rebuilt baselines").dependOn(&live_modules_smoke.step);

    const live_features_examples = b.addSystemCommand(&.{ "bash", "scripts/live-features-examples-smoke.sh" });
    live_features_examples.setEnvironmentVariable("BIRNAM_BIN", b.getInstallPath(.bin, "birnam"));
    live_features_examples.setEnvironmentVariable("BIRNAM_LIB_DIR", b.getInstallPath(.lib, ""));
    live_features_examples.step.dependOn(stage1);
    b.step("test-live-feature-examples", "Run the documented I1b and L2 examples").dependOn(&live_features_examples.step);

    verify_step.dependOn(test_step);
    verify_step.dependOn(&repl_goldens.step);
    verify_step.dependOn(&source_fallback.step);
    verify_step.dependOn(project_check_step);
    verify_step.dependOn(&image_save_smoke.step);
    verify_step.dependOn(&install_smoke.step);
    verify_step.dependOn(&isolate_smoke.step);
    verify_step.dependOn(&isolate_replacement_smoke.step);
    verify_step.dependOn(&isolate_rollout_smoke.step);
    verify_step.dependOn(&live_modules_smoke.step);
    verify_step.dependOn(&live_features_examples.step);

    // The siphon/REPL host's own unit tests (e.g. the REPL bracket balancer).
    const main_test_mod = b.createModule(.{
        .root_source_file = b.path("src/id/main.zig"),
        .target = target,
        .optimize = optimize,
    });
    main_test_mod.addOptions("build_opts", build_opts);
    const main_tests = b.addTest(.{ .root_module = main_test_mod });
    main_tests.linkLibC();
    const run_main_tests = b.addRunArtifact(main_tests);
    b.step("test-main", "siphon/REPL host unit tests").dependOn(&run_main_tests.step);
    test_step.dependOn(&run_main_tests.step);
}
