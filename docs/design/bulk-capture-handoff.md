# Bulk capture handoff

Generated synchronous bodies previously sent `captureReadMode` and then one
`captureWordAt:` per slot. For the measured Integer addition, that was 41 reader
sends and 15 mode queries, plus their 15 current-binding checks. The live Block
already covered its immutable capture edges throughout the call. Repeating pure
projections added dispatch work without adding protection.

## Implemented contract

Execution implements `tryBorrowCaptureOperands:` as an ordinary Id message on
Object. It accepts a native Word count and returns either native Unit (decline)
or native Bytes containing exactly that many ordered, native-endian Oop
identities. This is in-process ABI marshaling of opaque references, not a portable
image format or permission to inspect another owner's payload.

Successful borrowing requires an initialized portable Block with enough capture
slots, its current canonical bulk query, direct mode query and direct reader,
and the canonical Id binding-query services including the matcher itself.
Execution checks these together through current `nativeBindingsMatch:`. A
forwarding query wrapper cannot obtain a canonical grant while its own binding
is selected. Admission is repeated for each call, with no retained verdict or
cached executable dispatch.

Execution reads only its own storage and writes a reused private byte buffer.
Every returned reference must be nonnull and covered by the live receiver's
immutable capture edges. No callbacks occur after positive canonical admission
and before returning the borrow. The consumer copies all references into its
call-local array before any further Id call. Later requests may overwrite the
owner buffer; they cannot alter the copied identities. Cells are copied as
identities, so sharing and mutation continue through their usual messages.

A replacement native provider must honor this same borrowing/lifetime contract
or return Unit. A successful borrow cannot contain arbitrary newly created
objects: those require separate roots. Replaced ordinary readers retain the
existing protected per-result path, including fresh results and collecting
callbacks. Native providers remain trusted ABI participants; validating the
byte length cannot prove that an arbitrary provider owns the returned edges.

Unit or an unbound bulk query uses the original direct/factory mode and reader
path. Unsupported Method receivers and insufficient capture slots decline,
preserving existing inspection and absent-slot behavior. Other native failures
propagate. Wrong carriers, wrong byte lengths, null nonempty buffers and null
references fail rather than silently interpreting malformed data.

This adds no Id floor entry, payload layout dependency in emitted code,
language capture container, alternate heap, machine-code startup compilation,
or code lifetime shortcut. Rust cold bootstrap support stays intact. Existing
compiled bodies can use the retained per-slot protocol; explicit compilation
emits the new attempt.

## Validation and proof scope

`Capture.lean` adds ordered-list copy, receiver-edge collection coverage and
current-bulk-binding admission lemmas. The copy lemma models identity copying;
the native-endian encoding and C `memcpy` correspondence remain implementation
obligations tested by the native fixture. It does not prove the Rust/C runtime.

`BulkCaptures.tla` checks duplicate aliases, collection, later buffer overwrite,
current admission and receiver lifetime in a finite model. Required broken cases
admit a stale bulk provider, copy after an intervening overwrite, reorder slots,
or release the receiver too early. The existing capture and invocation models
cover fresh-reader fallback and caller/code protection. Neither package proves
arbitrary native providers, concurrent dispatch mutation or a moving collector.
The implementation retains Execution's existing serialized invocation model.

Native capture tests exercise empty/prefix/duplicate batches, replacement and
forwarding providers, unbound fallback, malformed results and failures, fresh
replacement results with precise collection, and a later bulk query overwriting
the buffer during a collecting body callback. Existing local/capture root reuse,
Word helper, invocation and language value tests check adjacent boundaries.

## Measurement and acceptance

[Receipt](../measurements/bulk-capture-handoff-20261009.json) records source/library
and evidence hashes. A fresh temporary release prefix rebuilt authored artifacts
and passed the installer's saved-image startup gate. No user installation was
changed and startup speed was not benchmarked.

| Per successful Integer addition | Before | After |
| --- | ---: | ---: |
| Actual `+` activation sends | 338 | 297 |
| Capture setup sends | 71 | 30 |
| Individual capture reads | 41 | 0 |
| Bulk queries | 0 | 15 |
| Capture binding checks | 15 | 15 |
| Allocated Objects | 4 | 4 |

The 41-send reduction is 12.1% of the addition activation. The benchmark including
its fixture measures 339.375 versus 298.375 sends per addition. Allocations remain
three Words and one Integer, with no Block construction. Unrelated work categories
are unchanged. Short four-second cross-prefix CPU samples observed about 47.94
versus 46.86 microseconds per addition. This small difference is directional only:
these were sequential runs on a shared machine, and the earlier run overlapped
bootstrap construction. The paired profiles within each prefix both exercise
production defaults; their ratios do not compare this implementation to the old
one. The reliable result here is the deterministic send reduction.

Acceptance passed 33 focused Rust tests, 15 native checks across 10 binaries,
14 task assertions at quanta 128 and 1, 119 Lean theorem checks, and all 62 TLC
configurations including required unsafe counterexamples. The native checks cover
capture access, lifetime reuse, helpers, ABI inputs, value semantics, closure
construction, materialization and generation/code lifetime. This is focused
acceptance, not a claim that the entire exhaustive test matrix was rerun.
