//! Persisted preferences + light session state, stored as JSON in the XDG
//! config dir. Each app defines its own settings type; this module loads and
//! saves it safely and keeps per-page state blobs (history, memory, graph
//! equations, …).
//!
//! # Sections and their budgets
//!
//! The file is made of sections: each top-level field (a preference), and
//! each page state in [`PAGES`]. Each has a budget, in bytes of compact
//! JSON ([`section_budget`]), and the whole file one, [`MAX_SETTINGS_BYTES`]:
//!
//! - a save leaves out a section over its budget, and if the file would
//!   still be over its own, the pages with the smallest budgets (the
//!   largest of them first), so a saved file always loads, and reports
//!   what it left out. The apps' own sections stay within theirs: the
//!   calculator's state trims its History to fit
//!   ([`calcvm::MAX_STATE_BYTES`]), and a graph session's 14 equations of
//!   1000 characters take under 100 KB;
//! - a load reads files up to [`MAX_READ_BYTES`], drops only a section over
//!   its budget (a field of the wrong type only resets itself), and keeps
//!   the file as `.bad` when it drops anything. The calculator's state
//!   isn't bounded there beyond the file, so one saved before saves were
//!   budgeted keeps its memory: the calculator restores it (leaving out
//!   only what it can't replay) and its next save trims it. Such a file,
//!   over [`MAX_SETTINGS_BYTES`], is kept as `.bad` too, as it was.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::value::RawValue;

/// The most a save writes (see the module docs).
pub const MAX_SETTINGS_BYTES: u64 = 4 << 20;

/// Settings files bigger than this aren't read: they are kept as `.bad` and
/// defaults are used. Four times what a save writes, room for a session
/// saved before saves were budgeted (nine long History items took 13.7 MB).
pub const MAX_READ_BYTES: u64 = 4 * MAX_SETTINGS_BYTES;

/// The top-level field that holds the page states ([`HasPages`]), each a
/// section of its own.
pub const PAGES: &str = "pages";

/// The calculator's page state (`calcvm`'s saved state, as a string).
pub const CALCULATOR_PAGE: &str = "calculator";

/// The graphing page's state (the saved equations, `crate::graph`).
pub const GRAPHING_PAGE: &str = "graphing";

/// The budget of a preference (a top-level field other than [`PAGES`]).
pub const FIELD_BUDGET: usize = 16 << 10;

/// The budget of a page state other than the calculator's and graphing's
/// (the converter's takes a few hundred bytes).
pub const PAGE_BUDGET: usize = 64 << 10;

/// The budget of the graphing page's state: 14 equations of 1000
/// characters, escaped, take under 100 KB.
pub const GRAPHING_BUDGET: usize = 256 << 10;

/// The budget of a section in a save: of page `key`, or of a preference
/// (`None`). With the calculator's 3 MiB, the apps' pages and preferences
/// (GMNB has eleven) add up to under 3.5 MiB.
pub fn section_budget(page: Option<&str>) -> usize {
    match page {
        None => FIELD_BUDGET,
        Some(CALCULATOR_PAGE) => calcvm::MAX_STATE_BYTES,
        Some(GRAPHING_PAGE) => GRAPHING_BUDGET,
        Some(_) => PAGE_BUDGET,
    }
}

/// The budget of a section in a load: as in a save, but the calculator's
/// state is only bounded by the file (see the module docs).
fn load_budget(page: Option<&str>) -> usize {
    match page {
        Some(CALCULATOR_PAGE) => MAX_READ_BYTES as usize,
        page => section_budget(page),
    }
}

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

    /// Loads `path` section by section: a field of the wrong type
    /// (`"vulkan": "false"`) falls back to its default, a section over its
    /// budget (see the module docs) is dropped, and the others are kept.
    /// When anything is lost (a bad field or section, a file that is not a
    /// JSON object, over [`MAX_READ_BYTES`] or unreadable: then everything
    /// is default), or the file is over [`MAX_SETTINGS_BYTES`] (the next
    /// save trims it), it is first kept as `<path>.bad`, since the next save
    /// replaces it.
    pub fn load_from(path: PathBuf) -> Store<T> {
        let data = match std::fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => T::default(),
            _ => match read_bounded(&path, MAX_READ_BYTES) {
                Some(bytes) => {
                    let (data, complete) = match read_sections(&bytes) {
                        Some((value, within)) => {
                            let (data, whole) = from_value_lenient(value);
                            (data, whole && within)
                        }
                        None => (T::default(), false),
                    };
                    // A file over what a save writes (one saved before
                    // saves were budgeted) loads whole, but the next save
                    // trims it: kept as well.
                    if !complete || bytes.len() as u64 > MAX_SETTINGS_BYTES {
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

    /// Write atomically (unique temp file, then rename), within the
    /// budgets (see the module docs): a section over its budget is left out
    /// of the file, which is still written, and the save then reports it as
    /// an error. A file over [`MAX_SETTINGS_BYTES`] is never written.
    pub fn save(&self) -> Result<(), String> {
        if self.path.as_os_str().is_empty() {
            return Ok(());
        }
        let mut value = serde_json::to_value(&*self.data.borrow())
            .map_err(|e| format!("could not serialise settings: {e}"))?;
        let (bytes, left_out) =
            fit_sections(&mut value).map_err(|e| format!("could not serialise settings: {e}"))?;
        if bytes.len() as u64 > MAX_SETTINGS_BYTES {
            return Err(format!(
                "settings not saved to {}: {} bytes, over {MAX_SETTINGS_BYTES}",
                self.path.display(),
                bytes.len()
            ));
        }
        write_atomic(&self.path, &bytes)
            .map_err(|e| format!("could not save settings to {}: {e}", self.path.display()))?;
        if left_out.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "settings saved to {} without {} (over budget)",
                self.path.display(),
                left_out.join(", ")
            ))
        }
    }
}

