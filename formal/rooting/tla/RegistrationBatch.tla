----------------------- MODULE RegistrationBatch ------------------------
EXTENDS Naturals, Sequences, FiniteSets
CONSTANTS MaxTokens, RequestCount, Bug
ASSUME /\ MaxTokens > 1 /\ RequestCount > 1
       /\ Bug \in {"none", "unrooted-inputs", "partial-decline"}
VARIABLES canonical, physical, canonicalNext, physicalNext, requests, remaining,
          canonicalTokens, physicalTokens, native, stage, heap, callerRoots
vars == <<canonical,physical,canonicalNext,physicalNext,requests,remaining,
          canonicalTokens,physicalTokens,native,stage,heap,callerRoots>>
Required == {requests[i] : i \in 1..RequestCount} \ {0}
Init == /\ requests \in [1..RequestCount -> 0..2]
        /\ canonical = <<>> /\ physical = <<>>
        /\ canonicalNext \in 0..MaxTokens /\ physicalNext = canonicalNext
        /\ canonicalTokens = <<>> /\ physicalTokens = <<>> /\ remaining = requests
        /\ native \in BOOLEAN /\ stage = "proof" /\ heap = {1,2}
        /\ callerRoots = IF Bug = "unrooted-inputs" THEN {} ELSE Required
Proof == /\ stage = "proof" /\ stage' = "ready"
         /\ UNCHANGED <<canonical,physical,canonicalNext,physicalNext,requests,remaining,
                        canonicalTokens,physicalTokens,native,heap,callerRoots>>
Change == /\ stage = "proof" /\ native' = ~native
          /\ UNCHANGED <<canonical,physical,canonicalNext,physicalNext,requests,remaining,
                         canonicalTokens,physicalTokens,stage,heap,callerRoots>>
Collect == /\ heap' = heap \cap (callerRoots \cup {physical[i][2] : i \in 1..Len(physical)})
           /\ UNCHANGED <<canonical,physical,canonicalNext,physicalNext,requests,remaining,
                          canonicalTokens,physicalTokens,native,stage,callerRoots>>
Eligible == native /\ 0 \notin {requests[i] : i \in 1..RequestCount}
            /\ physicalNext+RequestCount <= MaxTokens
Take == /\ stage = "ready" /\ Eligible
        /\ canonical' = [i \in 1..RequestCount |-> <<canonicalNext+i,requests[i]>>]
        /\ physical' = [i \in 1..RequestCount |-> <<physicalNext+i,requests[i]>>]
        /\ canonicalTokens' = [i \in 1..RequestCount |-> canonicalNext+i]
        /\ physicalTokens' = [i \in 1..RequestCount |-> physicalNext+i]
        /\ canonicalNext' = canonicalNext+RequestCount /\ physicalNext' = physicalNext+RequestCount
        /\ stage' = "done"
        /\ UNCHANGED <<requests,remaining,native,heap,callerRoots>>
Decline == /\ stage = "ready" /\ ~Eligible /\ stage' = "fallback"
           /\ physical' = IF Bug = "partial-decline" THEN Append(physical,<<physicalNext+1,1>>) ELSE physical
           /\ UNCHANGED <<canonical,canonicalNext,physicalNext,requests,remaining,
                          canonicalTokens,physicalTokens,native,heap,callerRoots>>
One == /\ stage = "fallback" /\ Len(remaining)>0
       /\ IF Head(remaining)=0 \/ canonicalNext=MaxTokens THEN
              /\ stage' = "failed"
              /\ UNCHANGED <<canonical,physical,canonicalNext,physicalNext,remaining,canonicalTokens,physicalTokens>>
          ELSE
              /\ canonical' = Append(canonical,<<canonicalNext+1,Head(remaining)>>)
              /\ physical' = Append(physical,<<physicalNext+1,Head(remaining)>>)
              /\ canonicalNext' = canonicalNext+1 /\ physicalNext' = physicalNext+1
              /\ canonicalTokens' = Append(canonicalTokens,canonicalNext+1)
              /\ physicalTokens' = Append(physicalTokens,physicalNext+1)
              /\ remaining' = Tail(remaining)
              /\ stage' = IF Len(remaining)=1 THEN "done" ELSE "fallback"
       /\ UNCHANGED <<requests,native,heap,callerRoots>>
Next == Proof \/ Change \/ Collect \/ Take \/ Decline \/ One
Spec == Init /\ [][Next]_vars
ExactRegistrations == canonical = physical /\ canonicalNext = physicalNext /\ canonicalTokens = physicalTokens
InputsAlive == stage \notin {"failed", "done"} => Required \subseteq heap
=============================================================================
