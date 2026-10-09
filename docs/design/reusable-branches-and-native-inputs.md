# Reusable branch handlers and native input protection

Implemented 2026-10-08. Integer's authored arithmetic uses shared handlers instead
of constructing two captured Blocks for each decision. Boolean provides a general
data-selection API so other Birnam code can use the same pattern. Native Word and
Integer entries borrow their caller's input coverage while retaining protection
for fresh allocations inside the call.

## General Birnam control API

`selectIfTrue:ifFalse:otherwise:` returns one supplied Object without invoking it.
Canonical true chooses the first, false the second, and another Boolean identity
the otherwise value. Its policy is Birnam source over the existing generic Word
identity-selection messages. It is an ordinary replaceable Id method.

```birnam
let success := [:factory :bits | [factory integerFromBits: bits]];
let failure := [:factory :bits | [factory integerOverflow]];
let unsupported := [:factory :bits | nil];
let handler := [fits selectIfTrue: success ifFalse: failure otherwise: unsupported];
[handler value: factory value: resultBits]
```

The handlers are created once; changing data is passed as arguments through
current `value:` messages. Selection accepts arbitrary Objects, not just Blocks,
and does not dictate arity or native-result conversion. A native caller may invoke
the selected callable through Id with appropriate carriers. Existing thunk-based
conditionals and their deferred native-result ABI remain supported and unchanged.
This slice introduces no compiler closure virtualization or new native dispatch.

## Integer source rewrite

Shared handlers cover left validation, exact Error precedence, right Integer
recognition, Float fallback, type failures, overflow/result construction,
multiplication and division/remainder. Retained operations, checks, getters and
constructors use current Id messages in their defined order. The selected handler
gets the actual factory/receiver, preserving derived factory behavior.
Noncanonical truth outcomes use the nil/failure handler appropriate to that stage.
Unselected handlers do not evaluate divisions, conversions or diagnostic literals.

Successful addition no longer allocates branch Blocks or private vtables and no
longer invalidates Word's global-epoch meaning cache by installing those methods.
Shared handlers retain module/code owners. The old private conditional/constructor
interception history is removed from this implementation; retained operations and
the new selector remain replaceable. Signed and mixed-number policy stay in Birnam.

## Native numeric inputs

The Id ABI requires the caller to protect receiver, Object operands and native
code/context owners until invocation returns, including callback reentry.
Word/Integer entry wrappers therefore do not acquire duplicate input tokens.
The guarded Word expression validates all Object carriers before callbacks and
still owns copied byte/operand buffers.

Fresh result boxes are not incoming inputs. Their first protection remains before
type/size validation callbacks. Intermediate diagnostics, iteration values and
obtained factories likewise keep needed protection. No foreign payload is read;
representation/type/permission checks remain current. Rust required for cold
bootstrap remains, and native image startup does not silently recompile supplied code.

## Proof and runtime correspondence

`Branches.lean` proves default decision/result and effect-order equivalence for
the addition decision tree, including noncanonical truth outcomes, and instantiates
the general invocation input-coverage theorem for native numerics.
`ReusableBranches.tla` checks current selected-handler dispatch, single-branch
execution, callback/collection lifetime, failure and no replay. Broken choices,
stale entries, missing argument protection and replay must fail.

These are scoped models, not a verified compiler/runtime. The signed-boundary and
mixed-Float oracle checks actual source behavior. Boolean tests verify no candidate
invocation during selection, noncanonical selection, malformed carriers and current
collecting/failing replacements. A saved authored entry is Method-rooted while
its binding is replaced; the entry alone does not own its context/code.
Native tests collect inside representation queries, check caller-held inputs
without numeric re-retains, and verify fresh results survive that callback.

## Ordinary initializer persistence

The startup image gate exposed a pre-existing compiler distinction: synchronous
artifact bodies were requested using an inert candidate factory, so top-level
lets did not publish globals although source metadata projected capture-free
functions as globals. Shared `intNil2` first exposed the mismatch. The source
probe confirmed the globals were absent; the generation guard correctly refused.

Ordinary synchronous artifacts now request `moduleFactory:source:dispositions:`
from NativeBackend, which publishes root lets through current Globals messages.
The matching optimization-options message preserves benchmark controls. Inert
`factory:` remains for live candidate/materialization consumers and does not
publish globals. Both routes share the emitter with explicit root persistence;
only a resumable root may be persistent. The compiler test verifies this distinction.
The image/generation guard stays strict, and native image loading still maps code.

## Measurement and acceptance

[Receipt](../measurements/reusable-branches-and-native-inputs-20261008.json) records
the hashed census, libraries, timing and validation. The fresh temporary release
installation includes the initializer persistence fix and rebuilt authored bodies.

| Per successful addition | Before | Shared handlers | Plus native input borrowing |
| --- | ---: | ---: | ---: |
| Sends inside actual `+` | 513 | 362 | 338 |
| Census including fixture bookkeeping | 514.375 | 363.375 | 339.375 |
| Branch Blocks constructed | 8 | 0 | 0 |
| Physical Id Objects | 20 | 4 | 4 |
| Logical root registrations including fixture work | 87.0625 | 75.0625 | 60.0625 |

Shared handlers remove 151 sends; native input borrowing removes another 24.
Combined actual-addition savings are **175 sends, 34.1%**. Four Objects remain:
three Word boxes and the Integer result. Native input borrowing removes fifteen
logical registrations, including the five-value Word expression batch. All
tracked root tokens drain to zero.

Four-second active-host CPU samples measured 50.06 us/addition using the same
rewritten source with the previous numeric input-rooting libraries, versus
48.23 us with input borrowing, about 3.7% lower. This is a separate-version
observation, not a paired confidence interval. The earlier source-only timing
run overlapped source edits and is excluded. No whole-startup speed claim is made.

Passed 84 affected Rust tests, eighteen native integration checks across thirteen
binaries, fourteen task assertions at quanta 128/1, and the fresh release installation's
saved-bootstrap/tool-image checks. Native gates include the independent signed
Integer/Float/error oracle, Boolean replacements and native result carriers,
collecting numeric validation, Word permission fallbacks, root lifetime, entered
code/materialization and six generation-ledger checks. The formal gate passes
116 Lean theorems, 57 TLC configurations and all required negative mutations.
The user installation is unchanged. Full workspace/image stress and cold-cache
qualification remain separate from this focused acceptance.
