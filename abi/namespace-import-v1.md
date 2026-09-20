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

[Namespace factory v7](namespace-v7.md) creates the canonical runtime-owned
service. Id retains its owned allocation and exposes the mutable descriptor
through `birnam_namespace_import_provider_v1` after `id_init`. The host and
Native Loader supply it as the fourth `module_init` argument after presence.
A module that retains an API captures its matching live import descriptor at
that same selection point, including early initialization and callback reentry.
Incoming registration/adoption uses incoming capabilities where it used the
incoming API. Reload never replaces a saved execution selection implicitly.

IdApi v10 has 31 fields and 248 bytes, with no import slot. Float/Word prepare
v3 and the six System/Block v3 installers carry imports after presence;
Range/Vector/Map prepare v3 carry imports immediately after API. Numeric and
Block declarations v4 compare canonical function/context identities against
live selections without invoking them. Isolate worker and OOP codec bridges
use v2 exports with imports immediately after API.

No production slot loan, aggregate compatibility table, API-address registry
or process-global replacement selection is provided. Independent historical
C oracles may use per-caller callback slots solely to compare old/new traces.
