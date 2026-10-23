# Independent Oop-only Id fixtures

`id.h` is a separately written C declaration, not generated from Rust. Id message
arguments/results are Oops. Methods additionally receive the Id object and their
context Oop. Argument pointers/counts and native module descriptors are C transport
and object-file metadata, not alternative kinds of message value.

`fixture.c` owns its Node payloads and uses messages for Id/Roots/GC and its actor's
reference storage. It checks calls through separately loaded code, replacement,
custom lookup, DNU, collection during calls, finalization, and invalid contexts.
`failure.c` publishes a callable Method before returning a Fault; the driver
collects and invokes the published method afterward. `rejected.c` aborts if its
incompatible initializer is entered. `rejected-null.c` has no initializer.
`tcc-caller.c` is compiled with the vendored compiler and calls real Id and GC.

`bench.c` builds twice. Its v1 control uses the unchanged independent v1 C header
against the installed baseline. Its v2 build uses `id.h` and the isolated libraries.
The two variants perform the same requested workload but have different allocation
populations and bookkeeping. Do not attribute their whole difference to carriers.

Run `bash scripts/oop-id-slice-test.sh` from this worktree. Run
`python3 scripts/oop-id-slice-bench.py` only after building both benchmark callers.
See the [qualification report](../../../docs/design/oop-only-id-slice-20261023.md).
