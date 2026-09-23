# Vector

The initializer takes Namespace and publishes completed `Vector`, `VectorVt` and
`VectorNode` objects. It needs Collection's parent behavior, Object Model, Roots,
Integer, String and Boolean/Nil identities. Bootstrap constructs the shared parent;
there is no Collection installer library or Execution prerequisite.

Vector retains its persistent 32-way trie with a 32-element tail. `add:` shares
unchanged paths and returns a new Vector. Its public messages are:

- `new`, `clone`: empty Vector (clone retains the established emptying behavior).
- `size`: Integer; `at:` Integer index: element or nil.
- `add:` Object, `,` Vector: persistent Vector results.
- `copyFrom:to:` Integer bounds: half-open slice, or nil for invalid bounds.
- `reversed`: new Vector; `indexOf:` Object: Integer index or nil.
- `do:` callable: send `value:` to each element, return the original Vector.
- `inject:into:` initial value and callable: fold using `value:value:`.
- `sortBy:` callable: compute each key once, stable insertion sort with live `<`.
- `printString`: String in `#[a b c]` form, using live element printing.

Iteration keeps the existing language-result behavior: `do:` ignores callback
results; folding carries each result. Native protocol failures propagate. Callables
are opaque receivers; they do not require a native Block table.

Only Vector interprets its payloads. Object Model allocates and traces declared
reference slots; scoped Roots messages protect inputs, intermediate paths, copied
elements and sort keys across allocating or reentrant calls. Construction scopes
discard obsolete intermediate versions as a result grows. Payload extents are
checked before native reads. Dependencies and root providers belong to the
initialized instance; no process or thread-local sort-root stack remains.

`indexOf:` uses identity first, then live `collectionEquals:` on the element.
See [Map's key protocol](../birnam-map/README.md#keys). The private `kernel.rs`
remains the existing pure Rust persistent-vector implementation for Rust consumers;
it is not a second native module API.
