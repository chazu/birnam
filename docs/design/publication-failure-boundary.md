# Publication failure boundary experiment

2026-10-15. **Experiment complete; publication migration remains unimplemented.**
This investigates the first gate in [method publication ownership](method-publication-owner.md).
It found a usable fresh-process calling convention and a reproducible image defect
that prevents adopting it yet. No production runtime, bootstrap, ABI or image-format
change is made by this experiment.

## Subsequent design clarification

The user has rejected two language error conventions and does not require backwards
compatibility here. The [shared-invocation investigation](shared-invocation-investigation.md)
now supersedes the recommendation below: unify ordinary result/error behavior and
investigate shared invocation machinery before preserving adapter distinctions.
The measured image mismatch remains valid evidence of the existing implementation;
it does not establish that its two behaviors should become a language feature.

## Oop integration checkpoint (2026-10-27)

The active harness now retains nine native publication scenarios across Store,
directly materialized synchronous policy and explicit compiled native policy.
The superseded negative adapter comparisons below remain historical evidence.
Porting the active contract exposed suppression of callback-local handling by the
typed native adapter's ambient preservation flag. Removing that implicit flag
change fixes the 27 comparisons without adding an Id protocol or publication
owner. Native/conversion and core checks pass; this does not complete cold
installation qualification or the remaining language migration.
[Current receipt](../measurements/publication-callback-policy-oop-20261027.json).

## Required behavior

Compare against actual Store `addMethod:on:with:`, not a rewritten reference:

- Native authorization failure returns the exact native status/detail before the
  dictionary changes. Ordinary non-true authorization values are returned as vetoes.
- A native notification failure returns that exact failure after the dictionary
  change. It does not undo the method or replay the callback.
- Ordinary Error objects returned by notifications are successful void results.
- A callback can handle the Error produced by its own ordinary inner native send.
  Moving the caller must not silently change that callback's handling policy.
- A normal source caller still observes the language conversion at its own boundary.

The fixture uses a real Id dictionary, native Methods and current callback sends.
Its native Probe supplies test state, controlled failures and recording/collection;
the artifact owner itself sends directly to the authorizer, dictionary and observer.
It is an experimental partial publisher: optional description, Generation admission,
full validation, bootstrap cutover and compensation are not implemented here.

## Existing protocols and observations

The existing operation is **`Execution evaluatePreservingFailures:`**. The earlier
plan's spelling `preserveNativeFailures:` was wrong; that selector does not exist.
Execution internally keeps a dynamically scoped `preserve_native_failures` flag.
The native-message request adapter also sets it while evaluating a request.

| Experiment | Native failure | Callback-local handling | Conclusion |
| --- | --- | --- | --- |
| Existing Rust Store | Exact status/detail | Preserved | Reference behavior |
| Ordinary resumable authored policy | Authorization becomes Error; notification failure can be discarded as a value | Preserved | Loses the required native channel |
| Whole-body or per-send preservation scope | Native failures escape, sometimes replaced by a nested-machine failure | Suppressed inside entered callbacks | Not equivalent |
| Deferred request inside an ordinary body | Can lose the failure again at the surrounding language boundary | Suppressed during the request | Not equivalent |
| Deferred request as native tail result (`m`) | Exact outer native failure in tested cases | Suppressed during the request | Not equivalent for arbitrary existing observers |
| Raw zero-argument deferred tail result (`v`) | Exact | Preserved | Useful narrow control; not a general sequencing interface |
| Directly materialized synchronous policy Block | Exact | Preserved | Works for the tested direct invocation, but is not yet an installed method |
| Method defined by source evaluation with an `o` native signature | Becomes Error at the resumable named adapter | Preserved | A body signature alone is insufficient |
| Artifact with synchronous bodies **and synchronous named adapters** | Exact | Preserved | Matches Store in the fresh-process matrix; fails image preservation below |

The main matrix has nine scenarios across nine policy paths (81 comparisons),
plus eleven deferred-boundary cases. It covers denial, native failures on both
sides of commit, deliberate Error values, callbacks handling or returning an
inner failure, and normal success. Assertions compare the successful artifact
route with Store's result, actual dictionary state and event order. Additional
checks cover collecting callbacks, native detail 17 versus 99, live replacement
on the same callback receiver, restoration of the ambient conversion policy, and
an ordinary source caller of the artifact owner. Callbacks are not replayed.

Some rejected nested/resumable preservation paths produce `(1, 101)` instead of
`(881, 17)`, and one request path fails before cleanup is observed. This is a
separate observed limitation of those existing paths, not a new publication
implementation or a reason to normalize different failures as equivalent. The
experiment logs it; it does not repair the resumable engine.

