------------------------ MODULE SuppliedTemplates ------------------------
EXTENDS Naturals
CONSTANT Bug
ASSUME Bug \in {"none", "arity", "sync", "replay", "nested"}
VARIABLES phase, outer, nested, calls, synchronousUnits, entered, lookup
vars == <<phase, outer, nested, calls, synchronousUnits, entered, lookup>>
Init == /\ phase = "start" /\ outer = 0 /\ nested = 0 /\ calls = 0
        /\ synchronousUnits = 0 /\ entered = 0 /\ lookup = 1
Select == /\ phase = "start" /\ phase' = "loading" /\ calls' = 1
          /\ UNCHANGED <<outer, nested, synchronousUnits, entered, lookup>>
Nested == /\ phase = "loading" /\ phase' = "returning" /\ nested' = 20
          /\ UNCHANGED <<outer, calls, synchronousUnits, entered, lookup>>
Publish == /\ phase = "returning" /\ phase' = "bound"
           /\ outer' = IF Bug = "arity" THEN nested ELSE 10
           /\ nested' = IF Bug = "nested" THEN 10 ELSE nested
           /\ calls' = IF Bug = "replay" THEN 2 ELSE calls
           /\ entered' = 10
           /\ synchronousUnits' = IF Bug = "sync" THEN 1 ELSE 2
           /\ UNCHANGED lookup
Retire == /\ phase = "bound" /\ phase' = "retired"
          /\ outer' = 0 /\ nested' = 0 /\ lookup' = 2
          /\ UNCHANGED <<calls, synchronousUnits, entered>>
Next == Select \/ Nested \/ Publish \/ Retire
Spec == Init /\ [][Next]_vars
SelectedCodePreserved == phase = "bound" => outer = 10
NestedContextPreserved == phase = "bound" => nested = 20
SynchronousStateIndependent == phase \in {"bound", "retired"} => synchronousUnits = 2
NoInitializerReplay == calls <= 1
EnteredCodeRetained == phase = "retired" => entered = 10 /\ lookup = 2
=============================================================================
