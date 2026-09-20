# Namespace import capability v1

A 32-byte caller-owned live descriptor: version 1, byte size, opaque context,
and `import(context, const char *name) -> Oop`. The independent
[C mirror](fixtures/namespace-import-v1/namespace_import_v1.h) fixes the layout.
The name follows the selected operation's C-string contract; its result may be
raw null. This capability neither validates names nor adds interning or lookup.

The recipient snapshots the current function/context together only at the
original import call. Earlier callbacks can replace either field. The selected
callback may reenter, collect or alter the descriptor; the current invocation
retains its selected pair. Its returned Oop is still a result snapshot.
No shared Rust reference to the descriptor spans the call. The descriptor,
selected context and code must stay valid for the synchronous borrow, and the
caller retains the existing root obligations for arguments and results.

[Error Value v3](error-value-v3.md) receives this capability per invocation and
does not retain it. There is no global import dependency or API-address registry.
The first migrating native consumers lend their existing mutable IdApi.import
slot. That adapter exposes only the slot address and reads its current function
when invoked; it neither takes an early function snapshot nor passes an aggregate
table to the import operation. The slot and callback code outlive the loan.

Canonical import ownership, direct consumers, native initializer/preparation
transport and removal of IdApi.import remain subsequent work. At this checkpoint
IdApi is still v9 and Namespace factory remains v6.
