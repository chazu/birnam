# Remaining sends in Integer addition

Analysis date: 2026-10-08. Runtime changes committed as `0e791ffd`.
Successful public `Integer +` uses **513 Id sends** in the latest warmed trace.
The enclosing benchmark adds 1.375 sends/addition of amortized bookkeeping,
giving the previously reported 514.375. Sixteen measured `+` activations each
contain exactly 513 sends. Error, overflow and mixed Float paths differ.

## Disjoint causal breakdown

Each send is counted once. Root/name/validation sends nested under a constructor,
capture operation or conversion are charged to that cause. These are message
counts, not CPU shares or a theoretical minimum.

| Cause | Sends | Share | Function |
| --- | ---: | ---: | --- |
| Branch Block construction | 124 | 24.2% | Eight closures, their private vtables/value methods, allocation and immediate protection |
| Capture loading | 94 | 18.3% | 62 capture reads plus sixteen mode queries and sixteen atomic binding checks |
| Numeric carrier conversion | 82 | 16.0% | Read two Integer payloads into boxed Words, then construct the Integer result |
| Other lifetime protection and cleanup | 76 | 14.8% | Private retains/releases and five guarded cleanup batches outside the other categories |
| Word arithmetic and its guards | 62 | 12.1% | Wrapping addition, fused overflow expression, lookup/permission/type checks and result allocation |
| Error/type and binding lookup | 32 | 6.2% | Nil/type tests, current ErrorVt lookup, reflection and operation selection |
| Policy and helper dispatch | 25 | 4.9% | Enter arithmetic helpers, choose branches and invoke selected handlers |
| Boolean data selection and remaining naming | 18 | 3.5% | Ten identity selections and eight naming sends not charged above |
| **Total actual addition** | **513** | **100%** | |

## Why the largest categories exist

The authored Integer policy has four conditional decisions on this successful
path: whether the left operand is supported, whether the right operand is an
exact Error, whether it is an Integer, and whether the arithmetic overflows.
The current conditional API takes zero-argument thunks. Both branches are
constructed before selecting one, so these four decisions create eight Blocks.
Each gets a private vtable and `value` method; four Blocks are never invoked.
Those unused constructions account for **62 of the 124 sends**. Across all eight,
the category contains 96 constructor-subtree sends, sixteen factory-lookup sends
and twelve immediate retain sends. Construction also creates sixteen of the
twenty physical Objects: eight Blocks and eight vtables.

Capture loading happens across sixteen entered authored bodies, not just the
outer `+` method. Each body fetches the lexical environment used by its helper.
The default reader is one Id send per captured reference. Its two-message
admission verifies that borrowed results are immutable receiver edges rather
than fresh replacement results. Reusable handlers with explicit parameters could
remove some of these environments; this is not a claim that all 94 sends can go.

The conversion category has two optional-Word reads (54 sends combined), one
Word-to-Integer result conversion (25) and three immediate result retains.
Most work inside those calls is boxing, representation validation and protection,
not the fixed adapter entry. Addition still allocates three Word boxes and one
Integer result. Eliminating boxes is a separate lowering/API investigation: the
current authored bodies and Word methods consume Objects.

The actual addition has 81 individual retain and 47 individual release messages,
spread across the above categories. A five-value batch brings logical acquisitions
to 86; other releases occur in batches. The full trace reports approximately
18 registrations/addition overlapping a live token in another activation, and
zero same-activation token duplicates. This overlap count excludes permanent
owner roots and Execution's native invocation scopes. It is evidence for auditing
native input protection, not permission to remove eighteen registrations: covering
scope duration and fresh-result acquisition still need proof.

Word's fused expression caches recognized meanings under the global behavior
epoch. Creating private Block vtables and installing their methods changes that
epoch, so unchanged Word meanings are checked again. The trace contains three
`usesDefaultLookup:` and three `bind:to:` calls inside the fused expression.
These six sends are already inside the 62-send arithmetic category. The causal
interpretation follows `expression.rs` and the observed construction sequence;
no separate timing experiment establishes its CPU share.

## Next targets

**Highest-leverage structural target: simplify Integer's authored branch API to
reuse parameterized handlers.** Select an existing callable and pass its data
through current Id messages instead of allocating two captured zero-argument
Blocks for every decision. Investigate this as a library/source redesign before
general compiler closure virtualization. The 124 construction sends are the
current cost surface, not a promised saving after replacement dispatch costs.
Fewer helper closures may also reduce capture loading, temporary protection and
unrelated Word-cache invalidations; those effects overlap and must be measured.

Keep signed/overflow/Float policy in Birnam. Preserve left/right validation and
Error precedence, noncanonical Boolean behavior, current calls to retained
arithmetic/getter/factory methods, identity/shared Cells, and result/failure order.
Removed private branch constructors and their old helper interception history
need not be recreated. Changing retained operations requires ordinary dispatch or
the existing current-meaning proof/fallback rules. Lean should relate the old and
new decision trees and effect order; TLA+ should cover branch selection, callbacks,
collection and failure. The independent Integer oracle and changed-provider tests
remain implementation checks.

**Next small lifetime audit: native Word/Integer receiver and argument roots.**
Their wrappers still acquire tokens for inputs while the Id ABI requires callers
to keep those inputs alive through the call. Reuse the invocation-coverage proof
per owner and verify all native callers, including cold bootstrap and reentry.
Freshly allocated outputs before callback/type validation are different: keep
their first protection. Measure the result independently from source branching.

Later candidates are reducing conservative Word-cache invalidation and virtual
Word intermediates. Those require dependency/recovery analysis; the small fixed
adapter itself is no longer the dominant conversion cost.

## Evidence

The [breakdown receipt](../measurements/addition-send-breakdown-20261008.json)
identifies the original hashed census, the filtered `+` subtrees and the exact
causal partition. It reuses the validated runtime receipt and runs no benchmark.
Filtering changes analysis IDs only; the original trace remains intact. Aggregate
root statistics come from the original complete trace and are labeled accordingly.
