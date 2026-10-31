# Current implementation

The [invariants](INVARIANTS.md) define the design requirements. This file is a
map of today's implementation, not a requirement to retain its language, crate
layout, tables or helper objects.

Birnam objects interact through Id messages. Native method bodies also use
ordinary algorithm code. Neither a crate boundary nor a repeated implementation
pattern is a reason to create another language service or message protocol.

## Object model and execution

Id handles object references, allocation facts, vtables, method lookup and direct
entry into selected implementations. It supplies bootstrap implementations for
the initial objects. Generic dispatch does not interpret their payloads or require
an application message on the selected Method.

Ordinary heap objects currently have contiguous header/payload allocations;
implementations may refer to other objects or separate storage. Moving storage
must preserve identity and all supported references.

ObjectModel implements ordinary object slots, cloning and private vtables.
String, Bytes, numbers and collections implement their data operations. Roots
keeps references supplied by native code; Collect follows references and reclaims
unused objects. These are implementation responsibilities, not language-level
ownership or borrowing rules.

Execution implements Blocks, Machines, frames, continuation handling and recovery.
The compiler produces executable Methods; installing a Method in a dictionary is
a separate action. Captured Methods and running or suspended computations keep
their code and objects usable even when bindings change.

Generated TCC code keeps temporary references in native call buffers reported by
the code Unit's GC owner. Explicit Roots operations and the standalone native-provider
token path remain available. This is implementation lifetime bookkeeping, not a
root-management rule for Birnam programs.

Task scheduling policy is substantially written in Birnam. Tasks within a domain
share objects and interleave; current parallel workers use separate processes.
See the [concurrency guide](docs/guide/07-concurrency.md) for the actual interface
and the distinction between implemented behavior and future work.

## Compilation, startup and images

PEG implements parsing machinery. Lang defines the grammar. Codegen translates
programs, and TCC supplies native compilation and relocation. Their helper
libraries are implementation code, not another object-dispatch system.

The executable starts through Siphon and Id, loads Bootstrap through messages,
and restores the compiled startup image. CLI tools have a separate tools image.
The source builder first builds the native implementation, uses the build-only
Lang seed to compile the required Birnam modules, and creates those images. The
published installation excludes that seed. See [Bootstrap](rust/birnam-bootstrap/src/lib.rs).

Compatible compiled artifacts provide the executable code used by an image.
Recorded source supports editing, inspection and explicit rebuilding; it is not
an automatic substitute for compatible saved code. Image restores objects through
their messages. See the [image guide](docs/guide/04-images.md) and
[runtime behavior reference](docs/contracts/runtime.md).

## State and optional facilities

Ordinary object state, Cells, Globals, method dictionaries and reflection do not
require a fact database. Store supplies explicitly selected facts and history;
Relational supplies queries over that data. Their self-description remains part
of those facilities. Shipping them does not enroll ordinary objects in their
bookkeeping. Documentation tools may explicitly load them.

## Native implementation

The implementation currently uses Rust and C/libtcc alongside Birnam source.
This is not a language commitment. Native method implementations use only their
objects' defined representations; other objects are addressed through messages.
Private helper calls do not create an alternative API for changing another
object's state or invoking its behavior.

Bootstrap control Methods currently hold their arguments or reply state in their
own allocation, with a self-context, instead of a separate Sequence. The Method
implementation reports those references for GC and validates its own layout;
generic allocation and other objects remain unaware of that representation.

Current bulk byte transfers use bounded groups of Oops and a temporary receiving
object. Native consumers collect those groups in their own buffers. The executable
facade currently declares its receiving callback through TCC. These mechanisms
are replaceable implementation choices, not invariants. The
[byte-message reference](docs/contracts/bytes.md) describes current behavior.

## Source map

| Work | Current implementation |
| --- | --- |
| Id and native message ABI | `rust/birnam-id`, `rust/birnam-abi` |
| Root tracking and GC | `rust/birnam-root-registry`, `rust/birnam-collect` |
| Ordinary objects and data | `rust/birnam-object-model`, value and collection crates |
| Execution and recovery | [Execution](rust/birnam-execution/README.md) |
| Parsing and compilation | `rust/birnam-peg`, `modules/lang.birnam`, Codegen, NativeBackend, TCC |
| Persistence | [Image](rust/birnam-image/README.md) |
| Language policy and libraries | `modules/` |
| Application startup and tools | Host, CLI, project support and `rust/birnam-build` |

For checks, use [validation instructions](docs/validation.md). For development
direction, start from the invariants and current behavior, not a migration log.
The [archive](docs/archive/README.md) preserves earlier decisions and experiments.
