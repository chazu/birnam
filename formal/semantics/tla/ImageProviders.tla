------------------------- MODULE ImageProviders -------------------------
EXTENDS Naturals, Sequences, FiniteSets
CONSTANT Bug
ASSUME Bug \in {"none", "ambient", "late", "missing"}
VARIABLES phase, declared, available, active, position, calls, restored
vars == <<phase,declared,available,active,position,calls,restored>>
Names == {"store","relational"}
Required == {declared[i] : i \in DOMAIN declared}
Init == /\ phase = "activate" /\ declared \in {<<>>,<<"store">>,<<"store","relational">>}
        /\ available \in SUBSET Names /\ active = {} /\ position = 1
        /\ calls = 0 /\ restored = FALSE
Activate == /\ phase = "activate" /\ position <= Len(declared)
            /\ calls' = calls + 1
            /\ IF declared[position] \in available \/ Bug = "missing"
                  THEN /\ active' = active \cup {declared[position]}
                       /\ position' = position + 1 /\ phase' = phase
                  ELSE /\ phase' = "rejected" /\ UNCHANGED <<active,position>>
            /\ UNCHANGED <<declared,available,restored>>
Ready == /\ phase = "activate" /\ (position > Len(declared) \/ Bug = "late")
         /\ phase' = "heap" /\ UNCHANGED <<declared,available,active,position,calls,restored>>
Ambient == /\ phase = "activate" /\ declared = <<>> /\ Bug = "ambient"
           /\ calls' = 1 /\ active' = {"store"} /\ phase' = "heap"
           /\ UNCHANGED <<declared,available,position,restored>>
Restore == /\ phase = "heap" /\ phase' = "done" /\ restored' = TRUE
           /\ UNCHANGED <<declared,available,active,position,calls>>
Next == Activate \/ Ready \/ Ambient \/ Restore
Spec == Init /\ [][Next]_vars
NoAmbientActivation == calls <= Len(declared)
ProvidersBeforeHeap == phase \in {"heap","done"} => Required \subseteq active
UnavailableRefused == active \subseteq available
=======================================================================
