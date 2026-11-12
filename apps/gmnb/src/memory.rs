//! GMNB's memory ceiling: 4 GiB, whatever the theme or the calculation
//! (docs/theming.md, "Limits": the memory of the whole of GMNB).
//!
//! Three layers hold it:
//!
//! 1. **At the source**, a theme's frame may have the renderer hold so much
//!    off-screen and no more, whatever the window's size
//!    (`gmnb_theme::cost`, "what a frame holds off-screen"): the window's
//!    layers share it. A frame past it is refused, as past any cap.
//! 2. **A backstop** ([`Backstop`]): after a frame that painted a theme's
//!    layer or the graph, at most four times a second, GMNB reads its own
//!    memory (`/proc/self/statm`). Past [`BACKSTOP`] the session is saved
//!    and the theme is switched off (the base look, a toast, and Settings
//!    says why). Nothing is read at rest: only a painted frame asks.
//! 3. **The ceiling** ([`set_ceiling`]): the kernel's `RLIMIT_DATA`, what
//!    the process's private writable memory (its heap, its threads' stacks,
//!    the driver's own data) may come to, set first thing in `main` (the
//!    launcher and the Flatpak run it alike). Past it an allocation fails and
//!    GMNB exits (Rust and GLib abort on a failed allocation; the software
//!    renderer leaves out, with a warning, what it can't draw until then):
//!    the settings are saved as they change, and the session (History,
//!    Memory, the equations) when the backstop trips, so a restart has
//!    them.
//!
//! `RLIMIT_AS` would count the address space the GPU drivers reserve and
//! never touch (under the base look, 1.6 GB on NVIDIA's Vulkan, 2.1 GB on
//! GL on llvmpipe). `RLIMIT_DATA` counts private writable memory: 161 to
//! 187 MB with each built-in theme on NVIDIA's Vulkan at 760 × 640 and
//! 3840 × 2160; 515 MB under the base look on llvmpipe (most of it its
//! threads' stacks); 62 MB on the software renderer. NVIDIA's buffers for
//! what is drawn are shared with the card, and not counted: the limit at
//! the source bounds them, and the backstop, which reads what is resident,
//! sees them. Programs GMNB starts (the app for folders) get the limit GMNB
//! was started with back ([`ceiling_not_inherited`]).

use std::sync::atomic::{AtomicU64, Ordering};

/// What the whole of GMNB may come to: 4 GiB.
pub const CEILING: u64 = 4 << 30;

/// Where the backstop switches a theme off: 3.5 GiB, half a gigabyte
/// under the ceiling.
pub const BACKSTOP: u64 = 7 << 29;

/// The soft and hard `RLIMIT_DATA` GMNB was started with, for the programs
/// it starts (`u64::MAX`: none was set before [`set_ceiling`]).
static STARTED_SOFT: AtomicU64 = AtomicU64::new(u64::MAX);
static STARTED_HARD: AtomicU64 = AtomicU64::new(u64::MAX);

/// Sets the ceiling: the soft `RLIMIT_DATA` lowered to [`CEILING`] (one
/// already lower is kept; the hard limit is left alone). A program GMNB
/// starts gets back the limit GMNB was started with. Call it first thing
/// in `main`, before any other thread starts.
pub fn set_ceiling() {
    let mut limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: `limit` is a valid rlimit to write into.
    if unsafe { libc::getrlimit(libc::RLIMIT_DATA, &mut limit) } != 0 {
        return;
    }
    let ceiling = CEILING.min(limit.rlim_max);
    if limit.rlim_cur <= ceiling {
        return;
    }
    STARTED_SOFT.store(limit.rlim_cur, Ordering::Relaxed);
    STARTED_HARD.store(limit.rlim_max, Ordering::Relaxed);
    let lowered = libc::rlimit {
        rlim_cur: ceiling,
        rlim_max: limit.rlim_max,
    };
    // SAFETY: a valid rlimit; lowering the soft limit is always allowed.
    if unsafe { libc::setrlimit(libc::RLIMIT_DATA, &lowered) } != 0 {
        return;
    }
    // SAFETY: the handler only reads two atomics and makes one system call,
    // which is all a forked child of a threaded process may do.
    unsafe { libc::pthread_atfork(None, None, Some(ceiling_not_inherited)) };
}

/// In a child GMNB forks (GLib starting the app for folders, a browser):
/// the limit GMNB was started with, not GMNB's ceiling, which is GMNB's
/// own and could stop a file manager or an editor.
extern "C" fn ceiling_not_inherited() {
    let started = libc::rlimit {
        rlim_cur: STARTED_SOFT.load(Ordering::Relaxed),
        rlim_max: STARTED_HARD.load(Ordering::Relaxed),
    };
    // SAFETY: a valid rlimit, no higher than the hard limit in force.
    unsafe { libc::setrlimit(libc::RLIMIT_DATA, &started) };
}

