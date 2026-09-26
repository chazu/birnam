# Relational over Id messages

Bootstrap loads Relational after Store and collections, before Execution/compiler.
The initializer takes the Namespace Object, extends the existing Store behavior,
and returns Store. It publishes the existing `LogicVar`, `LogicVarVt`,
`LogicVarFactoryVt`, `EntityVt` and `RelCollectorVt` names and keeps `db` naming
Store. Repeated loading preserves installed methods and LogicVar identities.

## Unchanged language API

| Receiver | Selectors |
|---|---|
| LogicVar | `named:` |
| LogicVar instance | `name`, `printString` (and existing copy behavior through `clone`/`new`) |
| Store | `entity`, `findAll:where:`, `findAll:where:asOf:`, `findAll:where:since:`, `findAll:where:between:and:` |
| Store | `count:where:`, `sum:where:`, `min:where:`, `max:where:`, `avg:where:` |
| Store | `groupBy:aggregate:of:where:`, `pull:spec:`, `defineRule:params:body:`, `rebuildRules` |

The evaluator and its independent matching tests are unchanged. Queries retain
value matching, first-occurrence order and deduplication, joins, predicates,
negation, disjunction, and rule fixpoints. Query terms and bodies use the existing
LogicVar/Vector shapes, including the 512-element parser bound. Rules remain
ordinary `__rule-name__` Store facts; each query reconstructs them from its
snapshot. `rebuildRules` remains a successful no-op. Pull retains the existing
nesting limit and Store cardinality interpretation.

History queries use the same Store windows and nonnegative Integer arguments.
Malformed query shapes answer nil. Empty count/sum answer Integer zero; empty
min/max/avg answer nil. Integer-only averages retain integer division; mixed
numeric aggregates retain the evaluator's Float promotion and overflow behavior.
The source spellings (`?x`, Vector literals, and all selectors) are unchanged.

## Message boundary and lifetime

The adapter sends Store `liveTriplesDo:`, `liveTriplesAsOf:do:`,
`liveTriplesSince:do:` or `liveTriplesBetween:and:do:` with a collector Object.
Store sends that Object `value:value:value:` for each triple. A collector is active
only during that snapshot and contains no pointer to a native stack frame.
Escaped collectors reject later deliveries. The existing million-triple limit
still bounds a snapshot.

Integer `wordOf:`/`fromWord:`, Float `bitsOf:`/`fromBits:`, String `bytes`/`fromBytes:`,
Id behavior/symbol messages, and Vector/Map messages replace service-table access
and foreign payload reads. Collection factories and query-shape behaviors retain
their existing lookup through Namespace at query time. Every send uses current
lookup, including GC `visit:` messages. Native protocol failures propagate unchanged.

Like Store, Relational keeps invocation-local roots while dependency sends can
collect or reenter. Raw snapshot identities and rule vectors remain rooted after
Store enumeration, even if a replacement Store method removes their original
references. Output maps/vectors and recursive pull intermediates stay rooted until
the query returns. No native-state borrow spans a send. LogicVars retain their
existing canonical name cache and one owned reference; their owner reports that
reference through `referencesOf:do:`.

Production exports follow the same descriptor and
symbol-borrow convention as Store; there is no compatibility service table.

## Validation

`mise run native-messages` builds and audits the real dylib and runs the
Relational integration binary alongside the other production modules. It tests
query results/order, history windows, empty aggregates, rule updates, nested pull,
LogicVar identity/copying, compiled source, collecting method replacements,
reentrant queries, replaced Namespace factories, failed sends and cleanup, and
snapshot identity retention.

`mise exec -- cargo test --offline --locked -p birnam-relational --lib` runs the
unchanged evaluator tests, including the independent eager matching oracle.
The manual timing test remains ignored. Full Host/CLI, Image and task-policy
integration remain separate work; this migration adds no optimization policy.