/// The length of `json` (valid JSON) written compactly: without the
/// whitespace between tokens.
fn compact_len(json: &str) -> usize {
    let (mut n, mut in_string, mut escaped) = (0, false, false);
    for b in json.bytes() {
        if in_string {
            n += 1;
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
        } else if !matches!(b, b' ' | b'\t' | b'\n' | b'\r') {
            n += 1;
            in_string = b == b'"';
        }
    }
    n
}

/// Whether `raw` is over `budget` written compactly.
fn over_budget(raw: &RawValue, budget: usize) -> bool {
    raw.get().len() > budget && compact_len(raw.get()) > budget
}

/// A settings file's sections that are within their load budgets, as one
/// object, and whether every section was (`None` if it isn't a JSON
/// object). Sections are split off unparsed, so one over its budget is
/// never parsed.
fn read_sections(bytes: &[u8]) -> Option<(serde_json::Value, bool)> {
    use serde_json::Value;
    let fields: BTreeMap<String, &RawValue> = serde_json::from_slice(bytes).ok()?;
    let mut within = true;
    // A section within its budget, parsed (`None` if it isn't, or doesn't
    // parse: nested too deep).
    let mut section = |raw: &RawValue, budget: usize| {
        let value = (!over_budget(raw, budget))
            .then(|| serde_json::from_str::<Value>(raw.get()).ok())
            .flatten();
        within &= value.is_some();
        value
    };
    let mut object = serde_json::Map::new();
    for (key, raw) in fields {
        let pages = (key == PAGES)
            .then(|| serde_json::from_str::<BTreeMap<String, &RawValue>>(raw.get()).ok())
            .flatten();
        let value = match pages {
            Some(pages) => Value::Object(
                pages
                    .into_iter()
                    .filter_map(|(page, raw)| {
                        section(raw, load_budget(Some(&page))).map(|state| (page, state))
                    })
                    .collect(),
            ),
            None => match section(raw, load_budget(None)) {
                Some(value) => value,
                None => continue,
            },
        };
        object.insert(key, value);
    }
    Some((Value::Object(object), within))
}

