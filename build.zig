const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    // -- libffi static lib (Phase 19 Track B — sole FFI engine) --------------
    // Vendored from libffi v3.4.6 with the arm64-apple-darwin generated headers
    // (fficonfig.h / include/ffi.h / include/ffitarget.h produced by configure).
    // The sole FFI engine since Phase 19 B0b retired dyncall — full C-ABI calls
    // incl. arm64 struct-by-value (+ closures for 19.2). NOTE: aarch64-only
    // headers for now — x86_64 needs its own generated set (regen on that host).
    const libffi_lib = b.addLibrary(.{
        .name = "ffi",
        .linkage = .static,
        .root_module = b.createModule(.{
            .target = target,
            .optimize = optimize,
            .link_libc = true,
        }),
    });
    libffi_lib.addCSourceFiles(.{
        .root = b.path("vendor/libffi"),
        .files = &.{
            "src/prep_cif.c",
            "src/types.c",
            "src/raw_api.c",
            "src/java_raw_api.c",
            "src/closures.c",
            "src/tramp.c",
            "src/aarch64/ffi.c",
            "src/aarch64/sysv.S",
        },
        // dlmalloc.c is #included by closures.c when needed, not compiled alone.
        // gnu11 (not c11): libffi uses the GNU `asm` keyword + extensions.
        .flags = &.{ "-std=gnu11", "-DHAVE_CONFIG_H" },
    });
    libffi_lib.addIncludePath(b.path("vendor/libffi")); // fficonfig.h
    libffi_lib.addIncludePath(b.path("vendor/libffi/include")); // ffi.h, ffitarget.h, ffi_common.h
    libffi_lib.addIncludePath(b.path("vendor/libffi/src/aarch64")); // internal.h
    // NOT installed: libffi is a build-time static archive linked into the
    // shell / modules (see `linkLibrary(libffi_lib)` below). The installed
    // tree needs only `bin/birnam` + the module dylibs — shipping `lib/libffi.a`
    // would be dead weight.

    // -- Build-time kernel-EID mirror (single source of truth) ---------------
    // `src/kernel/kernel_eids.zig` is the ONE place well-known EIDs are defined.
    // A small host-side generator reflects over it and emits a dependency-free
    // `kernel_eids.zig` that dylib modules import as `@import("kernel_eids")` —
    // replacing the hand-copied `const NAME: u64 = N;` blocks that rot silently
    // when the kernel renumbers an EID. A kernel rename now fails the module
    // build (fail-loud) instead of baking a wrong number into a dylib.
    const kernel_eids_src = b.createModule(.{
        .root_source_file = b.path("src/kernel/kernel_eids.zig"),
        .target = b.graph.host,
        .optimize = .Debug,
    });
    const gen_eids_mod = b.createModule(.{
        .root_source_file = b.path("tools/gen_eid_mirror.zig"),
        .target = b.graph.host,
        .optimize = .Debug,
    });
    gen_eids_mod.addImport("kernel_eids", kernel_eids_src);
    const gen_eids_exe = b.addExecutable(.{ .name = "gen_eid_mirror", .root_module = gen_eids_mod });
    const run_gen_eids = b.addRunArtifact(gen_eids_exe);
    const eid_mirror_path = run_gen_eids.addOutputFileArg("kernel_eids.zig");
    // Consumed via `mod.addImport("kernel_eids", eid_mirror_mod)` by every
    // module graph that compiles a mirror-using source (lang dylib + AOT variants).
    const eid_mirror_mod = b.createModule(.{ .root_source_file = eid_mirror_path });

    // -- Shared module-side ABI (single source of truth) ---------------------
    // The C-ABI IdApi/ShellContext/FactCallback struct definitions live in one
    // file (src/kernel/module_abi.zig); the kernel imports it relatively and every
    // dylib module imports it as `@import("module_abi")` instead of hand-copying
    // the struct. Wired as a named module because Zig forbids a dylib from
    // `@import`-ing a file outside its own module root.
    //
    // The named module roots at a build-time COPY of the file (not the original),
    // mirroring the kernel_eids mirror pattern. Under static AOT the kernel
    // (id_api.zig, relative import) and the statically-linked dylib sources
    // (peg/lang, named import) share ONE module graph; if both pointed at the same
    // physical file, Zig would reject it as "file in two modules". The copy is a
    // distinct path so the two never collide, byte-identical (re-copied each
    // build → no drift), and structurally identical so the `@ptrCast` across the
    // ABI boundary works exactly as it already does across the dylib boundary.
    const module_abi_copy = b.addWriteFiles().addCopyFile(b.path("src/kernel/module_abi.zig"), "module_abi.zig");
    const module_abi_mod = b.createModule(.{ .root_source_file = module_abi_copy });

    // -- Shell executable ----------------------------------------------------

    const shell_mod = b.createModule(.{
        .root_source_file = b.path("src/shell.zig"),
        .target = target,
        .optimize = optimize,
    });

    // Version string for the shell banner — read from ./VERSION at build time
    // (the single source of truth the release script bumps).
    const shell_opts = b.addOptions();
    const version_raw = @embedFile("VERSION");
    shell_opts.addOption([]const u8, "version", std.mem.trim(u8, version_raw, " \t\r\n"));
    shell_mod.addOptions("build_opts", shell_opts);

    const shell = b.addExecutable(.{
        .name = "birnam",
        .root_module = shell_mod,
    });
    shell.linkLibrary(libffi_lib);
    shell.linkLibC();
    b.installArtifact(shell);

    // -- Phase 14 M1: AOT static executable (baked image + modules) ----------
    // `zig build aot` produces a standalone binary embedding src/aot_embed.bim
    // with peg + lang statically linked (no dylibs). Slice: --aot=static,
    // no strip.
    //
    // M4: gate the `birnam_module_init` C export. Dylib builds export it
    // (dlopen entry); the static AOT binary imports module sources and calls
    // moduleInitImpl directly, so it must NOT export (duplicate symbol across
    // statically-linked modules).
    const dylib_init_opts = b.addOptions();
    dylib_init_opts.addOption(bool, "export_init", true);

    const aot_step = b.step("aot", "Build the static AOT executables (embed src/aot_embed.bim)");

    // No-strip artifact: full tower, REPL over baked state.
    const full_opts = b.addOptions();
    full_opts.addOption(bool, "export_init", false);
    full_opts.addOption(bool, "strip_malleable", false);
    full_opts.addOption([]const u8, "entry_selector", "");
    full_opts.addOption(i64, "entry_recv", 0);
    full_opts.addOption([]const u8, "entry_recv_name", "");
    full_opts.addOption(bool, "strip_ast", false);
    full_opts.addOption(bool, "dynamic", false);
    full_opts.addOption(bool, "strip_history", false);
    full_opts.addOption([]const u8, "embed_path", "aot_embed.bim");
    const aot_full_mod = b.createModule(.{
        .root_source_file = b.path("src/aot_main.zig"),
        .target = target,
        .optimize = optimize,
    });
    aot_full_mod.addOptions("build_opts", full_opts); // peg/lang @import resolve here
    aot_full_mod.addImport("kernel_eids", eid_mirror_mod); // lang.zig mirror
    aot_full_mod.addImport("module_abi", module_abi_mod);
    const aot_full = b.addExecutable(.{ .name = "birnam-aot", .root_module = aot_full_mod });
    aot_full.linkLibrary(libffi_lib);
    aot_full.linkLibC();
    aot_step.dependOn(&b.addInstallArtifact(aot_full, .{}).step);

    // --strip=malleable artifact: pure-VM, no parser/frontend; entry sends
    // `double` to SmallInt 5 (=> 10) to demonstrate baked bytecode dispatch.
    const mall_opts = b.addOptions();
    mall_opts.addOption(bool, "export_init", false);
    mall_opts.addOption(bool, "strip_malleable", true);
    mall_opts.addOption([]const u8, "entry_selector", "double");
    mall_opts.addOption(bool, "strip_ast", false);
    mall_opts.addOption(i64, "entry_recv", 5);
    mall_opts.addOption([]const u8, "entry_recv_name", "");
    mall_opts.addOption(bool, "dynamic", false);
    mall_opts.addOption(bool, "strip_history", false);
    mall_opts.addOption([]const u8, "embed_path", "aot_embed.bim");
    const aot_mall_mod = b.createModule(.{
        .root_source_file = b.path("src/aot_main.zig"),
        .target = target,
        .optimize = optimize,
    });
    aot_mall_mod.addOptions("build_opts", mall_opts);
    aot_mall_mod.addImport("kernel_eids", eid_mirror_mod); // lang.zig mirror
    aot_mall_mod.addImport("module_abi", module_abi_mod);
    const aot_mall = b.addExecutable(.{ .name = "birnam-aot-malleable", .root_module = aot_mall_mod });
    aot_mall.linkLibrary(libffi_lib);
    aot_mall.linkLibC();
    aot_step.dependOn(&b.addInstallArtifact(aot_mall, .{}).step);

    // Parameterized AOT build for the `birnam --aot` driver. The driver copies
    // the user's image to `-Daot-embed`, sets strip/entry flags, runs this
    // step, then copies zig-out/bin/<aot-name> to the user's -o path.
    {
        const opts = b.addOptions();
        opts.addOption(bool, "export_init", false);
        opts.addOption(bool, "strip_malleable", b.option(bool, "aot-strip-malleable", "AOT: drop parser/compiler/frontend") orelse false);
        opts.addOption(bool, "strip_ast", b.option(bool, "aot-strip-ast", "AOT: drop user AST trees") orelse false);
        opts.addOption([]const u8, "entry_selector", b.option([]const u8, "aot-entry-selector", "AOT: entry selector") orelse "");
        opts.addOption(i64, "entry_recv", b.option(i64, "aot-entry-recv", "AOT: entry SmallInt receiver") orelse 0);
        opts.addOption([]const u8, "entry_recv_name", b.option([]const u8, "aot-entry-recv-name", "AOT: entry receiver = top-level binding name") orelse "");
        opts.addOption([]const u8, "embed_path", b.option([]const u8, "aot-embed", "AOT: image to embed (relative to src/)") orelse "aot_embed.bim");
        opts.addOption(bool, "dynamic", b.option(bool, "aot-dynamic", "AOT: dynamic mode (dlopen modules at startup)") orelse false);
        opts.addOption(bool, "strip_history", b.option(bool, "aot-strip-history", "AOT: freeze store read-only (no tx-writer/history)") orelse false);
        const name = b.option([]const u8, "aot-name", "AOT: output executable name") orelse "birnam-aot-app";
        const mod = b.createModule(.{ .root_source_file = b.path("src/aot_main.zig"), .target = target, .optimize = optimize });
        mod.addOptions("build_opts", opts);
        mod.addImport("kernel_eids", eid_mirror_mod); // lang.zig mirror
        mod.addImport("module_abi", module_abi_mod);
        const exe = b.addExecutable(.{ .name = name, .root_module = mod });
        exe.linkLibrary(libffi_lib);
        exe.linkLibC();
        const build_step = b.step("aot-build", "Parameterized AOT build (driven by `birnam --aot`)");
        build_step.dependOn(&b.addInstallArtifact(exe, .{}).step);
    }

    // -- Hello dylib (Phase 5 smoke-test module) -----------------------------

    const hello_mod = b.createModule(.{
        .root_source_file = b.path("src/modules/hello/hello.zig"),
        .target = target,
        .optimize = optimize,
    });
    hello_mod.addImport("module_abi", module_abi_mod);
    const hello_lib = b.addLibrary(.{
        .name = "birnam_hello",
        .root_module = hello_mod,
        .linkage = .dynamic,
    });
    b.installArtifact(hello_lib);

    // -- REPL dylib (Phase 6) -------------------------------------------------

    const repl_mod = b.createModule(.{
        .root_source_file = b.path("src/modules/repl/repl.zig"),
        .target = target,
        .optimize = optimize,
    });
    repl_mod.addImport("module_abi", module_abi_mod);
    const repl_lib = b.addLibrary(.{
        .name = "birnam_repl",
        .root_module = repl_mod,
        .linkage = .dynamic,
    });
    b.installArtifact(repl_lib);

    // -- Query dylib (Phase 7) ------------------------------------------------

    const query_mod = b.createModule(.{
        .root_source_file = b.path("src/modules/query/query.zig"),
        .target = target,
        .optimize = optimize,
    });
    query_mod.addImport("module_abi", module_abi_mod);
    const query_lib = b.addLibrary(.{
        .name = "birnam_query",
        .root_module = query_mod,
        .linkage = .dynamic,
    });
    b.installArtifact(query_lib);

    // -- JSON dylib (Phase 17.10 — birnam-authored, files in json.birnam) -----
    const json_mod = b.createModule(.{
        .root_source_file = b.path("src/modules/json/json.zig"),
        .target = target,
        .optimize = optimize,
    });
    json_mod.addImport("module_abi", module_abi_mod);
    const json_lib = b.addLibrary(.{
        .name = "birnam_json",
        .root_module = json_mod,
        .linkage = .dynamic,
    });
    b.installArtifact(json_lib);

    // -- stdlib dylib (Phase 17.13 — birnam-authored Test/Stats/Csv/Log) ------
    const stdlib_mod = b.createModule(.{
        .root_source_file = b.path("src/modules/stdlib/stdlib.zig"),
        .target = target,
        .optimize = optimize,
    });
    stdlib_mod.addImport("module_abi", module_abi_mod);
    const stdlib_lib = b.addLibrary(.{
        .name = "birnam_stdlib",
        .root_module = stdlib_mod,
        .linkage = .dynamic,
    });
    b.installArtifact(stdlib_lib);

    // -- Net dylib (Phase 17.13 — TCP over std.net) --------------------------
    const net_mod = b.createModule(.{
        .root_source_file = b.path("src/modules/net/net.zig"),
        .target = target,
        .optimize = optimize,
    });
    net_mod.addImport("module_abi", module_abi_mod);
    const net_lib = b.addLibrary(.{
        .name = "birnam_net",
        .root_module = net_mod,
        .linkage = .dynamic,
    });
    b.installArtifact(net_lib);

    // -- PEG dylib (Phase 10.0) ----------------------------------------------

    const peg_mod = b.createModule(.{
        .root_source_file = b.path("src/modules/peg/peg.zig"),
        .target = target,
        .optimize = optimize,
    });
    peg_mod.addOptions("build_opts", dylib_init_opts);
    peg_mod.addImport("module_abi", module_abi_mod);
    const peg_lib = b.addLibrary(.{
        .name = "birnam_peg",
        .root_module = peg_mod,
        .linkage = .dynamic,
    });
    b.installArtifact(peg_lib);

    // -- Lang dylib (Phase 10.1) ---------------------------------------------

    const lang_mod = b.createModule(.{
        .root_source_file = b.path("src/modules/lang/lang.zig"),
        .target = target,
        .optimize = optimize,
    });
    lang_mod.addOptions("build_opts", dylib_init_opts);
    lang_mod.addImport("kernel_eids", eid_mirror_mod); // single-source-of-truth EID mirror
    lang_mod.addImport("module_abi", module_abi_mod);
    const lang_lib = b.addLibrary(.{
        .name = "birnam_lang",
        .root_module = lang_mod,
        .linkage = .dynamic,
    });
    b.installArtifact(lang_lib);

    // -- Tests ---------------------------------------------------------------

    const test_step = b.step("test", "Run unit tests");

    const hamt_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/hamt_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const hamt_tests = b.addTest(.{ .root_module = hamt_test_mod });
    const run_hamt_tests = b.addRunArtifact(hamt_tests);
    test_step.dependOn(&run_hamt_tests.step);

    // libtcc JIT wrapper (W1 → libtcc, docs/10a). Links the system libtcc; the
    // dev install lives under /usr/local (vendoring like libffi is a later step).
    const jit_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/jit_test.zig"),
        .target = target,
        .optimize = optimize,
        .link_libc = true,
    });
    jit_test_mod.addIncludePath(.{ .cwd_relative = "/usr/local/include" });
    jit_test_mod.addLibraryPath(.{ .cwd_relative = "/usr/local/lib" });
    jit_test_mod.linkSystemLibrary("tcc", .{});
    const jit_tests = b.addTest(.{ .root_module = jit_test_mod });
    const run_jit_tests = b.addRunArtifact(jit_tests);
    test_step.dependOn(&run_jit_tests.step);

    const rrb_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/rrb_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const rrb_tests = b.addTest(.{ .root_module = rrb_test_mod });
    const run_rrb_tests = b.addRunArtifact(rrb_tests);
    test_step.dependOn(&run_rrb_tests.step);

    const ref_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/ref_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const ref_tests = b.addTest(.{ .root_module = ref_test_mod });
    const run_ref_tests = b.addRunArtifact(ref_tests);
    test_step.dependOn(&run_ref_tests.step);

    const vm_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/vm_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const vm_tests = b.addTest(.{ .root_module = vm_test_mod });
    const run_vm_tests = b.addRunArtifact(vm_tests);
    test_step.dependOn(&run_vm_tests.step);

    const bytecode_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/bytecode_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const bytecode_tests = b.addTest(.{ .root_module = bytecode_test_mod });
    const run_bytecode_tests = b.addRunArtifact(bytecode_tests);
    test_step.dependOn(&run_bytecode_tests.step);

    const image_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/image_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const image_tests = b.addTest(.{ .root_module = image_test_mod });
    image_tests.linkLibrary(libffi_lib); // bootstrap → ffi → dyncall
    image_tests.linkLibC();
    const run_image_tests = b.addRunArtifact(image_tests);
    test_step.dependOn(&run_image_tests.step);

    const prune_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/prune_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const prune_tests = b.addTest(.{ .root_module = prune_test_mod });
    const run_prune_tests = b.addRunArtifact(prune_tests);
    test_step.dependOn(&run_prune_tests.step);

    const arena_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/arena_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const arena_tests = b.addTest(.{ .root_module = arena_test_mod });
    const run_arena_tests = b.addRunArtifact(arena_tests);
    test_step.dependOn(&run_arena_tests.step);

    const arenas_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/arenas_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const arenas_tests = b.addTest(.{ .root_module = arenas_test_mod });
    const run_arenas_tests = b.addRunArtifact(arenas_tests);
    test_step.dependOn(&run_arenas_tests.step);

    const gc_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/gc_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const gc_tests = b.addTest(.{ .root_module = gc_test_mod });
    const run_gc_tests = b.addRunArtifact(gc_tests);
    test_step.dependOn(&run_gc_tests.step);

    const tx_engine_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/tx_engine_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const tx_engine_tests = b.addTest(.{ .root_module = tx_engine_test_mod });
    const run_tx_engine_tests = b.addRunArtifact(tx_engine_tests);
    test_step.dependOn(&run_tx_engine_tests.step);

    const compiler_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/compiler_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const compiler_tests = b.addTest(.{ .root_module = compiler_test_mod });
    const run_compiler_tests = b.addRunArtifact(compiler_tests);
    test_step.dependOn(&run_compiler_tests.step);

    const dispatch_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/dispatch_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const dispatch_tests = b.addTest(.{ .root_module = dispatch_test_mod });
    const run_dispatch_tests = b.addRunArtifact(dispatch_tests);
    test_step.dependOn(&run_dispatch_tests.step);

    // Phase 11.9 tail-call probe — CI gate. ReleaseFast forces the optimizer
    // to honor `@call(.always_tail, ...)`. Debug mode also runs the probe
    // via the regular test step but tolerates the deeper frames since
    // Debug's stack discipline is non-representative of release builds.
    const tail_call_probe_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/tail_call_probe_test.zig"),
        .target = target,
        .optimize = .ReleaseFast,
    });
    const tail_call_probe = b.addTest(.{ .root_module = tail_call_probe_mod });
    const run_tail_call_probe = b.addRunArtifact(tail_call_probe);
    test_step.dependOn(&run_tail_call_probe.step);

    const ot_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/object_table_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const ot_tests = b.addTest(.{ .root_module = ot_test_mod });
    ot_tests.linkLibrary(libffi_lib);
    ot_tests.linkLibC();
    const run_ot_tests = b.addRunArtifact(ot_tests);
    test_step.dependOn(&run_ot_tests.step);

    const fs_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/fact_store_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const fs_tests = b.addTest(.{ .root_module = fs_test_mod });
    const run_fs_tests = b.addRunArtifact(fs_tests);
    test_step.dependOn(&run_fs_tests.step);

    const boot_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/bootstrap_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const boot_tests = b.addTest(.{ .root_module = boot_test_mod });
    boot_tests.linkLibrary(libffi_lib);
    boot_tests.linkLibC();
    const run_boot_tests = b.addRunArtifact(boot_tests);
    test_step.dependOn(&run_boot_tests.step);

    const str_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/strings_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const str_tests = b.addTest(.{ .root_module = str_test_mod });
    str_tests.linkLibrary(libffi_lib);
    str_tests.linkLibC();
    const run_str_tests = b.addRunArtifact(str_tests);
    test_step.dependOn(&run_str_tests.step);

    // Module integration tests depend on the hello dylib being installed.
    const mod_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/module_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const mod_tests = b.addTest(.{ .root_module = mod_test_mod });
    mod_tests.linkLibrary(libffi_lib);
    mod_tests.linkLibC();
    const run_mod_tests = b.addRunArtifact(mod_tests);
    run_mod_tests.step.dependOn(b.getInstallStep());
    test_step.dependOn(&run_mod_tests.step);

    const repl_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/repl_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const repl_tests = b.addTest(.{ .root_module = repl_test_mod });
    repl_tests.linkLibrary(libffi_lib);
    repl_tests.linkLibC();
    const run_repl_tests = b.addRunArtifact(repl_tests);
    run_repl_tests.step.dependOn(b.getInstallStep());
    test_step.dependOn(&run_repl_tests.step);

    const query_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/query_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const query_tests = b.addTest(.{ .root_module = query_test_mod });
    const run_query_tests = b.addRunArtifact(query_tests);
    test_step.dependOn(&run_query_tests.step);

    const peg_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/peg_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const peg_tests = b.addTest(.{ .root_module = peg_test_mod });
    peg_tests.linkLibrary(libffi_lib);
    peg_tests.linkLibC();
    const run_peg_tests = b.addRunArtifact(peg_tests);
    test_step.dependOn(&run_peg_tests.step);

    const qmod_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/query_module_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const qmod_tests = b.addTest(.{ .root_module = qmod_test_mod });
    qmod_tests.linkLibrary(libffi_lib);
    qmod_tests.linkLibC();
    const run_qmod_tests = b.addRunArtifact(qmod_tests);
    run_qmod_tests.step.dependOn(b.getInstallStep());
    test_step.dependOn(&run_qmod_tests.step);

    const pmod_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/peg_module_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const pmod_tests = b.addTest(.{ .root_module = pmod_test_mod });
    pmod_tests.linkLibrary(libffi_lib);
    pmod_tests.linkLibC();
    const run_pmod_tests = b.addRunArtifact(pmod_tests);
    run_pmod_tests.step.dependOn(b.getInstallStep());
    test_step.dependOn(&run_pmod_tests.step);

    const lmod_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/lang_module_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const lmod_tests = b.addTest(.{ .root_module = lmod_test_mod });
    lmod_tests.linkLibrary(libffi_lib);
    lmod_tests.linkLibC();
    const run_lmod_tests = b.addRunArtifact(lmod_tests);
    run_lmod_tests.step.dependOn(b.getInstallStep());
    test_step.dependOn(&run_lmod_tests.step);

    const rule_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/rule_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const rule_tests = b.addTest(.{ .root_module = rule_test_mod });
    const run_rule_tests = b.addRunArtifact(rule_tests);
    test_step.dependOn(&run_rule_tests.step);

    const agg_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/aggregate_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const agg_tests = b.addTest(.{ .root_module = agg_test_mod });
    const run_agg_tests = b.addRunArtifact(agg_tests);
    test_step.dependOn(&run_agg_tests.step);

    const sq_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/saved_query_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const sq_tests = b.addTest(.{ .root_module = sq_test_mod });
    sq_tests.linkLibrary(libffi_lib);
    sq_tests.linkLibC();
    const run_sq_tests = b.addRunArtifact(sq_tests);
    test_step.dependOn(&run_sq_tests.step);

    const pull_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/pull_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const pull_tests = b.addTest(.{ .root_module = pull_test_mod });
    const run_pull_tests = b.addRunArtifact(pull_tests);
    test_step.dependOn(&run_pull_tests.step);

    const reify_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/reification_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const reify_tests = b.addTest(.{ .root_module = reify_test_mod });
    reify_tests.linkLibrary(libffi_lib);
    reify_tests.linkLibC();
    const run_reify_tests = b.addRunArtifact(reify_tests);
    test_step.dependOn(&run_reify_tests.step);

    const pred_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/predicate_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const pred_tests = b.addTest(.{ .root_module = pred_test_mod });
    pred_tests.linkLibrary(libffi_lib);
    pred_tests.linkLibC();
    const run_pred_tests = b.addRunArtifact(pred_tests);
    test_step.dependOn(&run_pred_tests.step);

    const promo_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/promotion_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const promo_tests = b.addTest(.{ .root_module = promo_test_mod });
    promo_tests.linkLibrary(libffi_lib);
    promo_tests.linkLibC();
    const run_promo_tests = b.addRunArtifact(promo_tests);
    test_step.dependOn(&run_promo_tests.step);

    const mbind_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/metabind_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const mbind_tests = b.addTest(.{ .root_module = mbind_test_mod });
    mbind_tests.linkLibrary(libffi_lib);
    mbind_tests.linkLibC();
    const run_mbind_tests = b.addRunArtifact(mbind_tests);
    test_step.dependOn(&run_mbind_tests.step);

    const boxed_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/boxed_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const boxed_tests = b.addTest(.{ .root_module = boxed_test_mod });
    boxed_tests.linkLibrary(libffi_lib);
    boxed_tests.linkLibC();
    const run_boxed_tests = b.addRunArtifact(boxed_tests);
    test_step.dependOn(&run_boxed_tests.step);

    const ffi_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/ffi_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const ffi_tests = b.addTest(.{ .root_module = ffi_test_mod });
    ffi_tests.linkLibrary(libffi_lib);
    ffi_tests.linkLibC();
    const run_ffi_tests = b.addRunArtifact(ffi_tests);
    test_step.dependOn(&run_ffi_tests.step);

    const macho_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/macho_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const macho_tests = b.addTest(.{ .root_module = macho_test_mod });
    const run_macho_tests = b.addRunArtifact(macho_tests);
    // Mach-O tests read libbirnam_hello.dylib from zig-out/lib; ensure the
    // install step has run first.
    run_macho_tests.step.dependOn(b.getInstallStep());
    test_step.dependOn(&run_macho_tests.step);

    const ffi_mod_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/ffi_module_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const ffi_mod_tests = b.addTest(.{ .root_module = ffi_mod_test_mod });
    ffi_mod_tests.linkLibrary(libffi_lib);
    ffi_mod_tests.linkLibC();
    const run_ffi_mod_tests = b.addRunArtifact(ffi_mod_tests);
    // Symbols-enumeration tests read libbirnam_hello.dylib from zig-out/lib.
    run_ffi_mod_tests.step.dependOn(b.getInstallStep());
    test_step.dependOn(&run_ffi_mod_tests.step);

    const card_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/cardinality_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const card_tests = b.addTest(.{ .root_module = card_test_mod });
    card_tests.linkLibrary(libffi_lib);
    card_tests.linkLibC();
    const run_card_tests = b.addRunArtifact(card_tests);
    test_step.dependOn(&run_card_tests.step);

    const loader_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/loader_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const loader_tests = b.addTest(.{ .root_module = loader_test_mod });
    loader_tests.linkLibrary(libffi_lib);
    loader_tests.linkLibC();
    const run_loader_tests = b.addRunArtifact(loader_tests);
    run_loader_tests.step.dependOn(b.getInstallStep());
    test_step.dependOn(&run_loader_tests.step);

    // -- Benchmarks (ReleaseFast, separate step) ------------------------------

    const bench_step = b.step("bench", "Run microbenchmarks (ReleaseFast)");

    const hamt_for_bench = b.createModule(.{
        .root_source_file = b.path("src/kernel/hamt.zig"),
        .target = target,
        .optimize = .ReleaseFast,
    });
    const hamt_bench_mod = b.createModule(.{
        .root_source_file = b.path("bench/hamt_bench.zig"),
        .target = target,
        .optimize = .ReleaseFast,
    });
    hamt_bench_mod.addImport("hamt", hamt_for_bench);
    const hamt_bench_exe = b.addExecutable(.{
        .name = "hamt_bench",
        .root_module = hamt_bench_mod,
    });
    const run_hamt_bench = b.addRunArtifact(hamt_bench_exe);
    bench_step.dependOn(&run_hamt_bench.step);

    const rrb_for_bench = b.createModule(.{
        .root_source_file = b.path("src/kernel/rrb.zig"),
        .target = target,
        .optimize = .ReleaseFast,
    });
    const rrb_bench_mod = b.createModule(.{
        .root_source_file = b.path("bench/rrb_bench.zig"),
        .target = target,
        .optimize = .ReleaseFast,
    });
    rrb_bench_mod.addImport("rrb", rrb_for_bench);
    const rrb_bench_exe = b.addExecutable(.{
        .name = "rrb_bench",
        .root_module = rrb_bench_mod,
    });
    const run_rrb_bench = b.addRunArtifact(rrb_bench_exe);
    bench_step.dependOn(&run_rrb_bench.step);

    // Kernel-primitive bench (dispatch + field read). Lives in src/kernel/ so it
    // imports kernel siblings by relative path (like the *_test.zig files).
    // Self-capped allocator (FixedBufferAllocator) — safe by construction. NOT
    // auto-run by `zig build bench`; build with `zig build kernel-bench` then run
    // explicitly under `timeout`, e.g.:
    //   timeout 30 ./zig-out/bin/kernel_bench 1000000
    const kbench_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/kernel_bench.zig"),
        .target = target,
        .optimize = .ReleaseFast,
    });
    // ReleaseFast libffi for the (ReleaseFast) bench — matching optimize modes
    // avoids a ubsan-runtime link mismatch against the default-optimize lib.
    const libffi_rf = b.addLibrary(.{
        .name = "ffi_rf",
        .linkage = .static,
        .root_module = b.createModule(.{
            .target = target,
            .optimize = .ReleaseFast,
            .link_libc = true,
        }),
    });
    libffi_rf.addCSourceFiles(.{
        .root = b.path("vendor/libffi"),
        .files = &.{
            "src/prep_cif.c",
            "src/types.c",
            "src/raw_api.c",
            "src/java_raw_api.c",
            "src/closures.c",
            "src/tramp.c",
            "src/aarch64/ffi.c",
            "src/aarch64/sysv.S",
        },
        .flags = &.{ "-std=gnu11", "-DHAVE_CONFIG_H" },
    });
    libffi_rf.addIncludePath(b.path("vendor/libffi"));
    libffi_rf.addIncludePath(b.path("vendor/libffi/include"));
    libffi_rf.addIncludePath(b.path("vendor/libffi/src/aarch64"));

    const kbench_exe = b.addExecutable(.{
        .name = "kernel_bench",
        .root_module = kbench_mod,
    });
    kbench_exe.linkLibrary(libffi_rf);
    kbench_exe.linkLibC();
    const kbench_build_step = b.step("kernel-bench", "Build kernel_bench exe (run manually under timeout)");
    kbench_build_step.dependOn(&b.addInstallArtifact(kbench_exe, .{}).step);

    // Fact-WRITE bench (the memory-growing class). CappedAllocator (real free +
    // hard 512 MiB ceiling) fences it. NOT auto-run. Run under timeout with a
    // small count first, watch peak, scale carefully:
    //   timeout 30 ./zig-out/bin/kernel_write_bench distinct 10000
    const wbench_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/kernel_write_bench.zig"),
        .target = target,
        .optimize = .ReleaseFast,
    });
    const wbench_exe = b.addExecutable(.{
        .name = "kernel_write_bench",
        .root_module = wbench_mod,
    });
    wbench_exe.linkLibrary(libffi_rf);
    wbench_exe.linkLibC();
    const wbench_build_step = b.step("write-bench", "Build kernel_write_bench exe (run manually under timeout)");
    wbench_build_step.dependOn(&b.addInstallArtifact(wbench_exe, .{}).step);
}
