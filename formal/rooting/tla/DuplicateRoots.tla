-------------------------- MODULE DuplicateRoots --------------------------
EXTENDS Naturals, FiniteSets, Sequences, TLC

CONSTANTS Objects, Edges, MaxDepth, MaxCopies, MaxEpoch, InitiallyPermitted, Bug
ASSUME /\ Objects # {}
       /\ Edges \subseteq Objects \X Objects
       /\ MaxDepth > 0 /\ MaxCopies > 1 /\ MaxEpoch > 0
       /\ InitiallyPermitted \in BOOLEAN
       /\ Bug \in {"none", "omit-first", "early-release", "ignore-permission",
                   "revoke-without-materializing", "leak-on-exit"}

NonSelfEdges == {e \in Objects \X Objects : e[1] # e[2]}
Scopes == 1..MaxDepth
Keys == Scopes \X Objects
Zero == [k \in Keys |-> 0]
NoModes == [s \in Scopes |-> FALSE]
NoEpochs == [s \in Scopes |-> 0]

VARIABLES canonical, physical, stack, modes, guardEpoch, permitted, epoch,
          pending, canonicalHeap, physicalHeap
vars == <<canonical, physical, stack, modes, guardEpoch, permitted, epoch,
          pending, canonicalHeap, physicalHeap>>

Top == Len(stack)
Active == {s \in Scopes : s <= Top}
RootSet(counts) == {o \in Objects : \E s \in Scopes : counts[<<s,o>>] > 0}
Needed == RootSet(canonical)
RECURSIVE Closure(_, _)
Closure(roots, n) == IF n = 0 THEN roots ELSE
    LET before == Closure(roots, n-1)
    IN before \cup {o \in Objects : \E r \in before : <<r,o>> \in Edges}
Reach(counts) == Closure(RootSet(counts), Cardinality(Objects))
Minimum(n) == IF n = 0 THEN 0 ELSE 1
Expected(k) == IF modes[k[1]] THEN Minimum(canonical[k]) ELSE canonical[k]

Init == /\ canonical = Zero /\ physical = Zero /\ stack = <<1>>
        /\ modes = NoModes /\ guardEpoch = NoEpochs
        /\ permitted = InitiallyPermitted /\ epoch = 0 /\ pending = FALSE
        /\ canonicalHeap = Objects /\ physicalHeap = Objects

(* Acquire is an owner permission witness, not a compiler assumption. It grants
   a lease: revocation cannot expose token counts until materialization finishes.
   The stale-epoch check is repeated at each optimized keep. *)
Acquire == /\ ~pending /\ ~modes[Top]
           /\ \A o \in Objects : canonical[<<Top,o>>] = 0
           /\ permitted \/ Bug = "ignore-permission"
           /\ modes' = [modes EXCEPT ![Top] = TRUE]
           /\ guardEpoch' = [guardEpoch EXCEPT ![Top] = epoch]
           /\ UNCHANGED <<canonical, physical, stack, permitted, epoch, pending,
                          canonicalHeap, physicalHeap>>

Keep(o) == LET k == <<Top,o>>
               omit == modes[Top] /\ guardEpoch[Top] = epoch /\
                       (permitted \/ Bug = "ignore-permission") /\ physical[k] > 0
           IN /\ ~pending /\ o \in canonicalHeap /\ o \in physicalHeap
              /\ canonical[k] < MaxCopies
              /\ canonical' = [canonical EXCEPT ![k] = @ + 1]
              /\ physical' = IF omit \/ Bug = "omit-first" THEN physical
                              ELSE [physical EXCEPT ![k] = @ + 1]
              /\ UNCHANGED <<stack, modes, guardEpoch, permitted, epoch, pending,
                             canonicalHeap, physicalHeap>>

(* A generated Call holds all its tokens until exit. Nested activation is also
   callback reentry; every scope owns its own protection, never borrows blindly.
   Logical values can become dead before exit, but this narrow model keeps them
   live conservatively until that scope exits. *)
Enter == /\ ~pending /\ Top < MaxDepth
         /\ stack' = Append(stack, Top+1)
         /\ UNCHANGED <<canonical, physical, modes, guardEpoch, permitted, epoch,
                        pending, canonicalHeap, physicalHeap>>
Exit == /\ ~pending /\ Top > 1
        /\ canonical' = [k \in Keys |-> IF k[1] = Top THEN 0 ELSE canonical[k]]
        /\ physical' = IF Bug = "leak-on-exit" THEN physical ELSE
                          [k \in Keys |-> IF k[1] = Top THEN 0 ELSE physical[k]]
        /\ modes' = [modes EXCEPT ![Top] = FALSE]
        /\ guardEpoch' = [guardEpoch EXCEPT ![Top] = 0]
        /\ stack' = SubSeq(stack, 1, Top-1)
        /\ UNCHANGED <<permitted, epoch, pending, canonicalHeap, physicalHeap>>

(* Same transition covers normal return and exception unwinding. The producer
   keeps protection until a returned/escaped value is retained by its caller.
   Handoff explicitly roots the value in the parent before child Exit. *)
Handoff(o) == LET k == <<Top-1,o>>
                 omit == modes[Top-1] /\ permitted /\ guardEpoch[Top-1] = epoch
                         /\ physical[k] > 0
             IN /\ ~pending /\ Top > 1 /\ canonical[<<Top,o>>] > 0
                /\ canonical[k] < MaxCopies
                /\ canonical' = [canonical EXCEPT ![k] = @+1]
                /\ physical' = IF omit THEN physical
                                ELSE [physical EXCEPT ![k] = @+1]
                /\ UNCHANGED <<stack, modes, guardEpoch, permitted, epoch, pending,
                               canonicalHeap, physicalHeap>>

Collect == /\ canonicalHeap' = canonicalHeap \cap Reach(canonical)
           /\ physicalHeap' = physicalHeap \cap Reach(physical)
           /\ UNCHANGED <<canonical, physical, stack, modes, guardEpoch,
                          permitted, epoch, pending>>

(* Deoptimization is incremental, with collection allowed between steps.
   Lease remains valid until all omitted logical registrations are restored.
   Restoration is owner bookkeeping under the lease, not replay of arbitrary
   foreign retain callbacks. New application actions wait for this barrier. *)
RequestRevoke == /\ ~pending /\ permitted /\ epoch < MaxEpoch
                 /\ pending' = TRUE
                 /\ UNCHANGED <<canonical, physical, stack, modes, guardEpoch,
                                permitted, epoch, canonicalHeap, physicalHeap>>
Materialize(k) == /\ pending /\ physical[k] < canonical[k]
                  /\ physical' = [physical EXCEPT ![k] = @+1]
                  /\ UNCHANGED <<canonical, stack, modes, guardEpoch, permitted,
                                 epoch, pending, canonicalHeap, physicalHeap>>
CommitRevoke == /\ pending /\ physical = canonical
                /\ permitted' = FALSE /\ pending' = FALSE /\ epoch' = epoch+1
                /\ modes' = NoModes
                /\ UNCHANGED <<canonical, physical, stack, guardEpoch,
                               canonicalHeap, physicalHeap>>
Grant == /\ ~pending /\ ~permitted /\ epoch < MaxEpoch
         /\ permitted' = TRUE /\ epoch' = epoch+1
         /\ UNCHANGED <<canonical, physical, stack, modes, guardEpoch, pending,
                        canonicalHeap, physicalHeap>>

(* These are deliberately incorrect algorithms, not environmental freedoms. *)
EarlyRelease(k) == /\ Bug = "early-release" /\ physical[k] > 0
                   /\ physical' = [physical EXCEPT ![k] = 0]
                   /\ UNCHANGED <<canonical, stack, modes, guardEpoch, permitted,
                                  epoch, pending, canonicalHeap, physicalHeap>>
BadRevoke == /\ Bug = "revoke-without-materializing"
             /\ permitted /\ ~pending /\ epoch < MaxEpoch
             /\ permitted' = FALSE /\ epoch' = epoch+1
             /\ UNCHANGED <<canonical, physical, stack, modes, guardEpoch, pending,
                            canonicalHeap, physicalHeap>>
Next == Acquire \/ (\E o \in Objects : Keep(o) \/ Handoff(o)) \/ Enter \/ Exit
        \/ Collect \/ RequestRevoke \/ CommitRevoke \/ Grant \/ BadRevoke
        \/ (\E k \in Keys : Materialize(k) \/ EarlyRelease(k))
CanonicalNext == (\E o \in Objects : Keep(o) \/ Handoff(o)) \/ Enter \/ Exit \/ Collect
CanonicalSpec == Init /\ [][CanonicalNext]_vars
Spec == Init /\ [][Next]_vars
DeoptNext == (\E k \in Keys : Materialize(k)) \/ CommitRevoke
FairSpec == Spec /\ WF_vars(DeoptNext)
RecoveryCompletes == pending ~> ~pending

TypeOK == /\ canonical \in [Keys -> 0..MaxCopies]
          /\ physical \in [Keys -> 0..MaxCopies]
          /\ stack = [s \in 1..Top |-> s]
          /\ Top \in Scopes /\ modes \in [Scopes -> BOOLEAN]
          /\ guardEpoch \in [Scopes -> 0..MaxEpoch]
          /\ permitted \in BOOLEAN /\ pending \in BOOLEAN /\ epoch \in 0..MaxEpoch
          /\ canonicalHeap \subseteq Objects /\ physicalHeap \subseteq Objects
RootCoverage == Needed \subseteq Reach(physical)
NeededAlive == Needed \subseteq physicalHeap
ReachabilityEquality == Reach(canonical) = Reach(physical)
HeapEquality == canonicalHeap = physicalHeap
Representation == \A k \in Keys :
    IF pending THEN Expected(k) <= physical[k] /\ physical[k] <= canonical[k]
    ELSE physical[k] = Expected(k)
PermissionValid == \A s \in Active : modes[s] => permitted /\ guardEpoch[s] = epoch
(* Token multiplicity is observable unless the owner lease says otherwise.
   Token ids and swap-remove visitation order are intentionally not abstracted
   into an equivalence for unpermitted calls: those take the ordinary path. *)
ObservableEquality == ~permitted => physical = canonical
InactiveEmpty == \A k \in Keys : k[1] \notin Active =>
                   canonical[k] = 0 /\ physical[k] = 0
=============================================================================
