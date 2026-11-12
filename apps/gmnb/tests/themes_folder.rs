//! The themes folder in the running app (docs/theming.md, "Files and
//! where they go"): the built-in themes written into it once, deleted for
//! good and restored from Settings; themes dragged in, or deleted, while
//! Settings is open, shown without a restart; nothing watching the folder
//! once Settings is closed; and *Open themes folder* opening it in the app
//! the user's `mimeapps.list` names for folders. GMNB is started on a
//! private compositor, driven by `GMNB_STEPS`.
//!
//! It needs a display: `GMNB_TEST_WAYLAND_DISPLAY` (or
//! `DGMNB_TEST_WAYLAND_DISPLAY`, which the workspace's tests are run
//! with) names a compositor (a headless weston's socket); without one the
//! test is skipped.
//!
//! **Nothing here can reach the desktop's file manager or its portal.**
//! Each run has a profile of its own (its own XDG folders, no
//! `XDG_CONFIG_DIRS` of the system's, no sound) and either no session bus
//! or a private one with no services but the test's stand-ins. A run that
//! presses *Open themes folder* is started in a mount namespace of its own
//! in which every folder of apps outside the profile is hidden behind an
//! empty one: the system's, the user's, those `XDG_DATA_DIRS` names, and
//! those NixOS's GLib reads whatever `XDG_DATA_DIRS` says (setting it alone
//! hid nothing there: GLib still found the desktop's editor for folders,
//! and it was started). Before GMNB starts there, `gio mime` is asked, in
//! the same namespace, which apps it finds for folders: the test's own
//! (which writes down what it was asked to open) or none, or the test
//! stops before anything is pressed.

use std::io::BufRead;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gtk::prelude::*;
use gtk::{gio, glib};

/// The compositor to run on, as an absolute path (each run has its own
/// runtime folder, which a socket's bare name would be looked for in).
fn display() -> Option<PathBuf> {
    let named = std::env::var_os("GMNB_TEST_WAYLAND_DISPLAY")
        .or_else(|| std::env::var_os("DGMNB_TEST_WAYLAND_DISPLAY"))?;
    let named = PathBuf::from(named);
    if named.is_absolute() {
        return Some(named);
    }
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")?;
    Some(Path::new(&runtime).join(named))
}

macro_rules! display_or_skip {
    () => {
        match display() {
            Some(d) => d,
            None => {
                eprintln!("skipped: no GMNB_TEST_WAYLAND_DISPLAY or DGMNB_TEST_WAYLAND_DISPLAY");
                return;
            }
        }
    };
}

/// A profile of one test's own: XDG folders, a runtime folder, a folder of
/// apps, and (if it asks) an app for folders that writes down what it was
/// asked to open.
struct Profile(PathBuf);

impl Profile {
    fn new(name: &str) -> Profile {
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "gmnb-folder-{}-{name}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        for sub in [
            "config", "data", "cache", "state", "run", "etc-xdg", "bin", "services", "empty",
        ] {
            std::fs::create_dir_all(dir.join(sub)).unwrap();
        }
        let run = dir.join("run");
        std::fs::set_permissions(&run, std::os::unix::fs::PermissionsExt::from_mode(0o700))
            .unwrap();
        Profile(dir)
    }

    fn path(&self, sub: &str) -> PathBuf {
        self.0.join(sub)
    }

    /// The themes folder, as GMNB makes it.
    fn themes(&self) -> PathBuf {
        self.path("data/gmnb/themes")
    }

    /// What the test's app for folders was asked to open, a line each.
    fn opened(&self) -> PathBuf {
        self.path("opened.txt")
    }

    /// The test's app for folders' own limits (`/proc/self/limits`), as it
    /// was started with them.
    fn its_limits(&self) -> PathBuf {
        self.path("limits.txt")
    }

    /// Installs the test's app for folders, and names it the default for
    /// folders in the profile's `mimeapps.list`.
    fn with_folder_app(self) -> Profile {
        let script = self.path("bin/test-files");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\ncat /proc/self/limits > '{}'\nprintf '%s\\n' \"$@\" >> '{}'\n",
                self.its_limits().display(),
                self.opened().display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
        let apps = self.path("data/applications");
        std::fs::create_dir_all(&apps).unwrap();
        std::fs::write(
            apps.join("gmnb-test-files.desktop"),
            format!(
                "[Desktop Entry]\nType=Application\nName=Test files\nExec={} %f\n\
                 MimeType=inode/directory;\n",
                script.display()
            ),
        )
        .unwrap();
        std::fs::write(
            self.path("config/mimeapps.list"),
            "[Default Applications]\ninode/directory=gmnb-test-files.desktop\n",
        )
        .unwrap();
        self
    }
}