/// What GMNB holds now, in bytes: the larger of its resident memory and
/// its private writable memory (what the ceiling counts), from
/// `/proc/self/statm`. `None` if it can't be read.
pub fn used() -> Option<u64> {
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let fields: Vec<u64> = statm
        .split_whitespace()
        .map(|f| f.parse().ok())
        .collect::<Option<_>>()?;
    // size resident shared text lib data dt, in pages.
    let (resident, data) = (*fields.get(1)?, *fields.get(5)?);
    // SAFETY: sysconf has no preconditions.
    let page = u64::try_from(unsafe { libc::sysconf(libc::_SC_PAGESIZE) }).ok()?;
    Some(resident.max(data) * page)
}

/// Gives the memory the allocator holds free back to the system: glibc
/// keeps what was freed, the renderer's surfaces among it, for the next
/// allocation (on GL on llvmpipe at 3840 × 2160, a theme switched off at
/// 1.6 GB left 1.3 GB resident; given back, 0.75 GB, the base look's 0.43
/// and what the renderer keeps).
pub fn give_back() {
    #[cfg(target_env = "gnu")]
    // SAFETY: malloc_trim has no preconditions.
    unsafe {
        libc::malloc_trim(0);
    }
}

/// Bytes as the user is told them, in gigabytes of 2^30 bytes, as the
/// ceiling is: "3.6 GB".
pub fn gigabytes(bytes: u64) -> String {
    let gb = bytes as f64 / f64::from(1u32 << 30);
    if gb.fract() == 0.0 {
        format!("{gb:.0} GB")
    } else {
        format!("{gb:.1} GB")
    }
}

/// The backstop's reading: each frame that painted a theme's layer (or
/// the graph) asks [`Backstop::painted`], which says whether it is time to
/// read the memory; [`Backstop::check`] reads it.
pub struct Backstop {
    /// Where it trips: [`BACKSTOP`], or `GMNB_TEST_MEMORY_BACKSTOP` (bytes),
    /// for the tests.
    threshold: u64,
    /// When the memory was last read (the layers' clock, seconds).
    last: std::cell::Cell<f64>,
    /// A read is due after the frame being painted.
    due: std::cell::Cell<bool>,
    /// The last reading was past the threshold (it is acted on once, until
    /// a reading is under it again).
    over: std::cell::Cell<bool>,
}

/// Reads are at least this far apart (seconds): four a second while frames
/// are painted, each a read of `/proc/self/statm` (some 20 µs).
const EVERY: f64 = 0.25;

impl Default for Backstop {
    fn default() -> Self {
        let threshold = std::env::var("GMNB_TEST_MEMORY_BACKSTOP")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(BACKSTOP);
        Backstop {
            threshold,
            last: std::cell::Cell::new(f64::NEG_INFINITY),
            due: std::cell::Cell::new(false),
            over: std::cell::Cell::new(false),
        }
    }
}

impl Backstop {
    /// A frame painted at `now` (seconds) something the backstop watches:
    /// true if a read is to be made once the frame is done (the caller
    /// schedules [`Backstop::check`]); false if one is already due, or one
    /// was made less than a quarter of a second ago.
    pub fn painted(&self, now: f64) -> bool {
        if self.due.get() || now - self.last.get() < EVERY {
            return false;
        }
        self.due.set(true);
        self.last.set(now);
        true
    }

    /// Reads the memory: `Some(bytes)` the first time a reading is past the
    /// threshold (again only once a reading has been under it).
    pub fn check(&self) -> Option<u64> {
        self.due.set(false);
        self.trips(used()?)
    }

    /// A theme was loaded: the next reading past the threshold is acted on
    /// again, even if GMNB never came back under it (what the allocator
    /// keeps mapped after a theme is switched off still counts).
    pub fn rearm(&self) {
        self.over.set(false);
    }

    fn trips(&self, used: u64) -> Option<u64> {
        let over = used >= self.threshold;
        let was = self.over.replace(over);
        (over && !was).then_some(used)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ceiling_is_four_gib_and_the_backstop_under_it() {
        assert_eq!(CEILING, 4 * 1024 * 1024 * 1024);
        assert_eq!(BACKSTOP, 3584 * 1024 * 1024);
        const { assert!(BACKSTOP < CEILING) };
    }

    #[test]
    fn what_is_used_is_read() {
        let used = used().expect("/proc/self/statm");
        // A test binary holds a few megabytes at least, and far less than
        // the ceiling.
        assert!(used > 1 << 20 && used < CEILING, "{used}");
    }

    #[test]
    fn the_backstop_reads_at_most_four_times_a_second_and_trips_once() {
        let b = Backstop {
            threshold: 100,
            ..Backstop::default()
        };
        assert!(b.painted(10.0));
        // Due already: the frames of the same moment don't ask again.
        assert!(!b.painted(10.0));
        assert_eq!(b.trips(50), None);
        b.due.set(false);
        assert!(!b.painted(10.1));
        assert!(b.painted(10.3));
        b.due.set(false);
        assert_eq!(b.trips(150), Some(150));
        // Past it still: told once.
        assert_eq!(b.trips(160), None);
        assert_eq!(b.trips(90), None);
        assert_eq!(b.trips(120), Some(120));
        // A theme loaded while still past it: acted on again.
        assert_eq!(b.trips(130), None);
        b.rearm();
        assert_eq!(b.trips(130), Some(130));
    }
}
