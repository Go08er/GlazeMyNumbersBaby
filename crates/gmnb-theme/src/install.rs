//! The built-in themes in the themes folder (docs/theming.md, "Files and
//! where they go"). GMNB carries every built-in theme's files
//! ([`crate::theme::BUILT_IN`]) and writes each one's folder into the
//! themes folder once, the first time it starts for a user. What it wrote
//! is recorded: a [`Record`] of each id and the [`digest`] of the folder.
//! The record is what keeps a theme the user deleted deleted.
//!
//! - A built-in theme that a newer GMNB carries changed replaces its folder
//!   only if the folder is still exactly what GMNB wrote. A folder the user
//!   changed is theirs, and nothing is said about it.
//! - A folder with a built-in theme's id that GMNB didn't write is the
//!   user's, and is never written to. (One that is exactly what GMNB
//!   carries counts as GMNB's: a folder written by another GMNB, or by one
//!   that stopped before its settings were saved.)
//! - Nothing here follows a link where a theme's folder or its files go. A
//!   folder is written into a new hidden folder beside where it goes (a
//!   name no theme can have), each file created new, and renamed into place
//!   only if nothing is there by then.
//!
//! Nothing here knows the toolkit or the settings file: the app keeps the
//! record, and serves a built-in theme from its own copy for a run in which
//! its folder couldn't be written ([`Pass::embedded`]).

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, SystemTime};

use crate::theme::{BuiltIn, THEME_FILE};

/// What GMNB has written into the themes folder: each built-in theme's id,
/// and the [`digest`] of the folder it last wrote (or found as it would
/// have written it).
pub type Record = BTreeMap<String, String>;

/// What a folder holds, in the order a digest reads it: each path in it,
/// and for a file its bytes (`None`: a folder).
type Tree<'a> = BTreeMap<String, Option<Cow<'a, [u8]>>>;

/// The most a folder may hold for it to be read here (in all, and in
/// entries): a folder bigger than this is not one GMNB wrote.
const MAX_TREE_BYTES: u64 = 64 << 20;
const MAX_TREE_ENTRIES: usize = 4096;

/// What GMNB's hidden folders in the themes folder start with: a folder
/// being written, or an old one on its way out. No theme id starts with a
/// dot ([`crate::theme::valid_id`]), so no list shows them.
const HIDDEN: &str = ".gmnb-";

/// How old a hidden folder left behind (by a GMNB that stopped while
/// writing one) is before it is cleared away.
const STALE: Duration = Duration::from_secs(3600);

/// What a pass over the built-in themes did ([`install`], [`restore`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pass {
    /// The record afterwards (the caller saves it when it changed).
    pub record: Record,
    /// The built-in themes that have no folder because writing it failed:
    /// their own copies stand in for this run.
    pub embedded: Vec<String>,
    /// The folders written where there was none.
    pub written: Vec<String>,
    /// The folders replaced by the version this GMNB carries.
    pub updated: Vec<String>,
    /// What went wrong, a line each.
    pub failed: Vec<String>,
}

/// The digest of the folder GMNB writes for `theme`: SHA-256 over each
/// path in it, in order, and each file's length and bytes.
pub fn digest(theme: &BuiltIn) -> String {
    digest_tree(&embedded_tree(theme))
}

/// The digest of the folder `dir` as [`digest`] reads one; `None` if it
/// isn't a folder GMNB could have written (it is a link, or holds one, or
/// anything but folders and files, a name that isn't UTF-8, or more than a
/// theme's folder).
pub fn folder_digest(dir: &Path) -> Option<String> {
    read_tree(dir).map(|tree| digest_tree(&tree))
}

/// The built-in themes of `built_in` with nothing at their name in
/// `themes`: what *Restore built-in themes* brings back.
pub fn missing(themes: &Path, built_in: &[BuiltIn]) -> Vec<String> {
    built_in
        .iter()
        .filter(|b| is_missing(&themes.join(b.id)))
        .map(|b| b.id.to_string())
        .collect()
}

