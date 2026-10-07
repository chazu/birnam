//! Thread CPU clock, independent of scheduling delay. No wall-clock fallback.
use std::os::raw::c_long;
#[repr(C)]
struct Timespec {
    seconds: c_long,
    nanos: c_long,
}
unsafe extern "C" {
    fn clock_gettime(clock: i32, result: *mut Timespec) -> i32;
}
#[cfg(target_os = "macos")]
const THREAD_CLOCK: i32 = 16; // Darwin SDK _time.h: CLOCK_THREAD_CPUTIME_ID.
#[cfg(target_os = "linux")]
const THREAD_CLOCK: i32 = 3;
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
compile_error!("microbench requires a supported thread CPU clock");
pub fn cpu_ns() -> u64 {
    let mut t = Timespec {
        seconds: 0,
        nanos: 0,
    };
    assert_eq!(
        unsafe { clock_gettime(THREAD_CLOCK, &raw mut t) },
        0,
        "thread CPU clock unavailable"
    );
    t.seconds as u64 * 1_000_000_000 + t.nanos as u64
}

#[cfg(test)]
mod tests {
    #[test]
    fn thread_clock_excludes_sleep() {
        let wall = std::time::Instant::now();
        let start = super::cpu_ns();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let cpu = super::cpu_ns() - start;
        assert!(
            cpu < (wall.elapsed().as_nanos() / 2) as u64,
            "CPU timer included sleeping time"
        );
    }
}