impl Drop for Profile {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A private session bus with no services but what the test registers:
/// `org.freedesktop.portal.Desktop` can be listed as one that starts on
/// demand (as a desktop's portal is), its starter doing nothing.
struct Bus {
    daemon: Child,
    addr: String,
}

impl Bus {
    /// `None` (and the test skips) without a working `dbus-daemon`.
    fn start(profile: &Profile) -> Option<Bus> {
        std::fs::write(
            profile.path("services/org.freedesktop.portal.Desktop.service"),
            "[D-BUS Service]\nName=org.freedesktop.portal.Desktop\nExec=/bin/false\n",
        )
        .ok()?;
        let conf = profile.path("bus.conf");
        std::fs::write(
            &conf,
            format!(
                r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-BUS Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:path={}</listen>
  <auth>EXTERNAL</auth>
  <servicedir>{}</servicedir>
  <policy context="default">
    <allow send_destination="*"/>
    <allow receive_sender="*"/>
    <allow own="*"/>
  </policy>
</busconfig>"#,
                profile.path("bus").display(),
                profile.path("services").display()
            ),
        )
        .ok()?;
        let Ok(mut daemon) = Command::new("dbus-daemon")
            .arg(format!("--config-file={}", conf.display()))
            .args(["--nofork", "--print-address=1"])
            .env_remove("DBUS_SESSION_BUS_ADDRESS")
            .stdout(Stdio::piped())
            .spawn()
        else {
            eprintln!("skipped: no dbus-daemon");
            return None;
        };
        let mut addr = String::new();
        let _ = std::io::BufReader::new(daemon.stdout.take()?).read_line(&mut addr);
        let addr = addr.trim().to_string();
        if addr.is_empty() {
            eprintln!("skipped: dbus-daemon didn't start");
            let _ = daemon.kill();
            return None;
        }
        Some(Bus { daemon, addr })
    }
}

impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
    }
}

/// A stand-in desktop portal on the test's bus: it owns the portal's name
/// and has the OpenURI interface at version 4, which GTK 4.22's file
/// launcher takes to mean "use the portal" even outside a sandbox. It
/// answers nothing (every call fails) and writes down every call and every
/// property asked of it.
struct Portal {
    asked: Arc<Mutex<Vec<String>>>,
    main: glib::MainLoop,
    thread: Option<std::thread::JoinHandle<()>>,
}

const OPEN_URI: &str = r#"<node>
  <interface name="org.freedesktop.portal.OpenURI">
    <method name="OpenURI">
      <arg type="s" direction="in"/><arg type="s" direction="in"/>
      <arg type="a{sv}" direction="in"/><arg type="o" direction="out"/>
    </method>
    <method name="OpenFile">
      <arg type="s" direction="in"/><arg type="h" direction="in"/>
      <arg type="a{sv}" direction="in"/><arg type="o" direction="out"/>
    </method>
    <method name="OpenDirectory">
      <arg type="s" direction="in"/><arg type="h" direction="in"/>
      <arg type="a{sv}" direction="in"/><arg type="o" direction="out"/>
    </method>
    <property name="version" type="u" access="read"/>
  </interface>
</node>"#;

impl Portal {
    fn start(addr: &str) -> Portal {
        let asked = Arc::new(Mutex::new(Vec::new()));
        let (tx, rx) = std::sync::mpsc::channel();
        let (addr, log) = (addr.to_string(), asked.clone());
        let thread = std::thread::spawn(move || {
            let context = glib::MainContext::new();
            context
                .with_thread_default(|| {
                    let conn = gio::DBusConnection::for_address_sync(
                        &addr,
                        gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
                            | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
                        None,
                        gio::Cancellable::NONE,
                    )
                    .expect("the stand-in portal on the bus");
                    let node = gio::DBusNodeInfo::for_xml(OPEN_URI).unwrap();
                    let interface = node
                        .lookup_interface("org.freedesktop.portal.OpenURI")
                        .unwrap();
                    let (calls, gets) = (log.clone(), log.clone());
                    let _registered = conn
                        .register_object("/org/freedesktop/portal/desktop", &interface)
                        .method_call(move |_, _, _, interface, method, _, invocation| {
                            calls
                                .lock()
                                .unwrap()
                                .push(format!("{}.{method}", interface.unwrap_or("")));
                            invocation.return_dbus_error(
                                "org.freedesktop.DBus.Error.Failed",
                                "a stand-in portal opens nothing",
                            );
                        })
                        .property(move |_, _, _, interface, property| {
                            gets.lock()
                                .unwrap()
                                .push(format!("get {interface}.{property}"));
                            4u32.to_variant()
                        })
                        .build()
                        .unwrap();
                    conn.call_sync(
                        Some("org.freedesktop.DBus"),
                        "/org/freedesktop/DBus",
                        "org.freedesktop.DBus",
                        "RequestName",
                        Some(&("org.freedesktop.portal.Desktop", 4u32).to_variant()),
                        None,
                        gio::DBusCallFlags::NONE,
                        5000,
                        gio::Cancellable::NONE,
                    )
                    .expect("the portal's name");
                    let main = glib::MainLoop::new(Some(&context), false);
                    tx.send(main.clone()).unwrap();
                    main.run();
                })
                .unwrap();
        });
        let main = rx.recv().expect("the stand-in portal started");
        Portal {
            asked,
            main,
            thread: Some(thread),
        }
    }

