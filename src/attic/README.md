# src/attic — reference-only salvage from the legacy kernel

These files are **not part of the build.** They are the parts of the pre-`src/id`
fact-store kernel worth keeping in the working tree as a reference while the
corresponding capability is rebuilt on the libid floor. Everything else from the
old kernel was deleted (it's recoverable from git history — see the commit that
introduced this directory).

Their `@import`s point at kernel files that no longer exist, so they will **not
compile** — read them for the algorithms and on-disk formats, not as live code.

| file(s) | reference for the upcoming arc |
|---|---|
| `image.zig`, `freeze.zig`, `prune.zig`, `mount.zig`, `loader.zig` | persistence / images |
| `ffi.zig`, `ffi_module.zig` | struct-by-value FFI (libffi marshalling) — note `src/id/alien.zig` does FFI via Tcc shims instead |
| `macho.zig`, `elf.zig` | native section emit / link (imaging native code) |
| `gc.zig`, `arena.zig`, `arenas.zig` | GC (the id floor has finalizers; this is the fuller mark/sweep) |
| `boxed.zig` | boxed values (relevant to value-slot globals / Float) |

When a capability lands on the id floor, delete its reference file here.
