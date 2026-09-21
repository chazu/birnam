//! Diagnostic source lifetimes span separate native calls and REPL evaluations.
//! Observers keep weak host references only; ordinary Id roots own the consumer.
use super::*;
use birnam_native_compiler::{HeapAllocationTotals, Unit};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Weak;

#[derive(Default)]
pub(super) struct Owners(Vec<(&'static str, Box<dyn Fn() -> bool>)>);
impl Owners {
    pub(super) fn rc<T: 'static>(&mut self, name: &'static str, value: &Rc<T>) {
        let weak = Rc::downgrade(value);
        self.0
            .push((name, Box::new(move || weak.strong_count() != 0)));
    }
    pub(super) fn arc<T: 'static>(&mut self, name: &'static str, weak: Weak<T>) {
        self.0
            .push((name, Box::new(move || weak.strong_count() != 0)));
    }
    fn live(&self) -> usize {
        self.0.iter().filter(|(_, live)| live()).count()
    }
    fn assert_released(&self) {
        for (name, live) in &self.0 {
            assert!(!live(), "retained consumer owner: {name}");
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Storage {
    native: (usize, usize),
    heap: HeapAllocationTotals,
}
impl Storage {
    fn read() -> Self {
        Self {
            native: Unit::resource_totals(),
            heap: Unit::compiler_heap_totals(),
        }
    }
    fn print(self, phase: &str, generation: u64, watched: usize, live: usize) {
        println!(
            "consumer-lifetime phase={phase} generation={generation} watched={watched} live={live} native_units={} relocated_bytes={} heap_blocks={} heap_bytes={}",
            self.native.0, self.native.1, self.heap.blocks, self.heap.requested_bytes
        );
    }
}
struct Watch {
    generation: u64,
    owners: Owners,
}
thread_local! {
    static BASELINE: Cell<Option<Storage>> = const { Cell::new(None) };
    static WATCH: RefCell<Option<Watch>> = const { RefCell::new(None) };
}

pub(super) fn install(r: &Runtime, vt: Oop) {
    for (name, method) in [
        (b"lifetimeBaseline".as_slice(), baseline as Method),
        (b"lifetimePrepare:captures:".as_slice(), prepare as Method),
        (b"lifetimeWarm:input:escape:".as_slice(), warm as Method),
        (b"lifetimeEscaped:input:".as_slice(), escaped as Method),
        (b"lifetimeHeld".as_slice(), held as Method),
        (b"lifetimeReleased".as_slice(), released as Method),
    ] {
        r.add_method(vt, r.intern(name).unwrap(), method);
    }
}

unsafe extern "C" fn baseline(_: Oop, _: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 0);
    with_source_probe_native_roots(|r| {
        assert!(WATCH.with_borrow(Option::is_none));
        let storage = Storage::read();
        BASELINE.set(Some(storage));
        storage.print("baseline", 0, 0, 0);
        r.canonical().nil
    })
}

unsafe extern "C" fn prepare(_: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 2);
    with_source_probe_native_roots(|r| {
        assert!(WATCH.with_borrow(Option::is_none));
        assert!(BASELINE.get().is_some());
        let block = r.send_slice(
            r.import(b"Codegen").unwrap(),
            r.intern(b"materializeResumable:captures:").unwrap(),
            unsafe { core::slice::from_raw_parts(args, count) },
        );
        let _block = ScopedOopRootGuard::new(r, block);
        let StacklessTarget::Registered { entry } = r
            .resolve_stackless_target(block, r.intern(b"value:").unwrap())
            .unwrap()
        else {
            panic!("lifetime target must be a registered source factory");
        };
        let target = r
            .materialize_registered_target(entry, block, &[r.canonical().nil])
            .unwrap();
        let generation = target.descriptor.identity().generation;
        unsafe { (*r.state.get()).execution.release_acquired_entry(entry) }.unwrap();
        drop(target);
        let mut owners = Owners::default();
        r.compiler().native_tier
            .borrow()
            .watch_generation(generation, &mut owners);
        // At least the actual descriptor and cold candidate input are observed.
        assert!(owners.live() >= 2);
        Storage::read().print("prepared", generation, owners.0.len(), owners.live());
        WATCH.replace(Some(Watch { generation, owners }));
        block
    })
}

unsafe extern "C" fn warm(_: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 3);
    let (block, input, escape) = unsafe { (*args, *args.add(1), *args.add(2)) };
    with_source_probe_native_roots(|r| {
        let escaping = r.integer_value(escape) != 0;
        let _block = ScopedOopRootGuard::new(r, block);
        let _input = ScopedOopRootGuard::new(r, input);
        let invoke = || {
            let answer = r.send_slice(block, r.intern(b"value:").unwrap(), &[input]);
            if escaping {
                assert_eq!(
                    unsafe { (*r.state.get()).heap.read_payload_word(answer, 1) },
                    Some(input.addr())
                );
            } else {
                assert_eq!(answer, input);
            }
            answer
        };
        for _ in 0..128 {
            invoke();
        }
        r.gc_collect_for_source_probe();
        let before = native_tier::closure_elision::counters();
        let completed_before = native_tier::closure_elision::completed_calls();
        let kernels_before = numeric::ENTRY_PREDICTION_KERNELS.get();
        let counted_before = unsafe { (*r.state.get()).heap.counted_frame_index_count() };
        let allocated_before = unsafe { (*r.state.get()).heap.bytes_since_collection() };
        let mut answer = r.canonical().nil;
        for _ in 0..8 {
            answer = invoke();
        }
        let after = native_tier::closure_elision::counters();
        let completed = native_tier::closure_elision::completed_calls() - completed_before;
        println!(
            "consumer-lifetime-activity hits={} materialized={} completed={completed} escaping={}",
            after.1 - before.1,
            after.2 - before.2,
            usize::from(escaping)
        );
        println!(
            "consumer-lifetime-numeric kernels={} counted_indices={} allocation_bytes={}",
            numeric::ENTRY_PREDICTION_KERNELS.get() - kernels_before,
            unsafe { (*r.state.get()).heap.counted_frame_index_count() } - counted_before,
            unsafe { (*r.state.get()).heap.bytes_since_collection() } - allocated_before
        );
        WATCH.with_borrow_mut(|watch| {
            let watch = watch.as_mut().unwrap();
            r.compiler().native_tier
                .borrow()
                .watch_generation(watch.generation, &mut watch.owners);
            Storage::read().print(
                "warm",
                watch.generation,
                watch.owners.0.len(),
                watch.owners.live(),
            );
        });
        answer
    })
}

