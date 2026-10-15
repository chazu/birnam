------------------------- MODULE GlobalsOwner -------------------------
EXTENDS Naturals
CONSTANT Bug
ASSUME Bug \in {"none", "lost", "foreign", "credits", "image", "construction", "aba"}
VARIABLES phase, value, foreign, revision, bank, snapshot, foreignWritten,
          committed, saved, epoch, preparedEpoch, preparedRevision
vars == <<phase,value,foreign,revision,bank,snapshot,foreignWritten,
          committed,saved,epoch,preparedEpoch,preparedRevision>>
Init == /\ phase = "ready" /\ value = "absent" /\ foreign = 0 /\ revision = 0
        /\ bank = 2 /\ snapshot = 0 /\ foreignWritten = FALSE /\ committed = FALSE
        /\ saved = "absent" /\ epoch = 0 /\ preparedEpoch = 0 /\ preparedRevision = 0
Read == /\ phase = "ready" /\ phase' = "prepared" /\ snapshot' = revision
        /\ preparedEpoch' = epoch /\ preparedRevision' = revision
        /\ UNCHANGED <<value,foreign,revision,bank,foreignWritten,committed,saved,epoch>>
(* A collecting/reentrant send may update an unrelated binding between read and CAS. *)
Foreign == /\ phase = "prepared" /\ ~foreignWritten
           /\ foreign' = 1 /\ foreignWritten' = TRUE /\ revision' = revision + 1
           /\ UNCHANGED <<phase,value,bank,snapshot,committed,saved,epoch,preparedEpoch,preparedRevision>>
Publish == /\ phase = "prepared" /\ phase' = "published"
           /\ committed' = (Bug = "lost" \/ snapshot = revision)
           /\ value' = IF committed' THEN "nil" ELSE value
           /\ revision' = IF committed' THEN revision + 1 ELSE revision
           /\ foreign' = IF Bug = "lost" /\ committed' THEN 0 ELSE foreign
           /\ bank' = IF committed' THEN bank - 1 ELSE bank
           /\ UNCHANGED <<snapshot,foreignWritten,saved,epoch,preparedEpoch,preparedRevision>>
Inverse == /\ phase = "published" /\ phase' = "inverted"
           /\ value' = IF committed THEN "absent" ELSE value
           /\ foreign' = IF Bug = "foreign" THEN 0 ELSE foreign
           /\ bank' = IF committed /\ Bug # "credits" THEN bank - 1 ELSE bank
           /\ UNCHANGED <<revision,snapshot,foreignWritten,committed,saved,epoch,preparedEpoch,preparedRevision>>
Image == /\ phase = "inverted" /\ phase' = "image"
         /\ saved' = value /\ bank' = IF Bug = "image" THEN bank ELSE 0
         /\ UNCHANGED <<value,foreign,revision,snapshot,foreignWritten,committed,epoch,preparedEpoch,preparedRevision>>
(* Initial methods on a not-yet-observable behavior change no live lookup. *)
Construct == /\ phase = "image" /\ phase' = "constructed"
             /\ preparedEpoch' = epoch
             /\ epoch' = IF Bug = "construction" THEN epoch + 1 ELSE epoch
             /\ UNCHANGED <<value,foreign,revision,bank,snapshot,foreignWritten,committed,saved,preparedRevision>>
ABA == /\ phase = "constructed" /\ phase' = "done"
       /\ epoch' = IF Bug = "aba" THEN epoch ELSE epoch + 2
       /\ UNCHANGED <<value,foreign,revision,bank,snapshot,foreignWritten,committed,saved,preparedEpoch,preparedRevision>>
Next == Read \/ Foreign \/ Publish \/ Inverse \/ Image \/ Construct \/ ABA
Spec == Init /\ [][Next]_vars
ForeignSurvives == foreignWritten => foreign = 1
OwnInverse == phase \in {"inverted","image","constructed","done"} => value = "absent"
CreditsConsumed == phase = "inverted" /\ committed => bank = 0
ImageHasNoBank == phase = "image" => bank = 0
FreshDoesNotInvalidate == phase = "constructed" => epoch = preparedEpoch
ExistingABAInvalidates == phase = "done" => epoch > preparedEpoch
=======================================================================
