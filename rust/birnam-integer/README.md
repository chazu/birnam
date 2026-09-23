# Integer

`libbirnam_integer` initializes with Namespace and returns Integer. It requires
Roots, GC, ObjectModel, ObjectVt, String, true, false and nil; Error must already
be published. It publishes Integer and IntegerVt. Repeat loading returns the
published object without reinstalling methods. Float and Block are not needed
to construct this family.

The instance owns its selectors and family identities. Eight raw payload bytes
hold an i64; only Integer reads that representation. Object Model allocates and
shallow-clones the storage. A GC reference owner declares no object edges,
including through private behaviors. Recognition preserves the existing exact
Integer behavior rule: a changed behavior is not an exact Integer. No process
singleton, service table, foreign layout or cached dependency entry is used.

| Receiver / selector | Arguments | Result |
|---|---|---|
| Integer `fromWord:` | native Word | Integer interpreting the Word as signed 64-bit bits. |
| Integer `wordOf:` | Object | Native Word for an exact Integer, Unit otherwise. |
| Integer instance `word` | none | Native Word containing its signed bits. |
| Integer instance `+`, `-`, `*`, `/`, `mod:` | Object | Checked i64 arithmetic, or Error. Division truncates toward zero; MIN mod -1 is zero. |
| Integer instance `=` | Object | Exact Integer value equality, false for another family. |
| Integer instance `<`, `<=`, `>`, `>=` | Object | Comparison, preserving existing f64 comparison rounding. |
| Integer instance `printString` | none | Decimal String through current `fromBytes:`. |
| Integer instance `to:do:` | end Integer, callable Object | Inclusive iteration by live `value:` sends; receiver on completion, first Error on failure. MAX is visited without overflowing the loop counter. |
| Integer `installFloat:` | completed Float Object | Unit; install mixed arithmetic/comparisons and `asFloat`. |

The mixed extension is a complete private context created after Float exists.
It reads Float through `bitsOf:` and constructs through `fromBits:`. It replaces
only its own original Integer implementations and preserves prior user changes;
`asFloat` is installed only if absent. It retains Float for its methods' lifetime,
without a future service slot in Integer. Repeat extension installation is rejected.
Bootstrap owns this sequencing.

Results use the current `fromWord:`/`fromBits:` factories. Errors use the current
Namespace Error binding and its `kind:message:` method, with String diagnostics.
The existing live ErrorVt binding determines incoming Error propagation, except
that equality returns false. Overflow, zeroDivide and typeMismatch retain their
established meanings. Receivers, arguments and intermediate results are rooted
across sends; no borrowed native state spans a dependency call.

Run `mise run native-messages`. Production checks cover arithmetic boundaries,
rounding, mixed operands, replacement, collecting callbacks, iteration and raw
payload tracing. Descriptor-based construction and numeric declarations v4/v5
are retired. Execution's admission, deferred materialization and recovery work
remains preserved but unintegrated; no optimization declaration is asserted for
this new message path until its effects are proved by the Execution conversion.
