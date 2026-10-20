------------------------- MODULE AlienRecipes -------------------------
EXTENDS Naturals
CONSTANT Bug
ASSUME Bug \in {"none", "stale", "nested", "retain", "stamp", "cache"}
VARIABLES phase, current, registration, held, oldCodeAlive, saved, destination, stamp, again, cache, savedCache
vars == <<phase,current,registration,held,oldCodeAlive,saved,destination,stamp,again,cache,savedCache>>
(* 0 = absent, 1 = outer wrapper, 2 = nested wrapper, 3 = authored replacement. *)
Init == /\ phase = "callback" /\ current = 1 /\ registration = 1
        /\ held \in BOOLEAN /\ oldCodeAlive = TRUE /\ saved = 0 /\ destination = 0 /\ stamp = 0 /\ again = 0
        /\ cache \in {"original", "edited"} /\ savedCache = cache
Callback == /\ phase = "callback" /\ phase' = "record"
            /\ current' \in {0,1,2,3}
            /\ registration' = IF current' = 2 THEN 2 ELSE registration
            /\ UNCHANGED <<held,oldCodeAlive,saved,destination,stamp,again,cache,savedCache>>
Record == /\ phase = "record" /\ phase' = "collect"
          /\ registration' = IF current = 1 \/ Bug = "nested" THEN 1 ELSE registration
          /\ UNCHANGED <<current,held,oldCodeAlive,saved,destination,stamp,again,cache,savedCache>>
Collect == /\ phase = "collect" /\ phase' = "snapshot"
           /\ oldCodeAlive' = (current = 1 \/ held \/ Bug = "retain")
           /\ UNCHANGED <<current,registration,held,saved,destination,stamp,again,cache,savedCache>>
Snapshot == /\ phase = "snapshot" /\ phase' = "restore"
            /\ saved' = IF Bug = "stale" \/ registration = current THEN registration ELSE 0
            /\ savedCache' = cache
            /\ UNCHANGED <<current,registration,held,oldCodeAlive,destination,stamp,again,cache>>
Restore == /\ phase = "restore" /\ phase' = "resnapshot"
           /\ destination' = IF saved = 0 THEN 0 ELSE saved + 10
           /\ stamp' = IF Bug = "stamp" THEN saved ELSE destination'
           /\ cache' = IF Bug = "cache" THEN "original" ELSE cache
           /\ UNCHANGED <<current,registration,held,oldCodeAlive,saved,again,savedCache>>
Resnapshot == /\ phase = "resnapshot" /\ phase' = "done"
              /\ again' = IF destination = stamp THEN saved ELSE 0
              /\ UNCHANGED <<current,registration,held,oldCodeAlive,saved,destination,stamp,cache,savedCache>>
Next == Callback \/ Record \/ Collect \/ Snapshot \/ Restore \/ Resnapshot
Spec == Init /\ [][Next]_vars
NoRetiredCodeRoot == phase \in {"snapshot","restore","resnapshot","done"} /\ current # 1 /\ ~held => ~oldCodeAlive
HeldCodeLives == held => oldCodeAlive
CurrentRecipeOnly == phase \in {"restore","resnapshot","done"} => saved = IF current \in {1,2} THEN current ELSE 0
CachePreserved == phase \in {"resnapshot","done"} => cache = savedCache
RoundTrip == phase = "done" => again = saved
=======================================================================
