const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    // -- Shared library: libbirnam_id ----------------------------------------

    const id_mod = b.createModule(.{
        .root_source_file = b.path("src/id/id.zig"),
        .target = target,
        .optimize = optimize,
    });

    const id_lib = b.addLibrary(.{
        .name = "birnam_id",
        .root_module = id_mod,
        .linkage = .dynamic,
    });
    b.installArtifact(id_lib);

    // -- Shared library: libbirnam_peg ---------------------------------------

    const peg_mod = b.createModule(.{
        .root_source_file = b.path("src/peg/peg.zig"),
        .target = target,
        .optimize = optimize,
    });

    const peg_lib = b.addLibrary(.{
        .name = "birnam_peg",
        .root_module = peg_mod,
        .linkage = .dynamic,
    });
    b.installArtifact(peg_lib);

    // -- Shared library: libbirnam_lang --------------------------------------

    const lang_mod = b.createModule(.{
        .root_source_file = b.path("src/lang/lang.zig"),
        .target = target,
        .optimize = optimize,
    });

    const lang_lib = b.addLibrary(.{
        .name = "birnam_lang",
        .root_module = lang_mod,
        .linkage = .dynamic,
    });
    b.installArtifact(lang_lib);

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
    b.installArtifact(shell);

    // -- Tests ---------------------------------------------------------------

    const id_test_mod = b.createModule(.{
        .root_source_file = b.path("src/id/id.zig"),
        .target = target,
        .optimize = optimize,
    });

    const id_tests = b.addTest(.{
        .root_module = id_test_mod,
    });
    const run_id_tests = b.addRunArtifact(id_tests);

    const peg_test_mod = b.createModule(.{
        .root_source_file = b.path("src/peg/peg.zig"),
        .target = target,
        .optimize = optimize,
    });

    // PEG tests need access to id module internals for bootstrap
    const id_for_peg_test = b.createModule(.{
        .root_source_file = b.path("src/id/id.zig"),
        .target = target,
        .optimize = optimize,
    });
    peg_test_mod.addImport("id", id_for_peg_test);

    const peg_tests = b.addTest(.{
        .root_module = peg_test_mod,
    });
    const run_peg_tests = b.addRunArtifact(peg_tests);

    const lang_test_mod = b.createModule(.{
        .root_source_file = b.path("src/lang/lang.zig"),
        .target = target,
        .optimize = optimize,
    });

    // Lang tests need access to id module internals for bootstrap
    const id_for_lang_test = b.createModule(.{
        .root_source_file = b.path("src/id/id.zig"),
        .target = target,
        .optimize = optimize,
    });
    lang_test_mod.addImport("id", id_for_lang_test);

    const lang_tests = b.addTest(.{
        .root_module = lang_test_mod,
    });
    const run_lang_tests = b.addRunArtifact(lang_tests);

    const test_step = b.step("test", "Run unit tests");
    test_step.dependOn(&run_id_tests.step);
    test_step.dependOn(&run_peg_tests.step);
    test_step.dependOn(&run_lang_tests.step);
}
