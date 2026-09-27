# Range

The initializer takes Namespace, returns the completed Range prototype and
publishes `Range`/`RangeVt`. It installs `Integer to:` once both owners exist,
preserving an existing replacement. The argument must be an Integer; endpoints
are inclusive. The prototype remains `0 to: 0`.

Range owns two signed 64-bit endpoints. It uses Integer messages for recognition
and construction, String for printing, Vector for materialization, and ordinary
Id sends for callbacks. No Block or Execution dependency is needed.

- `size` returns Integer, zero for descending ranges, nil if the size exceeds
  `i64::MAX`.
- `includes:` accepts an Integer and returns Boolean.
- `at:` accepts a nonnegative Integer index and returns Integer or nil.
- `do:` sends `value:` inclusively and returns the range; callback results are
  ignored as before.
- `inject:into:` folds inclusively with `value:value:` and returns the result.
- `printString` prints the sequence as `#[...]`.
- `reversed` returns a Vector; `,` appends a Vector to the forward sequence.

Loop/index arithmetic uses wider intermediates so either signed endpoint works
without wraparound. Object Model provides shallow cloning. Range's inherited GC
method excludes raw bits in precise mode, including through private behaviors. Scoped roots
protect callback arguments, accumulators and materialized results. Native protocol
failures propagate. Mutable state is attached to the initialized instance.

Payload enumeration reads GC `precisePayloads` by message before each object.
In precise mode, raw bits are not references; in conservative mode the owner
checks its own payload words through Id `allocationAt:` and sends `visit:` for
matching allocations. Private behaviors keep the same representation ownership.
See [System GC controls](../birnam-system-gc/README.md).