Synchronous artifact compilation is already used for native-facing authored
modules such as Trace. Its selector is
`Codegen compile:output:from:synchronousBodies:`. It is not a new Rust publication
service, cached callback or monkeypatch guard. The important distinction is the
**whole installed calling convention**, including the named adapter, rather than
merely the source body's representation.

## Image roundtrip defect

The minimal image fixture defines a native-facing `publish` method that sends
`Execution invalidRequest` and would otherwise return `#success`. It contains
no test-native callback addresses. Compile/load it using the synchronous artifact
path, save an image, remove the artifact file, then restore:

| State | Native caller observes |
| --- | --- |
| Fresh artifact | Native failure `(INVALID_REQUEST=1, detail=0)` |
| Mapped image restoration | Loaded Error with kind `#nativeInvalidRequest` |
| Rebuilt image restoration | Loaded Error with kind `#nativeInvalidRequest` |

The loader's return value is checked according to its actual contract: true means
mapped and false means rebuilt, not failed. The restored calls are checked after
collection. No source or original artifact file is required for the reproduction.

The source explanation agrees with the experiment:
[`image::Plan::new`](../../rust/birnam-codegen/src/image.rs) emits named-adapter
requests with `synchronous: false`, even though it includes both styles for
program bodies. A fresh synchronous artifact uses synchronous named adapters;
reconstructing its binding with a resumable adapter introduces language failure
conversion. The mapped image also uses a generated image bundle, so mapping does
not preserve the original artifact's adapter convention automatically.

Consequently, the fresh-process success does **not** establish an image-safe
publication owner. The image characterization test deliberately asserts the
current mismatch; passing it confirms this known defect, not migration readiness.
When corrected, replace those expectations with preservation of exact `(1, 0)`.

## Original recommendation and next boundary (superseded)

Fix the generic persistence of native-facing method calling conventions before
moving publication. Record/preserve the installed adapter convention in its owned
method/binding metadata and carry it through image bundle generation and rebuild.
Do not infer it solely from the presence of a native signature: the source-evaluated
`o` method and the synchronous-artifact `o` method already have different observed
failure behavior.

This distinction is behavior-bearing calling-convention metadata, not a choice
to change silently for performance. The experiment does not establish equivalence
of arbitrary synchronous and resumable code, or authorize changing ordinary
source Error handling.

That work needs an explicit compatibility decision for images lacking the
additional distinction. Preserve their documented historical interpretation;
do not silently reinterpret every native-signature method. Determine whether the
existing selection metadata can express the distinction or whether a versioned
encoding is required, then add positive mapped/rebuilt tests for both conventions.
This experiment does not choose or implement that encoding.

After image preservation is established, use the ordinary authored synchronous
artifact route as the candidate publication boundary and rerun the native oracle
matrix. Then finish the remaining description-ordering, process-local hook,
bootstrap cutover and Store-absent publication gates. No new language-wide error
rule or permanent native publisher is justified by these results.

## Evidence and limits

- [`publication_failure_boundary.rs`](../../rust/birnam-siphon/tests/publication_failure_boundary.rs)
  and its [authored fixture](../../tests/fixtures/publication-failure-boundary.birnam).
- [`publication_failure_image.rs`](../../rust/birnam-siphon/tests/publication_failure_image.rs)
  and its [minimal fixture](../../tests/fixtures/publication-failure-image.birnam).
- Lean `PublicationFailure` proves channel/detail loss cannot be inverted from the
  converted value alone, distinguishes ordinary Error from failure, and shows why
  an ambient preservation policy changes a callback's handling. It does not prove
  compiler or ABI implementation correctness.
- TLA+ `PublicationImage` checks the required roundtrip contract and produces
  counterexamples when either restore path replaces the adapter convention. Its
  safe configuration models the intended contract, not today's implementation.
- [Source-bound receipt](../measurements/publication-failure-boundary-20261015.json)
  records the runtime libraries, observations, checks and formal results.

The checks passed in five native integration binaries. The full formal gate
checked 273 Lean theorems, rejected 63 mutations and obtained all 153 expected
TLC outcomes, including the two image counterexamples. These successes verify
the experiment and its known-defect characterization, not the missing image fix.

Qualification uses the prior audited production prefix with unchanged runtime
sources. It runs the new experiments plus existing publication, incoming-native
and invocation-input checks. This is not a new release, full Store-free startup,
performance measurement or whole publication implementation proof.


Reproduction:

```sh
CARGO_TARGET_DIR=.cargo-target/native-message-tests \
BIRNAM_TEST_MODULE_DIRECTORY=PREFIX/lib CARGO_PROFILE_DEV_PANIC=unwind \
  mise exec -- cargo test --offline --locked -p birnam-siphon \
  --test publication_failure_boundary --test publication_failure_image \
  --test publication_contract --test native_incoming --test invocation_inputs \
  -- --include-ignored --nocapture
mise exec -- python3 scripts/check-rooting-model.py
```
