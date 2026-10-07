----------------------- MODULE InvocationInputs ------------------------
EXTENDS Naturals, FiniteSets
CONSTANT Bug
ASSUME Bug \in {"none","uncovered-input","early-scope-exit","missing-result","return-gap","lost-code"}
VARIABLES stage, heap, runtimeRoots, generatedRoots, localRoots, callerRoots,
          linksInput, nested, result, currentCode, returned, changed
vars == <<stage,heap,runtimeRoots,generatedRoots,localRoots,callerRoots,
          linksInput,nested,result,currentCode,returned,changed>>
(* 1 receiver/caller holder, 2 input, 3 fresh result, 4 entered code, 5 replacement. *)
Init == /\ stage = "body" /\ heap = {1,2,4,5}
        /\ runtimeRoots = (IF Bug = "uncovered-input" THEN {1,4} ELSE {1,2,4})
        /\ generatedRoots = {} /\ localRoots = {} /\ callerRoots = {1}
        /\ linksInput = TRUE /\ nested = FALSE /\ result = 2
        /\ currentCode = 4 /\ returned = FALSE /\ changed = FALSE
Roots == runtimeRoots \cup generatedRoots \cup localRoots \cup callerRoots \cup {currentCode}
Reach == Roots \cup (IF 1 \in Roots /\ linksInput THEN {2} ELSE {})
Needed == IF stage = "done" THEN {result}
          ELSE IF stage \in {"cleanup","handoff"} THEN {result,4}
          ELSE {1,2,4}
Collect == /\ heap' = heap \cap Reach
           /\ UNCHANGED <<stage,runtimeRoots,generatedRoots,localRoots,callerRoots,
                          linksInput,nested,result,currentCode,returned,changed>>
Callback == /\ stage = "body" /\ linksInput /\ linksInput' = FALSE
            /\ runtimeRoots' = (IF Bug = "early-scope-exit" THEN {1,4} ELSE runtimeRoots)
            /\ UNCHANGED <<stage,heap,generatedRoots,localRoots,callerRoots,nested,
                           result,currentCode,returned,changed>>
EnterNested == /\ stage = "body" /\ ~nested /\ nested' = TRUE
               /\ UNCHANGED <<stage,heap,runtimeRoots,generatedRoots,localRoots,
                              callerRoots,linksInput,result,currentCode,returned,changed>>
ExitNested == /\ stage = "body" /\ nested /\ nested' = FALSE
              /\ UNCHANGED <<stage,heap,runtimeRoots,generatedRoots,localRoots,
                             callerRoots,linksInput,result,currentCode,returned,changed>>
Replace == /\ stage = "body" /\ ~changed /\ changed' = TRUE /\ currentCode' = 5
           /\ runtimeRoots' = (IF Bug = "lost-code" THEN runtimeRoots \ {4} ELSE runtimeRoots)
           /\ UNCHANGED <<stage,heap,generatedRoots,localRoots,callerRoots,linksInput,
                          nested,result,returned>>
Fresh == /\ stage = "body" /\ ~nested /\ result = 2 /\ result' = 3 /\ heap' = heap \cup {3}
         /\ localRoots' = (IF Bug = "missing-result" THEN {} ELSE {3})
         /\ stage' = "cleanup"
         /\ UNCHANGED <<runtimeRoots,generatedRoots,callerRoots,linksInput,nested,
                        currentCode,returned,changed>>
FinishInput == /\ stage = "body" /\ ~nested /\ stage' = "cleanup"
               /\ UNCHANGED <<heap,runtimeRoots,generatedRoots,localRoots,callerRoots,
                              linksInput,nested,result,currentCode,returned,changed>>
Cleanup == /\ stage = "cleanup" /\ stage' = "handoff"
           /\ localRoots' = (IF Bug = "return-gap" THEN {} ELSE localRoots)
           /\ UNCHANGED <<heap,runtimeRoots,generatedRoots,callerRoots,linksInput,
                          nested,result,currentCode,returned,changed>>
(* No collection gap during transfer: keep the result until caller protection.
   This is a premise about the existing result path, not input elision itself. *)
Transfer == /\ stage = "handoff" /\ stage' = "done" /\ returned' = TRUE
            /\ callerRoots' = callerRoots \cup {result}
            /\ runtimeRoots' = {} /\ generatedRoots' = {} /\ localRoots' = {}
            /\ UNCHANGED <<heap,linksInput,nested,result,currentCode,changed>>
Next == Collect \/ Callback \/ EnterNested \/ ExitNested \/ Replace \/ Fresh \/ FinishInput \/ Cleanup \/ Transfer
Spec == Init /\ [][Next]_vars
NeededAlive == Needed \subseteq heap
NoLeakedInvocation == stage = "done" => runtimeRoots = {} /\ generatedRoots = {} /\ localRoots = {}
=============================================================================
