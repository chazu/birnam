# Custom lookup breaks tail-call reuse in the Oop migration

Status: repaired by the [compiler/Execution continuation integration](oop-continuation-id.md).
The original assertion is unchanged and now passes with 2 frames for both lookup
modes. The diagnosis and observations below describe the earlier failing runtime;
they are retained as regression evidence, not current acceptance status.

## Reproduction

At the recorded failing checkpoint, `bash scripts/oop-id-slice-test.sh` failed
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

These were the restrictions at the failing checkpoint, before the user explicitly
authorized a continuation-aware addition to Id. The current repair uses that
public contract; the prohibitions on replay, foreign payload access and implicit
application dispatch remain in force.

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

## Repair requirement; ownership change is not established

The controlled diagnosis confirms that the driver skips registration recognition
for custom lookup, then re-enters the registered Method through a non-tail nested
drive. Removing lookup-time GC/bookkeeping does not change the failure. A diagnostic
classification control restores frame reuse, showing that the registry and the
retained tail-transfer algorithm work.

The required repair is to preserve the actual selected Method and original
Call/TailCall context through one lookup, while retaining ordinary native dispatch
and the frozen Id contract. The diagnostic control is not safe to retain: its
native fallback can replay custom lookup.

The earlier claim that an ownership decision was necessarily required was
premature. No ownership change or new Id operation has been shown necessary by
this diagnosis. Further dependent migration remains held because the correctness
gate fails, not because a proposed architecture change has been approved or proven
unavoidable.

The project instructions remain applicable:

> An open invariant violation blocks further migration that depends on that mechanism.

The original assertion remains mandatory. The current repair does not imply
completion of the broader language migration or a completion push.