unsafe extern "C" fn held(_: Oop, _: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 0);
    with_source_probe_native_roots(|r| {
        WATCH.with_borrow(|watch| {
            let watch = watch.as_ref().unwrap();
            assert!(unsafe {
                (*r.state.get())
                    .execution
                    .has_code_generation(watch.generation)
            });
            assert!(watch.owners.live() > 0);
            Storage::read().print(
                "held",
                watch.generation,
                watch.owners.0.len(),
                watch.owners.live(),
            );
        });
        r.canonical().nil
    })
}

unsafe extern "C" fn escaped(_: Oop, args: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 2);
    with_source_probe_native_roots(|r| {
        let (closure, input) = unsafe { (*args, *args.add(1)) };
        assert_eq!(
            r.send_slice(closure, r.intern(b"value").unwrap(), &[]),
            input
        );
        WATCH.with_borrow(|watch| {
            let watch = watch.as_ref().unwrap();
            assert!(unsafe {
                (*r.state.get())
                    .execution
                    .has_code_generation(watch.generation)
            });
            Storage::read().print(
                "escaped",
                watch.generation,
                watch.owners.0.len(),
                watch.owners.live(),
            );
        });
        r.canonical().nil
    })
}

unsafe extern "C" fn released(_: Oop, _: *const Oop, count: usize) -> Oop {
    assert_eq!(count, 0);
    with_source_probe_native_roots(|r| {
        WATCH.with_borrow(|watch| {
            let watch = watch.as_ref().unwrap();
            let storage = Storage::read();
            storage.print(
                "released",
                watch.generation,
                watch.owners.0.len(),
                watch.owners.live(),
            );
            watch.owners.assert_released();
            assert!(!unsafe {
                (*r.state.get())
                    .execution
                    .has_code_generation(watch.generation)
            });
            assert_eq!(
                storage,
                BASELINE.get().unwrap(),
                "complete consumer compiler storage release"
            );
        });
        WATCH.replace(None);
        r.canonical().nil
    })
}