/// The pass GMNB makes as it starts, for each theme of `built_in`, with
/// `record` (what it wrote before) and the themes folder `themes`:
///
/// - never written: its folder is written, unless something is at its
///   name (the user's; or GMNB's own, if it is exactly what GMNB carries,
///   which is then recorded);
/// - written, and gone: the user deleted it, and it stays deleted;
/// - written, and this GMNB carries another version: the folder is
///   replaced if it is still exactly what GMNB wrote, else kept;
/// - a folder that couldn't be written is tried again at the next start,
///   and its embedded copy stands in until then.
pub fn install(themes: &Path, built_in: &[BuiltIn], record: &Record) -> Pass {
    let mut pass = Pass {
        record: record.clone(),
        ..Pass::default()
    };
    sweep(themes);
    for theme in built_in {
        let id = theme.id.to_string();
        let target = themes.join(theme.id);
        let now = digest(theme);
        match record.get(theme.id) {
            Some(last) if *last == now => {}
            Some(last) => match fs::symlink_metadata(&target) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => pass
                    .failed
                    .push(format!("{}: {e}; left as it is", target.display())),
                Ok(_) if folder_digest(&target).as_ref() != Some(last) => {
                    // The user's now: kept, and not looked at again.
                    pass.record.insert(id, now);
                }
                Ok(_) => match replace(themes, &target, theme, last) {
                    Ok(true) => {
                        pass.record.insert(id.clone(), now);
                        pass.updated.push(id);
                    }
                    Ok(false) => {
                        pass.record.insert(id, now);
                    }
                    Err(e) => pass.failed.push(format!(
                        "couldn't update {}: {e}; the version there stays, and the next start \
                         tries again",
                        target.display()
                    )),
                },
            },
            None => match fs::symlink_metadata(&target) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => write(&mut pass, themes, theme),
                Err(e) => {
                    pass.failed.push(format!(
                        "{}: {e}; GMNB's own copy of the theme is used for this run",
                        target.display()
                    ));
                    pass.embedded.push(id);
                }
                Ok(_) => {
                    if is_as_carried(&target, theme) {
                        pass.record.insert(id, now);
                    }
                }
            },
        }
    }
    pass
}

/// *Restore built-in themes*: the folder of each theme of `built_in` with
/// nothing at its name is written (and recorded). A folder that is there
/// is never written to.
pub fn restore(themes: &Path, built_in: &[BuiltIn], record: &Record) -> Pass {
    let mut pass = Pass {
        record: record.clone(),
        ..Pass::default()
    };
    sweep(themes);
    for theme in built_in {
        if is_missing(&themes.join(theme.id)) {
            write(&mut pass, themes, theme);
        }
    }
    pass
}

/// Writes `theme`'s folder where there is none, into `pass`.
fn write(pass: &mut Pass, themes: &Path, theme: &BuiltIn) {
    let id = theme.id.to_string();
    let target = themes.join(theme.id);
    match place(themes, &target, theme) {
        Ok(true) => {
            pass.record.insert(id.clone(), digest(theme));
            pass.written.push(id);
        }
        // Something took its place meanwhile (another GMNB, most likely).
        Ok(false) => {
            if is_as_carried(&target, theme) {
                pass.record.insert(id, digest(theme));
            }
        }
        Err(e) => {
            pass.failed.push(format!(
                "couldn't write {}: {e}; GMNB's own copy of the theme is used for this run, \
                 and the next start tries again",
                target.display()
            ));
            pass.embedded.push(id);
        }
    }
}

fn is_missing(path: &Path) -> bool {
    matches!(fs::symlink_metadata(path), Err(e) if e.kind() == io::ErrorKind::NotFound)
}

/// Whether `dir` is a folder holding exactly what GMNB writes for `theme`
/// (not a link to one). Its shape (paths and sizes) is compared first:
/// only a folder of the same shape is read.
pub fn is_as_carried(dir: &Path, theme: &BuiltIn) -> bool {
    let tree = embedded_tree(theme);
    let shape: BTreeMap<&str, Option<u64>> = tree
        .iter()
        .map(|(path, bytes)| (path.as_str(), bytes.as_ref().map(|b| b.len() as u64)))
        .collect();
    let Some(found) = read_shape(dir) else {
        return false;
    };
    let found: BTreeMap<&str, Option<u64>> = found.iter().map(|(p, s)| (p.as_str(), *s)).collect();
    found == shape && read_tree(dir).is_some_and(|read| read == tree)
}

