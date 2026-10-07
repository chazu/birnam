--------------------------- MODULE BatchCleanup ---------------------------
EXTENDS Naturals, Sequences
CONSTANTS MaxTokens, RequestCount, MaxEpoch, Bug
ASSUME /\ MaxTokens > 1 /\ RequestCount > 1 /\ MaxEpoch > 0
       /\ Bug \in {"none", "stale-guard", "wrong-order"}
Tokens == 1..MaxTokens
InitialTokens == [i \in Tokens |-> i]
Reverse(xs) == [i \in 1..Len(xs) |-> xs[Len(xs)-i+1]]
Remove(xs, token) ==
    IF token \notin {xs[i] : i \in 1..Len(xs)} THEN xs ELSE
    LET i == CHOOSE i \in 1..Len(xs) : xs[i] = token
    IN [j \in 1..(Len(xs)-1) |-> IF j = i THEN xs[Len(xs)] ELSE xs[j]]
RECURSIVE Run(_, _)
Run(xs, requests) == IF Len(requests) = 0 THEN xs
                    ELSE Run(Remove(xs, Head(requests)), Tail(requests))

VARIABLES canonical, physical, canonicalEffects, physicalEffects,
          requests, remaining, native, epoch, witness, stage
vars == <<canonical, physical, canonicalEffects, physicalEffects,
          requests, remaining, native, epoch, witness, stage>>
Init == /\ canonical = InitialTokens /\ physical = InitialTokens
        /\ requests \in [1..RequestCount -> 0..MaxTokens]
        /\ remaining = <<>> /\ canonicalEffects = <<>> /\ physicalEffects = <<>>
        /\ native \in BOOLEAN /\ epoch = 0 /\ witness = 0 /\ stage = "probe"
Probe == /\ stage = "probe" /\ witness' = epoch /\ stage' = "checked"
         /\ UNCHANGED <<canonical, physical, canonicalEffects, physicalEffects,
                        requests, remaining, native, epoch>>
Replace == /\ stage \in {"probe", "checked", "fallback"} /\ epoch < MaxEpoch
           /\ native' = ~native /\ epoch' = epoch+1
           /\ UNCHANGED <<canonical, physical, canonicalEffects, physicalEffects,
                          requests, remaining, witness, stage>>
Eligible == native /\ witness = epoch
TakeBatch == /\ stage = "checked" /\ (Eligible \/ Bug = "stale-guard")
             /\ canonical' = Run(canonical, Reverse(requests))
             /\ physical' = Run(physical, IF Bug = "wrong-order" THEN requests ELSE Reverse(requests))
             /\ canonicalEffects' = IF native THEN canonicalEffects ELSE
                                      canonicalEffects \o Reverse(requests)
             /\ UNCHANGED physicalEffects
             /\ stage' = "done"
             /\ UNCHANGED <<requests, remaining, native, epoch, witness>>
BeginFallback == /\ stage = "checked" /\ ~Eligible /\ Bug # "stale-guard"
                 /\ stage' = "fallback" /\ remaining' = Reverse(requests)
                 /\ UNCHANGED <<canonical, physical, canonicalEffects, physicalEffects,
                                requests, native, epoch, witness>>
FallbackOne == /\ stage = "fallback" /\ Len(remaining) > 0
               /\ canonical' = Remove(canonical, Head(remaining))
               /\ physical' = Remove(physical, Head(remaining))
               /\ canonicalEffects' = IF native THEN canonicalEffects ELSE
                                        Append(canonicalEffects, Head(remaining))
               /\ physicalEffects' = IF native THEN physicalEffects ELSE
                                       Append(physicalEffects, Head(remaining))
               /\ remaining' = Tail(remaining)
               /\ stage' = IF Len(remaining) = 1 THEN "done" ELSE "fallback"
               /\ UNCHANGED <<requests, native, epoch, witness>>
Next == Probe \/ Replace \/ TakeBatch \/ BeginFallback \/ FallbackOne
Spec == Init /\ [][Next]_vars
ExactRetainedOrder == canonical = physical
ExactCustomCallbacks == canonicalEffects = physicalEffects
(* The default owner release is callback-free. An owner-thread batch has no
   intermediate observer points; when that witness is invalid, ordinary live
   sends are performed one by one, including changes between those sends. *)
=============================================================================
