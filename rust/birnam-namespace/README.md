# Namespace

## Binding inspection

`liveBindingsDo:` takes a visitor receiving `binding:value:`. It snapshots names
and values in export order, omits raw Unit (absence), and includes bound language
nil. `bindingsDo:` retains its existing inclusion of Unit values for native
consumers. Both root the copied pairs and visitor across callbacks that replace
exports, collect, or reenter. Native callback failures propagate and unwind the
temporary inventory; ordinary callback return values are ignored.

Reflect's authored `bindingsIn:` consumes `liveBindingsDo:` on Namespace or Globals
and returns ordinary name/value Maps. Enumeration is separate from source lookup
precedence and does not create facts.

## Source global resolution

`resolveGlobal:` performs current Namespace nil/Globals reads, a current Globals
read, and an export fallback using the fetched nil identity. Nested owner scopes
report those temporary values through `rootsDo:` while getters may rebind or
collect. The compiler delegates this pipeline through Id; exports/getter entries
remain live. See [contract and tests](../../docs/design/operation-dispatch-and-global-resolution.md).