// ---- Writing -------------------------------------------------------------------

/// A name for a new hidden folder in `themes`.
fn fresh_name(themes: &Path, id: &str, what: &str) -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    themes.join(format!(
        "{HIDDEN}{what}-{id}-{}-{nanos}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ))
}

/// Writes `theme`'s folder into a new hidden folder of `themes`, every file
/// and folder in it created new (a link where one goes fails it), and
/// flushed to the disk: the folder it returns.
fn write_new(themes: &Path, theme: &BuiltIn) -> io::Result<PathBuf> {
    fs::create_dir_all(themes)?;
    let new = fresh_name(themes, theme.id, "new");
    fs::create_dir(&new)?;
    let written = (|| {
        let tree = embedded_tree(theme);
        for (path, bytes) in &tree {
            let at = new.join(path);
            match bytes {
                None => fs::create_dir(&at)?,
                Some(bytes) => {
                    let mut file = fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .custom_flags(libc::O_NOFOLLOW)
                        .open(&at)?;
                    file.write_all(bytes)?;
                    file.sync_all()?;
                }
            }
        }
        for (path, _) in tree.iter().filter(|(_, bytes)| bytes.is_none()) {
            sync_dir(&new.join(path))?;
        }
        sync_dir(&new)
    })();
    match written {
        Ok(()) => Ok(new),
        Err(e) => {
            let _ = fs::remove_dir_all(&new);
            Err(e)
        }
    }
}

fn sync_dir(dir: &Path) -> io::Result<()> {
    fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
        .open(dir)?
        .sync_all()
}

/// Writes `theme`'s folder at `target`, where there is none: `Ok(false)`
/// if something is there by the time it is written (which is left).
fn place(themes: &Path, target: &Path, theme: &BuiltIn) -> io::Result<bool> {
    let new = write_new(themes, theme)?;
    match rename_new(&new, target) {
        Ok(true) => {
            let _ = sync_dir(themes);
            Ok(true)
        }
        other => {
            let _ = fs::remove_dir_all(&new);
            other
        }
    }
}

/// Replaces the folder `target`, which held exactly the folder of digest
/// `last`, with `theme`'s: `Ok(false)` if it changed before it could be
/// (it is then left as it is).
fn replace(themes: &Path, target: &Path, theme: &BuiltIn, last: &str) -> io::Result<bool> {
    let new = write_new(themes, theme)?;
    let old = fresh_name(themes, theme.id, "old");
    let moved = rename_new(target, &old);
    if !matches!(moved, Ok(true)) {
        let _ = fs::remove_dir_all(&new);
        return moved.and(Err(io::Error::other("couldn't move the old folder aside")));
    }
    // Changed between the look and the move: put back.
    if folder_digest(&old).as_deref() != Some(last) {
        let _ = fs::remove_dir_all(&new);
        return match rename_new(&old, target) {
            Ok(true) => Ok(false),
            _ => Err(io::Error::other(format!(
                "it changed while being replaced; it is at {}",
                old.display()
            ))),
        };
    }
    match rename_new(&new, target) {
        Ok(true) => {
            let _ = fs::remove_dir_all(&old);
            let _ = sync_dir(themes);
            Ok(true)
        }
        // Something took its place meanwhile: it stays, and the old
        // folder (GMNB's, unchanged) goes.
        Ok(false) => {
            let _ = fs::remove_dir_all(&new);
            let _ = fs::remove_dir_all(&old);
            Ok(false)
        }
        Err(e) => {
            let _ = rename_new(&old, target);
            let _ = fs::remove_dir_all(&new);
            Err(e)
        }
    }
}

/// Renames `from` to `to` unless something is at `to` (a folder, a file,
/// a link, even an empty folder, which a plain rename would replace):
/// `Ok(false)` then, and nothing is moved.
fn rename_new(from: &Path, to: &Path) -> io::Result<bool> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::ffi::OsStrExt;
        let c = |p: &Path| {
            std::ffi::CString::new(p.as_os_str().as_bytes())
                .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))
        };
        let (from_c, to_c) = (c(from)?, c(to)?);
        // SAFETY: two NUL-terminated paths that outlive the call.
        let r = unsafe {
            libc::syscall(
                libc::SYS_renameat2,
                libc::AT_FDCWD,
                from_c.as_ptr(),
                libc::AT_FDCWD,
                to_c.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        if r == 0 {
            return Ok(true);
        }
        let e = io::Error::last_os_error();
        match e.raw_os_error() {
            Some(libc::EEXIST) => return Ok(false),
            // A filesystem that can't refuse to replace: looked at first.
            Some(libc::EINVAL) | Some(libc::ENOSYS) => {}
            _ => return Err(e),
        }
    }
    if !is_missing(to) {
        return Ok(false);
    }
    fs::rename(from, to).map(|()| true)
}

