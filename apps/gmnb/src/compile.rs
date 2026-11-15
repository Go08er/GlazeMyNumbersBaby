//! A theme's WebAssembly modules compiled for the hub (docs/theming.md,
//! "Modules"): on a thread started for the compile and gone after it.
//! What it made reaches the main loop in one place, [`arrived`], once per
//! theme applied; nothing waits or ticks meanwhile, and at rest there is
//! no such thread. The base look and themes without modules never come
//! here.

use gmnb_theme::module::{Compiled, Job};
use gtk::glib;

use crate::theme::Hub;

/// Compiles `job` on a thread of its own: the hub is handed what it made,
/// with `token`, on the main loop ([`Hub::compiled`]). `Some`: no thread
/// could be started, and every module of the job is refused (the caller
/// installs that at once).
pub fn start(job: Job, token: u64) -> Option<Compiled> {
    // The thread is left to end by itself, once it has handed its work
    // over.
    job.spawn(move |done| arrived(token, done)).err()
}

/// What a compile thread made, handed to the main loop.
fn arrived(token: u64, done: Compiled) {
    glib::MainContext::default().invoke(move || {
        if let Some(hub) = Hub::current() {
            hub.compiled(token, done);
        }
    });
}

/// Compiles `job` on a thread of its own and waits for it: GMNB starting
/// with a theme with modules, whose window isn't shown before they are
/// ready (no flash of another look). What it waits for is bounded by the
/// bytes it compiles ([`gmnb_theme::Limits::module_bytes`]).
pub fn now(job: Job) -> Compiled {
    let (tx, rx) = std::sync::mpsc::channel();
    match job.spawn(move |done| {
        let _ = tx.send(done);
    }) {
        Ok(thread) => {
            let done = rx.recv();
            let _ = thread.join();
            // (`Job::run` refuses a module whose compile panics: the
            // thread always hands its work over.)
            done.expect("the compile thread hands its work over")
        }
        Err(refused) => refused,
    }
}
