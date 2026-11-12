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

/// How many folders without a `theme.luau` yet are watched besides those
/// Settings lists: themes being copied in, whose file arrives after them.
const PENDING: usize = 16;

pub struct Watch {
    dir: PathBuf,
    on_change: Box<dyn Fn()>,
    /// The chosen theme's id, as it is when the folder is read.
    chosen: Box<dyn Fn() -> String>,
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
    /// `chosen`: the chosen theme's id, whose folder is always watched.
    pub fn start(
        dir: &Path,
        chosen: impl Fn() -> String + 'static,
        on_change: impl Fn() + 'static,
    ) -> Rc<Watch> {
        let _ = std::fs::create_dir_all(dir);
        let watch = Rc::new(Watch {
            dir: dir.to_path_buf(),
            on_change: Box::new(on_change),
            chosen: Box::new(chosen),
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
        // The folders Settings lists (the built-in themes', the chosen
        // theme's wherever it falls, and the first of the others by id: the
        // user's inotify watches are all of theirs, and a hostile folder has
        // thousands), and the first few that are not themes yet (a theme
        // being copied in: its `theme.luau` arrives after its folder). A
        // folder that is no theme never takes a listed theme's place.
        let listed = theme::discover_keeping(Some(&self.dir), &[], None, &(self.chosen)());
        want.extend(listed.iter().filter_map(|s| s.dir().map(Path::to_path_buf)));
        let mut pending: Vec<PathBuf> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                if !name.to_str().is_some_and(theme::valid_id) {
                    continue;
                }
                let path = entry.path();
                if path.is_dir() && !path.join(theme::THEME_FILE).is_file() {
                    pending.push(path);
                }
            }
        }
        pending.sort();
        pending.truncate(PENDING);
        want.extend(pending);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// What is watched is what Settings lists, and the chosen theme's
    /// folder wherever it falls: folders that are no theme (yet) sorted
    /// before the themes never take a listed theme's place, and only the
    /// first few of them are watched (round 4, area A: the first 67 folders
    /// by id were watched, whatever they held, and a chosen theme sorted
    /// late was not).
    #[test]
    fn the_folders_watched_are_those_listed_and_the_chosen_one() {
        let dir = std::env::temp_dir().join(format!("gmnb-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let theme = |id: &str| {
            std::fs::create_dir_all(dir.join(id)).unwrap();
            std::fs::write(dir.join(id).join(theme::THEME_FILE), "return {}").unwrap();
        };
        for i in 0..20 {
            std::fs::create_dir_all(dir.join(format!("a{i:02}"))).unwrap();
        }
        for i in 0..70 {
            theme(&format!("t{i:02}"));
        }
        theme("zz-chosen");
        let watch = Watch::start(&dir, || "zz-chosen".to_string(), || {});
        let watched: Vec<PathBuf> = watch.monitors.borrow().keys().cloned().collect();
        watch.stop();
        let _ = std::fs::remove_dir_all(&dir);
        let has = |id: &str| watched.contains(&dir.join(id));
        assert!(watched.contains(&dir));
        assert!(has("zz-chosen"), "the chosen theme");
        for i in 0..theme::MAX_USER_FOLDERS {
            assert!(has(&format!("t{i:02}")), "t{i:02}, listed");
        }
        assert!(
            !has(&format!("t{:02}", theme::MAX_USER_FOLDERS)),
            "past the cap"
        );
        let pending = (0..20).filter(|i| has(&format!("a{i:02}"))).count();
        assert_eq!(pending, PENDING);
        assert_eq!(watched.len(), 1 + 1 + theme::MAX_USER_FOLDERS + PENDING);
    }
}
