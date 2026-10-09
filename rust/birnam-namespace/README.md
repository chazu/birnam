# Namespace

## Source global resolution

`resolveGlobal:` performs current Namespace nil/Globals reads, a current Globals
read, and an export fallback using the fetched nil identity. Nested owner scopes
report those temporary values through `rootsDo:` while getters may rebind or
collect. The compiler delegates this pipeline through Id; exports/getter entries
remain live. See [contract and tests](../../docs/design/operation-dispatch-and-global-resolution.md).
