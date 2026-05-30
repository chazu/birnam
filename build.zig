const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    // -- dyncall static lib (Phase 9 vendored FFI engine) --------------------
    // Macro-driven preprocessor selects the right arch source per platform.
    // arm64 darwin and x86_64 darwin/linux carried; other archs trimmed.
    const dyncall_lib = b.addLibrary(.{
        .name = "dyncall",
        .linkage = .static,
        .root_module = b.createModule(.{
            .target = target,
            .optimize = optimize,
            .link_libc = true,
        }),
    });
    dyncall_lib.addCSourceFiles(.{
        .root = b.path("vendor/dyncall-1.4/dyncall"),
        .files = &.{
            "dyncall_api.c",
            "dyncall_callf.c",
            "dyncall_callvm.c",
            "dyncall_callvm_base.c",
            "dyncall_vector.c",
            "dyncall_aggregate.c",
            "dyncall_call.S",
        },
        .flags = &.{"-std=c99"},
    });
    dyncall_lib.addIncludePath(b.path("vendor/dyncall-1.4/dyncall"));
    b.installArtifact(dyncall_lib);

    // -- Shell executable ----------------------------------------------------

    const shell_mod = b.createModule(.{
        .root_source_file = b.path("src/shell.zig"),
        .target = target,
        .optimize = optimize,
    });

    const shell = b.addExecutable(.{
        .name = "birnam",
        .root_module = shell_mod,
    });
    shell.linkLibrary(dyncall_lib);
    shell.linkLibC();
    b.installArtifact(shell);

    // -- Hello dylib (Phase 5 smoke-test module) -----------------------------

    const hello_mod = b.createModule(.{
        .root_source_file = b.path("src/modules/hello/hello.zig"),
        .target = target,
        .optimize = optimize,
    });
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
    const query_lib = b.addLibrary(.{
        .name = "birnam_query",
        .root_module = query_mod,
        .linkage = .dynamic,
    });
    b.installArtifact(query_lib);

    // -- PEG dylib (Phase 10.0) ----------------------------------------------

    const peg_mod = b.createModule(.{
        .root_source_file = b.path("src/modules/peg/peg.zig"),
        .target = target,
        .optimize = optimize,
    });
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

    const ot_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/object_table_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const ot_tests = b.addTest(.{ .root_module = ot_test_mod });
    ot_tests.linkLibrary(dyncall_lib);
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
    boot_tests.linkLibrary(dyncall_lib);
    boot_tests.linkLibC();
    const run_boot_tests = b.addRunArtifact(boot_tests);
    test_step.dependOn(&run_boot_tests.step);

    const str_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/strings_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const str_tests = b.addTest(.{ .root_module = str_test_mod });
    str_tests.linkLibrary(dyncall_lib);
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
    mod_tests.linkLibrary(dyncall_lib);
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
    repl_tests.linkLibrary(dyncall_lib);
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
    peg_tests.linkLibrary(dyncall_lib);
    peg_tests.linkLibC();
    const run_peg_tests = b.addRunArtifact(peg_tests);
    test_step.dependOn(&run_peg_tests.step);

    const qmod_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/query_module_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const qmod_tests = b.addTest(.{ .root_module = qmod_test_mod });
    qmod_tests.linkLibrary(dyncall_lib);
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
    pmod_tests.linkLibrary(dyncall_lib);
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
    lmod_tests.linkLibrary(dyncall_lib);
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
    sq_tests.linkLibrary(dyncall_lib);
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
    reify_tests.linkLibrary(dyncall_lib);
    reify_tests.linkLibC();
    const run_reify_tests = b.addRunArtifact(reify_tests);
    test_step.dependOn(&run_reify_tests.step);

    const pred_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/predicate_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const pred_tests = b.addTest(.{ .root_module = pred_test_mod });
    pred_tests.linkLibrary(dyncall_lib);
    pred_tests.linkLibC();
    const run_pred_tests = b.addRunArtifact(pred_tests);
    test_step.dependOn(&run_pred_tests.step);

    const promo_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/promotion_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const promo_tests = b.addTest(.{ .root_module = promo_test_mod });
    promo_tests.linkLibrary(dyncall_lib);
    promo_tests.linkLibC();
    const run_promo_tests = b.addRunArtifact(promo_tests);
    test_step.dependOn(&run_promo_tests.step);

    const mbind_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/metabind_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const mbind_tests = b.addTest(.{ .root_module = mbind_test_mod });
    mbind_tests.linkLibrary(dyncall_lib);
    mbind_tests.linkLibC();
    const run_mbind_tests = b.addRunArtifact(mbind_tests);
    test_step.dependOn(&run_mbind_tests.step);

    const boxed_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/boxed_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const boxed_tests = b.addTest(.{ .root_module = boxed_test_mod });
    boxed_tests.linkLibrary(dyncall_lib);
    boxed_tests.linkLibC();
    const run_boxed_tests = b.addRunArtifact(boxed_tests);
    test_step.dependOn(&run_boxed_tests.step);

    const ffi_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/ffi_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const ffi_tests = b.addTest(.{ .root_module = ffi_test_mod });
    ffi_tests.linkLibrary(dyncall_lib);
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
    ffi_mod_tests.linkLibrary(dyncall_lib);
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
    card_tests.linkLibrary(dyncall_lib);
    card_tests.linkLibC();
    const run_card_tests = b.addRunArtifact(card_tests);
    test_step.dependOn(&run_card_tests.step);

    const loader_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/loader_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const loader_tests = b.addTest(.{ .root_module = loader_test_mod });
    loader_tests.linkLibrary(dyncall_lib);
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
}
