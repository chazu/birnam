use super::*;
use core::cell::{Cell, RefCell};

unsafe extern "C" fn first(_: Oop, _: birnam_abi::MarkFn) { panic!("selection invoked first hook"); }
unsafe extern "C" fn second(_: Oop, _: birnam_abi::MarkFn) { panic!("selection invoked second hook"); }
unsafe extern "C" fn third(_: Oop, _: birnam_abi::MarkFn) { panic!("selection invoked third hook"); }
fn oop(index: usize) -> Oop { ptr::without_provenance_mut(index + 1) }
fn address(hook: Option<TraceHook>) -> Option<usize> { hook.map(|hook| hook as usize) }

struct Graph {
    entries: Vec<EntryV1>,
    reads: RefCell<Vec<Oop>>,
}
unsafe extern "C" fn read(context: *const c_void, vtable: Oop, output: *mut EntryV1) -> c_int {
    let graph = unsafe { &*context.cast::<Graph>() };
    graph.reads.borrow_mut().push(vtable);
    let Some(entry) = vtable.addr().checked_sub(1).and_then(|index| graph.entries.get(index)) else { return 0; };
    unsafe { output.write(*entry) };
    1
}
impl Graph {
    fn source(&self, budget: usize) -> SourceV1 {
        SourceV1 { context: ptr::from_ref(self).cast(), budget, read }
    }
}

#[test]
fn all_three_record_graphs_match_independent_shortest_path_oracle() {
    let hooks: [TraceHook; 3] = [first, second, third];
    // Each parent can be any record, null, or an unregistered identity. The
    // independent oracle computes all-pairs shortest paths, not a parent walk.
    for encoding in 0..125usize {
        let parents = [encoding % 5, encoding / 5 % 5, encoding / 25];
        let mut distance = [[99usize; 3]; 3];
        for index in 0..3 {
            distance[index][index] = 0;
            if parents[index] < 3 {
                distance[index][parents[index]] = distance[index][parents[index]].min(1);
            }
        }
        for via in 0..3 { for start in 0..3 { for end in 0..3 {
            distance[start][end] = distance[start][end].min(distance[start][via] + distance[via][end]);
        } } }
        for mask in 0..8 {
            let graph = Graph {
                entries: (0..3).map(|index| EntryV1 {
                    parent: if parents[index] == 3 { ptr::null_mut() } else { oop(parents[index]) },
                    slot: SlotV1 { hook: (mask & (1 << index) != 0).then_some(hooks[index]) },
                }).collect(),
                reads: RefCell::new(Vec::new()),
            };
            for budget in 0..6 { for start in 0..3 {
                let nearest = (0..3).filter(|end| mask & (1 << end) != 0 && distance[start][*end] < budget)
                    .min_by_key(|end| distance[start][*end]);
                let actual = unsafe { resolve(&graph.source(budget), oop(start)) };
                assert_eq!(address(actual), nearest.map(|index| hooks[index] as usize), "parents={parents:?} mask={mask} budget={budget} start={start}");
                graph.reads.borrow_mut().clear();
            } }
        }
    }
}

#[test]
fn exact_reads_keep_budget_missing_membership_and_live_parent_order() {
    let mut graph = Graph { entries: vec![
        EntryV1 { parent: oop(1), slot: SlotV1::EMPTY },
        EntryV1 { parent: oop(0), slot: SlotV1::EMPTY },
        EntryV1 { parent: ptr::null_mut(), slot: unsafe { slot(third) } },
    ], reads: RefCell::new(Vec::new()) };
    assert!(unsafe { resolve(&graph.source(5), oop(0)) }.is_none());
    assert_eq!(*graph.reads.borrow(), [oop(0), oop(1), oop(0), oop(1), oop(0)]);
    graph.reads.borrow_mut().clear();
    assert!(unsafe { resolve(&graph.source(0), oop(0)) }.is_none());
    assert!(unsafe { resolve(&graph.source(5), ptr::null_mut()) }.is_none());
    assert!(graph.reads.borrow().is_empty());
    graph.entries[1].parent = oop(4);
    assert!(unsafe { resolve(&graph.source(5), oop(0)) }.is_none());
    assert_eq!(*graph.reads.borrow(), [oop(0), oop(1), oop(4)]);
    graph.reads.borrow_mut().clear();
    graph.entries[1].parent = oop(2);
    assert_eq!(address(unsafe { resolve(&graph.source(3), oop(0)) }), Some(third as *const () as usize));
    assert_eq!(*graph.reads.borrow(), [oop(0), oop(1), oop(2)]);
    graph.entries[1].slot = unsafe { slot(second) };
    assert_eq!(address(unsafe { resolve(&graph.source(3), oop(0)) }), Some(second as *const () as usize));
    graph.entries[0].slot = unsafe { slot(first) };
    assert_eq!(address(unsafe { resolve(&graph.source(3), oop(0)) }), Some(first as *const () as usize));
}

