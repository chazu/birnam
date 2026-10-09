# Invocation and capture investigation

The accepted starting addition trace has 177 sends, including fifteen capture
requests and fifteen Id binding-proof requests. There are no per-addition Block
allocations. Each synchronous method/Block body with captures asks its actual
receiver for a borrow; this is not a loop reading every capture with a separate
message. A named method adapter still sends its implementation's current `value:`
selector. Execution scopes the method, receiver and Object arguments through the
call; generated code borrows that coverage.

## Implemented simplification

Capture admission previously required five canonical service bindings: lookup,
binding, intern, symbol lookup and the matcher. Intern and symbol lookup are not
invoked by the capture operation: selectors are already held in Execution's
traced name inventory, and the compiler resolves its selectors before requesting
the borrow. Their replacement need not deny an otherwise valid immutable capture
borrow. They remain live wherever actually used for name resolution.

Execution now shares `birnam-abi::BindingGuard` with Word/ObjectModel/GC and keeps
only the three relevant service witnesses. The old separate request marshaller
and constructor witness-discovery logic are removed. A fixed three-row stack
buffer holds target entries; no per-request heap allocation is introduced.

A canonical bulk request has three service rows and three target rows rather
than five service rows and three target rows: six instead of eight, a 25% smaller
proof request. At fifteen bulk requests per addition this removes thirty row
comparisons inside Id. Send count remains 177 and allocation count remains three
Words plus one Integer. This is less internal guard work, not a measured CPU gain.

Native regression replaces `symbolNamed:` while requesting captures with a
pre-resolved selector: the borrow remains valid and the unrelated replacement is
not called. Existing tests cover replaced readers/grants, forwarding wrappers,
collection, repeated aliases, Method invocation on non-Blocks and shared Cells.
The reader, borrowing grant, bulk operation, lookup, bind and matcher still have
exact current method/context witnesses. Lean records the fixed-selector dependency
set alongside existing borrowing coverage; the existing TLA+ bulk model and unsafe
variants remain applicable. These are conditional owner contracts.

## Invocation environment: investigated, not implemented

Having Execution pass an environment alongside every authored call would require
a carrier/calling-convention change or an additional owner message. Existing
Methods can be invoked with non-Block receivers; capture readers/grants can be
replaced during ordinary execution, and nested callbacks/tasks need separate
covering scopes. A cached environment keyed only by Method misses receiver and
current-reader differences. A receiver-address cache without lifetime/identity
coverage can be reused after collection. Direct compiler reads of Execution's
storage would bypass the owner boundary and are rejected.

Today one current receiver request plus one owner-authority proof is the concrete
capture path. This is not a proof that two sends is the universal minimum. A
larger invocation redesign could transfer a current admitted environment at the
selected call boundary, but needs explicit receiver, reader, alias, task/frame
lifetime and invalidation contracts. It is not a safe local deletion of either
send. Reducing helper boundaries also changes observable/replaced methods unless
performed within a guarded compiler region. Those changes are deferred rather
than claimed as completed by this slice.

Acceptance passed 5 native checks, 139 Lean theorems and all 84 TLA+
configurations. The bounded warmed census reproduced 177 sends with stable inputs.
See the [receipt](../measurements/invocation-capture-investigation-20261009.json).
