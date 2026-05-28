# Birnam Vision

A **system** is a binary that runs as an executable OR loads as a library, and which exposes a complete, fully malleable computing environment in the COLA / idst / Piumarta tradition.

## Core Tenets

1. **Everything is an object.** Vtables, selectors, modules, source, bytecode, method caches, history records — all reachable via `send`.
2. **Bootstrap is minimal and visible.** A small kernel (id model) brings itself up; everything else loads on top as modules.
3. **Modules assemble the system at runtime.** Like idst loading `.so` files. A birnam image is a graph of live modules, not a monolith.
4. **Live mutation is first-class.** Adding/replacing a method is a normal operation, not a rebuild. The REPL is an object whose `run` method dispatches messages to other objects.
5. **History is preserved.** Mutations to vtables, modules, and source are logged. Time travel, diff, blame all derive from this log.
6. **Two emission modes:** live image (full reflection) and AOT artifact (stripped, no compiler/history/reflection). Same source compiles to either.
7. **Alien code is absorbable.** Any `.so`/`.dylib` can be loaded, introspected, and incrementally wrapped into the object world from the REPL.

## Execution Modes

| Mode | Use | Reflection | Compiler present |
|------|-----|------------|------------------|
| **Live image** | Development, REPL, exploratory wrapping | full | yes |
| **Bytecode VM** | Portable execution, default AOT target | minimal | no |
| **Native AOT** | Stripped applications, fast startup | none (or opt-in) | no |

All three share the same object model. Native AOT is bytecode + a precompiled VM, or (later) direct lowering.

## Architecture

```
+--------------------------------------------------------+
|                       shell (exe)                      |
|   - loads modules (dylib or image segment)             |
|   - export/import table (name -> opaque pointer)       |
|   - file watcher (dev only)                            |
+-----------------------+--------------------------------+
                        |
   +--------------------+--------------------+
   |        id module (libbirnam_id)         |   <- bootstrap kernel
   |  vtable, send/bind, mcache, selectors,  |
   |  prototype object, allocation           |
   +--------------------+--------------------+
                        |
   +--------+----------+----------+----------+----------+
   |        |          |          |          |          |
  peg    reader    compiler     vm        ffi       image
  AST    parse     IR + AOT   bytecode   alien     snapshot
                                          .so      + history
```

Each box is a dylib that imports `id_api` from the kernel and exports its own API. Modules can be replaced live.

## Memory

- **Arenas now.** Two regions: permanent (kernel, interned selectors) and image (everything else). Image arena is snapshot-able.
- **GC deferred.** Object layout (-1 vtable trick, no interior pointers across object boundaries) stays moving-friendly so a copying/compacting GC can drop in later.

## Image and History

- **Image** = serialized object graph of the image arena + interned tables. Loadable as alternative to fresh bootstrap.
- **History** = append-only log of mutations (`methodAt:put:with:`, prototype creation, module load, alien wrap). Each vtable carries its own history vector; the image-wide log is a view over those.
- **Persistence**: snapshot writes image arena to disk; log is either embedded in snapshot or stored alongside.
- Reload via file watcher and REPL mutation both go through the same log.

## Compilation Path

Source (.bir or whatever) → PEG parse → AST objects → desugar → IR objects → bytecode objects.

- **VM** consumes bytecode directly.
- **AOT** = bytecode + VM linked together, with image arena baked in and reflection optionally stripped.
- **Native lowering** is a later layer over the same IR. Candidates: hand-rolled emitter (Piumarta's path), QBE (small embeddable SSA backend), or LLVM (heavy, last resort). No tinycc — wrong abstraction layer.
- The compiler is itself a birnam module. Self-hosting target: PEG grammar parsed by birnam-PEG; compiler written in birnam.

## Alien Code Absorption

REPL workflow for an unknown shared library:

```
> mod := Module load: "/usr/lib/libfoo.dylib"
> mod symbols                          " enumerate Mach-O/ELF symtab "
> mod wrap: 'foo_open' as: #Foo>>open
> mod wrap: 'foo_read' as: #Foo>>readInto:size: signature: #(int ptr int)
> Foo new open readInto: buf size: 1024
```

- **Symbol enumeration**: bundled Mach-O + ELF readers (small, hand-rolled, not libelf).
- **Calling unknown signatures**: dyncall or libffi for argument marshalling.
- Wrap definitions are persisted objects — once wrapped, the wrap survives image save/load.

## REPL

REPL is an object. Its `run` method reads a line, parses to a message send, dispatches to a target object. Shell-level REPL exists only as the bootstrap front door until the id REPL module loads.

Syntax: Smalltalk-ish. Unary, binary, keyword messages. Cascades. Block closures. Final syntax TBD; PEG grammar will be the source of truth.

## Build Outputs

- `birnam` — shell executable
- `libbirnam_<module>.dylib` — each runtime module
- `image.bir` — snapshot of a live system
- `<app>` — AOT-compiled application (shell + bytecode VM + image, stripped)

## Non-Goals (For Now)

- Concurrency / threads / actors. Single-threaded image.
- Distributed images. Local only.
- Sandboxing. Trusted code only.
- Beating C performance. Match Smalltalk-VM ballpark.

## Open Questions

- Reload via file watch: keep, or replace entirely with image mutation? Lean: keep for dev, mutation is canonical.
- History granularity: every send, or only mutations? Lean: mutations only; send-traces are opt-in profiling.
- Image format: custom binary, or use Cap'n Proto / FlatBuffers? Lean: custom — image arena is already a contiguous blob, dump it.
- Selector table across image save/load: re-intern on load, or freeze pointers? Lean: re-intern, fix up references via relocation table.

## Roadmap (rough)

1. **Now**: id kernel (done), PEG (done), module system (done), shell (done).
2. **Next**: REPL-as-object module. Replace shell REPL.
3. **Then**: alien .so absorption — symbol reader + dyncall + wrap object.
4. **Then**: language module — PEG grammar for birnam source, AST objects, simple AST interpreter.
5. **Then**: bytecode + VM. AST → bytecode compiler in birnam.
6. **Then**: image snapshot + history log.
7. **Then**: AOT — emit shell + VM + image as standalone binary.
8. **Later**: native codegen over IR.
9. **Later**: GC.