struct Floor {
    api: Cell<*mut ApiV1>,
    reenter: Cell<bool>,
    events: RefCell<Vec<Option<usize>>>,
}
impl Floor {
    fn new() -> Self { Self { api: Cell::new(ptr::null_mut()), reenter: Cell::new(false), events: RefCell::new(Vec::new()) } }
    fn descriptor(&self) -> FloorV1 {
        FloorV1 { version: 1, byte_size: size_of::<FloorV1>(), context: ptr::from_ref(self).cast_mut().cast(),
            admit: Some(admit), replace: Some(replace) }
    }
}
unsafe extern "C" fn admit(context: *mut c_void) {
    let floor = unsafe { &*context.cast::<Floor>() };
    floor.events.borrow_mut().push(None);
    if floor.reenter.replace(false) {
        unsafe { birnam_abi::trace_hooks::set(floor.api.get(), oop(0), second) };
    }
}
unsafe extern "C" fn replace(context: *mut c_void, vtable: Oop, slot: SlotV1) {
    assert_eq!(vtable, oop(0));
    let floor = unsafe { &*context.cast::<Floor>() };
    floor.events.borrow_mut().push(address(slot.hook));
}

#[test]
fn copied_floor_admission_reentry_and_independent_capabilities() {
    let first_floor = Floor::new();
    let second_floor = Floor::new();
    let mut descriptor = first_floor.descriptor();
    let a = unsafe { create(&descriptor) }; assert!(!a.is_null());
    descriptor.context = second_floor.descriptor().context;
    let b = unsafe { create(&descriptor) }; assert!(!b.is_null() && a != b);
    assert!(first_floor.events.borrow().is_empty() && second_floor.events.borrow().is_empty());
    first_floor.api.set(a); first_floor.reenter.set(true);
    unsafe { birnam_abi::trace_hooks::set(a, oop(0), first) };
    assert_eq!(*first_floor.events.borrow(), [None, None, Some(second as *const () as usize), Some(first as *const () as usize)]);
    assert!(second_floor.events.borrow().is_empty());
    unsafe { (*a).context = ptr::null_mut(); (*a).set = None; }
    let graph = Graph { entries: vec![EntryV1 { parent: ptr::null_mut(), slot: unsafe { FACTORY.slot.unwrap()(third) } }], reads: RefCell::new(Vec::new()) };
    assert_eq!(address(unsafe { FACTORY.resolve.unwrap()(&graph.source(1), oop(0)) }), Some(third as *const () as usize));
    unsafe { destroy(a); birnam_abi::trace_hooks::set(b, oop(0), second); destroy(b); }
    assert_eq!(*second_floor.events.borrow(), [None, Some(second as *const () as usize)]);
}

#[test]
fn factory_rejects_invalid_floor_without_invoking_it() {
    let floor = Floor::new();
    unsafe {
        assert!(create(ptr::null()).is_null());
        let mut bad = floor.descriptor(); bad.version = 2; assert!(create(&bad).is_null());
        bad = floor.descriptor(); bad.byte_size -= 1; assert!(create(&bad).is_null());
        bad = floor.descriptor(); bad.admit = None; assert!(create(&bad).is_null());
        bad = floor.descriptor(); bad.replace = None; assert!(create(&bad).is_null());
    }
    assert!(floor.events.borrow().is_empty());
}
