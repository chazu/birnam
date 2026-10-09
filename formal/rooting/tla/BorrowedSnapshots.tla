------------------------ MODULE BorrowedSnapshots --------------------
EXTENDS Naturals, FiniteSets
CONSTANT Bug
ASSUME Bug \in {"none","snapshot","inputs","early"}
VARIABLES phase, heap, caller, local, edge
vars == <<phase,heap,caller,local,edge>>
Init == /\ phase = "read" /\ heap = {1,2,3,4,5}
        /\ caller = IF Bug = "inputs" THEN {1} ELSE {1,2}
        /\ local = {} /\ edge = 3
(* 1 Map, 2 key, 3 old node, 4 value, 5 new node after rehash. *)
Roots == caller \cup local
Reach == Roots \cup (IF 1 \in Roots THEN {edge} ELSE {})
         \cup (IF 3 \in Roots \/ 5 \in Roots \/ 1 \in Roots THEN {4} ELSE {})
Needed == IF phase = "done" THEN {} ELSE
          IF phase = "read" THEN {1,2} ELSE {1,2,3}
Read == /\ phase = "read" /\ phase' = "callback"
        /\ local' = IF Bug = "snapshot" THEN {} ELSE {3}
        /\ UNCHANGED <<heap,caller,edge>>
Rehash == /\ phase = "callback" /\ phase' = "find" /\ edge' = 5
          /\ UNCHANGED <<heap,caller,local>>
Early == /\ Bug = "early" /\ phase = "find" /\ local' = {}
         /\ UNCHANGED <<phase,heap,caller,edge>>
Collect == /\ heap' = heap \cap Reach /\ UNCHANGED <<phase,caller,local,edge>>
Finish == /\ phase = "find" /\ phase' = "done" /\ local' = {} /\ caller' = {}
          /\ UNCHANGED <<heap,edge>>
Next == Read \/ Rehash \/ Early \/ Collect \/ Finish
Spec == Init /\ [][Next]_vars
NeededAlive == Needed \subseteq heap
======================================================================