    fn asked(&self) -> Vec<String> {
        self.asked.lock().unwrap().clone()
    }

    fn forget(&self) {
        self.asked.lock().unwrap().clear();
    }
}

impl Drop for Portal {
    fn drop(&mut self) {
        self.main.quit();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Every folder of apps outside the test's own that GLib might read: the
/// system's and the user's, those of `XDG_DATA_DIRS` and `NIX_PROFILES`,
/// and those NixOS's GLib reads whatever `XDG_DATA_DIRS` says. Each as it
/// really is (links followed), once.
fn app_dirs() -> Vec<PathBuf> {
    let user = std::env::var("USER").unwrap_or_default();
    let home = std::env::var("HOME").unwrap_or_default();
    let mut shares: Vec<PathBuf> = [
        "/usr/share",
        "/usr/local/share",
        "/run/current-system/sw/share",
        "/nix/var/nix/profiles/system/sw/share",
        "/nix/var/nix/profiles/default/share",
        "/var/lib/flatpak/exports/share",
    ]
    .iter()
    .map(PathBuf::from)
    .collect();
    for at in [
        format!("/etc/profiles/per-user/{user}/share"),
        format!("/nix/var/nix/profiles/per-user/{user}/share"),
        format!("/nix/var/nix/profiles/per-user/{user}/profile/share"),
        format!("{home}/.local/share"),
        format!("{home}/.nix-profile/share"),
        format!("{home}/.local/state/nix/profile/share"),
        format!("{home}/.local/share/flatpak/exports/share"),
    ] {
        shares.push(at.into());
    }
    let listed = std::env::var("XDG_DATA_DIRS").unwrap_or_default();
    shares.extend(
        listed
            .split(':')
            .filter(|d| !d.is_empty())
            .map(PathBuf::from),
    );
    let profiles = std::env::var("NIX_PROFILES").unwrap_or_default();
    shares.extend(
        profiles
            .split_whitespace()
            .map(|p| Path::new(p).join("share")),
    );
    let mut out: Vec<PathBuf> = Vec::new();
    for share in shares {
        let Ok(apps) = std::fs::canonicalize(share.join("applications")) else {
            continue;
        };
        if apps.is_dir() && !out.contains(&apps) {
            out.push(apps);
        }
    }
    out
}

/// The util-linux `mount` from `PATH` (not a setuid wrapper, which
/// refuses in a namespace).
fn plain_mount() -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join("mount"))
        .filter_map(|m| std::fs::canonicalize(m).ok())
        .find(|m| {
            std::fs::metadata(m)
                .is_ok_and(|md| md.is_file() && md.permissions().mode() & 0o4000 == 0)
        })
}

/// `program` with `args`, to be started where every folder of
/// [`app_dirs`] is an empty one: in a mount namespace of its own (a user
/// namespace for the mounts, and in it one that maps the user back, so
/// that inside it the user is still the user). `None` if that can't be
/// set up here.
fn masked(profile: &Profile, program: &Path, args: &[&str]) -> Option<Command> {
    const SCRIPT: &str = r#"mount=$1 empty=$2 uid=$3 gid=$4 n=$5; shift 5
while [ "$n" -gt 0 ]; do "$mount" --bind "$empty" "$1" || exit 97; shift; n=$((n - 1)); done
exec unshare --user --map-user="$uid" --map-group="$gid" -- "$@""#;
    let mount = plain_mount()?;
    let empty = profile.path("empty");
    let me = std::fs::metadata(&empty).ok()?;
    let dirs = app_dirs();
    let mut cmd = Command::new("unshare");
    cmd.args([
        "--user",
        "--map-root-user",
        "--mount",
        "--",
        "/bin/sh",
        "-c",
        SCRIPT,
        "sh",
    ])
    .arg(mount)
    .arg(&empty)
    .arg(me.uid().to_string())
    .arg(me.gid().to_string())
    .arg(dirs.len().to_string())
    .args(&dirs)
    .arg(program)
    .args(args);
    Some(cmd)
}