/// The compact JSON length of `value`.
fn json_len(value: &serde_json::Value) -> usize {
    struct Count(usize);
    impl std::io::Write for Count {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0 += buf.len();
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count(0);
    let _ = serde_json::to_writer(&mut count, value);
    count.0
}

/// Leaves out of `value` (a settings object) each section over its save
/// budget, then, while the file would be over [`MAX_SETTINGS_BYTES`], the
/// pages with the smallest budgets, the largest of them first (see the
/// module docs). Returns the file and what was left out.
fn fit_sections(value: &mut serde_json::Value) -> serde_json::Result<(Vec<u8>, Vec<String>)> {
    use serde_json::Value;
    let mut left_out = Vec::new();
    if let Value::Object(fields) = value {
        fields.retain(|key, field| {
            if key == PAGES
                && let Value::Object(pages) = field
            {
                pages.retain(|page, state| {
                    let within = json_len(state) <= section_budget(Some(page));
                    if !within {
                        left_out.push(format!("{PAGES}.{page}"));
                    }
                    within
                });
                return true;
            }
            let within = json_len(field) <= section_budget(None);
            if !within {
                left_out.push(key.clone());
            }
            within
        });
    }
    let mut bytes = serde_json::to_vec_pretty(value)?;
    if bytes.len() as u64 > MAX_SETTINGS_BYTES
        && let Some(Value::Object(pages)) = value.get_mut(PAGES)
    {
        // Pages leave the file at least their compact length, so it's
        // within once the estimate is.
        let mut order: Vec<(usize, usize, String)> = pages
            .iter()
            .map(|(page, state)| (section_budget(Some(page)), json_len(state), page.clone()))
            .collect();
        order.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)).then(a.2.cmp(&b.2)));
        let mut estimate = bytes.len();
        for (_, len, page) in order {
            if estimate as u64 <= MAX_SETTINGS_BYTES {
                break;
            }
            pages.remove(&page);
            estimate -= len;
            left_out.push(format!("{PAGES}.{page}"));
        }
        bytes = serde_json::to_vec_pretty(value)?;
    }
    Ok((bytes, left_out))
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

        let huge = vec![b' '; (MAX_READ_BYTES + 1) as usize];
        std::fs::write(&path, &huge).unwrap();
        assert!(read_bounded(&path, MAX_READ_BYTES).is_none());
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

    impl HasPages for App {
        fn pages(&self) -> &PageStates {
            &self.pages
        }
        fn pages_mut(&mut self) -> &mut PageStates {
            &mut self.pages
        }
    }

    use calcvm::{Button, CalcMode, CalculatorViewModel};
    use serde_json::{Value, json};

    /// The calculator's state after MS of 7, then nine Scientific
    /// calculations of 40 pastes of a 100-term sum (one made, the others
    /// alike), as a build before saves were budgeted saved it: the History
    /// in `"s"."m"."h"` and again in `"x"."hc"`.
    fn nine_long_calculations() -> String {
        let mut vm = CalculatorViewModel::new();
        vm.set_mode(CalcMode::Scientific);
        vm.press(Button::Seven);
        vm.press(Button::Memory);
        let sum = vec!["1"; 100].join("+");
        for _ in 0..40 {
            assert!(vm.paste(&sum));
        }
        vm.press(Button::Equals);
        let mut state: Value = serde_json::from_str(&vm.save_state()).unwrap();
        let nine = Value::Array(vec![state["s"]["m"]["h"][0].clone(); 9]);
        state["s"]["m"]["h"] = nine.clone();
        state["x"]["hc"] = nine;
        state["x"].as_object_mut().unwrap().remove("hm");
        state.to_string()
    }

    fn equations() -> Value {
        let list: Vec<crate::graph::SavedEquation> = ["x^2", "sin(x)", "a*x+1"]
            .iter()
            .enumerate()
            .map(|(i, text)| crate::graph::SavedEquation {
                text: (*text).into(),
                color: i,
                style: "dash".into(),
                hidden: i == 1,
            })
            .collect();
        serde_json::to_value(list).unwrap()
    }

    /// What a calculator restored from `state` shows: its display,
    /// expression, memory and History.
    fn restored(state: &Value) -> (String, String, Vec<String>, Vec<calcvm::HistoryEntry>) {
        let mut vm = CalculatorViewModel::new();
        vm.restore_state(state.as_str().unwrap());
        (
            vm.display_value(),
            vm.expression(),
            vm.memory(),
            vm.history(),
        )
    }

    /// R16-M-01: nine long Scientific calculations saved 13.7 MB of
    /// calculator state, a file the next start refused whole (preferences,
    /// equations, memory and History reset). Such a file is now read, the
    /// calculator restores all of it, and its next save trims the oldest
    /// History to fit: that file stays within the ceiling and loads whole.
    #[test]
    fn a_long_calculator_session_never_costs_the_file() {
        let path = tmp("long");
        let bad = path.with_file_name("settings.json.bad");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let converter = json!({ "currency_from": "EUR", "currency_to": "JPY" });
        let file = json!({
            "mode": "scientific", "vulkan": false, "width": 500,
            "pages": {
                "calculator": nine_long_calculations(),
                "graphing": equations(),
                "converter": converter,
            },
        });
        let old = serde_json::to_vec_pretty(&file).unwrap();
        assert!(old.len() as u64 > 3 * MAX_SETTINGS_BYTES, "{}", old.len());
        std::fs::write(&path, &old).unwrap();

        let store = Store::<App>::load_from(path.clone());
        // Nothing is lost, but the next save trims it: kept as it was.
        assert_eq!(std::fs::read(&bad).unwrap(), old);
        std::fs::remove_file(&bad).unwrap();
        {
            let data = store.data.borrow();
            assert_eq!(
                (data.mode.as_str(), data.vulkan, data.width),
                ("scientific", false, 500)
            );
            assert_eq!(data.pages["graphing"], equations());
            assert_eq!(data.pages["converter"], converter);
        }
        let (display, expression, memory, history) =
            restored(&store.page_state("calculator").unwrap());
        assert_eq!(
            (display.as_str(), memory.len(), history.len()),
            ("3,961", 1, 9)
        );

        // The app's next save.
        let mut vm = CalculatorViewModel::new();
        vm.restore_state(store.page_state("calculator").unwrap().as_str().unwrap());
        store.set_page_state("calculator", Value::String(vm.save_state()));
        drop(vm);
        store.save().unwrap();
        let len = std::fs::metadata(&path).unwrap().len();
        assert!(len <= MAX_SETTINGS_BYTES, "{len}");

        let again = Store::<App>::load_from(path.clone());
        assert!(!bad.exists(), "the saved file loads whole");
        assert_eq!(*again.data.borrow(), *store.data.borrow());
        let kept = restored(&again.page_state("calculator").unwrap());
        // The three newest items fit beside the expression line.
        assert_eq!(kept, (display, expression, memory, history[..3].to_vec()));
    }

    /// A section over its budget is dropped alone: the rest of the file,
    /// the other pages included, is kept, and the file is kept as `.bad`.
    #[test]
    fn a_section_over_its_budget_only_drops_itself() {
        let path = tmp("section");
        let bad = path.with_file_name("settings.json.bad");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        // Over budget: a preference and the converter's page; nested too
        // deep to parse: another page.
        let text = format!(
            r#"{{"mode": "{}", "vulkan": false, "width": 500,
                "pages": {{"converter": "{}", "graphing": {}, "deep": {}{},
                           "calculator": "{{}}"}}}}"#,
            "m".repeat(FIELD_BUDGET),
            "c".repeat(PAGE_BUDGET),
            equations(),
            "[".repeat(200),
            "]".repeat(200),
        );
        std::fs::write(&path, &text).unwrap();

        let store = Store::<App>::load_from(path.clone());
        let data = store.data.borrow();
        assert_eq!(data.mode, "standard", "over its budget: the default");
        assert!(!data.vulkan);
        assert_eq!(data.width, 500);
        assert_eq!(data.pages.len(), 2, "{:?}", data.pages.keys());
        assert_eq!(data.pages["graphing"], equations());
        assert_eq!(data.pages["calculator"], "{}");
        assert_eq!(std::fs::read_to_string(&bad).unwrap(), text);

        // Whitespace doesn't count: a section within its budget written
        // compactly is kept however it is indented.
        let spaced = format!(
            r#"{{"pages": {{"converter": [{}"x"]}}}}"#,
            "\"\",\n        ".repeat(PAGE_BUDGET / 4)
        );
        assert!(spaced.len() > 2 * PAGE_BUDGET);
        std::fs::remove_file(&bad).unwrap();
        std::fs::write(&path, &spaced).unwrap();
        let store = Store::<App>::load_from(path);
        assert!(store.page_state("converter").is_some());
        assert!(!bad.exists());
    }

    /// A save leaves out a section over its budget, says so, and writes the
    /// rest: a file that loads whole.
    #[test]
    fn a_save_leaves_out_a_section_over_its_budget() {
        let path = tmp("save-section");
        let bad = path.with_file_name("settings.json.bad");
        let store: Store<App> = Store::load_from(path.clone());
        store.data.borrow_mut().width = 500;
        store.data.borrow_mut().mode = "m".repeat(FIELD_BUDGET);
        store.set_page_state("graphing", equations());
        store.set_page_state("converter", json!("c".repeat(PAGE_BUDGET)));
        let e = store.save().unwrap_err();
        assert!(e.contains("mode") && e.contains("pages.converter"), "{e}");

        let again = Store::<App>::load_from(path);
        assert!(!bad.exists());
        let data = again.data.borrow();
        assert_eq!((data.mode.as_str(), data.width), ("standard", 500));
        assert_eq!(data.pages.len(), 1);
        assert_eq!(data.pages["graphing"], equations());
    }

    /// However many pages a file holds (only a hand-made one has more than
    /// the apps' three), a save never writes more than the ceiling: the
    /// pages with the smallest budget go first, the largest of them first.
    #[test]
    fn a_save_never_writes_past_the_ceiling() {
        let path = tmp("ceiling");
        let store: Store<App> = Store::load_from(path.clone());
        let calculator = "c".repeat(calcvm::MAX_STATE_BYTES - 2);
        store.set_page_state("calculator", json!(calculator));
        store.set_page_state("graphing", equations());
        for i in 0..40 {
            store.set_page_state(&format!("extra{i:02}"), json!("e".repeat(30_000 + i)));
        }
        let e = store.save().unwrap_err();
        let len = std::fs::metadata(&path).unwrap().len();
        assert!(len <= MAX_SETTINGS_BYTES, "{len}");
        let again = Store::<App>::load_from(path);
        let data = again.data.borrow();
        assert_eq!(data.pages["calculator"], json!(calculator));
        assert_eq!(data.pages["graphing"], equations());
        // 3 MiB and 40 pages of 30 KB: 6 left out, the largest.
        let extra: Vec<&String> = data
            .pages
            .keys()
            .filter(|k| k.starts_with("extra"))
            .collect();
        assert_eq!(extra.len(), 34, "{e}");
        assert_eq!(extra.last().unwrap().as_str(), "extra33");
    }
}
