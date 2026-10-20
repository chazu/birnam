------------------------- MODULE ImageRecipeOrder -------------------------
EXTENDS Naturals
CONSTANT Bug
ASSUME Bug \in {"none", "lookup", "maps-first", "index-early"}
VARIABLES phase, changedHash, needsForeign, ownerFound, foreignReady, mapsReady, indexed, hashed
vars == <<phase,changedHash,needsForeign,ownerFound,foreignReady,mapsReady,indexed,hashed>>
Init == /\ phase = "discover" /\ changedHash \in BOOLEAN /\ needsForeign \in BOOLEAN
        /\ ownerFound = FALSE /\ foreignReady = FALSE /\ mapsReady = FALSE
        /\ indexed = FALSE /\ hashed = FALSE
Discover == /\ phase = "discover" /\ phase' = "restore"
            /\ ownerFound' = ~(Bug = "lookup" /\ changedHash)
            /\ UNCHANGED <<changedHash,needsForeign,foreignReady,mapsReady,indexed,hashed>>
EarlyMaps == /\ phase = "restore" /\ Bug = "maps-first" /\ phase' = "repair"
             /\ hashed' = TRUE /\ mapsReady' = TRUE
             /\ UNCHANGED <<changedHash,needsForeign,ownerFound,foreignReady,indexed>>
Restore == /\ phase = "restore" /\ Bug # "maps-first" /\ phase' = "repair"
           /\ foreignReady' = ownerFound
           /\ indexed' = (Bug = "index-early")
           /\ UNCHANGED <<changedHash,needsForeign,ownerFound,mapsReady,hashed>>
Repair == /\ phase = "repair" /\ phase' = "index"
          /\ mapsReady' = TRUE /\ hashed' = TRUE
          /\ UNCHANGED <<changedHash,needsForeign,ownerFound,foreignReady,indexed>>
Index == /\ phase = "index" /\ phase' = "done" /\ indexed' = TRUE
         /\ UNCHANGED <<changedHash,needsForeign,ownerFound,foreignReady,mapsReady,hashed>>
Next == Discover \/ EarlyMaps \/ Restore \/ Repair \/ Index
Spec == Init /\ [][Next]_vars
OwnerDiscovered == phase # "discover" => ownerFound
HashDependenciesReady == hashed /\ needsForeign => foreignReady
IndexAfterRepair == indexed => mapsReady
=======================================================================
