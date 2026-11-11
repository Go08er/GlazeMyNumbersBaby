//! Settings' watch on the themes folder (`crate::theme::user_dir`): while
//! Settings is open, a theme folder that appears there (dragged in from a
//! file manager), goes, or whose `theme.luau` changes shows in its list
//! without a restart, a moment after the last change. Nothing watches the
//! folder while Settings is closed: GMNB wakes for nothing at rest.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use gmnb_theme::theme;
use gtk::prelude::*;
use gtk::{gio, glib};

/// How long the folder is left to settle after a change before it is read
/// again: a folder copied in arrives file by file.
const SETTLE: Duration = Duration::from_millis(300);

pub struct Watch {
    dir: PathBuf,
    on_change: Box<dyn Fn()>,
    /// A monitor on the themes folder, and one on each folder in it (a
    /// theme's `theme.luau` arrives after its folder, and a monitor on a
    /// folder sees only what is directly in it).
    monitors: RefCell<BTreeMap<PathBuf, gio::FileMonitor>>,
    /// The read the last change asked for, waiting for the folder to
    /// settle.
    pending: RefCell<Option<glib::SourceId>>,
}

impl Watch {
    /// Watches `dir` (made if it isn't there): `on_change` is called a
    /// moment after anything in it changed, until [`Watch::stop`].
    pub fn start(dir: &Path, on_change: impl Fn() + 'static) -> Rc<Watch> {
        let _ = std::fs::create_dir_all(dir);
        let watch = Rc::new(Watch {
            dir: dir.to_path_buf(),
            on_change: Box::new(on_change),
            monitors: RefCell::default(),
            pending: RefCell::default(),
        });
        watch.sync();
        watch
    }

    /// A monitor for the folder and for each folder in it now; none for
    /// one that has gone.
    fn sync(self: &Rc<Self>) {
        let mut want = vec![self.dir.clone()];
        // Folders a theme could be (a valid id, not the hidden ones GMNB
        // writes into), and no more than Settings lists: the user's inotify
        // watches are all of theirs, and a hostile folder has thousands.
        let mut folders: Vec<(String, PathBuf)> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let Some(id) = name.to_str().filter(|n| theme::valid_id(n)) else {
                    continue;
                };
                if entry.path().is_dir() {
                    folders.push((id.to_string(), entry.path()));
                }
            }
        }
        folders.sort();
        folders.truncate(theme::MAX_USER_FOLDERS + theme::BUILT_IN.len());
        want.extend(folders.into_iter().map(|(_, path)| path));
        let mut monitors = self.monitors.borrow_mut();
        monitors.retain(|path, monitor| {
            let keep = want.contains(path);
            if !keep {
                monitor.cancel();
            }
            keep
        });
        for path in want {
            if monitors.contains_key(&path) {
                continue;
            }
            let Ok(monitor) = gio::File::for_path(&path)
                .monitor_directory(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE)
            else {
                continue;
            };
            let weak = Rc::downgrade(self);
            monitor.connect_changed(move |_, _, _, _| {
                if let Some(watch) = weak.upgrade() {
                    watch.changed();
                }
            });
            monitors.insert(path, monitor);
        }
    }

    /// Something in the folder changed: it is read again once it has
    /// settled for [`SETTLE`].
    fn changed(self: &Rc<Self>) {
        if let Some(id) = self.pending.borrow_mut().take() {
            id.remove();
        }
        let weak = Rc::downgrade(self);
        let id = glib::timeout_add_local_once(SETTLE, move || {
            let Some(watch) = weak.upgrade() else { return };
            watch.pending.borrow_mut().take();
            watch.sync();
            (watch.on_change)();
        });
        *self.pending.borrow_mut() = Some(id);
    }

    /// Stops watching: every monitor cancelled, a read still waiting
    /// dropped.
    pub fn stop(&self) {
        if let Some(id) = self.pending.borrow_mut().take() {
            id.remove();
        }
        for (_, monitor) in std::mem::take(&mut *self.monitors.borrow_mut()) {
            monitor.cancel();
        }
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.stop();
    }
}
