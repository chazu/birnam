------------------------ MODULE SelectorTables ------------------------
EXTENDS Naturals, FiniteSets
CONSTANTS Capacity, Bug
ASSUME /\ Capacity \in {2,4} /\ Bug \in {"none","key","full","world"}
VARIABLES cache, requested, world, phase, answer
vars == <<cache,requested,world,phase,answer>>
(* Zero marks an empty bucket. Each nonzero entry denotes the canonical
   retained Symbol for that name; no method entry is cached. *)
Init == /\ cache \in [1..Capacity -> 0..5] /\ requested \in 1..5
        /\ world = 0 /\ phase = "ready" /\ answer = <<0,0>>
Slot(i) == ((requested+i-1) % Capacity)+1
Stops == {i \in 1..Capacity : cache[Slot(i)] = 0 \/
          cache[Slot(i)] = requested \/ Bug = "key"}
First == CHOOSE i \in Stops : \A j \in Stops : i <= j
Resolve == IF Stops = {} THEN IF Bug = "full" THEN 0 ELSE requested
           ELSE IF cache[Slot(First)] = 0 THEN requested ELSE cache[Slot(First)]
Replace == /\ phase = "ready" /\ world = 0 /\ world' = 1
           /\ UNCHANGED <<cache,requested,phase,answer>>
Send == /\ phase = "ready" /\ phase' = "done"
        /\ answer' = <<Resolve,IF Bug = "world" THEN 0 ELSE world>>
        /\ UNCHANGED <<cache,requested,world>>
Next == Replace \/ Send
Spec == Init /\ [][Next]_vars
CurrentNameAndDispatch == phase = "done" => answer = <<requested,world>>
=======================================================================
