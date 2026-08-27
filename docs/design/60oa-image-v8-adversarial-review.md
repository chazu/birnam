# 60OA — Image-v8 Identity Preservation Adversarial Review

> **Status:** Review complete 2026-08-27. The findings below are incorporated
> into accepted Design 60O.

## A1. Special-case only the exported Integer prototype

That repairs the observed example but leaves the same identity collapse for
unexported Integer boxes and for Float, Word, and String boxes. It is rejected
as a symptom patch. 60O applies one rule to every noncanonical Oop.

## A2. Promote a value only when it appears more than once or owns methods

That requires an identity-sensitive graph census, makes bytes depend on a
second policy, and can miss identity observations not visible to the writer.
It is rejected. Every noncanonical Oop receives a heap identity uniformly.

## A3. Treat primitive values as identityless

Birnam currently boxes these values, compares Oops, allows them to own private
vtables, and exposes them through ordinary sends. Declaring them identityless
would change the language to fit an image optimization. It is rejected.

## A4. Serialize the current raw payload

Integer, Float, and Word happen to have one-word floor layouts, but String owns
an out-of-line allocation and stores a native pointer. Raw layouts are Runtime
ABI, not portable meaning. It is rejected in favor of explicit constructor
locators for all four families.

## A4a. Keep using Word for both unsigned values and native pointers

That makes an imageable numeric value sometimes contain a stale process
address, permits pointer signatures to consume integers, and cannot express who
may free an allocation. It is rejected. `Word` remains portable numeric data;
the exported `ForeignAddress` family carries explicit process-local authority,
ownership, honest image rejection, and ordinary malleable methods.

## A4b. Hide ForeignAddress behind a private native handle

A private carrier would remove pointer behavior from Birnam's ordinary send
surface. It is rejected. The family prototype is exported, its protocol is
installed through normal methods and remains replaceable, and only the raw
address representation and ownership enforcement stay in the native floor.

## A4c. Allow numeric carriers temporarily at TCC or Alien boundaries

A compatibility path would preserve two meanings for `p` and let stale address
bits leak into v8 despite the new type. Backward compatibility is not a product
requirement. TCC symbol binding/lookup, generated pointer shims, callbacks,
Alien buffer operations, and Store's native authorizer registration all make
the clean cut together.

## A4d. Let all ForeignAddresses answer free

Borrowed symbols and callbacks are not allocated by Alien. Freeing them is
undefined behavior; freeing an owned pointer twice is equally invalid. The
two-word capability records its kind, `free` accepts only live Alien-owned
instances and clears before release, and the same idempotent transition is used
by its finalizer.

## A5. Constructor records accidentally replace canonical prototypes

The canonical `Integer` object satisfies Integer-family tests. Classification
must resolve singleton and export identity first. 60O fixes the ordering and
requires executable prototype-binding coverage.

## A6. A private-vtable box is no longer recognized as a primitive

Exact-vtable tests fail after `ensurePrivateVtable`. 60O requires ancestry-based
family recognition. Loading constructs the family value first; binding install
then creates or reuses a private vtable on that same relocated Oop.

## A7. Inline values survive through CodeBindingInventory

Keeping v1 fields would preserve the original split authority even if Heap were
fixed. CodeBindingInventory v2 removes inline Integer and String forms and uses
the same global HeapRefs as Facts and Heap.

## A8. The clean cut leaves a hidden v7 recovery path

A dormant reader creates ambiguous admission and untested compatibility. The
accepted migration renames the current reader/writer and release gate to v8,
bumps the header, replaces CodeBindingInventory bytes, and rejects every other
image version at the fixed header.

## A9. Uniform HeapRefs cause unbounded lifetime growth

They add one record per live noncanonical boxed object, not per historical
occurrence. Repeated references share a record; dead values are absent after
Store compaction and GC-root projection. Image size remains proportional to the
live graph, and 60J's repeated churn/resave plateau is retained as a gate.

## A10. Specialized locators leak into ordinary fields

That would allow identity to be bypassed again or make constructor placement
ambiguous. Structural preflight admits tags 13 through 16 only as locators with
zero words and rejects them everywhere else.

## Conclusion

No unresolved finding requires a policy choice. The uniform heap-identity rule
is the smallest change that preserves Birnam's boxed object model, ordinary send
surface, malleability, portable bytes, honest rejection, and bounded-live-set
lifetime invariant together.
