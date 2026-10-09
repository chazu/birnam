----------------------- MODULE NativeOwnerScopes ---------------------
EXTENDS Naturals, FiniteSets, Sequences
CONSTANT Bug
ASSUME Bug \in {"none","unreported","early-pop","drop-parent","leak-exit","return-gap"}
VARIABLES phase, heap, scopes, caller
vars == <<phase,heap,scopes,caller>>
Init == /\ phase = "begin" /\ heap = {1} /\ scopes = <<>> /\ caller = {}
Inventory == UNION {scopes[i] : i \in 1..Len(scopes)}
Reported == IF Bug = "unreported" THEN {} ELSE Inventory
Roots == {1} \cup Reported \cup caller
Needed == CASE phase = "child" -> {1,2,3}
            [] phase \in {"outer","after-child","gap","caller"} -> {1,2}
            [] OTHER -> {1}
Begin == /\ phase = "begin" /\ phase' = "outer" /\ heap' = heap \cup {2}
         /\ scopes' = <<{2}>> /\ UNCHANGED caller
Nested == /\ phase = "outer" /\ phase' = "child" /\ heap' = heap \cup {3}
          /\ scopes' = Append(scopes,{3}) /\ UNCHANGED caller
ChildExit == /\ phase = "child" /\ phase' = "after-child"
             /\ scopes' = IF Bug = "drop-parent" THEN <<>> ELSE SubSeq(scopes,1,Len(scopes)-1)
             /\ UNCHANGED <<heap,caller>>
EarlyPop == /\ Bug = "early-pop" /\ phase = "after-child" /\ scopes' = <<>>
            /\ UNCHANGED <<phase,heap,caller>>
(* The native handoff and root-free final-send path have no callback between
   returning a fresh reference and its caller accepting/protecting it. A caller
   with pending cleanup cannot use that compiler path; it keeps the old path. *)
Return == /\ phase = "after-child"
          /\ phase' = IF Bug = "return-gap" THEN "gap" ELSE "caller"
          /\ scopes' = IF Bug = "leak-exit" THEN scopes ELSE <<>>
          /\ caller' = IF Bug = "return-gap" THEN {} ELSE {2}
          /\ UNCHANGED heap
Handoff == /\ phase = "gap" /\ phase' = "caller" /\ caller' = {2}
           /\ UNCHANGED <<heap,scopes>>
CallerExit == /\ phase = "caller" /\ phase' = "done" /\ caller' = {}
              /\ UNCHANGED <<heap,scopes>>
Collect == /\ heap' = heap \cap Roots /\ UNCHANGED <<phase,scopes,caller>>
Next == Begin \/ Nested \/ ChildExit \/ EarlyPop \/ Return \/ Handoff \/ CallerExit \/ Collect
Spec == Init /\ [][Next]_vars
NeededAlive == Needed \subseteq heap
NoLeakedScope == phase = "done" => Inventory = {}
======================================================================
