//! Test-source GC must preserve native callers of the probe.
//!
//! Snapshot the native interval before creating probe scratch state. Collection
//! scans that outer interval and precise Id roots, excluding deeper probe frames.
//! In particular, a synchronous evaluator's Unit and native locals are valid
//! roots even when the generated Frames under test must prove their own roots.
use super::*;

#[derive(Clone, Copy)]
struct Range {
    runtime: *const Runtime,
    start: usize,
    bottom: usize,
}

thread_local! {
    static OUTER: Cell<Option<Range>> = const { Cell::new(None) };
}

struct Restore(Option<Range>);

impl Drop for Restore {
    fn drop(&mut self) {
        OUTER.set(self.0);
    }
}

// Keep probe scratch in a deeper physical frame in optimized builds too.
// Otherwise inlining can place new Oop locals above the saved root interval.
#[inline(never)]
fn run_body<T>(runtime: &Runtime, body: impl FnOnce(&Runtime) -> T) -> T {
    body(runtime)
}

pub(super) fn with_source_probe_native_roots<T>(body: impl FnOnce(&Runtime) -> T) -> T {
    with_active(|runtime| {
        if OUTER
            .get()
            .is_some_and(|range| core::ptr::eq(range.runtime, runtime))
        {
            // A replacement method or control callback belongs to the original
            // probe extent. Do not admit its temporary values as native roots.
            return body(runtime);
        }
        let bottom = unsafe { (*runtime.state.get()).metadata.stack_bottom() };
        with_native_root_range(bottom, |start| {
            let _restore = Restore(OUTER.replace(Some(Range {
                runtime,
                start,
                bottom,
            })));
            run_body(runtime, body)
        })
        .unwrap_or_else(|| {
            boundary::fatal_invariant("source probe requires a native caller root interval")
        })
    })
}

impl Runtime {
    pub(super) fn gc_collect_for_source_probe(&self) -> usize {
        let range = OUTER
            .get()
            .filter(|range| core::ptr::eq(range.runtime, self))
            .unwrap_or_else(|| {
                boundary::fatal_invariant("source probe collection outside its native root extent")
            });
        self.gc_collect_from_native_range(range.start, range.bottom)
    }
}
