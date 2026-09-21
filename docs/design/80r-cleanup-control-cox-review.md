# 80R — Cleanup-control review through a Brad Cox-inspired lens

> **Independent adversarial review, 2026-09-16.** Fixed baseline:
> `9973f4d785acfa7ec28bbb00b225b7bb1ec8e92e`.
> Primary input: 80N, with 80/80M for accepted decisions and 80L for the prior
> component-contract review. Every line anchor below refers to that commit.
> This is an interpretation of the earlier Cox-inspired lens, not impersonation
> or endorsement. No new historical claims are made.

## Verdict

**The additive approach is credible. I found no demonstrated requirement to
change IdApi v1, Method/StepMethod calling conventions, descriptor V2, or the
image-v8 grammar.** That is a design assessment, not compatibility qualification.
The new Machine/action protocol still needs its exact schema and implementation.
One concrete admission integration obligation follows from the existing source.

The component boundary is appropriately narrow: ordinary sends name operations;
the runtime checks a transfer's execution facts; library code owns cleanup policy.
Adding an operation does not require extending the C function table. Preserving
that table alone, however, does not prove that retained executable objects are
properly excluded from durable images.

## 1. A new action family does not inherit current image rejection automatically

**Severity: implementation acceptance blocker for the new action/capability,
not a demonstrated ABI or image-format conflict.** This makes 80N's existing
admission obligation concrete; it is not an objection to the proposed semantics.

**Anchors:** [80N](80n-cancellation-to-cleanup-transition.md), lines 63–77,
proposes a new canonical action and requires non-imageable live records.
At the baseline, `rust/birnam-image/src/lib.rs:777–795` explicitly enumerates
the eight current action prototype names to reject their instances.
`rust/birnam-runtime/src/runtime.rs:1179–1186` creates sibling action vtables under
Object, and lines 1379–1404 export their prototypes. There is no common action
ancestor in this construction that automatically classifies a ninth sibling.

**Concrete counterexample:** implementation adds an ExitAction sibling and its
publisher/decoder, leaving image classification unchanged. Application or
inspection code retains a published ExitAction in an otherwise durable object
graph. `image_rejection_kind` finds none of its eight known families. The promised
`stepActionNotImageable` rejection therefore does not follow. Other graph/layout
checks might reject the object for another reason; they are not evidence that
the live capability has been classified correctly. Giving the new vtable an
otherwise admissible exported identity can expose raw control words to ordinary
heap encoding. Either accidental serialization or an unrelated diagnostic misses
the stated execution-object admission contract.

**Smallest remedy:** explicitly add the new action family to the existing
classification, and classify any new opaque boundary-token family similarly.
Keep exported canonical prototypes distinguishable from live instances, as the
current helper already does. Do not classify ordinary inert outcome data as a
live capability merely because cleanup uses it. Demonstrate that retaining the
new live action/token rejects with the intended kind before durable publication,
while ordinary library code, canonical references, and admitted outcomes still
round-trip. This needs an admission implementation change, not a new image record
tag, image version, or native-library certification scheme.

## What supports compatibility, and what remains unproved

| Boundary | Evidence and precise remaining assumption |
| --- | --- |
| IdApi/Method | `rust/birnam-abi/src/lib.rs:97` fixes Method as an Oop-returning C function; `runtime.rs:1405–1484` installs Machine primitives through ordinary method bindings. Another correctly shaped method fits this route without a new table slot. Existing native client/module fixtures must remain unchanged and pass against the implementation. |
| StepMethod/action | 60A lines 86–95 reuses Method's shape. 60B lines 328–382 freezes current payloads and explicitly permits a later canonical vtable/layout version. Define the new payload, accessors, epoch/owner validation, and publisher; preserve old decoding and Abort. This is an execution-protocol addition, not a demonstrated calling-ABI break. |
| Descriptor V2 | 80N requires the target handler's existing result-bearing Call edge. No new serialized PC/edge form is proposed. The boundary-establishment and stopped-Machine entry operations still need a worked implementation proving that requirement. |
| Image-v8 | `birnam-image/src/lib.rs:1062–1063` already encodes exported identities by name. A new canonical identity can use that representation; live records must be excluded as above. Correct bootstrap export/binding reconstruction and mapped/rebuilt image behavior still need evidence. |
| Older runtime | 80N lines 45–47 correctly does not promise execution of new facilities on an old runtime. Unchanged format/ABI is not a feature-availability promise. Keep the existing explicit incompatibility behavior; do not silently fall back to Abort or rewrite an unavailable exit into ordinary return. |

The frozen native fixture README distinguishes the still-required native client
and module ABI evidence from the archived v4 image, which current runtimes
deliberately reject. Do not broaden this proposal into accepting obsolete images
or refreshing frozen fixtures to make qualification pass. Preserve the independent
source bootstrap and current artifact/native-module admission obligations.

The prior service/request lifetime and control-ingress findings are explicitly
settled in 80M. They are not reopened here. User policy replacements, application
statics, worker hot reload, and native-module certification remain outside this
review's requested protection scope.

Reviewed documents and source contracts only. No runtime tests, ABI gates, image
round-trips, input edits, or implementation changes were performed.
