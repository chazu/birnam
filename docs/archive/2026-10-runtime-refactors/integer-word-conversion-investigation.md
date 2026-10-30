# Integer–Word conversion investigation

> Archived 2026-10-30. Historical decisions, implementations and measurements;
> not current architectural authority or a work queue. Statements such as
> “current,” “required,” and “remaining” describe the recorded period.
> Use [Birnam invariants](../../../INVARIANTS.md) for current requirements.

The accepted addition trace still contains three temporary Words and one result
Integer. Two `optionalWordResultOf:selector:argument:resultFactory:` subtrees use
22 sends combined; `objectResultOf:selector:wordArgument:` uses eleven. These
33 sends include three allocations and representation checks, not just adapter
entry calls. Additional allocation/validation work belongs to the Word arithmetic
subtree and is excluded from this 33-send category.

## Targeted experiments

`integer_box_observability.rs` runs production authored addition on 7 and 9.
A current `Word fromBits:` replacement receives both input conversions, retains
the resulting distinct Objects, and finds them still alive after addition and
precise collection. Lexical nonescape is therefore insufficient: constructors
can publish those temporary identities. With the same replacement returning Word
10 for each input, addition returns 20. Restoring it returns 16 again.

A current `Word bits` replacement observes the third, result Word and returns
native 99. Addition then returns Integer 99, not 16. Removing the result conversion
would skip that actual getter and change the system's malleable behavior.
No private payload is read by this fixture; all operations use Id messages.

The existing allocation-replacement experiment in `word_expression.rs` confirms
that even the smaller within-body optimization restores original intermediate
Word constructions when its allocator is replaced. Its stateful permission test
also denies omission without a method-epoch change. The signed/mixed Error oracle
continues to pass.

## Decision

No unguarded boxing removal or specialized Rust Integer arithmetic kernel is
introduced. Production still has 177 sends and four allocated Objects. This
investigation adds concrete observability regressions and proof obligations,
not a CPU speedup or a claim that the canonical three Words are irreducible.
Moving the same conversions into another native helper would mostly shuffle
these sends and grow the protocol; that was rejected as a simplification.

The existing Word expression compiler covers a nonescaping Word-operation region
within a synchronous body. It does not prove the full Integer helper chain,
current factory/getter meanings, public/private method replacement, virtual Object
identity or restoration at unknown callback/Frame boundaries. The existing
`ExecutionRegion.tla` contract is the appropriate larger abstraction; its owner
schedule is a conditional payload-access bound, not a total real-operation minimum.

## Bounded implementation path

1. Represent conversions in a compiler region explicitly: two current Integer
   bit reads, pure generic Word descriptor operations, and result construction.
   Integer remains the representation owner; Word remains the bit-operation owner.
2. Include current factory, getter, helper and branch bindings in admission,
   along with allocation permission, custom lookup and code/Frame ownership.
   Arithmetic-operator guards alone miss the constructor/getter experiments above.
3. Recover the original Objects and identities before a callback/unknown send
   that can observe or retain them, without replaying completed side effects.
   Pure standard conversion omission must be admitted before replaced constructors
   execute. Permission and meaningful dependency checks remain current.
4. Compare that region to the independent signed/mixed oracle and these observable
   replacements. Measure allocations and sends separately. Only then enable it.

This is compiler-region work, not a local deletion from `native_abi.rs`. It has
not been implemented by this investigation. Lean adds an explicit counterexample
for dropped allocation/identity observations and requires conversion bindings in
certificates. The TLA+ region model retains recovery, permission, code lifetime,
current dispatch and owner-boundary counterexamples.

Validation passed three native checks, all 141 current Lean theorems/mutations and
the nine focused TLA+ region configurations, including expected counterexamples.
The production implementation did not change. See the [receipt](../../measurements/integer-word-conversion-investigation-20261009.json).
