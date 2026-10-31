# Native image admission before storage projection

Baseline `cd18e9c5`, 2026-10-31. The image refusal failure reproduced on the
unchanged runtime and blocked full image validation during earlier simplification
work. The failing case stores a Machine in Globals, both with ordinary and private
behavior, and expects `saveImage:` to reject it without replacing the destination.

## Cause and change

The ordinary `imagePayload` default returns the receiver. Image identified the
Machine's native family, then asked ObjectModel for its image storage size before
checking the owner's slot-admission policy. ObjectModel correctly rejected this
request: it does not describe Machine execution storage. That native
`invalidRequest` escaped instead of Image returning its documented rejection.

Before:

```text
imagePayload -> native family -> ObjectModel size query -> owner slot approval
```

After, for families outside the existing built-in image encodings:

```text
imagePayload -> native family -> owner advertises slot policy?
    absent: reject without a storage query
    present: size query -> current owner approval -> all-Oop layout validation
```

The owner is resolved again after the size query because replaced code can collect,
reenter, or change the applicable policy. No Method is cached across that call.
Temporary owner and approval references use Image's existing scoped roots. A
missing policy is a format refusal; a failure from an owner operation remains a
failure and is not caught and converted to nil.

No Machine-specific exception or new family catalog was added. A trial authored
Machine refusal was removed in favor of correcting the general admission order.
The duplicated lists of existing built-in slot encodings now share one private
predicate. `imageAcceptsSlots:` uses the module's fixed literal names rather than
allocating and interning a Name during each check. Image formats, Id, ObjectModel,
and native object representations are unchanged.

## Validation

The new `image_native_refusal` test failed against the baseline and passes with
the fix. It checks ordinary/private Machine rejection, unchanged destination
bytes, continued rejection of a Machine storage query by ObjectModel, propagation
of a replaced `imagePayload` failure, and propagation of an actual `invalidRequest`
from an applicable ObjectModel getter. These negative cases rule out blanket
error translation as a fix.

The slot-value roundtrip and snapshot-mutation tests pass. They cover supported
owner-approved native slots, Queue snapshots and aliases, live method behavior,
projection lifetime, intervening edits, and preservation of failed-save output.
The unchanged full image test includes mapped/rebuilt/stripped roundtrips,
unsupported capabilities, partial application, and repeated resaves.

The exact final binary passes that full image test: two parent tests and their
child roundtrips/resave checks. The final focused tests, workspace check with all
targets/features, and workspace library unit tests also pass. The workspace check
reports an existing unused-unsafe warning in the `round3_costs` example; this slice
does not change it. This is not a release qualification or installation claim.

Exact final results, source/library hashes, and raw-evidence paths are recorded
in [result.json](result.json). Earlier measurement reports retain their original
failure outcomes; this record is the later fix, not a rewrite of those receipts.

## Scope and cost

The save path for existing built-in families does not gain the new policy probe.
An unknown family with slot support has an additional owner/protocol availability
check before its size query; the later check preserves live approval after that
query. Families without support avoid an irrelevant storage call altogether.
The added selector is retained with the existing native declaration literals;
it is not a cached lookup verdict or a new service.

This is an admission-order correction and code consolidation. No general image
throughput improvement is claimed. It removes a known validation failure while
preserving the ownership of native storage and current message behavior.

## Reproduction

Build the changed Image library and use an isolated prefix containing it, then run:

```sh
BIRNAM_TEST_MODULE_DIRECTORY=/absolute/prefix/lib CARGO_TARGET_DIR=.cargo-target/oop-slice CARGO_PROFILE_DEV_PANIC=unwind mise exec -- cargo test --offline --locked -p birnam-siphon --test image_native_refusal --test image_slot_values --test image_snapshot_stamp --test image -- --include-ignored
```

Raw logs are under `.cargo-target/measurements/image-refusal-20261031`. The small
initial probe distinguished Machines from captured Integer and Block Methods;
only Machines produced the unexpected native error. The final regression and
existing full tests are the maintained replay path. The user's installed prefix
and remote branch are not modified by this validation.