/// Clears away the hidden folders a GMNB that stopped while writing left
/// in `themes` (an hour old or more).
fn sweep(themes: &Path) {
    let Ok(entries) = fs::read_dir(themes) else {
        return;
    };
    for entry in entries.flatten() {
        let stale = entry
            .file_name()
            .to_str()
            .is_some_and(|n| n.starts_with(HIDDEN))
            && entry
                .metadata()
                .and_then(|m| m.modified())
                .is_ok_and(|t| t.elapsed().is_ok_and(|age| age >= STALE));
        if stale {
            let path = entry.path();
            // A link is removed, not followed (`remove_dir_all` takes
            // only the link).
            let _ = match entry.file_type() {
                Ok(kind) if kind.is_dir() => fs::remove_dir_all(&path),
                _ => fs::remove_file(&path),
            };
        }
    }
}

// ---- Reading -------------------------------------------------------------------

/// What GMNB writes for `theme`: its `theme.luau`, its files, and the
/// folders they are in.
fn embedded_tree(theme: &BuiltIn) -> Tree<'static> {
    let mut tree = Tree::new();
    let files =
        std::iter::once((THEME_FILE, theme.source.as_bytes())).chain(theme.files.iter().copied());
    for (path, bytes) in files {
        for (i, _) in path.match_indices('/') {
            tree.insert(path[..i].to_string(), None);
        }
        tree.insert(path.to_string(), Some(Cow::Borrowed(bytes)));
    }
    tree
}

/// Each path in the folder `dir` and, for a file, its size: read without
/// following a link. `None` if it isn't a folder GMNB could have written
/// (see [`folder_digest`]).
fn read_shape(dir: &Path) -> Option<BTreeMap<String, Option<u64>>> {
    if !fs::symlink_metadata(dir).ok()?.is_dir() {
        return None;
    }
    let mut shape = BTreeMap::new();
    let (mut todo, mut bytes) = (vec![String::new()], 0u64);
    while let Some(rel) = todo.pop() {
        for entry in fs::read_dir(dir.join(&rel)).ok()? {
            let entry = entry.ok()?;
            let name = entry.file_name().into_string().ok()?;
            let path = if rel.is_empty() {
                name
            } else {
                format!("{rel}/{name}")
            };
            if shape.len() >= MAX_TREE_ENTRIES {
                return None;
            }
            // (The entry's own type: a link is a link.)
            let kind = entry.file_type().ok()?;
            if kind.is_dir() {
                shape.insert(path.clone(), None);
                todo.push(path);
            } else if kind.is_file() {
                let size = entry.metadata().ok()?.len();
                bytes = bytes.checked_add(size)?;
                if bytes > MAX_TREE_BYTES {
                    return None;
                }
                shape.insert(path, Some(size));
            } else {
                return None;
            }
        }
    }
    Some(shape)
}

