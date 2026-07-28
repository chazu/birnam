# Platform Support

Birnam currently supports **arm64 macOS** only. This applies to development
builds, the Rust-only `scripts/rust-install.sh` prefix installer, Rust stage-1
publication, and release bundles. The explicit Zig build/oracle lane remains
arm64-only as well.

Unsupported targets fail at build configuration time rather than compiling
artifacts that cannot link or boot. In particular, Linux and Intel macOS are
not supported source-build alternatives today.

## Why the boundary exists

- The vendored libtcc configuration builds the arm64 Mach-O backend.
- Birnam loads its kernel and AOT modules as `.dylib` artifacts.
- The Linux probe (`x86_64-linux`) cannot link libtcc into the dynamic `Tcc`
  module because its archive is not built as PIC.

## Porting a target

A supported port must land as one tested change set:

1. Build the appropriate libtcc backend and linker for the target, with PIC
   when it is linked into Birnam's dynamic modules.
2. Make artifact names and runtime discovery target-aware (`.dylib` on macOS,
   `.so` on Linux), including `stage1`, images, AOT output, wrappers, and
   release bundles.
3. Test a clean Rust source build, Rust stage-1 publication, the birnam-level
   suite, and a packaged release on that target. The Zig oracle lane must also
   retain its explicit rollback coverage while it remains supported.

Until all three are complete, the target remains unsupported.
