# Vector

The initializer takes Namespace and publishes completed `Vector`, `VectorVt` and
`VectorNode` objects. It needs Collection's parent behavior, Object Model, Roots,
Integer, Word, String, Bytes and Boolean/Nil identities. Bootstrap constructs the shared parent;
there is no Collection installer library or Execution prerequisite.

Vector retains its persistent 32-way trie with a 32-element tail. `add:` shares
unchanged paths and returns a new Vector. Its public messages are:

- `new`, `clone`: empty Vector (clone retains the established emptying behavior).
- `size`: Integer; `at:` Integer index: element or nil.
- `sizeWord`: immediate size Oop; `atWord:` accepts a Word or size Oop index.
- `add:` Object, `,` Vector: persistent Vector results.
- `copyFrom:to:` Integer bounds: half-open slice, or nil for invalid bounds.
- `reversed`: new Vector; `indexOf:` Object: Integer index or nil.
- `do:` callable: send `value:` to each element, return the original Vector.
- `inject:into:` initial value and callable: fold using `value:value:`.
- `sortBy:` callable: compute each key once, stable insertion sort with live `<`.
- `mergeSortBy:` callable: compute each key once in input order, stable bottom-up
  merge ordering with live `<` (strict, consistent ordering required). Comparisons
  follow merge order, rather than insertion order; failures propagate unchanged.
  This explicit operation lets Doc avoid quadratic work without changing the
  comparison sequence of existing `sortBy:` callers.
- `printString`: String in `#[a b c]` form, using live element printing.

Iteration keeps the existing language-result behavior: `do:` ignores callback
results; folding carries each result. Native protocol failures propagate. Callables
are opaque receivers; they do not require a native Block table.

Only Vector interprets its payloads. Object Model allocates and traces declared
reference slots. Id protects incoming call objects. An owner-local scoped list
protects detached tree snapshots, intermediate paths, copied elements and sort
keys across allocating or reentrant calls. Scope reset replaces its own suffix
with the current persistent result; dropping a nested scope preserves outer roots.
There are no per-element root tokens or cleanup sends. Payload extents are checked
before native reads. Only cold initialization uses Roots retain/release, before
this owner can register its completed root provider.

`indexOf:` uses identity first, then live `collectionEquals:` on the element.
See [Map's key protocol](../birnam-map/README.md#keys). The native trie is the sole
Vector implementation; the unused Rust-only duplicate has been removed.

All Id values are Oops. Printing asks each element for its current `printString`
and the result for `bytes`; it assembles an owned Bytes object and sends it to
String's current `fromBytes:`. No borrowed native byte pointer crosses Id.

Run `bash scripts/oop-id-slice-test.sh`. The fresh-process numeric/collection check
uses the real Integer, Float, String, Bytes and Vector owners. It checks trie/tail
growth boundaries through 1,070 elements, persistence, inherited Collection
behavior, nested sorting with fresh keys and allocation-time GC, folds, printing
with capacity-accounting GC, and String splitting into real Vectors. Full-language
Execution/compiler/image/worker acceptance and performance recovery remain pending.
