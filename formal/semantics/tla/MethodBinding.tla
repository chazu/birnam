--------------------------- MODULE MethodBinding ---------------------------
EXTENDS Naturals, FiniteSets
CONSTANT Bug
ASSUME Bug \in {"none", "registration-alias", "template-only-root", "owner-keeps-bindings"}
VARIABLES stage, live, registrations, codeAlive, collected
vars == <<stage, live, registrations, codeAlive, collected>>
Key(m) == IF Bug = "registration-alias" THEN 0 ELSE m
Init == /\ stage = 0 /\ live = {0} /\ registrations = {0}
        /\ codeAlive = TRUE /\ collected = {}
Bind == /\ stage < 2 /\ stage' = stage + 1
        /\ live' = live \cup {stage + 1}
        /\ registrations' = registrations \cup {Key(stage + 1)}
        /\ UNCHANGED <<codeAlive, collected>>
Drop(m) == /\ stage = 2 /\ m \in live
           /\ live' = live \ {m}
           /\ registrations' = registrations \ {Key(m)}
           /\ collected' = IF Bug = "owner-keeps-bindings" THEN collected ELSE collected \cup {m}
           /\ UNCHANGED <<stage, codeAlive>>
Collect == /\ stage = 2
           /\ codeAlive' = IF Bug = "template-only-root" THEN 0 \in live ELSE live # {}
           /\ UNCHANGED <<stage, live, registrations, collected>>
Next == Bind \/ (\E m \in {0, 1, 2}: Drop(m)) \/ Collect
Spec == Init /\ [][Next]_vars
SurvivorsCallable == \A m \in live: Key(m) \in registrations
SurvivorsRetainCode == live # {} => codeAlive
DroppedBindingsCollect == stage = 2 => {0, 1, 2} \ live \subseteq collected
=============================================================================
