# Runtime selector hashing — 2026-10-12

Logue's post-fix native sample concentrated time in hashing (23.2% of recorded
main-thread samples) and Id method lookup (12.9%). The earlier JSON application
showed similar costs. This slice reduces hashing inside the runtime; Logue and
JSON application algorithms are held fixed in its performance comparisons.

## Retained implementation

### Reuse the randomized Symbol-name digest

Id already hashes Symbol name bytes with a private randomized hasher. Its weak
index maps that 64-bit digest to a chain in the ordered Symbol inventory. The
standard HashMap then hashed the digest again with another randomized hasher.

The index now uses the completed digest directly as its table hash. It still
compares complete digest keys, and every candidate name in a collision chain
still compares full bytes. Original name hashing, canonical identity, insertion
order, embedded-NUL rejection, missing-name results and retirement/rebuilding
remain unchanged. The digest index owns no Symbol roots and exposes no new ABI.
It is private runtime storage, not a new Rust-only language operation.

New tests force many names onto one digest, use distinct digests with shared low
bits, grow the index, retire chain heads/middles/tails, check survivor order and
name accounting, and re-intern retired names. Ordinary randomized names,
separately allocated equivalent byte keys, empty names and UTF-8 are checked too.

### Read five retained selector identities directly

Four owners already intern these names once and keep them in immutable root
inventories. Their measured hot paths now read the same Symbols from fields:

| Owner | Selector identities |
| --- | --- |
| Integer | `tryRepresentationIdentityOf:behavior:bytes:` |
| ObjectModel | `tryRepresentationIdentityOf:behavior:bytes:` |
| GC | `allocationChargeFor:` |
| Namespace | `symbolBytesOf:`, `at:` |

The extra storage is eight bytes for each Integer, ObjectModel and GC owner
instance, and sixteen per Namespace instance on this host: forty bytes for the
usual group of four owners. It is not per application object or per message.
The original inventories still own root membership and enumeration order.

Only name identities are duplicated. Namespace values, selected methods,
permission results and contexts are not cached. Every target send, custom
lookup, getter, failure and existing admission/recovery path still runs normally.
This introduces no new speculation requiring deoptimization and removes no
existing deoptimization checks. It adds no monkeypatch guards to authored code.

## Investigation boundaries

The variants were measured separately: digest reuse versus baseline, then direct
owner fields on top of digest reuse. Both showed useful application results, so
both were retained. A final combined comparison uses more samples.

The initial eight-name Execution shortcut already exists; this slice does not
expand it indiscriminately. Native stack attribution identified the four owners
above. Caching Id's weakly held selector Objects, namespace values or executable
entries would have different lifetime/binding obligations and was not introduced.
Repeated primary name hashing and further owner inventories remain possible
follow-ups. Method-table lookup and first-match collection traversal remain the
other two agreed high-level targets.

## Validation scope

The new Lean lemmas show that selecting a digest bucket preserves full-name
lookup, including colliding names. A mutation returning the first bucket member
without comparing its name must fail. Existing retained-name/current-dispatch
and root-edge proofs apply to the owner fields. These abstract checks supplement
real native tests; they do not prove Rust HashMap or the generated machine code.

Production prefixes keep normal GC, the fixed 16 MiB threshold and compiler
resource counters disabled. Native fixtures using `CompiledFloor` require those
counters, so they run in a separate diagnostic prefix. The first attempted run
of those fixtures stopped at the missing diagnostic counter; it was rerun with
the required TCC build. Diagnostic libraries are excluded from performance runs.

Qualification includes a fresh production bootstrap build, Symbol collision and
retirement tests, numeric/representation fallback, allocation-accounting hooks,
namespace rebinding during GC, method observation, source-free saved images,
full module tests, and both application oracles. JSON acceptance also exercises
valid/invalid fixtures, 50/500-record inputs and edit–rebuild–run.

## Results

The [evidence record](../measurements/selector-hashing-20261012.json) contains the
isolated pilot runs, final comparison, confirmation run, runtime/source hashes,
caller attribution and validation receipts. Final prefixes differ only in five
native libraries: Id, GC, Integer, ObjectModel and Namespace. Their host binary,
compiled application artifacts and saved image files are identical. The compared
candidate libraries are byte-for-byte the rebuilt production libraries; the
freshly generated production images are qualified separately.

The combined ABBA comparison uses two fresh processes per variant, one warm-up,
and three timed samples per process. Each Logue sample has four consecutive
16-turn crowded batches. Because the JSON process timings varied noticeably,
a second, reversed-order BAAB comparison was collected for JSON. All recorded
JSON samples are retained in the pooled result; none is discarded as an outlier.

| Workload | Baseline median | Runtime changes | CPU reduction |
| --- | ---: | ---: | ---: |
| Logue, crowded 16-turn batch | 1,329.67 ms | 1,154.62 ms | 13.2% |
| JSON, full 50-record report | 333.62 ms | 311.85 ms | 6.5% |

Logue has 24 batch samples per variant; JSON has twelve samples across four
processes per variant. JSON's first comparison shows 4.6% improvement and its
confirmation shows 10.7%. The first baseline process contains a 523.56 ms sample;
its other two samples are 371.65 and 389.19 ms. The later baseline process medians
are around 331–336 ms. These measurements support a gain but not a precise,
universal percentage. The complete samples are retained in the receipt.

The separate pilots also favored both pieces: digest reuse improved both
applications, and the direct owner fields added gains on top of digest reuse.
Pilot results are exploratory; their smaller sample sets and pre-comment build
hashes are recorded separately from the final delivery comparison.

Final checks pass **10 Id unit tests**, **16 native test binaries/tests**,
**672 module assertions**, source-free mapped/rebuilt images, the JSON acceptance
and edit cycle, and Logue's 34 game assertions plus 31 measured checkpoints and
live replacement case. The formal gate passes **198 theorems**, rejects **40
mutations**, and checks **118 TLA+ configurations**. Production and diagnostic
prefixes are disposable; the user's normal installation was not changed.

The [fixed-digest follow-up](fixed-selector-digests.md) investigates repeated
primary name hashing for Id's fixed internal queries, while retaining current
weak-Symbol resolution and method lookup.
