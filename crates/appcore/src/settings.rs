//! Persisted preferences + light session state, stored as JSON in the XDG
//! config dir. Each app defines its own settings type; this module loads and
//! saves it safely and keeps per-page state blobs (history, memory, graph
//! equations, …).

use std::cell::RefCell;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use serde::Serialize;
use serde::de::DeserializeOwned;

/// Settings files bigger than this are ignored (defaults are used).
pub const MAX_SETTINGS_BYTES: u64 = 4 << 20;

/// Opaque per-page state, keyed by page.
pub type PageStates = serde_json::Map<String, serde_json::Value>;

/// A settings type that carries per-page state blobs.
pub trait HasPages {
    fn pages(&self) -> &PageStates;
    fn pages_mut(&mut self) -> &mut PageStates;
}

pub struct Store<T> {
    path: PathBuf,
    pub data: RefCell<T>,
}

impl<T: Serialize + DeserializeOwned + Default> Store<T> {
    /// `$XDG_CONFIG_HOME/<app>/settings.json`, or defaults if it's missing,
    /// too big or unreadable.
    pub fn load(app: &str) -> Store<T> {
        Self::load_from(crate::dirs::config_dir(app).join("settings.json"))
    }

    pub fn load_from(path: PathBuf) -> Store<T> {
        let data = read_bounded(&path, MAX_SETTINGS_BYTES)
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Store {
            path,
            data: RefCell::new(data),
        }
    }

    /// An in-memory store that never touches disk (screenshots, tests).
    pub fn ephemeral() -> Store<T> {
        Store {
            path: PathBuf::new(),
            data: RefCell::new(T::default()),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Write atomically (unique temp file, then rename).
    pub fn save(&self) -> Result<(), String> {
        if self.path.as_os_str().is_empty() {
            return Ok(());
        }
        let bytes = serde_json::to_vec_pretty(&*self.data.borrow())
            .map_err(|e| format!("could not serialise settings: {e}"))?;
        write_atomic(&self.path, &bytes)
            .map_err(|e| format!("could not save settings to {}: {e}", self.path.display()))
    }
}

impl<T: HasPages> Store<T> {
    pub fn page_state(&self, key: &str) -> Option<serde_json::Value> {
        self.data.borrow().pages().get(key).cloned()
    }

    pub fn set_page_state(&self, key: &str, value: serde_json::Value) {
        self.data
            .borrow_mut()
            .pages_mut()
            .insert(key.to_string(), value);
    }
}

/// Read a whole file unless it's larger than `max` bytes.
pub fn read_bounded(path: &Path, max: u64) -> Option<Vec<u8>> {
    let file = std::fs::File::open(path).ok()?;
    if file.metadata().ok()?.len() > max {
        return None;
    }
    let mut buf = Vec::new();
    file.take(max + 1).read_to_end(&mut buf).ok()?;
    (buf.len() as u64 <= max).then_some(buf)
}

/// Write `bytes` to `path` via a temp file unique to this process and call,
/// so concurrent writers never interleave into one file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    static SEQ: AtomicU32 = AtomicU32::new(0);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = path.with_file_name(format!(
        ".{name}.{}.{}.tmp",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let result = std::fs::write(&tmp, bytes).and_then(|_| std::fs::rename(&tmp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Serialize, serde::Deserialize, Default, Debug, PartialEq)]
    #[serde(default)]
    struct S {
        mode: String,
        pages: PageStates,
    }

    impl HasPages for S {
        fn pages(&self) -> &PageStates {
            &self.pages
        }
        fn pages_mut(&mut self) -> &mut PageStates {
            &mut self.pages
        }
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("appcore-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d.join("settings.json")
    }

    #[test]
    fn round_trips_and_leaves_no_temp_files() {
        let path = tmp("rt");
        let store: Store<S> = Store::load_from(path.clone());
        store.data.borrow_mut().mode = "graphing".into();
        store.set_page_state("calculator", serde_json::json!({"a": 1}));
        store.save().unwrap();
        let again: Store<S> = Store::load_from(path.clone());
        assert_eq!(again.data.borrow().mode, "graphing");
        assert_eq!(
            again.page_state("calculator"),
            Some(serde_json::json!({"a": 1}))
        );
        let files: Vec<_> = std::fs::read_dir(path.parent().unwrap()).unwrap().collect();
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn malformed_or_huge_files_fall_back_to_defaults() {
        let path = tmp("bad");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"{ not json").unwrap();
        assert_eq!(
            *Store::<S>::load_from(path.clone()).data.borrow(),
            S::default()
        );
        let huge = vec![b' '; (MAX_SETTINGS_BYTES + 1) as usize];
        std::fs::write(&path, huge).unwrap();
        assert!(read_bounded(&path, MAX_SETTINGS_BYTES).is_none());
        assert_eq!(*Store::<S>::load_from(path).data.borrow(), S::default());
    }
}