/// What the folder `dir` holds, read without following a link (see
/// [`folder_digest`]).
fn read_tree(dir: &Path) -> Option<Tree<'static>> {
    let shape = read_shape(dir)?;
    let mut tree = Tree::new();
    for (path, size) in shape {
        let Some(size) = size else {
            tree.insert(path, None);
            continue;
        };
        // Not through a link, and never blocking on a pipe put there
        // since its type was read.
        let file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(dir.join(&path))
            .ok()?;
        if !file.metadata().ok()?.is_file() {
            return None;
        }
        let mut bytes = Vec::with_capacity(size as usize);
        file.take(size + 1).read_to_end(&mut bytes).ok()?;
        if bytes.len() as u64 != size {
            return None;
        }
        tree.insert(path, Some(Cow::Owned(bytes)));
    }
    Some(tree)
}

fn digest_tree(tree: &Tree) -> String {
    let mut h = Sha256::default();
    for (path, bytes) in tree {
        match bytes {
            None => {
                h.update(b"d\0");
                h.update(path.as_bytes());
                h.update(b"\0");
            }
            Some(bytes) => {
                h.update(b"f\0");
                h.update(path.as_bytes());
                h.update(b"\0");
                h.update(&(bytes.len() as u64).to_le_bytes());
                h.update(bytes);
            }
        }
    }
    format!("sha256:{}", h.hex())
}

// ---- SHA-256 -------------------------------------------------------------------

/// SHA-256 (FIPS 180-4), as the record keeps it: a digest that means the
/// same to every GMNB that reads the record.
struct Sha256 {
    state: [u32; 8],
    block: [u8; 64],
    filled: usize,
    length: u64,
}

impl Default for Sha256 {
    fn default() -> Self {
        Sha256 {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            block: [0; 64],
            filled: 0,
            length: 0,
        }
    }
}

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

impl Sha256 {
    fn update(&mut self, mut data: &[u8]) {
        self.length = self.length.wrapping_add(data.len() as u64);
        while !data.is_empty() {
            let take = (64 - self.filled).min(data.len());
            self.block[self.filled..self.filled + take].copy_from_slice(&data[..take]);
            self.filled += take;
            data = &data[take..];
            if self.filled == 64 {
                let block = self.block;
                self.compress(&block);
                self.filled = 0;
            }
        }
    }

    fn compress(&mut self, block: &[u8; 64]) {
        let mut w = [0u32; 64];
        for (w, word) in w.iter_mut().zip(block.as_chunks::<4>().0) {
            *w = u32::from_be_bytes(*word);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut v = self.state;
        for (k, w) in K.iter().zip(w) {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7]
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(*k)
                .wrapping_add(w);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [
                t1.wrapping_add(t2),
                v[0],
                v[1],
                v[2],
                v[3].wrapping_add(t1),
                v[4],
                v[5],
                v[6],
            ];
        }
        for (a, b) in self.state.iter_mut().zip(v) {
            *a = a.wrapping_add(b);
        }
    }

    fn hex(mut self) -> String {
        let bits = self.length.wrapping_mul(8);
        self.update(&[0x80]);
        while self.filled != 56 {
            self.update(&[0]);
        }
        let mut tail = [0u8; 64];
        tail[..56].copy_from_slice(&self.block[..56]);
        tail[56..].copy_from_slice(&bits.to_be_bytes());
        self.compress(&tail);
        self.state.iter().map(|w| format!("{w:08x}")).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sha(data: &[u8]) -> String {
        let mut h = Sha256::default();
        h.update(data);
        h.hex()
    }

    /// The digest the record keeps is SHA-256's: the standard's vectors,
    /// and a message fed in pieces as one.
    #[test]
    fn sha256_is_sha256() {
        assert_eq!(
            sha(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        assert_eq!(
            sha(&[b'a'; 1_000_000]),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
        let data: Vec<u8> = (0..1000u32).map(|i| (i * 7) as u8).collect();
        let mut h = Sha256::default();
        for piece in data.chunks(63) {
            h.update(piece);
        }
        assert_eq!(h.hex(), sha(&data));
    }
}