/// What GIO finds for folders where a masked run starts (`gio mime
/// inode/directory` there, with the run's environment, in English): the
/// default app, and every app registered. First, there, every folder that
/// is to be hidden is looked into: each must be empty. `None` if that
/// can't be asked here; it stops the test if a folder isn't hidden or GIO
/// says something this doesn't read.
fn apps_for_folders(profile: &Profile) -> Option<(Option<String>, Vec<String>)> {
    const LOOK: &str = r#"for d; do printf 'entries %s\n' "$(ls -A "$d" | wc -l)"; done
LC_ALL=C LANGUAGE=C exec gio mime inode/directory"#;
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find(|d| d.join("gio").is_file())?;
    let dirs: Vec<String> = app_dirs().iter().map(|d| d.display().to_string()).collect();
    let mut args = vec!["-c", LOOK, "sh"];
    args.extend(dirs.iter().map(String::as_str));
    let mut cmd = masked(profile, Path::new("/bin/sh"), &args)?;
    isolate(&mut cmd, profile);
    let out = cmd.stderr(Stdio::null()).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let entries: Vec<&str> = text.lines().filter(|l| l.starts_with("entries ")).collect();
    assert_eq!(entries.len(), dirs.len(), "{text}");
    assert!(
        entries.iter().all(|l| *l == "entries 0"),
        "a folder of apps isn't hidden: {dirs:?}\n{text}"
    );
    assert!(
        text.contains("Default application for") || text.contains("No default applications for"),
        "what GIO said isn't read here: {text}"
    );
    let default = text
        .lines()
        .find(|l| l.starts_with("Default application"))
        .and_then(|l| l.rsplit_once(": "))
        .map(|(_, app)| app.trim().to_string());
    let mut apps = Vec::new();
    let mut listing = false;
    for line in text.lines() {
        if line.starts_with("Registered applications") {
            listing = true;
        } else if listing && line.starts_with('\t') {
            apps.push(line.trim().to_string());
        } else {
            listing = false;
        }
    }
    Some((default, apps))
}

/// The environment of every run: the profile's folders, nothing of the
/// session's, and nothing that would send GTK or GLib to a portal of
/// their own accord.
fn isolate(cmd: &mut Command, profile: &Profile) {
    for (name, _) in std::env::vars_os() {
        let name = name.to_string_lossy().to_string();
        if name.starts_with("GMNB_") || name.starts_with("PULSE_") {
            cmd.env_remove(&name);
        }
    }
    for name in [
        "DISPLAY",
        "WAYLAND_SOCKET",
        "DBUS_SESSION_BUS_ADDRESS",
        "AT_SPI_BUS_ADDRESS",
        "GDK_DEBUG",
        "GIO_USE_PORTALS",
        "GTK_USE_PORTAL",
    ] {
        cmd.env_remove(name);
    }
    cmd.env("XDG_RUNTIME_DIR", profile.path("run"))
        .env("XDG_CONFIG_HOME", profile.path("config"))
        .env("XDG_DATA_HOME", profile.path("data"))
        .env("XDG_CACHE_HOME", profile.path("cache"))
        .env("XDG_STATE_HOME", profile.path("state"))
        .env("XDG_CONFIG_DIRS", profile.path("etc-xdg"));
}

/// A GMNB running, its stderr gathered as it comes.
struct Running {
    child: Child,
    err: Arc<Mutex<String>>,
    reader: Option<std::thread::JoinHandle<()>>,
}

impl Running {
    /// GMNB in `profile`, with `env` (and `bus`, else no session bus).
    fn start(
        display: &Path,
        profile: &Profile,
        bus: Option<&Bus>,
        env: &[(&str, &str)],
    ) -> Running {
        let cmd = Command::new(env!("CARGO_BIN_EXE_gmnb"));
        Running::spawn(cmd, display, profile, bus, env)
    }

    /// The same, where no app but the profile's own can be found for
    /// folders ([`masked`]): `None` (the test skips) if that can't be set
    /// up here. Before GMNB starts, the test stops if GIO finds there any
    /// app for folders but `expected` (the default first).
    fn start_masked(
        display: &Path,
        profile: &Profile,
        bus: Option<&Bus>,
        env: &[(&str, &str)],
        expected: &[&str],
    ) -> Option<Running> {
        let Some((default, apps)) = apps_for_folders(profile) else {
            eprintln!("skipped: no namespace to hide the desktop's apps in (unshare, mount, gio)");
            return None;
        };
        // (An app only named in `mimeapps.list` isn't listed as
        // registered: the default is what GMNB asks for.)
        assert!(
            apps.iter().all(|a| expected.contains(&a.as_str())),
            "the apps for folders GMNB would find: {apps:?}"
        );
        assert_eq!(
            default.as_deref(),
            expected.first().copied(),
            "the app for folders GMNB would start"
        );
        let cmd = masked(profile, Path::new(env!("CARGO_BIN_EXE_gmnb")), &[])?;
        Some(Running::spawn(cmd, display, profile, bus, env))
    }

    fn spawn(
        mut cmd: Command,
        display: &Path,
        profile: &Profile,
        bus: Option<&Bus>,
        env: &[(&str, &str)],
    ) -> Running {
        isolate(&mut cmd, profile);
        cmd.env("WAYLAND_DISPLAY", display)
            .env("GDK_BACKEND", "wayland")
            .env("GSK_RENDERER", "cairo")
            .env("GTK_A11Y", "none")
            .env("NO_AT_BRIDGE", "1")
            .env("GSETTINGS_BACKEND", "memory")
            .env("GMNB_SOUND", "null")
            // No accent asked of a portal.
            .env("GMNB_ACCENT", "none")
            // In case the steps never get to `quit`.
            .env("GMNB_AUTOCLOSE_MS", "40000");
        if let Some(bus) = bus {
            cmd.env("DBUS_SESSION_BUS_ADDRESS", &bus.addr);
        }
        for (k, v) in env {
            cmd.env(k, v);
        }
        let mut child = cmd
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("GMNB started");
        let stderr = child.stderr.take().unwrap();
        let err = Arc::new(Mutex::new(String::new()));
        let gathered = err.clone();
        let reader = std::thread::spawn(move || {
            for line in std::io::BufReader::new(stderr).lines() {
                let Ok(line) = line else { break };
                let mut err = gathered.lock().unwrap();
                err.push_str(&line);
                err.push('\n');
            }
        });
        Running {
            child,
            err,
            reader: Some(reader),
        }
    }

