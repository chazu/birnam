//! Injected only into the isolated 74M diagnostic snapshot.
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::Once;
use std::time::Instant;

pub(super) static STAGES: AtomicU64 = AtomicU64::new(0);
pub(super) static ROWS: AtomicU64 = AtomicU64::new(0);
pub(super) static ELIGIBLE: AtomicU64 = AtomicU64::new(0);
pub(super) static REGISTERED: AtomicU64 = AtomicU64::new(0);
pub(super) static SELECTIONS: AtomicU64 = AtomicU64::new(0);
pub(super) static ENTRIES: AtomicU64 = AtomicU64::new(0);
pub(super) static INVALIDATIONS: AtomicU64 = AtomicU64::new(0);
pub(super) static COMPILES: AtomicU64 = AtomicU64::new(0);
pub(super) static FAILURES: AtomicU64 = AtomicU64::new(0);
pub(super) static COMPILE_NS: AtomicU64 = AtomicU64::new(0);
pub(super) static PREPARE_NS: AtomicU64 = AtomicU64::new(0);
pub(super) static SOURCE_BYTES: AtomicU64 = AtomicU64::new(0);
pub(super) static LIVE: AtomicU64 = AtomicU64::new(0);
pub(super) static PEAK: AtomicU64 = AtomicU64::new(0);
static REGISTER_EXIT: Once = Once::new();

unsafe extern "C" {
    fn atexit(callback: unsafe extern "C" fn()) -> i32;
}

pub(super) fn count(counter: &AtomicU64, amount: u64) {
    REGISTER_EXIT.call_once(|| assert_eq!(unsafe { atexit(report) }, 0));
    counter.fetch_add(amount, Relaxed);
}

pub(super) fn created() {
    count(&LIVE, 1);
    PEAK.fetch_max(LIVE.load(Relaxed), Relaxed);
}

pub(super) fn dropped() {
    LIVE.fetch_sub(1, Relaxed);
}

pub(super) struct Timer(pub(super) Instant, pub(super) &'static AtomicU64);
impl Drop for Timer {
    fn drop(&mut self) {
        count(self.1, self.0.elapsed().as_nanos() as u64);
    }
}

unsafe extern "C" fn report() {
    let fields = [
        ("stages", &STAGES), ("rows", &ROWS), ("eligible", &ELIGIBLE),
        ("registered", &REGISTERED), ("selections", &SELECTIONS),
        ("entries", &ENTRIES), ("invalidations", &INVALIDATIONS),
        ("compiles", &COMPILES), ("failures", &FAILURES),
        ("compile_ns", &COMPILE_NS), ("prepare_ns", &PREPARE_NS),
        ("source_bytes", &SOURCE_BYTES), ("live_units", &LIVE), ("peak_units", &PEAK),
    ];
    let values: Vec<_> = fields.iter().map(|(name, value)| format!("\"{name}\":{}", value.load(Relaxed))).collect();
    eprintln!("NATIVE_TIER_PROFILE {{{}}}", values.join(","));
}
