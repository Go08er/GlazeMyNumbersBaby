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

/// Suffix of the copy kept of a settings file that could not be loaded in
/// full (`settings.json.bad`), before defaults replace what was lost.
pub const BAD_COPY_SUFFIX: &str = ".bad";

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

    /// Loads `path` field by field: a field of the wrong type (`"vulkan":
    /// "false"`) falls back to its default and the others are kept. When
    /// anything is lost (a bad field, a file that is not a JSON object, too
    /// big or unreadable: then everything is default), the file is first
    /// kept as `<path>.bad`, since the next save replaces it.
    pub fn load_from(path: PathBuf) -> Store<T> {
        let data = match std::fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => T::default(),
            _ => match read_bounded(&path, MAX_SETTINGS_BYTES) {
                Some(bytes) => {
                    let (data, complete) = match serde_json::from_slice(&bytes) {
                        Ok(value) => from_value_lenient(value),
                        Err(_) => (T::default(), false),
                    };
                    if !complete {
                        let _ = write_atomic(&bad_copy_path(&path), &bytes);
                    }
                    data
                }
                // Too big to copy in memory: move it aside.
                None => {
                    let _ = std::fs::rename(&path, bad_copy_path(&path));
                    T::default()
                }
            },
        };
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

/// `<path>.bad`.
fn bad_copy_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(BAD_COPY_SUFFIX);
    PathBuf::from(name)
}

/// `value` as a `T`, field by field when it does not parse whole: an object
/// field that does not fit `T` takes its default and the others are kept
/// (each field is tried on its own against the defaults, so a large field
/// is not re-parsed per field). Anything but an object gives the defaults.
/// The flag says whether nothing was lost.
pub fn from_value_lenient<T: Serialize + DeserializeOwned + Default>(
    value: serde_json::Value,
) -> (T, bool) {
    use serde_json::Value;
    if let Ok(t) = T::deserialize(&value) {
        return (t, true);
    }
    let (Value::Object(fields), Ok(Value::Object(defaults))) =
        (value, serde_json::to_value(T::default()))
    else {
        return (T::default(), false);
    };
    let mut merged = defaults.clone();
    for (key, field) in fields {
        let mut one = defaults.clone();
        one.insert(key.clone(), field);
        let one = Value::Object(one);
        if T::deserialize(&one).is_ok()
            && let Value::Object(mut one) = one
            && let Some(field) = one.remove(&key)
        {
            merged.insert(key, field);
        }
    }
    let t = T::deserialize(&Value::Object(merged)).unwrap_or_default();
    (t, false)
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
    // create_new: never write through something already at the temp name
    // (a stale file or a planted symlink); the final rename replaces `path`
    // itself rather than following a symlink there.
    let result = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .and_then(|mut f| {
            use std::io::Write;
            f.write_all(bytes)?;
            f.sync_all()
        })
        .and_then(|_| std::fs::rename(&tmp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
        return result;
    }
    // Make the rename itself durable: sync the directory entry (best
    // effort; some filesystems refuse to sync a directory).
    if let Some(dir) = path.parent()
        && let Ok(d) = std::fs::File::open(if dir.as_os_str().is_empty() {
            Path::new(".")
        } else {
            dir
        })
    {
        let _ = d.sync_all();
    }
    Ok(())
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
        let bad = path.with_file_name("settings.json.bad");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"{ not json").unwrap();
        assert_eq!(
            *Store::<S>::load_from(path.clone()).data.borrow(),
            S::default()
        );
        assert_eq!(std::fs::read(&bad).unwrap(), b"{ not json");

        let huge = vec![b' '; (MAX_SETTINGS_BYTES + 1) as usize];
        std::fs::write(&path, &huge).unwrap();
        assert!(read_bounded(&path, MAX_SETTINGS_BYTES).is_none());
        let store = Store::<S>::load_from(path.clone());
        assert_eq!(*store.data.borrow(), S::default());
        // Kept aside before the defaults are saved over it.
        assert_eq!(std::fs::metadata(&bad).unwrap().len(), huge.len() as u64);
        store.save().unwrap();
        assert_eq!(std::fs::metadata(&bad).unwrap().len(), huge.len() as u64);
    }

    #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
    #[serde(default)]
    struct App {
        mode: String,
        vulkan: bool,
        width: i32,
        pages: PageStates,
    }

    impl Default for App {
        fn default() -> Self {
            App {
                mode: "standard".into(),
                vulkan: true,
                width: 360,
                pages: PageStates::new(),
            }
        }
    }

    /// One mistyped field (`"vulkan": "false"`, a string) used to reset every
    /// setting, history and equations included; now only that field does.
    #[test]
    fn a_mistyped_field_only_resets_itself() {
        let path = tmp("field");
        let bad = path.with_file_name("settings.json.bad");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let text = r#"{"mode": "graphing", "vulkan": "false", "width": 500,
            "pages": {"calculator": {"m": 1}, "graphing": [{"text": "x^2"}]}}"#;
        std::fs::write(&path, text).unwrap();

        let store = Store::<App>::load_from(path.clone());
        let data = store.data.borrow();
        assert_eq!(data.mode, "graphing");
        assert!(data.vulkan, "the mistyped field takes its default");
        assert_eq!(data.width, 500);
        assert_eq!(data.pages["calculator"], serde_json::json!({"m": 1}));
        assert_eq!(data.pages["graphing"], serde_json::json!([{"text": "x^2"}]));
        assert_eq!(std::fs::read_to_string(&bad).unwrap(), text);

        // A file that loads whole leaves no copy behind.
        std::fs::remove_file(&bad).unwrap();
        std::fs::write(&path, r#"{"vulkan": false}"#).unwrap();
        assert!(!Store::<App>::load_from(path.clone()).data.borrow().vulkan);
        assert!(!bad.exists());

        // Not an object: defaults.
        std::fs::write(&path, "[1, 2]").unwrap();
        assert_eq!(*Store::<App>::load_from(path).data.borrow(), App::default());
    }
}