    fn stderr(&self) -> String {
        self.err.lock().unwrap().clone()
    }

    /// The `n`th line (from 1) that starts with `start`, once there is one.
    fn line(&self, start: &str, n: usize) -> String {
        let deadline = Instant::now() + Duration::from_secs(40);
        loop {
            let found = self
                .stderr()
                .lines()
                .filter(|l| l.starts_with(start))
                .nth(n - 1)
                .map(str::to_string);
            if let Some(line) = found {
                return line;
            }
            assert!(
                Instant::now() < deadline,
                "no line {n} starting {start:?}: {}",
                self.stderr()
            );
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    /// Waits for GMNB to end (or ends it): all it wrote to stderr.
    fn finish(mut self) -> String {
        let deadline = Instant::now() + Duration::from_secs(60);
        while self.child.try_wait().unwrap().is_none() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(r) = self.reader.take() {
            let _ = r.join();
        }
        self.stderr()
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// What the profile's app for folders was asked to open, once it has been
/// asked (or after a while: nothing).
fn opened(profile: &Profile, wait: Duration) -> Vec<String> {
    let deadline = Instant::now() + wait;
    loop {
        if let Ok(text) = std::fs::read_to_string(profile.opened()) {
            return text.lines().map(str::to_string).collect();
        }
        if Instant::now() >= deadline {
            return Vec::new();
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// *Open themes folder* opens the themes folder in the app the user's
/// `mimeapps.list` names for folders (`inode/directory`), asked of GIO by
/// GMNB itself, outside a sandbox. GTK's own launcher (4.22) would hand
/// the folder to the desktop portal instead whenever the session bus has
/// one: here the bus has one (a stand-in GTK would take for the desktop's),
/// and neither GMNB nor GTK asks it anything of OpenURI.
#[test]
fn the_folder_button_opens_the_folder_in_the_app_for_folders() {
    let display = display_or_skip!();
    let profile = Profile::new("open").with_folder_app();
    let Some(bus) = Bus::start(&profile) else {
        return;
    };
    let portal = Portal::start(&bus.addr);
    // The bus is as a desktop's: the portal is listed as one that starts
    // on demand, and asked, says its OpenURI is version 4.
    {
        let conn = gio::DBusConnection::for_address_sync(
            &bus.addr,
            gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
                | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
            None,
            gio::Cancellable::NONE,
        )
        .unwrap();
        let call = |iface: &str, method: &str, dest: &str, path: &str, args: glib::Variant| {
            conn.call_sync(
                Some(dest),
                path,
                iface,
                method,
                Some(&args),
                None,
                gio::DBusCallFlags::NONE,
                5000,
                gio::Cancellable::NONE,
            )
            .unwrap()
        };
        let names = call(
            "org.freedesktop.DBus",
            "ListActivatableNames",
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            ().to_variant(),
        );
        let names: Vec<String> = names.child_value(0).get().unwrap();
        assert!(
            names.iter().any(|n| n == "org.freedesktop.portal.Desktop"),
            "{names:?}"
        );
        let version = call(
            "org.freedesktop.DBus.Properties",
            "Get",
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            ("org.freedesktop.portal.OpenURI", "version").to_variant(),
        );
        let version = version.child_value(0).as_variant().unwrap().get::<u32>();
        assert_eq!(version, Some(4));
        portal.forget();
    }
    let Some(app) = Running::start_masked(
        &display,
        &profile,
        Some(&bus),
        &[
            ("GMNB_PREFS", "1"),
            ("GMNB_STEPS", "wait:300;press:wc-open-themes;wait:1500;quit"),
        ],
        &["gmnb-test-files.desktop"],
    ) else {
        return;
    };
    let err = app.finish();
    let asked = opened(&profile, Duration::from_secs(10));
    assert_eq!(
        asked,
        [profile.themes().display().to_string()],
        "the app for folders wasn't asked to open the themes folder: {err}"
    );
    let portal_asked = portal.asked();
    assert!(
        !portal_asked.iter().any(|a| a.contains("OpenURI")),
        "the portal was asked: {portal_asked:?}"
    );
    assert!(!err.contains("Couldn't open the themes folder"), "{err}");
    // The app was started with the limits GMNB was started with, not
    // GMNB's memory ceiling (4 GiB of `RLIMIT_DATA`), which is GMNB's own.
    let data = |limits: &str| {
        limits
            .lines()
            .find(|l| l.starts_with("Max data size"))
            .map(|l| {
                l.split_whitespace()
                    .skip(3)
                    .take(2)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
    };
    let ours = std::fs::read_to_string("/proc/self/limits").unwrap();
    let its = std::fs::read_to_string(profile.its_limits()).unwrap_or_default();
    assert_eq!(
        data(&its),
        data(&ours),
        "the app for folders' limits: {its}"
    );
}

/// With no app for folders (none named, none there at all), *Open themes
/// folder* says so, and where the folder is.
#[test]
fn without_an_app_for_folders_the_button_says_where_the_folder_is() {
    let display = display_or_skip!();
    let profile = Profile::new("no-app");
    let Some(app) = Running::start_masked(
        &display,
        &profile,
        None,
        &[
            ("GMNB_PREFS", "1"),
            ("GMNB_STEPS", "wait:300;press:wc-open-themes;wait:500;quit"),
        ],
        &[],
    ) else {
        return;
    };
    let err = app.finish();
    let said = format!(
        "gmnb: Couldn't open the themes folder: no app is set to open folders. It is {}",
        profile.themes().display()
    );
    assert!(err.lines().any(|l| l == said), "{err}");
    assert!(opened(&profile, Duration::ZERO).is_empty());
}

/// What Settings' `themes` step says of the list.
fn themes_line(line: &str) -> (Vec<String>, bool) {
    let rest = line.strip_prefix("themes: ").unwrap_or(line);
    let (rows, restore) = rest.split_once("; restore ").unwrap_or((rest, ""));
    (
        rows.split_whitespace().map(str::to_string).collect(),
        restore == "shown",
    )
}

/// The built-in themes are written into the themes folder the first time
/// GMNB starts, and recorded; one deleted is not written again. Chosen and
/// deleted, GMNB starts in the base look and says so. Settings marks the
/// ones GMNB comes with, offers *Restore built-in themes* while one is
/// missing, and restoring brings it back (and the chosen theme with it).
#[test]
fn built_in_themes_are_installed_once_and_restored_from_settings() {
    let display = display_or_skip!();
    let profile = Profile::new("installed");
    let err = Running::start(&display, &profile, None, &[("GMNB_STEPS", "quit")]).finish();
    let mut names: Vec<String> = std::fs::read_dir(profile.themes())
        .unwrap_or_else(|e| panic!("no themes folder ({e}): {err}"))
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["aurora", "sprinkles", "swirls"], "{err}");
    let settings = std::fs::read_to_string(profile.path("config/gmnb/settings.json")).unwrap();
    let settings: serde_json::Value = serde_json::from_str(&settings).unwrap();
    for id in ["aurora", "sprinkles", "swirls"] {
        let digest = settings["built_in_themes"][id].as_str().unwrap_or("");
        assert!(digest.starts_with("sha256:"), "{id}: {settings}");
    }

    std::fs::remove_dir_all(profile.themes().join("aurora")).unwrap();
    let app = Running::start(
        &display,
        &profile,
        None,
        &[
            ("GMNB_THEME", "aurora"),
            ("GMNB_PREFS", "1"),
            (
                "GMNB_STEPS",
                "wait:300;themes;press:wc-restore-themes;wait:800;themes;quit",
            ),
        ],
    );
    let before = themes_line(&app.line("themes: ", 1));
    let after = themes_line(&app.line("themes: ", 2));
    let err = app.finish();
    assert!(
        err.lines()
            .any(|l| l == "gmnb: Theme “aurora” isn't installed"),
        "{err}"
    );
    // Aurora wasn't written again: missing, it is listed as the choice that
    // isn't installed.
    assert_eq!(
        before,
        (
            ["base", "sprinkles*", "swirls*", "aurora"]
                .map(String::from)
                .to_vec(),
            true
        ),
        "{err}"
    );
    assert_eq!(
        after,
        (
            ["base", "aurora*", "sprinkles*", "swirls*"]
                .map(String::from)
                .to_vec(),
            false
        ),
        "{err}"
    );
    assert!(profile.themes().join("aurora/theme.luau").is_file());
    // Restored, the chosen theme was loaded: no note of it missing after.
    let restored_at = err.find("themes: ").unwrap();
    assert_eq!(
        err[restored_at..].matches("isn't installed").count(),
        0,
        "{err}"
    );
}

/// The themes folder can't be written (its place is taken by a file; a
/// read-only home or a full disk alike): GMNB says so on stderr, once for
/// each built-in theme, and GMNB's own copies stand in for the run, listed
/// and marked as ever, the chosen one loaded. Nothing is recorded, so the
/// next start tries again.
#[test]
fn built_in_themes_that_cant_be_written_are_served_from_gmnbs_copies() {
    let display = display_or_skip!();
    let profile = Profile::new("unwritable");
    std::fs::create_dir_all(profile.path("data/gmnb")).unwrap();
    std::fs::write(profile.themes(), "not a folder").unwrap();
    let app = Running::start(
        &display,
        &profile,
        None,
        &[
            ("GMNB_THEME", "aurora"),
            ("GMNB_PREFS", "1"),
            ("GMNB_STEPS", "wait:300;themes;quit"),
        ],
    );
    let listed = themes_line(&app.line("themes: ", 1));
    let err = app.finish();
    assert_eq!(
        listed.0,
        ["base", "aurora*", "sprinkles*", "swirls*"],
        "{err}"
    );
    let said = err
        .lines()
        .filter(|l| l.contains("GMNB's own copy of the theme is used for this run"))
        .count();
    assert_eq!(said, 3, "{err}");
    assert!(!err.contains("isn't installed"), "{err}");
    assert!(!err.contains("couldn't be loaded"), "{err}");
    let settings =
        std::fs::read_to_string(profile.path("config/gmnb/settings.json")).unwrap_or_default();
    assert!(!settings.contains("sha256:"), "{settings}");
}

/// While Settings is open, a theme copied into the themes folder (its
/// folder first, its file a moment later, as a file manager copies) is
/// listed, and a built-in theme deleted there leaves the list; the chosen
/// theme deleted gives way to the base look, and GMNB says so.
#[test]
fn themes_dragged_in_or_deleted_show_while_settings_is_open() {
    let display = display_or_skip!();
    let profile = Profile::new("dragged");
    let app = Running::start(
        &display,
        &profile,
        None,
        &[
            ("GMNB_THEME", "aurora"),
            ("GMNB_PREFS", "1"),
            (
                "GMNB_STEPS",
                "wait:300;themes;wait:3000;themes;wait:3000;themes;quit",
            ),
        ],
    );
    let first = themes_line(&app.line("themes: ", 1));
    assert_eq!(
        first.0,
        ["base", "aurora*", "sprinkles*", "swirls*"],
        "{}",
        app.stderr()
    );
    // A theme copied in, and a built-in theme deleted.
    let zebra = profile.themes().join("zebra");
    std::fs::create_dir(&zebra).unwrap();
    std::thread::sleep(Duration::from_millis(150));
    std::fs::write(
        zebra.join("theme.luau"),
        "return { api = 1, name = 'Zebra' }",
    )
    .unwrap();
    std::fs::remove_dir_all(profile.themes().join("sprinkles")).unwrap();
    let second = themes_line(&app.line("themes: ", 2));
    assert_eq!(
        second,
        (
            ["base", "aurora*", "swirls*", "zebra"]
                .map(String::from)
                .to_vec(),
            true
        ),
        "{}",
        app.stderr()
    );
    assert!(!app.stderr().contains("isn't installed"));
    // The chosen theme deleted.
    std::fs::remove_dir_all(profile.themes().join("aurora")).unwrap();
    let third = themes_line(&app.line("themes: ", 3));
    let err = app.finish();
    assert_eq!(third.0, ["base", "swirls*", "zebra", "aurora"], "{err}");
    assert!(
        err.lines()
            .any(|l| l == "gmnb: Theme “aurora” isn't installed"),
        "{err}"
    );
}

/// R5-C-1: a chosen theme whose folder was deleted stays in Settings' list
/// as the choice ("Doesn't load: Not installed"); choosing another theme
/// left that row there, a phantom nobody could use, until the folder next
/// changed. Choosing another theme takes it out; the other rows stay.
#[test]
fn choosing_another_theme_takes_the_row_of_one_not_installed_out() {
    let display = display_or_skip!();
    let profile = Profile::new("phantom");
    let app = Running::start(
        &display,
        &profile,
        None,
        &[
            ("GMNB_THEME", "aurora"),
            ("GMNB_PREFS", "1"),
            (
                "GMNB_STEPS",
                "wait:300;themes;wait:3000;themes;choose:swirls;wait:600;themes;quit",
            ),
        ],
    );
    let first = themes_line(&app.line("themes: ", 1));
    assert_eq!(
        first.0,
        ["base", "aurora*", "sprinkles*", "swirls*"],
        "{}",
        app.stderr()
    );
    std::fs::remove_dir_all(profile.themes().join("aurora")).unwrap();
    let second = themes_line(&app.line("themes: ", 2));
    assert_eq!(
        second.0,
        ["base", "sprinkles*", "swirls*", "aurora"],
        "{}",
        app.stderr()
    );
    let third = themes_line(&app.line("themes: ", 3));
    let err = app.finish();
    assert_eq!(third.0, ["base", "sprinkles*", "swirls*"], "{err}");
}

/// The inotify watches process `pid` holds: each watched inode.
fn watched(pid: u32) -> Vec<u64> {
    let mut out = Vec::new();
    let Ok(fds) = std::fs::read_dir(format!("/proc/{pid}/fdinfo")) else {
        return out;
    };
    for fd in fds.flatten() {
        let Ok(info) = std::fs::read_to_string(fd.path()) else {
            continue;
        };
        for line in info.lines().filter(|l| l.starts_with("inotify wd:")) {
            let ino = line
                .split_whitespace()
                .find_map(|f| f.strip_prefix("ino:"))
                .and_then(|h| u64::from_str_radix(h, 16).ok());
            out.extend(ino);
        }
    }
    out
}

/// The context switches of all of process `pid`'s threads: how often any
/// of them slept and woke, or was put aside.
fn switches(pid: u32) -> u64 {
    let Ok(tasks) = std::fs::read_dir(format!("/proc/{pid}/task")) else {
        return 0;
    };
    tasks
        .flatten()
        .filter_map(|t| std::fs::read_to_string(t.path().join("status")).ok())
        .flat_map(|s| {
            s.lines()
                .filter(|l| l.contains("ctxt_switches"))
                .filter_map(|l| l.split_whitespace().nth(1)?.parse::<u64>().ok())
                .collect::<Vec<_>>()
        })
        .sum()
}

/// Settings watches the themes folder and each theme's folder while it is
/// open, and once it is closed nothing does (no inotify watch on any of
/// them is left in the process): the chosen theme (a theme of the test's own that
/// draws nothing) deleted then wakes nothing, and at rest GMNB doesn't wake at
/// all. Settings opened again notices it: the base look, and GMNB says so.
#[test]
fn nothing_watches_the_folder_once_settings_is_closed() {
    let display = display_or_skip!();
    let profile = Profile::new("closed");
    // A theme of the test's own that draws nothing, chosen: the built-in
    // ones all animate, and an animation would wake GMNB at rest.
    std::fs::create_dir_all(profile.themes().join("plain")).unwrap();
    std::fs::write(
        profile.themes().join("plain/theme.luau"),
        "return { api = 1, name = 'Plain' }",
    )
    .unwrap();
    let app = Running::start(
        &display,
        &profile,
        None,
        &[
            ("GMNB_THEME", "plain"),
            ("GMNB_PREFS", "1"),
            (
                "GMNB_STEPS",
                "wait:300;themes;wait:1500;close;wait:1000;themes;wait:6000;settings;wait:800;\
                 themes;quit",
            ),
        ],
    );
    let pid = app.child.id();
    app.line("themes: ", 1);
    let ours: Vec<u64> = std::iter::once(profile.themes())
        .chain(["aurora", "sprinkles", "swirls", "plain"].map(|id| profile.themes().join(id)))
        .map(|p| std::fs::metadata(&p).unwrap().ino())
        .collect();
    let open = watched(pid);
    for (ino, what) in ours
        .iter()
        .zip(["themes", "aurora", "sprinkles", "swirls", "plain"])
    {
        assert!(
            open.contains(ino),
            "Settings doesn't watch {what}: {open:?}"
        );
    }
    let closed_line = app.line("themes: ", 2);
    assert_eq!(
        themes_line(&closed_line).0,
        Vec::<String>::new(),
        "Settings is closed"
    );
    let closed = watched(pid);
    for ino in &ours {
        assert!(
            !closed.contains(ino),
            "still watched with Settings closed: {closed:?}"
        );
    }
    // The chosen theme deleted with Settings closed; then at rest.
    std::fs::remove_dir_all(profile.themes().join("plain")).unwrap();
    std::thread::sleep(Duration::from_millis(1500));
    let (before, start) = (switches(pid), Instant::now());
    std::thread::sleep(Duration::from_secs(3));
    let woke = switches(pid) - before;
    let took = start.elapsed();
    assert!(
        !app.stderr().contains("isn't installed"),
        "noticed with Settings closed"
    );
    let reopened = themes_line(&app.line("themes: ", 3));
    let err = app.finish();
    eprintln!(
        "at rest after Settings closed, over {:.1} s: {woke} context switches",
        took.as_secs_f64()
    );
    assert_eq!(woke, 0, "GMNB woke at rest after Settings closed: {err}");
    assert_eq!(
        reopened.0,
        ["base", "aurora*", "sprinkles*", "swirls*", "plain"],
        "{err}"
    );
    assert!(
        err.lines()
            .any(|l| l == "gmnb: Theme “plain” isn't installed"),
        "{err}"
    );
}

/// A hostile themes folder with thousands of folders in it: Settings
/// watches the folders a theme could be (a valid id), not all of them, and
/// no more than a fixed number (inotify watches are the user's, all
/// together).
#[test]
fn a_folder_of_thousands_of_folders_is_not_watched_all() {
    let display = display_or_skip!();
    let profile = Profile::new("thousands");
    for n in 0..1200 {
        std::fs::create_dir_all(profile.themes().join(format!("d{n}"))).unwrap();
        std::fs::create_dir_all(profile.themes().join(format!("Not A Theme {n}"))).unwrap();
    }
    let app = Running::start(
        &display,
        &profile,
        None,
        &[
            ("GMNB_THEME", ""),
            ("GMNB_PREFS", "1"),
            ("GMNB_STEPS", "wait:300;themes;wait:3000;quit"),
        ],
    );
    let pid = app.child.id();
    app.line("themes: ", 1);
    let open = watched(pid).len();
    let err = app.finish();
    assert!(
        open <= 100,
        "Settings watches {open} folders for 2400 in the themes folder: {err}"
    );
}
