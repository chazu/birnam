----------------------- MODULE ReusableBranches -----------------------
EXTENDS Naturals, FiniteSets, Sequences
CONSTANT Bug
ASSUME Bug \in {"none","run-both","stale-entry","drop-arguments","replay"}
VARIABLES phase, selected, versions, protected, heap, called, effects, failed
vars == <<phase,selected,versions,protected,heap,called,effects,failed>>
(* Three reusable handlers 1/2/3 and changing argument 4. Inputs are protected
   through selection and actual invocation; default selection invokes nothing. *)
Init == /\ phase = "select" /\ selected = 0 /\ versions = [h \in 1..3 |-> 0]
        /\ protected = {1,2,3,4} /\ heap = {1,2,3,4}
        /\ called = {} /\ effects = <<>> /\ failed = FALSE
Select == /\ phase = "select" /\ selected' \in 1..3 /\ phase' = "invoke"
          /\ protected' = IF Bug = "drop-arguments" THEN protected \ {4} ELSE protected
          /\ UNCHANGED <<versions,heap,called,effects,failed>>
Replace == /\ phase = "invoke" /\ selected # 0 /\ versions[selected] = 0
           /\ versions' = [versions EXCEPT ![selected] = 1]
           /\ UNCHANGED <<phase,selected,protected,heap,called,effects,failed>>
Invoke == /\ phase = "invoke" /\ phase' = "done"
          /\ LET used == IF Bug = "stale-entry" THEN 0 ELSE versions[selected]
             IN effects' = Append(effects, <<selected, used, versions[selected]>>)
          /\ called' = IF Bug = "run-both" THEN {1,2,3} ELSE {selected}
          /\ UNCHANGED <<selected,versions,protected,heap,failed>>
Fail == /\ phase = "invoke" /\ phase' = "done" /\ failed' = TRUE
        /\ UNCHANGED <<selected,versions,protected,heap,called,effects>>
Replay == /\ Bug = "replay" /\ phase = "done" /\ Len(effects) = 1
          /\ effects' = Append(effects, Head(effects))
          /\ UNCHANGED <<phase,selected,versions,protected,heap,called,failed>>
Collect == /\ heap' = heap \cap protected
           /\ UNCHANGED <<phase,selected,versions,protected,called,effects,failed>>
Next == Select \/ Replace \/ Invoke \/ Fail \/ Replay \/ Collect
Spec == Init /\ [][Next]_vars
NeededAlive == phase # "done" => {1,2,3,4} \subseteq heap
OnlyChosen == called \subseteq {selected}
CurrentEntry == \A i \in 1..Len(effects): effects[i][2] = effects[i][3]
NoReplay == Len(effects) <= 1 /\ (failed => Len(effects) = 0)
=============================================================================
