# Custom lookup breaks tail-call reuse in the Oop migration

Status: reproduced regression; repair unresolved. Remaining migration that depends
on the current dispatch/Execution boundary is held. This is not a qualified runtime
checkpoint. Id's public contract is frozen by the user's instruction and has not
been changed.

## Reproduction

Run `bash scripts/oop-id-slice-test.sh`. The expanded native source test now fails
a required assertion. The same compiled function is used for both lookup modes:

```birnam
[TailProbe tailTick]; [TailProbe tailRecurse]
```

`tailTick` counts calls, observes the actual Machine's `liveFrameCount`, and replaces
`tailRecurse` with a terminating native Method on the requested final iteration.
The custom lookup delegates to the same original lookup and also collects. Both
modes return the same Integer and execute exactly the requested number of ticks.

| Lookup | Calls | Peak live frames |
| --- | ---: | ---: |
| Default | 8 | 2 |
| Default | 24 | 2 |
| Custom, forwarding | 8 | 9 |
| Custom, forwarding | 24 | 25 |

The two expected frames are the Machine's run adapter and the reused source frame.
The custom case grows with recursion depth. This is more than coarser scheduling
or an unmeasured performance cost: proper source tail calls are not preserved.
The failure remains a normal mandatory assertion, not an ignored or expected-pass
test. [Evidence](../measurements/oop-custom-lookup-tail-20261023.json).

## Cause and preserved contract

`resumable_runtime::drive` only performs preliminary binding when lookup is proven
callback-free. Otherwise it enters a synchronous boundary and calls ordinary Id
send. When lookup returns a registered source Method, its entry calls
`start_registered`, which begins a nested drive and pushes a fresh frame. It cannot
replace the original tail frame while that synchronous invocation is waiting.

The older preserved contract explicitly requires custom lookup of a live
registered method to remain resumable, independently of how it was found:
[Design 60E, resolution](60e-resumable-registration-and-entry.md) and
[Design 60F, section 15.3](60f-resumable-compiler-lowering.md#153-calls-stack-depth-and-registration).
The old retained `machine.rs` implementation classifies the selected entry after
custom lookup and takes its ordinary registered tail path. Its carrier-dependent
implementation is not an acceptable compatibility path for this migration.

The earlier native-fallback correction fixed a different invariant violation:
lookup followed by `invokeOn:` lookup on the selected Method. The present fallback
avoids that violation and avoids replaying custom lookup, but loses general
stackless/tail behavior. It cannot be treated as full functional acceptance.

## Why a local workaround is not acceptable

- Adding an Id entry point, selector, Protocol field or side channel violates the
  current instruction to keep the Id contract fixed.
- Reflective `invokeOn:` on every selected Method restores the already rejected
  implicit application protocol.
- Binding with custom lookup and then doing an ordinary send can execute custom
  lookup twice, changing effects and possibly selecting a different Method.
- Reading Id's private Method payloads from Execution, retaining a service entry,
  or adding a parallel dispatcher violates the ownership/dispatch requirements.
- Treating a registered source call as a synchronous native call is the measured
  failure; accepting it would weaken the preserved tail-call/control contract.

## Ownership decision to resolve

Current method selection and native entry are private to Id/bootstrap Method
implementation; frame classification/scheduling live in a separate Execution
implementation. The full path needs to select once, keep the selected code alive,
and either transfer to its registered frame or enter its native implementation.
That work must not become another public Id protocol or foreign-layout read.

The candidate direction is to consolidate the private ownership of Method entry
and registered execution, while keeping minimal Id, payload opacity and the public
Id message contract unchanged. This is an ownership proposal, not an implemented
or approved design. Its physical linkage/bootstrap arrangement must demonstrate
that it does not merely hide a new inter-owner native service behind another name
or pull upper-language policy into minimal Id.

No such implementation is claimed here. That ownership question needs resolution
before extending suspension, continuations, recovery or dependent compiler/host
ports. The project instructions explicitly require the hold:

> An open invariant violation blocks further migration that depends on that mechanism.

The failing test and this diagnosis may be committed under those same instructions.
No installation, merge or completion push is warranted by this checkpoint.
