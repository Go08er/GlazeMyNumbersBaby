//! Processor time: what a thread has run, not what the clock on the wall
//! says. The media thread's quarter of a core is measured in it, so a
//! machine busy with other things doesn't make an animation rest longer.

use std::time::Duration;

/// The processor time the calling thread has used so far; `None` where
/// the system can't tell.
pub fn thread_time() -> Option<Duration> {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `clock_gettime` writes the one `timespec` it is given, which
    // lives on this stack; it keeps no pointer.
    let ok = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut ts) } == 0;
    (ok && ts.tv_sec >= 0 && ts.tv_nsec >= 0)
        .then(|| Duration::new(ts.tv_sec as u64, ts.tv_nsec as u32))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_threads_time_grows_with_its_work_and_not_with_its_sleep() {
        let Some(start) = thread_time() else {
            return;
        };
        std::thread::sleep(Duration::from_millis(30));
        let slept = thread_time().unwrap() - start;
        assert!(slept < Duration::from_millis(20), "{slept:?}");
        let mut x = 1u64;
        while thread_time().unwrap() - start < Duration::from_millis(30) {
            for i in 0..10_000u64 {
                x = x.wrapping_mul(6364136223846793005).wrapping_add(i);
            }
        }
        assert!(std::hint::black_box(x) != 0);
    }
}
