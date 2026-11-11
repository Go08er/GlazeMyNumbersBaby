//! The built-in themes in the themes folder (`gmnb_theme::install`):
//! written once, deleted for good, restored, updated only where the folder
//! is still exactly what GMNB wrote, never written over a folder of the
//! user's or through a link, and served from GMNB's own copy for a run in
//! which a folder couldn't be written (docs/theming.md, "Files and where
//! they go").

use std::fs;
use std::path::{Path, PathBuf};

use gmnb_theme::Limits;
use gmnb_theme::install::{self, Record};
use gmnb_theme::theme::{self, BUILT_IN, BuiltIn, Origin, Theme};

/// A themes folder of the test's own (gone when dropped).
struct Folder(PathBuf);

impl Folder {
    fn new(name: &str) -> Folder {
        let dir = std::env::temp_dir().join(format!("gmnb-install-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Folder(dir)
    }

    fn themes(&self) -> PathBuf {
        self.0.join("themes")
    }

    fn theme(&self, id: &str) -> PathBuf {
        self.themes().join(id)
    }

    /// The entries of the themes folder, by name.
    fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.themes())
            .map(|d| {
                d.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        // (A test may have left a folder read-only.)
        let _ = std::process::Command::new("chmod")
            .args(["-R", "u+w"])
            .arg(&self.0)
            .status();
        let _ = fs::remove_dir_all(&self.0);
    }
}

const ONE_V1: BuiltIn = BuiltIn {
    id: "one",
    source: "return { api = 1, name = 'One' }",
    files: &[
        ("fonts/One.ttf", b"a font"),
        ("fonts/OFL.txt", b"a licence"),
    ],
};
const ONE_V2: BuiltIn = BuiltIn {
    id: "one",
    source: "return { api = 1, name = 'One', description = 'Two now.' }",
    files: &[
        ("fonts/One.ttf", b"a better font"),
        ("images/new.png", b"png"),
    ],
};
const TWO: BuiltIn = BuiltIn {
    id: "two",
    source: "return { api = 1, name = 'Two' }",
    files: &[],
};

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Every file in `dir` and its text, by path (a link as "-> target").
fn contents(dir: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut todo = vec![PathBuf::new()];
    while let Some(rel) = todo.pop() {
        for entry in fs::read_dir(dir.join(&rel)).unwrap().flatten() {
            let path = rel.join(entry.file_name());
            let kind = entry.file_type().unwrap();
            let name = path.to_string_lossy().into_owned();
            if kind.is_symlink() {
                let to = fs::read_link(entry.path()).unwrap();
                out.push((name, format!("-> {}", to.display())));
            } else if kind.is_dir() {
                out.push((name, "/".into()));
                todo.push(path);
            } else {
                out.push((name, read(&entry.path())));
            }
        }
    }
    out.sort();
    out
}

/// What GMNB writes for `theme`, as [`contents`] reads it.
fn written(theme: &BuiltIn) -> Vec<(String, String)> {
    let mut out = vec![("theme.luau".to_string(), theme.source.to_string())];
    for (path, bytes) in theme.files {
        out.push((
            path.to_string(),
            String::from_utf8_lossy(bytes).into_owned(),
        ));
        let parent = Path::new(path).parent().unwrap().to_string_lossy();
        if !parent.is_empty() && !out.iter().any(|(p, _)| *p == parent) {
            out.push((parent.into_owned(), "/".into()));
        }
    }
    out.sort();
    out
}

/// The first start: each built-in theme's folder is written, exactly, and
/// recorded; the next start writes nothing.
#[test]
fn the_first_start_installs_each_built_in_once() {
    let f = Folder::new("first");
    let pass = install::install(&f.themes(), &[ONE_V1, TWO], &Record::new());
    assert_eq!(pass.written, ["one", "two"]);
    assert!(
        pass.failed.is_empty() && pass.embedded.is_empty(),
        "{pass:?}"
    );
    assert_eq!(contents(&f.theme("one")), written(&ONE_V1));
    assert_eq!(contents(&f.theme("two")), written(&TWO));
    assert_eq!(
        pass.record,
        Record::from([
            ("one".to_string(), install::digest(&ONE_V1)),
            ("two".to_string(), install::digest(&TWO)),
        ])
    );
    // Nothing hidden is left behind.
    assert_eq!(f.names(), ["one", "two"]);
    assert!(install::is_as_carried(&f.theme("one"), &ONE_V1));
    assert!(!install::is_as_carried(&f.theme("one"), &ONE_V2));
    assert_eq!(
        install::folder_digest(&f.theme("one")),
        Some(install::digest(&ONE_V1))
    );
    let again = install::install(&f.themes(), &[ONE_V1, TWO], &pass.record);
    assert_eq!(again.record, pass.record);
    assert!(
        again.written.is_empty() && again.updated.is_empty(),
        "{again:?}"
    );
}

/// A built-in theme the user deleted stays deleted: the record says it
/// was written. *Restore built-in themes* brings back what is missing and
/// nothing else; a folder that is there (the user's, or one they changed)
/// is never written to.
#[test]
fn a_deleted_theme_stays_deleted_until_restored() {
    let f = Folder::new("deleted");
    let first = install::install(&f.themes(), &[ONE_V1, TWO], &Record::new());
    fs::remove_dir_all(f.theme("one")).unwrap();
    fs::write(
        f.theme("two").join("theme.luau"),
        "return { api = 1, name = 'Mine' }",
    )
    .unwrap();
    let next = install::install(&f.themes(), &[ONE_V1, TWO], &first.record);
    assert!(next.written.is_empty(), "{next:?}");
    assert_eq!(next.record, first.record);
    assert_eq!(f.names(), ["two"]);
    assert_eq!(install::missing(&f.themes(), &[ONE_V1, TWO]), ["one"]);

    let restored = install::restore(&f.themes(), &[ONE_V1, TWO], &next.record);
    assert_eq!(restored.written, ["one"]);
    assert_eq!(contents(&f.theme("one")), written(&ONE_V1));
    assert_eq!(
        read(&f.theme("two/theme.luau")),
        "return { api = 1, name = 'Mine' }"
    );
    assert!(install::missing(&f.themes(), &[ONE_V1, TWO]).is_empty());
    // Restoring again does nothing.
    let again = install::restore(&f.themes(), &[ONE_V1, TWO], &restored.record);
    assert!(again.written.is_empty() && again.record == restored.record);
    // The whole themes folder gone: restore makes it again.
    fs::remove_dir_all(f.themes()).unwrap();
    let all = install::restore(&f.themes(), &[ONE_V1, TWO], &again.record);
    assert_eq!(all.written, ["one", "two"]);
}

/// A newer GMNB carries a built-in theme changed: a folder still exactly
/// as GMNB wrote it is replaced by the new version (and recorded); one the
/// user changed (a file edited, one added, one taken away) is theirs, and
/// is left as it is, without a word, and not looked at again.
#[test]
fn an_update_replaces_only_a_folder_as_gmnb_wrote_it() {
    let f = Folder::new("update");
    let v1 = install::install(&f.themes(), &[ONE_V1], &Record::new());
    let v2 = install::install(&f.themes(), &[ONE_V2], &v1.record);
    assert_eq!(v2.updated, ["one"]);
    assert!(v2.failed.is_empty(), "{v2:?}");
    assert_eq!(contents(&f.theme("one")), written(&ONE_V2));
    assert_eq!(v2.record["one"], install::digest(&ONE_V2));
    assert_eq!(f.names(), ["one"], "nothing hidden left behind");

    type Change = fn(&Path);
    let changes: [(&str, Change); 4] = [
        ("edited", |d| {
            fs::write(d.join("fonts/One.ttf"), "my font").unwrap()
        }),
        ("added", |d| fs::write(d.join("notes.txt"), "mine").unwrap()),
        ("taken away", |d| {
            fs::remove_file(d.join("fonts/OFL.txt")).unwrap()
        }),
        ("a folder added", |d| {
            fs::create_dir(d.join("sounds")).unwrap()
        }),
    ];
    for (what, change) in changes {
        let f = Folder::new("kept");
        let v1 = install::install(&f.themes(), &[ONE_V1], &Record::new());
        change(&f.theme("one"));
        assert!(!install::is_as_carried(&f.theme("one"), &ONE_V1), "{what}");
        let mine = contents(&f.theme("one"));
        let v2 = install::install(&f.themes(), &[ONE_V2], &v1.record);
        assert!(
            v2.updated.is_empty() && v2.failed.is_empty(),
            "{what}: {v2:?}"
        );
        assert_eq!(contents(&f.theme("one")), mine, "{what}");
        assert_eq!(v2.record["one"], install::digest(&ONE_V2), "{what}");
        assert_eq!(f.names(), ["one"], "{what}");
    }

    // Deleted before the update: it stays deleted.
    let f = Folder::new("update-deleted");
    let v1 = install::install(&f.themes(), &[ONE_V1], &Record::new());
    fs::remove_dir_all(f.theme("one")).unwrap();
    let v2 = install::install(&f.themes(), &[ONE_V2], &v1.record);
    assert!(v2.updated.is_empty() && v2.written.is_empty(), "{v2:?}");
    assert!(f.names().is_empty());
}

/// A folder with a built-in theme's id that GMNB didn't write is the
/// user's: never written to, nor recorded, whatever it is (a theme, an
/// empty folder, a file). One that is exactly what GMNB carries is taken
/// to be GMNB's (one written by another GMNB, or by one that stopped
/// before saving its settings) and recorded.
#[test]
fn a_folder_gmnb_did_not_write_is_never_written_to() {
    let f = Folder::new("own");
    fs::create_dir_all(f.theme("one")).unwrap();
    fs::write(
        f.theme("one/theme.luau"),
        "return { api = 1, name = 'Mine' }",
    )
    .unwrap();
    fs::create_dir_all(f.theme("two")).unwrap();
    let pass = install::install(&f.themes(), &[ONE_V1, TWO], &Record::new());
    assert!(
        pass.written.is_empty() && pass.record.is_empty(),
        "{pass:?}"
    );
    assert_eq!(
        contents(&f.theme("one")),
        [(
            "theme.luau".to_string(),
            "return { api = 1, name = 'Mine' }".to_string()
        )]
    );
    assert!(contents(&f.theme("two")).is_empty());
    let pass = install::restore(&f.themes(), &[ONE_V1, TWO], &pass.record);
    assert!(pass.written.is_empty(), "{pass:?}");

    let f = Folder::new("own-file");
    fs::create_dir_all(f.themes()).unwrap();
    fs::write(f.theme("one"), "a file").unwrap();
    let pass = install::install(&f.themes(), &[ONE_V1], &Record::new());
    assert!(
        pass.written.is_empty() && pass.record.is_empty(),
        "{pass:?}"
    );
    assert_eq!(read(&f.theme("one")), "a file");

    // Exactly GMNB's: recorded, and so updated later like any other.
    let f = Folder::new("adopted");
    install::install(&f.themes(), &[ONE_V1], &Record::new());
    let lost = install::install(&f.themes(), &[ONE_V1], &Record::new());
    assert!(lost.written.is_empty(), "{lost:?}");
    assert_eq!(lost.record["one"], install::digest(&ONE_V1));
    let v2 = install::install(&f.themes(), &[ONE_V2], &lost.record);
    assert_eq!(v2.updated, ["one"]);
}

/// Nothing is followed through a link planted where a theme's folder or
/// one of its files goes: a link at the folder's name is the user's (not
/// written through, nor replaced, nor taken for GMNB's even when it leads
/// to exactly GMNB's files), and a folder GMNB wrote with a link put in it
/// is no longer GMNB's (an update leaves it, and what it leads to).
#[test]
fn a_planted_link_is_not_followed() {
    let f = Folder::new("links");
    let outside = f.0.join("outside");
    fs::create_dir_all(&outside).unwrap();
    fs::create_dir_all(f.themes()).unwrap();
    std::os::unix::fs::symlink(&outside, f.theme("one")).unwrap();
    let pass = install::install(&f.themes(), &[ONE_V1], &Record::new());
    assert!(
        pass.written.is_empty() && pass.record.is_empty(),
        "{pass:?}"
    );
    assert!(contents(&outside).is_empty(), "written through the link");
    assert!(fs::symlink_metadata(f.theme("one")).unwrap().is_symlink());
    let pass = install::restore(&f.themes(), &[ONE_V1], &pass.record);
    assert!(pass.written.is_empty() && contents(&outside).is_empty());

    // A link to a folder holding exactly GMNB's files.
    let f = Folder::new("link-to-copy");
    let copy = f.0.join("copy");
    install::install(&copy, &[ONE_V1], &Record::new());
    fs::create_dir_all(f.themes()).unwrap();
    std::os::unix::fs::symlink(copy.join("one"), f.theme("one")).unwrap();
    assert_eq!(install::folder_digest(&f.theme("one")), None);
    assert!(
        !install::is_as_carried(&f.theme("one"), &ONE_V1),
        "a link to a copy"
    );
    let pass = install::install(&f.themes(), &[ONE_V1], &Record::new());
    assert!(pass.record.is_empty(), "{pass:?}");
    let forged = Record::from([("one".to_string(), install::digest(&ONE_V1))]);
    let pass = install::install(&f.themes(), &[ONE_V2], &forged);
    assert!(pass.updated.is_empty(), "{pass:?}");
    assert_eq!(contents(&copy.join("one")), written(&ONE_V1));

    // A link put inside a folder GMNB wrote.
    let f = Folder::new("link-inside");
    let secret = f.0.join("secret.txt");
    fs::write(&secret, "secret").unwrap();
    let v1 = install::install(&f.themes(), &[ONE_V1], &Record::new());
    fs::remove_file(f.theme("one/fonts/One.ttf")).unwrap();
    std::os::unix::fs::symlink(&secret, f.theme("one/fonts/One.ttf")).unwrap();
    assert_eq!(install::folder_digest(&f.theme("one")), None);
    let v2 = install::install(&f.themes(), &[ONE_V2], &v1.record);
    assert!(v2.updated.is_empty(), "{v2:?}");
    assert_eq!(read(&secret), "secret");
    assert!(
        fs::symlink_metadata(f.theme("one/fonts/One.ttf"))
            .unwrap()
            .is_symlink()
    );
}

/// A folder that can't be written (here the themes folder's place is taken
/// by a file; a read-only home or a full disk alike): GMNB's own copy of
/// the theme stands in for the run, what went wrong is said, nothing is
/// recorded, and the next start tries again. Listed, the theme is there,
/// from GMNB's copy.
#[test]
fn a_failed_write_falls_back_to_gmnbs_own_copy() {
    let f = Folder::new("failed");
    fs::write(f.themes(), "not a folder").unwrap();
    let pass = install::install(&f.themes(), &[ONE_V1, TWO], &Record::new());
    assert_eq!(pass.embedded, ["one", "two"]);
    assert_eq!(pass.failed.len(), 2, "{pass:?}");
    assert!(pass.failed[0].contains("own copy"), "{}", pass.failed[0]);
    assert!(pass.record.is_empty() && pass.written.is_empty());
    let sources = theme::discover(Some(&f.themes()), &["aurora".to_string()], None);
    assert_eq!(sources.len(), 1);
    assert_eq!(
        (sources[0].id.as_str(), sources[0].origin),
        ("aurora", Origin::BuiltIn)
    );
    // The next start, with the file gone.
    fs::remove_file(f.themes()).unwrap();
    let next = install::install(&f.themes(), &[ONE_V1, TWO], &pass.record);
    assert_eq!(next.written, ["one", "two"]);
    assert!(next.embedded.is_empty());

    // A read-only themes folder (not for root, whom it doesn't stop).
    // SAFETY: geteuid has no preconditions.
    if unsafe { libc::geteuid() } != 0 {
        let f = Folder::new("read-only");
        fs::create_dir_all(f.themes()).unwrap();
        let mode = std::os::unix::fs::PermissionsExt::from_mode(0o555);
        fs::set_permissions(f.themes(), mode).unwrap();
        let pass = install::install(&f.themes(), &[ONE_V1], &Record::new());
        assert_eq!(pass.embedded, ["one"], "{pass:?}");
        assert!(pass.record.is_empty());
        let pass = install::restore(&f.themes(), &[ONE_V1], &pass.record);
        assert_eq!(pass.embedded, ["one"], "{pass:?}");
    }
}

/// What a GMNB that stopped while writing left behind (a hidden folder in
/// the themes folder, which no list shows) is cleared away once it is an
/// hour old; one being written now is left.
#[test]
fn hidden_folders_left_behind_are_cleared() {
    let f = Folder::new("sweep");
    let (old, new) = (
        f.theme(".gmnb-new-one-1-2-3"),
        f.theme(".gmnb-new-one-4-5-6"),
    );
    for dir in [&old, &new] {
        fs::create_dir_all(dir.join("fonts")).unwrap();
        fs::write(dir.join("theme.luau"), "x").unwrap();
    }
    let hour_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(7200);
    fs::File::open(&old)
        .unwrap()
        .set_modified(hour_ago)
        .unwrap();
    install::install(&f.themes(), &[TWO], &Record::new());
    assert_eq!(f.names(), [".gmnb-new-one-4-5-6", "two"]);
}

/// The digest's form is fixed (SHA-256 over each path in order, a folder
/// as `d`, a file as `f` with its length and bytes): a record written by
/// one GMNB means the same to the next, and a theme whose files moved
/// differs from one whose bytes did.
#[test]
fn the_digest_is_fixed() {
    assert_eq!(
        install::digest(&TWO),
        "sha256:4d2d392a4f8e1bcb88e05407ecb171e979566e8f12b7376d3a86aca3ece2026f"
    );
    const MOVED: BuiltIn = BuiltIn {
        id: "one",
        source: ONE_V1.source,
        files: &[("fonts/One.ttf", b"a font"), ("OFL.txt", b"a licence")],
    };
    assert_ne!(install::digest(&MOVED), install::digest(&ONE_V1));
    assert_ne!(install::digest(&ONE_V1), install::digest(&ONE_V2));
}

/// The themes GMNB comes with, installed into a themes folder, load from
/// their folders as they do from GMNB's own copies: the same name, no
/// notes, their fonts as files of the folder and the same credits.
#[test]
fn the_built_in_themes_load_from_their_folders() {
    let f = Folder::new("real");
    let pass = install::install(&f.themes(), &BUILT_IN, &Record::new());
    assert_eq!(pass.written.len(), BUILT_IN.len(), "{pass:?}");
    let sources = theme::discover(Some(&f.themes()), &[], None);
    let ids: Vec<&str> = sources.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["aurora", "sprinkles", "swirls"]);
    let limits = Limits {
        load: std::time::Duration::from_secs(20),
        ..Limits::default()
    };
    for source in sources {
        assert_eq!(source.origin, Origin::Folder);
        let id = source.id.clone();
        let folder = Theme::load(source, limits.clone()).unwrap();
        let own = Theme::load(theme::Source::built_in(&id).unwrap(), limits.clone()).unwrap();
        assert_eq!(folder.def.meta, own.def.meta, "{id}");
        assert!(folder.notes.is_empty(), "{id}: {:?}", folder.notes);
        assert_eq!(folder.def.fonts, own.def.fonts, "{id}");
        for font in folder.font_files() {
            assert!(matches!(font, theme::FontFile::Path(_)), "{id}");
        }
        assert_eq!(folder.font_files().len(), own.font_files().len(), "{id}");
        assert_eq!(folder.credits(), own.credits(), "{id}");
    }
}

/// A hostile themes folder with a great many theme folders (each one a
/// theme that never finishes loading) must not hold Settings' list for
/// minutes: the list is bounded in how many folders it reads and in the
/// time it spends on them.
#[test]
fn a_folder_of_many_hostile_themes_is_listed_in_bounded_time() {
    let f = Folder::new("many");
    for n in 0..400 {
        let dir = f.theme(&format!("spin-{n}"));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("theme.luau"), "while true do end").unwrap();
    }
    let start = std::time::Instant::now();
    let sources = theme::discover(Some(&f.themes()), &[], None);
    let listed = theme::list(&sources, &Limits::default());
    let took = start.elapsed();
    assert!(
        took < std::time::Duration::from_secs(8),
        "listing {} hostile themes took {took:?}",
        listed.len()
    );
    assert!(listed.iter().all(|l| l.meta.is_err()));
}
