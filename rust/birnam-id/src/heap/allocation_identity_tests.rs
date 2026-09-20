//! The same witness runs against the original scalar and extracted storage.
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
static FINALIZED: AtomicUsize = AtomicUsize::new(0);
unsafe extern "C" fn finalize(_: Oop) { FINALIZED.fetch_add(1,Ordering::SeqCst); }
fn snapshot(heap: &Heap, label: &str) {
    let entries=heap.storage_allocations();let mut previous=0;
    let rows:Vec<_>=entries.iter().map(|entry| {
        assert!(entry.identity>previous);previous=entry.identity;
        let block=&heap.blocks[&entry.object.addr()];
        let total=(8+entry.requested_size+7)&!7;
        assert_eq!(entry.allocation_bytes,total);assert_eq!(entry.padded_size,total-8);
        assert_eq!(block.metadata>>5,entry.identity);
        assert_eq!((block.metadata>>2)&7,(total-8-entry.requested_size) as u64);
        assert_eq!(block.permanence()==Permanence::Permanent,entry.flags & birnam_abi::storage::PERMANENT !=0);
        assert_eq!(heap.has_finalizer(entry.object),Some(entry.flags & birnam_abi::storage::HAS_FINALIZER !=0));
        assert_eq!(heap.base_address(entry.object).unwrap()%16,0);
        assert_eq!(heap.payload_address(entry.object).unwrap()-heap.base_address(entry.object).unwrap(),8);
        (entry.identity,entry.requested_size,entry.padded_size,entry.flags,block.marked())
    }).collect();
    assert_eq!(heap.allocated_bytes(),entries.iter().map(|entry| entry.allocation_bytes).sum::<usize>());
    println!("allocation-identity {label}: {rows:?}, charged {}",heap.bytes_since_collection());
}
#[test]
fn exact_geometry_marks_finalizers_detachment_and_independent_owners() {
    assert_eq!(size_of::<Block>(),32);
    let mut owners=vec![Heap::new(),Heap::new()];let mut heap=owners.remove(0);
    assert_eq!(heap.bytes_since_collection(),0);assert_eq!(heap.allocation_count(),0);
    snapshot(&heap,"empty moved owner");
    let mut objects=Vec::new();let mut charged=0;
    for requested in 0..16 {
        let permanence=if requested%2==0 {Permanence::Permanent} else {Permanence::Collectable};
        let object=heap.allocate(requested,permanence);objects.push(object);charged+=(8+requested+7)&!7;
        let block=&heap.blocks[&object.addr()];assert_eq!(block.identity(),requested as u64+1);
        assert_eq!(block.requested_size(),requested);assert!(!block.marked());
        assert!(unsafe { core::slice::from_raw_parts(object.cast::<u8>(),heap.payload_size(object).unwrap()) }.iter().all(|byte| *byte==0));
        assert_eq!(heap.bytes_since_collection(),charged);
    }
    snapshot(&heap,"all eight padding values and both permanence kinds");
    assert!(heap.set_vtable(objects[1],objects[0]));assert!(heap.set_finalizer(objects[1],finalize));
    assert!(!heap.set_finalizer(objects[0],finalize));
    assert!(heap.mark_collectable_address(objects[1].addr()));assert!(!heap.mark_collectable_address(objects[1].addr()));
    assert!(!heap.mark_collectable_address(objects[0].addr()));
    assert!(heap.detach_dead_collectable(objects[0].addr()).is_none());assert!(heap.detach_dead_collectable(objects[1].addr()).is_none());
    snapshot(&heap,"header finalizer and marking keep identities");
    heap.clear_marks();let detached=heap.detach_dead_collectable(objects[1].addr()).unwrap();
    assert_eq!(detached.block.identity(),2);assert_eq!(detached.object(),objects[1]);
    assert!(!heap.contains(objects[1]));assert_eq!(heap.bytes_since_collection(),charged);
    unsafe { detached.finalizer().unwrap()(detached.object()) };assert_eq!(FINALIZED.load(Ordering::SeqCst),1);detached.reclaim();
    snapshot(&heap,"detach and finalization leave sequence and charges alone");
    let next=heap.allocate(1,Permanence::Collectable);assert_eq!(heap.blocks[&next.addr()].identity(),17);
    snapshot(&heap,"allocation after reclaim");
    heap.reset_allocation_bytes();snapshot(&heap,"budget reset does not reset identities");
    let next=heap.allocate(9,Permanence::Collectable);assert_eq!(heap.blocks[&next.addr()].identity(),18);assert_eq!(heap.bytes_since_collection(),24);
    snapshot(&heap,"allocation after budget reset");
    assert_eq!(owners[0].allocation_count(),0);drop(heap);assert_eq!(FINALIZED.load(Ordering::SeqCst),1);
    let next=owners[0].allocate(0,Permanence::Collectable);assert_eq!(owners[0].blocks[&next.addr()].identity(),1);
    snapshot(&owners[0],"surviving independent Heap begins at one");
}
#[test]
fn reused_addresses_never_reuse_identity() {
    let mut heap=Heap::new();let mut seen=std::collections::HashMap::new();let mut reused=false;
    for identity in 1..=256 {
        let object=heap.allocate(56,Permanence::Collectable);assert_eq!(heap.blocks[&object.addr()].identity(),identity);
        if let Some(previous)=seen.insert(object.addr(),identity) { assert!(identity>previous);reused=true; }
        assert!(heap.reclaim_for_test(object));
    }
    assert!(reused,"the witness must exercise actual allocator address reuse");
    assert_eq!(heap.allocation_count(),0);assert_eq!(heap.bytes_since_collection(),256*64);
    println!("allocation-identity 256 allocate/reclaim cycles: observed address reuse with strictly new identities");
}
#[test]
fn last_valid_identity_preserves_metadata() {
    let mut heap=Heap::new();heap.set_allocation_identity_for_test((u64::MAX>>5)-1);
    let object=heap.allocate(3,Permanence::Permanent);
    assert_eq!(heap.blocks[&object.addr()].identity(),u64::MAX>>5);
    assert_eq!(heap.blocks[&object.addr()].metadata,((u64::MAX>>5)<<5)|(5<<2)|2);
    snapshot(&heap,"last valid identity");
}
#[test]
fn exhaustion_aborts_at_both_existing_checks() {
    use std::os::unix::process::ExitStatusExt;
    if let Ok(case)=std::env::var("BIRNAM_81BY_EXHAUSTION") {
        let mut heap=Heap::new();heap.set_allocation_identity_for_test(if case=="bound" {u64::MAX>>5} else {u64::MAX});
        heap.allocate(0,Permanence::Collectable);panic!("exhaustion returned");
    }
    for case in ["bound","overflow"] {
        let result=std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact","heap::allocation_identity_tests::exhaustion_aborts_at_both_existing_checks","--nocapture"])
            .env("BIRNAM_81BY_EXHAUSTION",case).output().unwrap();
        assert_eq!(result.status.signal(),Some(6));
        assert_eq!(String::from_utf8(result.stderr).unwrap(),"birnam-id invariant failed: allocation identity exhausted\n");
        println!("allocation-identity exhaustion {case}: SIGABRT, exact allocation identity exhausted diagnostic");
    }
}
