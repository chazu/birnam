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
// The legacy fact-store kernel that preceded this lives only in git history.
// Its unrecovered arcs (persistence/images, struct-by-value FFI, value-slot
// globals) are intentionally outside the active source tree.
pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});
    const Floor = enum { zig, rust };
    const selected_floor = b.option(
        Floor,
        "floor",
        "Native id floor installed as libbirnam_id.dylib (zig or rust)",
    ) orelse .rust;

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

    // Build/test-only ABI evidence. The JSON manifest is deliberately exposed
    // only to this test module; runtime, install, and artifact-selection code do
    // not import it.
    const idapi_manifest_mod = b.createModule(.{
        .root_source_file = b.path("abi/idapi_manifest.zig"),
    });
    const idapi_abi_test_mod = b.createModule(.{
        .root_source_file = b.path("src/id/idapi_abi_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    idapi_abi_test_mod.addImport("idapi_manifest", idapi_manifest_mod);
    const idapi_abi_tests = b.addTest(.{ .root_module = idapi_abi_test_mod });
    const run_idapi_abi_tests = b.addRunArtifact(idapi_abi_tests);
    const test_id_abi_step = b.step(
        "test-id-abi",
        "Verify the committed language-neutral IdApi ABI manifest",
    );
    test_id_abi_step.dependOn(&run_idapi_abi_tests.step);
    test_step.dependOn(&run_idapi_abi_tests.step);

    // Build the Rust cdylib in the repository-local Cargo target directory,
    // then compare its compiler-derived layout descriptor with the same frozen
    // manifest. This test loads the artifact but never calls id_init.
    const rust_id_build = b.addSystemCommand(&.{
        "cargo",
        "build",
        "--workspace",
        "--locked",
        "--offline",
    });
    rust_id_build.setEnvironmentVariable(
        "CARGO_TARGET_DIR",
        b.pathFromRoot(".cargo-target"),
    );
    const rust_id_abi_test_mod = b.createModule(.{
        .root_source_file = b.path("src/id/rust_idapi_abi_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    rust_id_abi_test_mod.addImport("idapi_manifest", idapi_manifest_mod);
    const rust_id_abi_tests = b.addTest(.{ .root_module = rust_id_abi_test_mod });
    const run_rust_id_abi_tests = b.addRunArtifact(rust_id_abi_tests);
    run_rust_id_abi_tests.setEnvironmentVariable(
        "BIRNAM_RUST_ID_DYLIB",
        b.pathFromRoot(".cargo-target/debug/libbirnam_id.dylib"),
    );
    run_rust_id_abi_tests.step.dependOn(&rust_id_build.step);
    test_id_abi_step.dependOn(&run_rust_id_abi_tests.step);
    test_step.dependOn(&run_rust_id_abi_tests.step);

    const rust_ffi_abort_smoke = b.addSystemCommand(&.{
        "bash",
        "scripts/rust-ffi-abort-smoke.sh",
    });
    const test_rust_ffi_abort_step = b.step(
        "test-rust-ffi-abort",
        "Prove Rust panics and fatal invariants abort at the C ABI boundary",
    );
    test_rust_ffi_abort_step.dependOn(&rust_ffi_abort_smoke.step);
    test_step.dependOn(&rust_ffi_abort_smoke.step);

    const rust_dylib_audit = b.addSystemCommand(&.{
        "bash",
        "scripts/rust-dylib-audit.sh",
    });
    const test_rust_dylib_audit_step = b.step(
        "test-rust-dylib-audit",
        "Audit Rust floor dependencies, install name, paths, and exported symbols",
    );
    test_rust_dylib_audit_step.dependOn(&rust_dylib_audit.step);
    test_step.dependOn(&rust_dylib_audit.step);

    const rust_runtime_churn = b.addSystemCommand(&.{
        "bash",
        "scripts/rust-runtime-churn-smoke.sh",
    });
    const test_rust_runtime_churn_step = b.step(
        "test-rust-runtime-churn",
        "Stress Rust-floor module loads, image boots, and isolate process lifecycles",
    );
    test_rust_runtime_churn_step.dependOn(&rust_runtime_churn.step);

    const rust_heap_layout_smoke = b.addSystemCommand(&.{
        "bash",
        "scripts/rust-heap-layout-smoke.sh",
    });
    const test_rust_heap_layout_step = b.step(
        "test-rust-heap-layout",
        "Inspect Rust raw blocks through an external C-layout fixture",
    );
    test_rust_heap_layout_step.dependOn(&rust_heap_layout_smoke.step);
    test_step.dependOn(&rust_heap_layout_smoke.step);

    const rust_membership_bench = b.addSystemCommand(&.{
        "bash",
        "scripts/rust-membership-bench.sh",
    });
    b.step(
        "bench-rust-id-membership",
        "Benchmark optimized Rust exact-address membership scaling",
    ).dependOn(&rust_membership_bench.step);

    const rust_dispatch_reentry_smoke = b.addSystemCommand(&.{
        "bash",
        "scripts/rust-dispatch-reentry-smoke.sh",
    });
    const test_rust_dispatch_reentry_step = b.step(
        "test-rust-dispatch-reentry",
        "Reenter Rust dispatch synchronously from an external Zig Method",
    );
    test_rust_dispatch_reentry_step.dependOn(&rust_dispatch_reentry_smoke.step);
    test_step.dependOn(&rust_dispatch_reentry_smoke.step);

    const rust_id_spike_smoke = b.addSystemCommand(&.{
        "bash",
        "scripts/rust-id-spike-smoke.sh",
    });
    const test_rust_id_spike_step = b.step(
        "test-rust-id-spike",
        "Exercise and inspect the complete Rust IdApi from a frozen Zig client",
    );
    test_rust_id_spike_step.dependOn(&rust_id_spike_smoke.step);
    test_step.dependOn(&rust_id_spike_smoke.step);

    const rust_gc_metadata_smoke = b.addSystemCommand(&.{
        "bash",
        "scripts/rust-gc-metadata-smoke.sh",
    });
    const test_rust_gc_metadata_step = b.step(
        "test-rust-gc-metadata",
        "Exercise Rust collection, foreign marking callbacks, and finalizer guards",
    );
    test_rust_gc_metadata_step.dependOn(&rust_gc_metadata_smoke.step);
    test_step.dependOn(&rust_gc_metadata_smoke.step);

    const rust_native_memory_smoke = b.addSystemCommand(&.{
        "bash",
        "scripts/rust-native-memory-smoke.sh",
    });
    const test_rust_native_memory_step = b.step(
        "test-rust-native-memory",
        "Run cross-language Rust GC/finalizer fixtures with Darwin allocator diagnostics",
    );
    test_rust_native_memory_step.dependOn(&rust_native_memory_smoke.step);

    const rust_stack_root_smoke = b.addSystemCommand(&.{
        "bash",
        "scripts/rust-stack-root-smoke.sh",
    });
    const test_rust_stack_root_step = b.step(
        "test-rust-stack-root",
        "Keep an Oop live in optimized C and recover it from Rust native roots",
    );
    test_rust_stack_root_step.dependOn(&rust_stack_root_smoke.step);
    test_step.dependOn(&rust_stack_root_smoke.step);

    const rust_image_save_smoke = b.addSystemCommand(&.{
        "bash",
        "scripts/rust-image-save-smoke.sh",
    });
    rust_image_save_smoke.setEnvironmentVariable("BIRNAM_BIN", b.getInstallPath(.bin, "birnam"));
    rust_image_save_smoke.setEnvironmentVariable("BIRNAM_LIB_DIR", b.getInstallPath(.lib, ""));
    const test_rust_image_save_step = b.step(
        "test-rust-image-save",
        "Save a representative image through the CLI with both Rust floor profiles",
    );
    test_rust_image_save_step.dependOn(&rust_image_save_smoke.step);
    test_step.dependOn(&rust_image_save_smoke.step);

    const rust_id_abi_fixture_smoke = b.addSystemCommand(&.{
        "bash",
        "scripts/rust-id-abi-fixture-smoke.sh",
    });
    rust_id_abi_fixture_smoke.setEnvironmentVariable("BIRNAM_BIN", b.getInstallPath(.bin, "birnam"));
    const test_rust_id_abi_fixture_step = b.step(
        "test-rust-id-abi-fixture",
        "Run the frozen native client and pre-rewrite image against both Rust profiles",
    );
    test_rust_id_abi_fixture_step.dependOn(&rust_id_abi_fixture_smoke.step);
    test_step.dependOn(&rust_id_abi_fixture_smoke.step);

    const rust_env_loader_smoke = b.addSystemCommand(&.{
        "bash",
        "scripts/rust-env-loader-smoke.sh",
    });
    const test_rust_env_loader_step = b.step(
        "test-rust-env-loader",
        "Exercise Rust canonical exports and opaque module reload state",
    );
    test_rust_env_loader_step.dependOn(&rust_env_loader_smoke.step);
    test_step.dependOn(&rust_env_loader_smoke.step);

    const scalar_conformance_smoke = b.addSystemCommand(&.{
        "bash",
        "scripts/scalar-conformance-smoke.sh",
    });
    const test_scalar_conformance_step = b.step(
        "test-scalar-conformance",
        "Run boxed scalar conformance against Zig and Rust floors",
    );
    test_scalar_conformance_step.dependOn(&scalar_conformance_smoke.step);
    test_step.dependOn(&scalar_conformance_smoke.step);

    const string_conformance_smoke = b.addSystemCommand(&.{
        "bash",
        "scripts/string-conformance-smoke.sh",
    });
    const test_string_conformance_step = b.step(
        "test-string-conformance",
        "Run copied-byte String conformance against Zig and Rust floors",
    );
    test_string_conformance_step.dependOn(&string_conformance_smoke.step);
    test_step.dependOn(&string_conformance_smoke.step);

    const prototype_conformance_smoke = b.addSystemCommand(&.{
        "bash",
        "scripts/prototype-conformance-smoke.sh",
    });
    const test_prototype_conformance_step = b.step(
        "test-prototype-conformance",
        "Run prototype and singleton conformance against Zig and Rust floors",
    );
    test_prototype_conformance_step.dependOn(&prototype_conformance_smoke.step);
    test_step.dependOn(&prototype_conformance_smoke.step);

    const rust_store_mop_smoke = b.addSystemCommand(&.{
        "bash",
        "scripts/rust-store-mop-smoke.sh",
    });
    const test_rust_store_mop_step = b.step(
        "test-rust-store-mop",
        "Run the Store MOP consistency law against Rust debug and release floors",
    );
    test_rust_store_mop_step.dependOn(&rust_store_mop_smoke.step);
    test_step.dependOn(&rust_store_mop_smoke.step);

    const rust_lower_rung_matrix = b.addSystemCommand(&.{
        "bash",
        "scripts/rust-lower-rung-matrix.sh",
    });
    const test_rust_lower_rungs_step = b.step(
        "test-rust-lower-rungs",
        "Run the complete isolated 256-test matrix under Zig and both Rust profiles",
    );
    test_rust_lower_rungs_step.dependOn(&rust_lower_rung_matrix.step);

    const floor_zig_tests = b.addSystemCommand(&.{
        "bash",
        "scripts/rust-lower-rung-matrix.sh",
        "zig",
    });
    const test_floor_zig_step = b.step(
        "test-floor-zig",
        "Run the complete isolated selected-floor test matrix under Zig",
    );
    test_floor_zig_step.dependOn(&floor_zig_tests.step);

    const floor_rust_tests = b.addSystemCommand(&.{
        "bash",
        "scripts/rust-lower-rung-matrix.sh",
        "rust",
    });
    const test_floor_rust_step = b.step(
        "test-floor-rust",
        "Run the complete isolated selected-floor test matrix under Rust debug and release",
    );
    test_floor_rust_step.dependOn(&floor_rust_tests.step);

    const floor_zig_verify = b.addSystemCommand(&.{
        "bash",
        "scripts/floor-verify.sh",
        "zig",
    });
    const verify_floor_zig_step = b.step(
        "verify-floor-zig",
        "Run isolated in-process, CLI, worker, project, and install verification under Zig",
    );
    verify_floor_zig_step.dependOn(&floor_zig_verify.step);

    const floor_rust_verify = b.addSystemCommand(&.{
        "bash",
        "scripts/floor-verify.sh",
        "rust",
    });
    const verify_floor_rust_step = b.step(
        "verify-floor-rust",
        "Run isolated in-process, CLI, worker, project, and install verification under Rust",
    );
    verify_floor_rust_step.dependOn(&floor_rust_verify.step);

    const cross_floor_artifact_smoke = b.addSystemCommand(&.{
        "bash",
        "scripts/cross-floor-artifact-smoke.sh",
    });
    const cross_floor_artifact_step = b.step(
        "test-cross-floor-artifacts",
        "Compile artifacts with each floor and execute them with both floors",
    );
    cross_floor_artifact_step.dependOn(&cross_floor_artifact_smoke.step);

    const cross_floor_image_smoke = b.addSystemCommand(&.{
        "bash",
        "scripts/cross-floor-image-smoke.sh",
    });
    const cross_floor_image_step = b.step(
        "test-cross-floor-images",
        "Save with each floor and semantically verify mapped and rebuilt images with both",
    );
    cross_floor_image_step.dependOn(&cross_floor_image_smoke.step);

    const cross_floor_error_smoke = b.addSystemCommand(&.{
        "bash",
        "scripts/cross-floor-error-smoke.sh",
    });
    const cross_floor_error_step = b.step(
        "test-cross-floor-errors",
        "Compare specified language-visible error kinds, messages, and syntax diagnostics",
    );
    cross_floor_error_step.dependOn(&cross_floor_error_smoke.step);

    const repeat_floor_verify = b.addSystemCommand(&.{
        "bash",
        "scripts/repeat-floor-verification.sh",
        "2",
    });
    const repeat_floor_verify_step = b.step(
        "verify-floor-repeat",
        "Run two clean full-floor verification cycles in alternating order",
    );
    repeat_floor_verify_step.dependOn(&repeat_floor_verify.step);

    // Stage-1 compilers need a runnable host and native floor before the final
    // install can be complete. Keep that bootstrap closure distinct so the
    // ordinary install step can depend on compiled production units without a
    // dependency cycle.
    const bootstrap_install = b.step("bootstrap-install", "Install the native floor and compiler host needed to build compiled units");
    const installed_zig_oracle = b.getInstallPath(.lib, "libbirnam_id_zig.dylib");
    scalar_conformance_smoke.setEnvironmentVariable(
        "BIRNAM_ZIG_ID_DYLIB",
        installed_zig_oracle,
    );
    scalar_conformance_smoke.step.dependOn(bootstrap_install);
    string_conformance_smoke.setEnvironmentVariable(
        "BIRNAM_ZIG_ID_DYLIB",
        installed_zig_oracle,
    );
    string_conformance_smoke.step.dependOn(bootstrap_install);
    prototype_conformance_smoke.setEnvironmentVariable(
        "BIRNAM_ZIG_ID_DYLIB",
        installed_zig_oracle,
    );
    prototype_conformance_smoke.step.dependOn(bootstrap_install);
    rust_lower_rung_matrix.step.dependOn(bootstrap_install);
    test_step.dependOn(&rust_lower_rung_matrix.step);
    const rust_floor_profile = if (optimize == .Debug) "debug" else "release";
    const rust_floor_build = b.addSystemCommand(&.{
        "cargo",
        "build",
        "--workspace",
        "--locked",
        "--offline",
    });
    rust_floor_build.setEnvironmentVariable(
        "CARGO_TARGET_DIR",
        b.pathFromRoot(".cargo-target"),
    );
    if (optimize != .Debug) rust_floor_build.addArg("--release");
    if (selected_floor == .rust) {
        const install_rust_floor = b.addInstallFileWithDir(
            b.path(b.fmt(
                ".cargo-target/{s}/libbirnam_id.dylib",
                .{rust_floor_profile},
            )),
            .lib,
            "libbirnam_id.dylib",
        );
        install_rust_floor.step.dependOn(&rust_floor_build.step);
        b.getInstallStep().dependOn(&install_rust_floor.step);
        bootstrap_install.dependOn(&install_rust_floor.step);
    }
    const conformance_floor = b.option(
        []const u8,
        "id-conformance-floor",
        "Path to the one floor dylib loaded by selected-floor tests",
    ) orelse b.getInstallPath(.lib, "libbirnam_id.dylib");
    const conformance_identity = b.option(
        []const u8,
        "id-conformance-identity",
        "Expected birnam_id_floor_identity for selected-floor tests",
    ) orelse if (selected_floor == .zig) "zig-id-v1" else "rust-id-v1";
    const conformance_digest = b.option(
        []const u8,
        "id-conformance-sha256",
        "Expected SHA-256 of the selected floor dylib (empty disables the assertion)",
    ) orelse "";

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
        test_exports: []const []const u8 = &.{},
    };
    const mods = [_]Mod{
        .{ .name = "birnam_id", .src = "id.zig", .test_src = "id.zig", .step = "test-id", .desc = "libid: the Piumarta object-system bootstrap proof" },
        .{ .name = "birnam_id_artifact", .src = "artifact_inspector.zig", .test_src = "artifact_inspector_test.zig", .step = "test-artifact", .desc = "minimal self-describing artifact inspection over IdApi" },
        .{ .name = "birnam_store", .src = "store.zig", .test_src = "store_test.zig", .step = "test-store", .desc = "rung 2: the cell/relational store bootstrap fixpoint" },
        .{ .name = "birnam_id_peg", .src = "peg.zig", .test_src = "peg_test.zig", .step = "test-peg", .desc = "rung 3a: the PEG engine on the libid floor" },
        .{ .name = "birnam_id_lang", .src = "lang.zig", .test_src = "lang_test.zig", .step = "test-lang", .desc = "rung 3b: birnam's grammar + parser via the peg message api" },
        .{ .name = "birnam_id_tcc", .src = "tcc.zig", .test_src = "tcc_test.zig", .step = "test-tcc", .desc = "rung 3c: libtcc as an id module — C to native", .lib_links_tcc = true, .test_links_tcc = true, .test_exports = &.{"birnam_probe"} },
        .{ .name = "birnam_id_codegen", .src = "codegen.zig", .test_src = "codegen_test.zig", .step = "test-codegen", .desc = "rung 3c: the birnam → C emitter", .test_links_tcc = true, .test_exports = &.{"birnam_cg_apply2"} },
        .{ .name = "birnam_id_prelude", .src = "prelude.zig", .test_src = "prelude_test.zig", .step = "test-prelude", .desc = "the primitive stdlib: Integer/Boolean/System" },
        .{ .name = "birnam_id_collection", .src = "collection.zig", .test_src = "collection_test.zig", .step = "test-collection", .desc = "the id-native collection library: Vector trie + Range" },
        .{ .name = "birnam_id_error", .src = "error.zig", .test_src = "error_test.zig", .step = "test-error", .desc = "F7: the error model (propagating values + DNU swap)" },
        .{ .name = "birnam_id_isolate", .src = "isolate.zig", .test_src = "isolate_test.zig", .step = "test-isolate", .desc = "Blocking isolates and wire-v1" },
        .{ .name = "birnam_id_relational", .src = "relational.zig", .test_src = "relational_test.zig", .step = "test-relational", .desc = "the relational reading: datalog join + pull" },
        .{
            .name = "birnam_id_alien",
            .src = "alien.zig",
            .test_src = "alien_test.zig",
            .step = "test-alien",
            .desc = "the FFI: wrap + absorb C libraries via Tcc",
            .test_links_tcc = true,
            .test_exports = &.{
                "birnam_t_addPoints",
                "birnam_t_apply",
                "birnam_t_apply2",
                "birnam_t_applyD",
                "birnam_t_applyPt",
                "birnam_t_applyS",
                "birnam_t_divmod",
                "birnam_t_dotVec2",
                "birnam_t_fillGreeting",
                "birnam_t_fire",
                "birnam_t_flipLine",
                "birnam_t_highPtr",
                "birnam_t_iota",
                "birnam_t_makePoint",
                "birnam_t_mixedVal",
                "birnam_t_mkBig",
                "birnam_t_mkMixed",
                "birnam_t_movePoint",
                "birnam_t_ptrIdentity",
                "birnam_t_scaleVec2",
                "birnam_t_sumArr",
                "birnam_t_sumBig",
                "birnam_t_sumBuf",
                "birnam_t_sumF2",
                "birnam_t_sumLine",
                "birnam_t_sumPoint",
                "birnam_t_uBig",
                "birnam_t_uIdentity",
                "birnam_t_uSmall",
                "birnam_t_viaPtCb",
            },
        },
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
        if (std.mem.eql(u8, m.name, "birnam_id")) {
            const install_zig_oracle = b.addInstallArtifact(lib, .{
                .dest_sub_path = "libbirnam_id_zig.dylib",
            });
            b.getInstallStep().dependOn(&install_zig_oracle.step);
            bootstrap_install.dependOn(&install_zig_oracle.step);
            if (selected_floor == .zig) {
                const install_zig_floor = b.addInstallArtifact(lib, .{
                    .dest_sub_path = "libbirnam_id.dylib",
                });
                b.getInstallStep().dependOn(&install_zig_floor.step);
                bootstrap_install.dependOn(&install_zig_floor.step);
            }
        } else {
            const install_lib = b.addInstallArtifact(lib, .{});
            b.getInstallStep().dependOn(&install_lib.step);
            bootstrap_install.dependOn(&install_lib.step);
        }

        // The test suite.
        const test_mod = b.createModule(.{
            .root_source_file = b.path("src/id/" ++ m.test_src),
            .target = target,
            .optimize = optimize,
        });
        test_mod.export_symbol_names = m.test_exports;
        if (m.test_links_tcc) test_mod.addIncludePath(b.path("vendor/libtcc-src"));
        const tests = b.addTest(.{ .root_module = test_mod });
        if (m.test_links_tcc) {
            tests.linkLibrary(libtcc_lib);
            // TCC integration tests intentionally resolve their exported Zig
            // fixtures through dlsym(RTLD_DEFAULT). ReleaseSafe otherwise
            // dead-strips those unreferenced executable symbols.
            tests.rdynamic = true;
        }
        tests.linkLibC();
        const run = b.addRunArtifact(tests);
        if (!std.mem.eql(u8, m.test_src, "id.zig")) {
            run.setEnvironmentVariable("BIRNAM_ID_DYLIB", conformance_floor);
            run.setEnvironmentVariable("BIRNAM_ID_FLOOR_IDENTITY", conformance_identity);
            run.setEnvironmentVariable("BIRNAM_ID_FLOOR_SHA256", conformance_digest);
            run.step.dependOn(bootstrap_install);
        }
        b.step(m.step, m.desc).dependOn(&run.step);
        test_step.dependOn(&run.step);
    }

    const artifact_preflight_test_mod = b.createModule(.{
        .root_source_file = b.path("src/id/artifact_preflight_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const artifact_preflight_tests = b.addTest(.{ .root_module = artifact_preflight_test_mod });
    artifact_preflight_tests.linkLibC();
    const artifact_preflight_sentinel_mod = b.createModule(.{
        .root_source_file = b.path("src/id/artifact_preflight_sentinel.zig"),
        .target = target,
        .optimize = optimize,
    });
    const artifact_preflight_sentinel = b.addLibrary(.{
        .name = "birnam_artifact_preflight_sentinel",
        .root_module = artifact_preflight_sentinel_mod,
        .linkage = .dynamic,
    });
    artifact_preflight_sentinel.linkLibC();
    const run_artifact_preflight_tests = b.addSystemCommand(&.{ "bash", "scripts/artifact-preflight-sentinel.sh" });
    run_artifact_preflight_tests.addArtifactArg(artifact_preflight_tests);
    run_artifact_preflight_tests.addArtifactArg(artifact_preflight_sentinel);
    run_artifact_preflight_tests.setEnvironmentVariable("BIRNAM_ID_DYLIB", conformance_floor);
    run_artifact_preflight_tests.setEnvironmentVariable("BIRNAM_ID_FLOOR_IDENTITY", conformance_identity);
    run_artifact_preflight_tests.setEnvironmentVariable("BIRNAM_ID_FLOOR_SHA256", conformance_digest);
    run_artifact_preflight_tests.step.dependOn(bootstrap_install);
    const artifact_preflight_test_step = b.step(
        "test-artifact-preflight",
        "Prove pointer-free artifact closure rejection before activation",
    );
    artifact_preflight_test_step.dependOn(&run_artifact_preflight_tests.step);
    test_step.dependOn(&run_artifact_preflight_tests.step);

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
    const install_siphon = b.addInstallArtifact(siphon_exe, .{});
    b.getInstallStep().dependOn(&install_siphon.step);
    bootstrap_install.dependOn(&install_siphon.step);

    // Birnam module source is embedded in its compiled unit. Native floor
    // sources and authored guides remain separate documentation resources.
    var source_dir = std.fs.cwd().openDir("src/id", .{ .iterate = true }) catch
        @panic("could not enumerate src/id for installed Zig-floor documentation");
    defer source_dir.close();
    var source_entries = source_dir.iterate();
    while (source_entries.next() catch @panic("could not enumerate src/id")) |entry| {
        if (entry.kind != .file or !std.mem.endsWith(u8, entry.name, ".zig") or
            std.mem.endsWith(u8, entry.name, "_test.zig")) continue;
        const install_source = b.addInstallFileWithDir(
            b.path(b.fmt("src/id/{s}", .{entry.name})),
            .prefix,
            b.fmt("share/birnam/src/id/{s}", .{entry.name}),
        );
        b.getInstallStep().dependOn(&install_source.step);
    }
    b.installDirectory(.{
        .source_dir = b.path("rust/birnam-id/src"),
        .install_dir = .prefix,
        .install_subdir = "share/birnam/src/rust/birnam-id/src",
        .include_extensions = &.{".rs"},
    });
    const rust_source_files = [_]struct { src: []const u8, dest: []const u8 }{
        .{ .src = "Cargo.toml", .dest = "share/birnam/src/rust/Cargo.toml" },
        .{ .src = "Cargo.lock", .dest = "share/birnam/src/rust/Cargo.lock" },
        .{ .src = "rust/birnam-id/Cargo.toml", .dest = "share/birnam/src/rust/birnam-id/Cargo.toml" },
        .{ .src = "rust/birnam-id/build.rs", .dest = "share/birnam/src/rust/birnam-id/build.rs" },
    };
    inline for (rust_source_files) |source| {
        const install_source = b.addInstallFileWithDir(
            b.path(source.src),
            .prefix,
            source.dest,
        );
        b.getInstallStep().dependOn(&install_source.step);
    }
    b.installDirectory(.{
        .source_dir = b.path("docs/guide"),
        .install_dir = .prefix,
        .install_subdir = "share/birnam/docs/guide",
        .include_extensions = &.{".md"},
    });
    // -- stage 1: AOT-compile birnam-source modules to installable dylibs -------
    // The compiler host still embeds the small self-hosted boot floor as its
    // recovery path. A complete ordinary install, however, always includes the
    // compiled self-hosted and production module closure.
    const stage1 = b.step("stage1", "AOT-compile the dependency-ordered self-hosted modules to installable dylibs");
    rust_image_save_smoke.step.dependOn(stage1);
    rust_id_abi_fixture_smoke.step.dependOn(stage1);
    // Each self-hosted module: `birnam --compile-module <src> <lib>/libbirnam_<name>.dylib`.
    const self_hosted = [_]struct { src: []const u8, name: []const u8 }{
        .{ .src = "src/id/modules.birnam", .name = "modules" },
        .{ .src = "src/id/stdlib.birnam", .name = "stdlib" },
        .{ .src = "src/id/compiled-unit.birnam", .name = "compiledUnit" },
        .{ .src = "src/id/toml.birnam", .name = "toml" },
        .{ .src = "src/id/config.birnam", .name = "config" },
        .{ .src = "src/id/app-config.birnam", .name = "appConfig" },
        .{ .src = "src/id/project-policy.birnam", .name = "projectPolicy" },
        .{ .src = "src/id/isolate.birnam", .name = "isolate" },
        // Runtime tools and libraries. These sources are build inputs only;
        // the installed dylibs carry their source and dependency metadata.
        .{ .src = "modules/html.birnam", .name = "html" },
        .{ .src = "modules/file.birnam", .name = "file" },
        .{ .src = "modules/json.birnam", .name = "json" },
        .{ .src = "modules/socket.birnam", .name = "socket" },
        .{ .src = "modules/time.birnam", .name = "time" },
        .{ .src = "modules/doc.birnam", .name = "doc" },
        .{ .src = "modules/zigdoc.birnam", .name = "zigdoc" },
        .{ .src = "modules/lsp.birnam", .name = "lsp" },
        .{ .src = "modules/repl.birnam", .name = "repl" },
        .{ .src = "modules/cli.birnam", .name = "cli" },
        .{ .src = "modules/http.birnam", .name = "http" },
        .{ .src = "modules/log.birnam", .name = "log" },
        .{ .src = "modules/markdown.birnam", .name = "markdown" },
        .{ .src = "modules/os.birnam", .name = "os" },
        .{ .src = "modules/process.birnam", .name = "process" },
        .{ .src = "modules/random.birnam", .name = "random" },
        .{ .src = "modules/test.birnam", .name = "test" },
        .{ .src = "modules/compiled-unit-docs.birnam", .name = "compiledUnitDocs" },
    };
    inline for (self_hosted) |m| {
        const compile = b.addRunArtifact(siphon_exe);
        compile.setEnvironmentVariable("BIRNAM_LIB_DIR", b.getInstallPath(.lib, ""));
        compile.addArg("--compile-module");
        compile.addFileArg(b.path(m.src));
        compile.addArg(b.getInstallPath(.lib, "libbirnam_" ++ m.name ++ ".dylib"));
        compile.step.dependOn(bootstrap_install); // exe + all kernel dylibs installed first
        stage1.dependOn(&compile.step);
    }
    b.getInstallStep().dependOn(stage1);

    // External floor conformance imports only IdApi and dynamically loads the
    // explicitly selected implementation. It follows the installed Rust default;
    // Zig-oracle runs override the floor in an isolated prefix.
    const conformance_mod = b.createModule(.{
        .root_source_file = b.path("src/id/conformance_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const conformance_tests = b.addTest(.{ .root_module = conformance_mod });
    conformance_tests.linkLibC();
    const run_conformance = b.addRunArtifact(conformance_tests);
    run_conformance.setEnvironmentVariable("BIRNAM_ID_DYLIB", conformance_floor);
    run_conformance.setEnvironmentVariable("BIRNAM_ID_FLOOR_IDENTITY", conformance_identity);
    run_conformance.setEnvironmentVariable("BIRNAM_ID_FLOOR_SHA256", conformance_digest);
    run_conformance.step.dependOn(bootstrap_install);

    const null_args_mod = b.createModule(.{
        .root_source_file = b.path("src/id/null_args_conformance_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const null_args_child = b.addExecutable(.{
        .name = "id-null-args-conformance",
        .root_module = null_args_mod,
    });
    null_args_child.linkLibC();
    const null_args_check = b.addSystemCommand(&.{ "bash", "scripts/null-args-conformance.sh" });
    null_args_check.addArtifactArg(null_args_child);
    null_args_check.setEnvironmentVariable("BIRNAM_ID_DYLIB", conformance_floor);
    null_args_check.setEnvironmentVariable("BIRNAM_ID_FLOOR_IDENTITY", conformance_identity);
    null_args_check.setEnvironmentVariable("BIRNAM_ID_FLOOR_SHA256", conformance_digest);
    null_args_check.step.dependOn(bootstrap_install);
    const conformance_step = b.step(
        "test-id-conformance",
        "Run external IdApi behavior against one identified selected floor",
    );
    conformance_step.dependOn(&run_conformance.step);
    conformance_step.dependOn(&null_args_check.step);
    test_step.dependOn(&run_conformance.step);
    test_step.dependOn(&null_args_check.step);

    const store_image_backfill_mod = b.createModule(.{
        .root_source_file = b.path("src/id/store_image_backfill_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const store_image_backfill_tests = b.addTest(.{ .root_module = store_image_backfill_mod });
    store_image_backfill_tests.linkLibC();
    const run_store_image_backfill = b.addRunArtifact(store_image_backfill_tests);
    run_store_image_backfill.setEnvironmentVariable("BIRNAM_ID_DYLIB", conformance_floor);
    run_store_image_backfill.setEnvironmentVariable("BIRNAM_ID_FLOOR_IDENTITY", conformance_identity);
    run_store_image_backfill.setEnvironmentVariable("BIRNAM_ID_FLOOR_SHA256", conformance_digest);
    run_store_image_backfill.step.dependOn(bootstrap_install);
    const store_image_backfill_step = b.step(
        "test-store-image-backfill",
        "Prove image-style Store replacement rebuilds the primitive MOP mirror",
    );
    store_image_backfill_step.dependOn(&run_store_image_backfill.step);
    test_step.dependOn(&run_store_image_backfill.step);

    const collection_gc_mod = b.createModule(.{
        .root_source_file = b.path("src/id/collection_gc_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const collection_gc_tests = b.addTest(.{ .root_module = collection_gc_mod });
    collection_gc_tests.linkLibC();
    const run_collection_gc = b.addRunArtifact(collection_gc_tests);
    run_collection_gc.setEnvironmentVariable("BIRNAM_ID_DYLIB", conformance_floor);
    run_collection_gc.setEnvironmentVariable("BIRNAM_ID_FLOOR_IDENTITY", conformance_identity);
    run_collection_gc.setEnvironmentVariable("BIRNAM_ID_FLOOR_SHA256", conformance_digest);
    run_collection_gc.step.dependOn(bootstrap_install);
    const collection_gc_step = b.step(
        "test-collection-gc",
        "Run the R4 tracing/sweeping collection group against an active collector",
    );
    collection_gc_step.dependOn(&run_collection_gc.step);
    test_step.dependOn(&run_collection_gc.step);

    const gc_layout_mark_mod = b.createModule(.{
        .root_source_file = b.path("src/id/gc_layout_mark_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const gc_layout_mark_tests = b.addTest(.{ .root_module = gc_layout_mark_mod });
    gc_layout_mark_tests.linkLibC();
    const run_gc_layout_mark = b.addRunArtifact(gc_layout_mark_tests);
    run_gc_layout_mark.setEnvironmentVariable("BIRNAM_ID_DYLIB", conformance_floor);
    run_gc_layout_mark.setEnvironmentVariable("BIRNAM_ID_FLOOR_IDENTITY", conformance_identity);
    run_gc_layout_mark.setEnvironmentVariable("BIRNAM_ID_FLOOR_SHA256", conformance_digest);
    run_gc_layout_mark.step.dependOn(bootstrap_install);
    const gc_layout_mark_step = b.step(
        "test-gc-layout-mark",
        "Compare precise layout mark decisions through one selected floor",
    );
    gc_layout_mark_step.dependOn(&run_gc_layout_mark.step);
    test_step.dependOn(&run_gc_layout_mark.step);

    // Selected-floor native address-registry comparator. The executable refuses
    // non-ReleaseSafe builds; later Rust parity uses the same binary with an
    // isolated dylib path and expected diagnostic identity.
    const membership_bench_mod = b.createModule(.{
        .root_source_file = b.path("src/id/membership_bench.zig"),
        .target = target,
        .optimize = optimize,
    });
    const membership_bench = b.addExecutable(.{
        .name = "id-membership-bench",
        .root_module = membership_bench_mod,
    });
    membership_bench.linkLibC();
    const run_membership_bench = b.addRunArtifact(membership_bench);
    run_membership_bench.setEnvironmentVariable("BIRNAM_ID_DYLIB", conformance_floor);
    run_membership_bench.setEnvironmentVariable("BIRNAM_ID_FLOOR_IDENTITY", conformance_identity);
    run_membership_bench.setEnvironmentVariable("BIRNAM_ID_FLOOR_SHA256", conformance_digest);
    run_membership_bench.step.dependOn(bootstrap_install);
    b.step(
        "bench-id-membership",
        "Benchmark selected-floor exact-address and scalar ABI paths (ReleaseSafe only)",
    ).dependOn(&run_membership_bench.step);

    const gc_baseline_mod = b.createModule(.{
        .root_source_file = b.path("src/id/gc_baseline.zig"),
        .target = target,
        .optimize = optimize,
    });
    const gc_baseline = b.addExecutable(.{
        .name = "id-gc-baseline",
        .root_module = gc_baseline_mod,
    });
    gc_baseline.linkLibC();
    const run_gc_baseline = b.addRunArtifact(gc_baseline);
    run_gc_baseline.setEnvironmentVariable("BIRNAM_ID_DYLIB", conformance_floor);
    run_gc_baseline.setEnvironmentVariable("BIRNAM_ID_FLOOR_IDENTITY", conformance_identity);
    run_gc_baseline.setEnvironmentVariable("BIRNAM_ID_FLOOR_SHA256", conformance_digest);
    run_gc_baseline.step.dependOn(bootstrap_install);
    b.step(
        "baseline-id-gc",
        "Record selected-floor GC allocation and live-count invariants",
    ).dependOn(&run_gc_baseline.step);

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
    project_check_smoke.step.dependOn(stage1);
    const project_check_step = b.step("test-project-check", "Run source-project resolution and build smoke tests");
    project_check_step.dependOn(&project_check_smoke.step);

    const lsp_project_smoke = b.addSystemCommand(&.{ "bash", "scripts/lsp-project-smoke.sh" });
    lsp_project_smoke.setEnvironmentVariable("BIRNAM_BIN", b.getInstallPath(.bin, "birnam"));
    lsp_project_smoke.step.dependOn(stage1);
    const lsp_project_step = b.step("test-lsp-project", "Run project-aware LSP protocol smoke tests");
    lsp_project_step.dependOn(&lsp_project_smoke.step);

    const termbox_load_smoke = b.addSystemCommand(&.{ "bash", "scripts/termbox-load-smoke.sh" });
    termbox_load_smoke.setEnvironmentVariable("BIRNAM_BIN", b.getInstallPath(.bin, "birnam"));
    termbox_load_smoke.step.dependOn(stage1);
    const termbox_load_step = b.step("test-termbox-load", "Load the reusable Alien-backed Termbox adapter");
    termbox_load_step.dependOn(&termbox_load_smoke.step);

    const emacs_mode_tests = b.addSystemCommand(&.{ "bash", "scripts/emacs-test.sh" });
    const emacs_mode_step = b.step("test-emacs", "Run birnam-mode project and lsp-mode integration tests");
    emacs_mode_step.dependOn(&emacs_mode_tests.step);

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

    // Pre-rewrite binaries and image are compatibility evidence, not build
    // inputs. Keep this gate opt-in so ordinary bootstrap and installation do
    // not require the fixture directory to exist.
    const idapi_fixture = b.addSystemCommand(&.{ "bash", "abi/fixtures/idapi-v1/verify.sh" });
    idapi_fixture.setEnvironmentVariable("BIRNAM_BIN", b.getInstallPath(.bin, "birnam"));
    idapi_fixture.setEnvironmentVariable("BIRNAM_ID_DYLIB", b.getInstallPath(.lib, "libbirnam_id.dylib"));
    idapi_fixture.step.dependOn(stage1);
    b.step(
        "test-id-abi-fixture",
        "Run frozen pre-rewrite native and image compatibility fixtures",
    ).dependOn(&idapi_fixture.step);

    const install_smoke = b.addSystemCommand(&.{ "bash", "scripts/install-smoke.sh" });
    install_smoke.setEnvironmentVariable("BIRNAM_PREFIX", b.getInstallPath(.prefix, ""));
    // The install step owns docs and other non-artifact resources in addition
    // to depending on stage1. Test the freshly assembled tree, not whatever a
    // previous install happened to leave in zig-out.
    install_smoke.step.dependOn(b.getInstallStep());
    b.step("test-install", "Run compiled runtime tools from an isolated installed tree").dependOn(&install_smoke.step);

    const isolate_smoke = b.addSystemCommand(&.{ "bash", "scripts/isolate-smoke.sh" });
    isolate_smoke.setEnvironmentVariable("BIRNAM_BIN", b.getInstallPath(.bin, "birnam"));
    isolate_smoke.setEnvironmentVariable("BIRNAM_LIB_DIR", b.getInstallPath(.lib, ""));
    isolate_smoke.step.dependOn(stage1);
    b.step("test-isolate-process", "Boot a worker image and round-trip a copied value").dependOn(&isolate_smoke.step);

    verify_step.dependOn(test_step);
    verify_step.dependOn(&repl_goldens.step);
    verify_step.dependOn(&source_fallback.step);
    verify_step.dependOn(project_check_step);
    verify_step.dependOn(lsp_project_step);
    verify_step.dependOn(termbox_load_step);
    verify_step.dependOn(emacs_mode_step);
    verify_step.dependOn(&image_save_smoke.step);
    verify_step.dependOn(&install_smoke.step);
    verify_step.dependOn(&isolate_smoke.step);

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
