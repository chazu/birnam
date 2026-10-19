------------------------- MODULE GlobalResolution --------------------
EXTENDS Naturals, FiniteSets, Sequences
CONSTANT Bug
ASSUME Bug \in {"none","nil-root","globals-root","new-nil","early-exit","getter-order"}
VARIABLES phase, heap, scope, nilNow, globalsNow, found, returned, seen
vars == <<phase,heap,scope,nilNow,globalsNow,found,returned,seen>>
Init == /\ phase = "nil" /\ heap = {1,2,3,4,6,7} /\ scope = {}
        /\ nilNow = 1 /\ globalsNow \in {0,2} /\ found = 0 /\ returned = 0 /\ seen = <<>>
(* 1 fetched nil, 2 fetched Globals, 3 export, 4 global hit, 6/7 rebound bindings. *)
Roots == {nilNow,globalsNow,3,4,6,7} \cup scope \cup (IF returned # 0 THEN {returned} ELSE {})
Needed == CASE phase = "global" -> {1,2}
            [] phase = "export" -> {1}
            [] phase = "done" -> {returned}
            [] OTHER -> {}
Nil == /\ phase = "nil" /\ phase' = "globals" /\ seen' = <<"nil">>
       /\ scope' = IF Bug = "nil-root" THEN {} ELSE {1}
       /\ UNCHANGED <<heap,nilNow,globalsNow,found,returned>>
Globals == /\ phase = "globals" /\ phase' = (IF globalsNow = 0 THEN "export" ELSE "global") /\ seen' = Append(seen,"globals")
           /\ scope' = IF Bug = "globals-root" \/ globalsNow = 0 THEN scope ELSE scope \cup {2}
           /\ UNCHANGED <<heap,nilNow,globalsNow,found,returned>>
Rebind == /\ phase = "global" /\ nilNow' = 6 /\ globalsNow' = 7
          /\ UNCHANGED <<phase,heap,scope,found,returned,seen>>
Read == /\ phase = "global" /\ found' \in {1,4}
        /\ phase' = IF found' = (IF Bug = "new-nil" THEN nilNow ELSE 1) THEN "export" ELSE "done"
        /\ returned' = IF phase' = "done" THEN found' ELSE 0
        /\ seen' = Append(seen,"value")
        /\ scope' = IF phase' = "done" THEN {} ELSE scope
        /\ UNCHANGED <<heap,nilNow,globalsNow>>
Export == /\ phase = "export" /\ phase' = "done" /\ returned' \in {1,3}
          /\ seen' = IF Bug = "getter-order" THEN <<"nil","globals","export","value">> ELSE Append(seen,"export")
          /\ scope' = {} /\ UNCHANGED <<heap,nilNow,globalsNow,found>>
EarlyExit == /\ Bug = "early-exit" /\ phase = "export" /\ scope' = {}
             /\ UNCHANGED <<phase,heap,nilNow,globalsNow,found,returned,seen>>
Collect == /\ heap' = heap \cap Roots
           /\ UNCHANGED <<phase,scope,nilNow,globalsNow,found,returned,seen>>
Next == Nil \/ Globals \/ Rebind \/ Read \/ Export \/ EarlyExit \/ Collect
Spec == Init /\ [][Next]_vars
NeededAlive == Needed \subseteq heap
GetterOrder == phase = "done" => seen = (IF found = 0 THEN <<"nil","globals","export">> ELSE IF found = 1 THEN <<"nil","globals","value","export">> ELSE <<"nil","globals","value">>)
======================================================================
